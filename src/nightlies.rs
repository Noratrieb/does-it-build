use std::collections::HashSet;
use std::hash::RandomState;

use color_eyre::eyre::{Context, OptionExt};
use color_eyre::Result;
use reqwest::StatusCode;
use time::Duration;
use tracing::debug;

use crate::db::{BuildMode, FinishedNightly};

const EARLIEST_CUTOFF_DATE: &str = "2023-01-01";

#[derive(Default)]
pub struct NightlyCache {
    /// Nightlies that exist.
    exists: HashSet<String>,
}

/// All nightlies that exist.
pub struct Nightlies {
    all: Vec<String>,
}

impl Nightlies {
    pub async fn fetch(cache: &mut NightlyCache) -> Result<Nightlies> {
        let manifests = reqwest::get("https://static.rust-lang.org/manifests.txt")
            .await
            .wrap_err("fetching https://static.rust-lang.org/manifests.txt")?
            .text()
            .await
            .wrap_err("fetching body of https://static.rust-lang.org/manifests.txt")?;
        let mut all = nightlies_from_manifest(&manifests)
            .into_iter()
            .filter(|date| date.as_str() > EARLIEST_CUTOFF_DATE)
            .collect::<Vec<_>>();

        all.sort();

        // The manifests is only updated weekly, which means new nightlies won't be contained.
        // We probe for their existence.
        let latest = all
            .last()
            .ok_or_eyre("did not find any nightlies in manifets.txt")?;

        for nightly in guess_more_recent_nightlies(&latest)? {
            if nightly_exists(&nightly, cache)
                .await
                .wrap_err_with(|| format!("checking whether {nightly} exists"))?
            {
                all.push(nightly);
            }
        }

        all.reverse();

        debug!("Loaded {} nightlies from the manifest and manual additions", all.len());
        Ok(Self { all })
    }

    pub fn select_latest_to_build(
        &self,
        already_finished: &[FinishedNightly],
    ) -> Option<(String, BuildMode)> {
        let already_finished = HashSet::<_, RandomState>::from_iter(already_finished.iter());

        self.all
            .iter()
            .flat_map(|nightly| [(nightly, BuildMode::Core), (nightly, BuildMode::MiriStd)])
            .find(|(nightly, mode)| {
                !already_finished.contains(&FinishedNightly {
                    nightly: (*nightly).to_owned(),
                    mode: *mode,
                })
            })
            .map(|(nightly, mode)| (nightly.clone(), mode))
    }
}

fn nightlies_from_manifest(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("static.rust-lang.org/dist/")?;
            let date = rest.strip_suffix("/channel-rust-nightly.toml")?;

            Some(date.to_owned())
        })
        .collect()
}

fn guess_more_recent_nightlies(latest: &str) -> Result<Vec<String>> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let latest = time::Date::parse(latest, format).wrap_err("latest nightly has invalid format")?;

    // manifests.txt is updated weekly, so let's try 8 just in case.
    Ok((1..=8)
        .filter_map(|offset| latest.checked_add(Duration::days(offset)))
        .map(|date| date.format(format).unwrap())
        .collect())
}

async fn nightly_exists(nightly: &str, cache: &mut NightlyCache) -> Result<bool> {
    if cache.exists.contains(nightly) {
        return Ok(true);
    }
    let url = format!("https://static.rust-lang.org/dist/{nightly}/channel-rust-nightly.toml");
    let resp = reqwest::get(&url).await.wrap_err("fetching channel")?;
    debug!(%nightly, %url, status = %resp.status(), "Checked whether a recent nightly exists");
    let exists = resp.status() == StatusCode::OK;
    if exists {
        cache.exists.insert(nightly.to_owned());
    }
    Ok(exists)
}

#[cfg(test)]
mod tests {
    #[test]
    fn manifest_parse() {
        let test_manifest = "static.rust-lang.org/dist/2024-08-22/channel-rust-nightly.toml
static.rust-lang.org/dist/2024-08-22/channel-rust-1.81.0-beta.toml
static.rust-lang.org/dist/2024-08-22/channel-rust-1.81.0-beta.6.toml
static.rust-lang.org/dist/2024-08-23/channel-rust-nightly.toml";

        let nightlies = super::nightlies_from_manifest(&test_manifest);
        assert_eq!(nightlies, vec!["2024-08-22", "2024-08-23"]);
    }

    #[test]
    fn guess() {
        let nightlies = super::guess_more_recent_nightlies("2024-08-28").unwrap();
        assert_eq!(
            nightlies,
            [
                "2024-08-29",
                "2024-08-30",
                "2024-08-31",
                "2024-09-01",
                "2024-09-02",
                "2024-09-03",
                "2024-09-04",
                "2024-09-05",
            ]
        );
    }
}
