use std::collections::HashSet;
use std::hash::RandomState;

use rootcause::prelude::ResultExt;
use tracing::debug;

use crate::db::{BuildMode, FinishedNightly};
use crate::Result;

const EARLIEST_CUTOFF_DATE: &str = "2023-01-01";

/// All nightlies that exist.
pub struct Nightlies {
    all: Vec<String>,
}

impl Nightlies {
    async fn get_body(url: &str) -> Result<String, rootcause::Report> {
        Ok(reqwest::get(url)
            .await
            .context("executing GET request")?
            .text()
            .await
            .context("fetching body")?)
    }

    pub async fn fetch() -> Result<Nightlies> {
        let url = "https://static.rust-lang.org/manifests.txt";
        let manifests = Nightlies::get_body(url)
            .await
            .context(format!("Fetching manifests.txt"))
            .attach(format!("url: {url}"))?;
        let mut all = nightlies_from_manifest(&manifests)
            .into_iter()
            .filter(|date| date.as_str() > EARLIEST_CUTOFF_DATE)
            .collect::<Vec<_>>();

        all.sort_by(|a, b| b.cmp(a)); // Reverse sort.

        debug!(
            "Loaded {} nightlies from the manifest and manual additions",
            all.len()
        );
        Ok(Self { all })
    }

    pub fn select_latest_to_build(
        &self,
        already_finished: &[FinishedNightly],
    ) -> Option<(String, BuildMode)> {
        let already_finished = HashSet::<_, RandomState>::from_iter(already_finished.iter());

        self.all
            .iter()
            .flat_map(|nightly| [(nightly, BuildMode::Std)])
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
}
