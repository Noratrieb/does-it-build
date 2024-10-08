use std::{
    fmt::{Debug, Display},
    num::NonZeroUsize,
    path::Path,
    time::Duration,
};

use color_eyre::{
    eyre::{bail, Context},
    Result,
};
use futures::StreamExt;
use tokio::process::Command;
use tracing::{debug, error, info};

use crate::{
    db::{BuildMode, Db, FullBuildInfo, Status},
    nightlies::{Nightlies, NightlyCache},
};

pub struct Toolchain(String);
impl Toolchain {
    pub fn from_nightly(nightly: &str) -> Self {
        Self(format!("nightly-{nightly}"))
    }
}
impl Debug for Toolchain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl Display for Toolchain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub async fn background_builder(db: Db) -> Result<()> {
    let mut nightly_cache = NightlyCache::default();
    loop {
        let nightlies = Nightlies::fetch(&mut nightly_cache)
            .await
            .wrap_err("fetching nightlies")?;
        let already_finished = db
            .finished_nightlies()
            .await
            .wrap_err("fetching finished nightlies")?;

        let next = nightlies.select_latest_to_build(&already_finished);
        match next {
            Some((nightly, mode)) => {
                info!(%nightly, %mode, "Building next nightly");
                let result = build_every_target_for_toolchain(&db, &nightly, mode)
                    .await
                    .wrap_err_with(|| format!("building targets for toolchain {nightly}"));
                if let Err(err) = result {
                    error!(%nightly, %mode, ?err, "Failed to build nightly");
                    db.finish_nightly_as_broken(&nightly, mode)
                        .await
                        .wrap_err("marking nightly as broken")?;
                }
            }
            None => {
                info!("No new nightly, waiting for an hour to try again");
                tokio::time::sleep(Duration::from_secs(1 * 60 * 60)).await;
            }
        }
    }
}

async fn targets_for_toolchain(toolchain: &Toolchain) -> Result<Vec<String>> {
    let output = Command::new("rustc")
        .arg(format!("+{toolchain}"))
        .arg("--print")
        .arg("target-list")
        .output()
        .await
        .wrap_err("failed to spawn rustc")?;
    if !output.status.success() {
        bail!(
            "failed to get target-list from rustc: {:?}",
            String::from_utf8(output.stderr)
        );
    }

    Ok(String::from_utf8(output.stdout)
        .wrap_err("rustc target-list is invalid UTF-8")?
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect())
}

#[tracing::instrument]
async fn install_toolchain(toolchain: &Toolchain, mode: BuildMode) -> Result<()> {
    info!(%toolchain, "Installing toolchain");

    let result = Command::new("rustup")
        .arg("toolchain")
        .arg("install")
        .arg(&toolchain.0)
        .arg("--profile")
        .arg("minimal")
        .output()
        .await
        .wrap_err("failed to spawn rustup")?;
    if !result.status.success() {
        bail!("rustup failed: {:?}", String::from_utf8(result.stderr));
    }
    let result = Command::new("rustup")
        .arg("component")
        .arg("add")
        .arg("rust-src")
        .arg("--toolchain")
        .arg(&toolchain.0)
        .output()
        .await
        .wrap_err("failed to spawn rustup")?;
    if !result.status.success() {
        bail!("rustup failed: {:?}", String::from_utf8(result.stderr));
    }
    if mode == BuildMode::MiriStd {
        let result = Command::new("rustup")
            .arg("component")
            .arg("add")
            .arg("miri")
            .arg("--toolchain")
            .arg(&toolchain.0)
            .output()
            .await
            .wrap_err("failed to spawn rustup")?;
        if !result.status.success() {
            bail!("rustup failed: {:?}", String::from_utf8(result.stderr));
        }
    }
    Ok(())
}

#[tracing::instrument]
async fn uninstall_toolchain(toolchain: &Toolchain) -> Result<()> {
    info!(%toolchain, "Uninstalling toolchain");

    let result = Command::new("rustup")
        .arg("toolchain")
        .arg("remove")
        .arg(&toolchain.0)
        .output()
        .await
        .wrap_err("failed to spawn rustup")?;
    if !result.status.success() {
        bail!(
            "rustup toolchain remove failed: {:?}",
            String::from_utf8(result.stderr)
        );
    }
    Ok(())
}

pub async fn build_every_target_for_toolchain(
    db: &Db,
    nightly: &str,
    mode: BuildMode,
) -> Result<()> {
    if db.is_nightly_finished(nightly, mode).await? {
        debug!("Nightly is already finished, not trying again");
        return Ok(());
    }

    let toolchain = Toolchain::from_nightly(nightly);
    install_toolchain(&toolchain, mode).await?;

    let targets = targets_for_toolchain(&toolchain)
        .await
        .wrap_err("failed to get targets")?;

    let concurrent = std::env::var("DOES_IT_BUILD_PARALLEL_JOBS")
        .map(|jobs| jobs.parse().unwrap())
        .unwrap_or_else(|_| {
            std::thread::available_parallelism()
                .unwrap_or(NonZeroUsize::new(2).unwrap())
                .get()
                / 2
        });

    let results = futures::stream::iter(
        targets
            .iter()
            .map(|target| build_single_target(&db, nightly, target, mode)),
    )
    .buffer_unordered(concurrent)
    .collect::<Vec<Result<()>>>()
    .await;
    for result in results {
        result?;
    }

    for target in targets {
        build_single_target(db, nightly, &target, mode)
            .await
            .wrap_err_with(|| format!("building target {target} for toolchain {toolchain}"))?;
    }

    // Mark it as finished, so we never have to build it again.
    db.finish_nightly(nightly, mode).await?;

    uninstall_toolchain(&toolchain).await?;

    Ok(())
}

#[tracing::instrument(skip(db))]
async fn build_single_target(db: &Db, nightly: &str, target: &str, mode: BuildMode) -> Result<()> {
    let existing = db
        .build_status_full(nightly, target, mode)
        .await
        .wrap_err("getting existing build")?;
    if existing.is_some() {
        debug!("Build already exists");
        return Ok(());
    }

    info!("Building target");

    let tmpdir = tempfile::tempdir().wrap_err("creating temporary directory")?;

    let result = build_target(
        tmpdir.path(),
        &Toolchain::from_nightly(nightly),
        target,
        mode,
    )
    .await
    .wrap_err("running build")?;

    db.insert(FullBuildInfo {
        nightly: nightly.into(),
        target: target.into(),
        status: result.status,
        stderr: result.stderr,
        mode,
    })
    .await?;

    Ok(())
}

struct BuildResult {
    status: Status,
    stderr: String,
}

/// Build a target core in a temporary directory and see whether it passes or not.
async fn build_target(
    tmpdir: &Path,
    toolchain: &Toolchain,
    target: &str,
    mode: BuildMode,
) -> Result<BuildResult> {
    let output = match mode {
        BuildMode::Core => {
            let init = Command::new("cargo")
                .args(["init", "--lib", "--name", "target-test"])
                .current_dir(&tmpdir)
                .output()
                .await
                .wrap_err("spawning cargo init")?;
            if !init.status.success() {
                bail!("init failed: {}", String::from_utf8(init.stderr)?);
            }

            let librs = tmpdir.join("src").join("lib.rs");
            std::fs::write(&librs, "#![no_std]\n")
                .wrap_err_with(|| format!("writing to {}", librs.display()))?;

            Command::new("cargo")
                .arg(format!("+{toolchain}"))
                .args(["build", "-Zbuild-std=core", "--release"])
                .args(["--target", target])
                .current_dir(&tmpdir)
                .output()
                .await
                .wrap_err("spawning cargo build")?
        }
        BuildMode::MiriStd => Command::new("cargo")
            .arg(format!("+{toolchain}"))
            .args(["miri", "setup"])
            .args(["--target", target])
            .current_dir(&tmpdir)
            .env("MIRI_SYSROOT", tmpdir)
            .output()
            .await
            .wrap_err("spawning cargo build")?,
    };

    let stderr = String::from_utf8(output.stderr).wrap_err("cargo stderr utf8")?;

    let status = if output.status.success() {
        Status::Pass
    } else {
        Status::Error
    };

    info!("Finished build");

    Ok(BuildResult { status, stderr })
}
