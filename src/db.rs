use std::{fmt::Display, str::FromStr};

use color_eyre::{
    eyre::{bail, Context},
    Result,
};
use serde::{Deserialize, Serialize};
use sqlx::{migrate::Migrator, sqlite::SqliteConnectOptions, Pool, Sqlite};

#[derive(Clone)]
pub struct Db {
    pub conn: Pool<Sqlite>,
}

pub static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(Debug, Clone, Copy, sqlx::Type, Serialize, PartialEq, Eq, Hash)]
#[sqlx(rename_all = "kebab-case")]
pub enum BuildMode {
    /// `build -Zbuild-std=core`
    Core,
    /// `check -Zbuild-std`
    Std,
}

impl Display for BuildMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => f.write_str("core"),
            Self::Std => f.write_str("std"),
        }
    }
}

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct BuildInfo {
    pub nightly: String,
    pub target: String,
    pub status: Status,
    pub mode: BuildMode,
}

#[derive(Clone, sqlx::FromRow, Serialize)]
pub struct FullBuildInfo {
    pub nightly: String,
    pub target: String,
    pub status: Status,
    pub stderr: String,
    pub mode: BuildMode,
    pub rustflags: Option<String>,
    pub build_date: Option<i64>,
    pub does_it_build_version: Option<String>,
    pub build_duration_ms: Option<i64>,
}

#[derive(Debug, PartialEq, Clone, Copy, sqlx::Type, Serialize, Deserialize)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Error,
    Pass,
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => f.write_str("error"),
            Self::Pass => f.write_str("pass"),
        }
    }
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq, Hash)]
pub struct FinishedNightly {
    pub nightly: String,
    pub mode: BuildMode,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq, Hash)]
pub struct FinishedNightlyWithBroken {
    pub nightly: String,
    pub mode: BuildMode,
    pub is_broken: bool,
    pub broken_error: Option<String>,
}

pub struct BuildStats {
    pub pass_count: u32,
    pub error_count: u32,
}

impl BuildStats {
    pub fn total(&self) -> u32 {
        self.pass_count + self.error_count
    }
}

impl Db {
    pub async fn open(path: &str) -> Result<Self> {
        let db_opts = SqliteConnectOptions::from_str(path)
            .wrap_err("parsing database URL")?
            .create_if_missing(true);

        let conn = Pool::connect_with(db_opts)
            .await
            .wrap_err_with(|| format!("opening db from `{path}`"))?;
        Ok(Self { conn })
    }

    pub async fn insert(&self, info: FullBuildInfo) -> Result<()> {
        sqlx::query(
            "INSERT INTO build_info (nightly, target, status, stderr, mode, rustflags, build_date, does_it_build_version, build_duration_ms) \
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?);",
        )
        .bind(info.nightly)
        .bind(info.target)
        .bind(info.status)
        .bind(info.stderr)
        .bind(info.mode)
        .bind(info.rustflags)
        .bind(info.build_date)
        .bind(info.does_it_build_version)
        .bind(info.build_duration_ms)
        .execute(&self.conn)
        .await
        .wrap_err("inserting build info into database")?;
        Ok(())
    }

    pub async fn history_for_target(&self, target: &str) -> Result<Vec<BuildInfo>> {
        sqlx::query_as::<_, BuildInfo>(
            "SELECT nightly, target, status, mode FROM build_info WHERE target = ?",
        )
        .bind(target)
        .fetch_all(&self.conn)
        .await
        .wrap_err("getting history for single target")
    }

    pub async fn history_for_nightly(&self, nightly: &str) -> Result<Vec<BuildInfo>> {
        sqlx::query_as::<_, BuildInfo>(
            "SELECT nightly, target, status, mode FROM build_info WHERE nightly = ?",
        )
        .bind(nightly)
        .fetch_all(&self.conn)
        .await
        .wrap_err("getting history for single nightly")
    }

    pub async fn nightly_info(&self, nightly: &str) -> Result<Vec<FinishedNightlyWithBroken>> {
        sqlx::query_as::<_, FinishedNightlyWithBroken>(
            "SELECT nightly, mode, is_broken, broken_error FROM finished_nightly WHERE nightly = ?",
        )
        .bind(nightly)
        .fetch_all(&self.conn)
        .await
        .wrap_err("getting finished_nightly for single nightly")
    }

    pub async fn target_list(&self) -> Result<Vec<String>> {
        #[derive(sqlx::FromRow)]
        struct TargetName {
            target: String,
        }

        sqlx::query_as::<_, TargetName>("SELECT DISTINCT target FROM build_info ORDER BY target")
            .fetch_all(&self.conn)
            .await
            .wrap_err("getting list of all targets")
            .map(|elems| elems.into_iter().map(|elem| elem.target).collect())
    }

    pub async fn nightly_list(&self) -> Result<Vec<String>> {
        #[derive(sqlx::FromRow)]
        struct NightlyName {
            nightly: String,
        }

        sqlx::query_as::<_, NightlyName>(
            "SELECT DISTINCT nightly FROM build_info ORDER BY nightly DESC",
        )
        .fetch_all(&self.conn)
        .await
        .wrap_err("getting list of all targets")
        .map(|elems| elems.into_iter().map(|elem| elem.nightly).collect())
    }

    pub async fn build_count(&self) -> Result<BuildStats> {
        #[derive(sqlx::FromRow)]
        struct BuildStat {
            build_count: u32,
            status: Status,
        }

        let results = sqlx::query_as::<_, BuildStat>(
            "SELECT COUNT(status) as build_count, status FROM build_info GROUP BY status",
        )
        .fetch_all(&self.conn)
        .await
        .wrap_err("getting list of all targets")?;

        let count = |status| {
            results
                .iter()
                .find(|row| row.status == status)
                .map(|row| row.build_count)
                .unwrap_or(0)
        };

        Ok(BuildStats {
            pass_count: count(Status::Pass),
            error_count: count(Status::Error),
        })
    }

    pub async fn build_status_full(
        &self,
        nightly: &str,
        target: &str,
        mode: BuildMode,
    ) -> Result<Option<FullBuildInfo>> {
        let result = sqlx::query_as::<_, FullBuildInfo>(
            "SELECT nightly, target, status, stderr, mode, rustflags, build_date, does_it_build_version, build_duration_ms FROM build_info
            WHERE nightly = ? AND target = ? AND mode = ?",
        )
        .bind(nightly)
        .bind(target)
        .bind(mode)
        .fetch_all(&self.conn)
        .await
        .wrap_err("getting build status from DB")?;
        Ok(result.first().cloned())
    }

    pub async fn finished_nightlies(&self) -> Result<Vec<FinishedNightly>> {
        let result =
            sqlx::query_as::<_, FinishedNightly>("SELECT nightly, mode from finished_nightly")
                .fetch_all(&self.conn)
                .await
                .wrap_err("fetching finished nightlies")?;

        Ok(result)
    }

    pub async fn is_nightly_finished(&self, nightly: &str, mode: BuildMode) -> Result<bool> {
        let result = sqlx::query_as::<_, FinishedNightly>(
            "SELECT nightly, mode from finished_nightly WHERE nightly = ? AND mode = ?",
        )
        .bind(nightly)
        .bind(mode)
        .fetch_all(&self.conn)
        .await
        .wrap_err("checking whether a nightly is finished")?;

        if result.len() > 1 {
            bail!("found more than one result for {nightly} {mode}");
        }

        Ok(result.len() == 1)
    }

    pub async fn finish_nightly(&self, nightly: &str, mode: BuildMode) -> Result<()> {
        sqlx::query("INSERT INTO finished_nightly (nightly, mode) VALUES (?, ?)")
            .bind(nightly)
            .bind(mode)
            .execute(&self.conn)
            .await
            .wrap_err("inserting finished nightly")?;
        Ok(())
    }

    pub async fn finish_nightly_as_broken(
        &self,
        nightly: &str,
        mode: BuildMode,
        error: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO finished_nightly (nightly, mode, is_broken, broken_error) VALUES (?, ?, TRUE, ?)")
            .bind(nightly)
            .bind(mode)
            .bind(error)
            .execute(&self.conn)
            .await
            .wrap_err("inserting finished broken nightly")?;
        Ok(())
    }
}
