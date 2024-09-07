use std::{fmt::Display, str::FromStr};

use color_eyre::{eyre::Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{migrate::Migrator, sqlite::SqliteConnectOptions, Pool, Sqlite};

#[derive(Clone)]
pub struct Db {
    pub conn: Pool<Sqlite>,
}

pub static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(sqlx::FromRow, Serialize, Deserialize)]
pub struct BuildInfo {
    pub nightly: String,
    pub target: String,
    pub status: Status,
}

#[derive(Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct FullBuildInfo {
    pub nightly: String,
    pub target: String,
    pub status: Status,
    pub stderr: String,
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

#[derive(sqlx::FromRow)]
struct FinishedNightly {
    nightly: String,
}

impl Db {
    pub async fn open(path: &str) -> Result<Self> {
        let db_opts = SqliteConnectOptions::from_str(path)
            .wrap_err("parsing database URL")?
            .create_if_missing(true);

        let conn = Pool::connect_with(db_opts)
            .await
            .wrap_err_with(|| format!("opening db from `{}`", path))?;
        Ok(Self { conn })
    }

    pub async fn insert(&self, info: FullBuildInfo) -> Result<()> {
        sqlx::query(
            "INSERT INTO build_info (nightly, target, status, stderr) VALUES (?, ?, ?, ?);",
        )
        .bind(info.nightly)
        .bind(info.target)
        .bind(info.status)
        .bind(info.stderr)
        .execute(&self.conn)
        .await
        .wrap_err("inserting build info into database")?;
        Ok(())
    }

    pub async fn build_status(&self) -> Result<Vec<BuildInfo>> {
        sqlx::query_as::<_, BuildInfo>("SELECT nightly, target, status FROM build_info")
            .fetch_all(&self.conn)
            .await
            .wrap_err("getting build status from DB")
    }

    pub async fn build_status_full(
        &self,
        nightly: &str,
        target: &str,
    ) -> Result<Option<FullBuildInfo>> {
        let result = sqlx::query_as::<_, FullBuildInfo>(
            "SELECT nightly, target, status, stderr FROM build_info
            WHERE nightly = ? AND target = ?",
        )
        .bind(nightly)
        .bind(target)
        .fetch_all(&self.conn)
        .await
        .wrap_err("getting build status from DB")?;
        Ok(result.first().cloned())
    }

    pub async fn finished_nightlies(&self) -> Result<Vec<String>> {
        let result = sqlx::query_as::<_, FinishedNightly>("SELECT nightly from finished_nightly")
            .fetch_all(&self.conn)
            .await
            .wrap_err("fetching fnished nightlies")?;

        Ok(result.into_iter().map(|nightly| nightly.nightly).collect())
    }

    pub async fn is_nightly_finished(&self, nightly: &str) -> Result<bool> {
        let result = sqlx::query_as::<_, FinishedNightly>(
            "SELECT nightly from finished_nightly WHERE nightly = ?",
        )
        .bind(nightly)
        .fetch_all(&self.conn)
        .await
        .wrap_err("fetching fnished nightlies")?;

        Ok(result.len() == 1)
    }

    pub async fn finish_nightly(&self, nightly: &str) -> Result<()> {
        sqlx::query("INSERT INTO finished_nightly (nightly) VALUES (?)")
            .bind(nightly)
            .execute(&self.conn)
            .await
            .wrap_err("inserting finished nightly")?;
        Ok(())
    }
}
