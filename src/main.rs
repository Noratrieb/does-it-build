use std::{collections::BTreeMap, num::NonZeroUsize, path::Path, process::Command, sync::Mutex};

use color_eyre::{
    eyre::{bail, WrapErr},
    Result,
};

fn main() -> Result<()> {
    let targets = String::from_utf8(
        Command::new("rustc")
            .arg("--print")
            .arg("target-list")
            .output()?
            .stdout,
    )?;

    let year = 2024;
    let current_month = 6_u32;
    let current_day = 12_u32;

    let dates = (current_month.saturating_sub(5)..=current_month).flat_map(|month| {
        if month == current_month && current_day <= 16 {
            vec![format!("{year}-{month:0>2}-01")]
        } else {
            vec![
                format!("{year}-{month:0>2}-01"),
                format!("{year}-{month:0>2}-15"),
            ]
        }
    });

    for date in dates {
        println!("Doing date {date}");

        let toolchain = format!("nightly-{date}");
        let result = Command::new("rustup")
            .arg("toolchain")
            .arg("install")
            .arg(&toolchain)
            .arg("--profile")
            .arg("minimal")
            .spawn()?
            .wait()?;
        if !result.success() {
            bail!("rustup failed");
        }
        let result = Command::new("rustup")
            .arg("component")
            .arg("add")
            .arg("rust-src")
            .arg("--toolchain")
            .arg(&toolchain)
            .spawn()?
            .wait()?;
        if !result.success() {
            bail!("rustup failed");
        }

        let queue = targets.lines().collect::<Vec<&str>>();
        let queue = &Mutex::new(queue);

        std::fs::create_dir_all("targets")?;

        let failures = Mutex::new(BTreeMap::new());

        let targets = Path::new("targets").join(&toolchain);

        std::thread::scope(|s| -> Result<()> {
            let mut handles = vec![];

            for _ in 0..std::thread::available_parallelism()
                .unwrap_or(NonZeroUsize::new(1).unwrap())
                .get()
            {
                let handle = s.spawn(|| -> Result<()> {
                    loop {
                        let target = {
                            let mut queue = queue.lock().unwrap();
                            let Some(next) = queue.pop() else {
                                return Ok(());
                            };
                            println!("remaining: {:>3 } - {next}", queue.len());
                            next
                        };
                        (|| -> Result<()> {
                            let target_dir = targets.join(target);
                            std::fs::create_dir_all(&target_dir)
                                .wrap_err("creating target src dir")?;

                            if !target_dir.join("Cargo.toml").exists() {
                                let init = Command::new("cargo")
                                    .args(["init", "--lib", "--name", "target-test"])
                                    .current_dir(&target_dir)
                                    .output()
                                    .wrap_err("spawning cargo init")?;
                                if !init.status.success() {
                                    bail!("init failed: {}", String::from_utf8(init.stderr)?);
                                }
                            }
                            let librs = target_dir.join("src").join("lib.rs");
                            std::fs::write(&librs, "#![no_std]\n")
                                .wrap_err_with(|| format!("writing to {}", librs.display()))?;

                            let output = Command::new("cargo")
                                .arg(format!("+{toolchain}"))
                                .args(["build", "-Zbuild-std=core", "--target"])
                                .arg(target)
                                .current_dir(&target_dir)
                                .output()
                                .wrap_err("spawning cargo build")?;
                            if !output.status.success() {
                                println!("failure: {target}");
                                let stderr = String::from_utf8(output.stderr)
                                    .wrap_err("cargo stderr utf8")?;
                                failures.lock().unwrap().insert(target.to_owned(), stderr);
                            }
                            Ok(())
                        })()
                        .wrap_err_with(|| format!("while checking {target}"))?;
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap()?;
            }

            Ok(())
        })?;

        std::fs::create_dir_all("results").wrap_err("creating results directory")?;
        std::fs::write(
            Path::new("results").join(date),
            failures
                .lock()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(","),
        )
        .wrap_err("writing results file")?;

        for (target, stderr) in failures.into_inner().unwrap() {
            println!("-----------------\nBROKEN TARGET: {target}\n{stderr}\n\n");
        }
    }

    Ok(())
}
