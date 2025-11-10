mod build;
mod db;
mod nightlies;
mod notification;
mod web;

use color_eyre::{eyre::WrapErr, Result};
use db::Db;
use tracing_subscriber::EnvFilter;

const VERSION: &str = env!("GIT_COMMIT");
const VERSION_SHORT: &str = env!("GIT_COMMIT_SHORT");

#[tokio::main]
async fn main() -> Result<()> {
    main_inner().await
}

async fn main_inner() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info")))
        .init();

    let db = Db::open(&std::env::var("DB_PATH").unwrap_or("db.sqlite".into())).await?;
    db::MIGRATOR
        .run(&db.conn)
        .await
        .wrap_err("running migrations")?;

    let send_pings = std::env::var("GITHUB_SEND_PINGS")
        .map(|_| true)
        .unwrap_or(false);
    let github_owner = std::env::var("GITHUB_OWNER").wrap_err("missing GITHUB_OWNER env var")?;
    let github_repo = std::env::var("GITHUB_REPO").wrap_err("missing GITHUB_REPO env var")?;
    let app_id = std::env::var("GITHUB_APP_ID")
        .wrap_err("missing GITHUB_APP_ID env var")?
        .parse::<u64>()
        .wrap_err("invalid GITHUB_APP_ID")?;
    let key = std::env::var("GITHUB_APP_PRIVATE_KEY")
        .wrap_err("missing GITHUB_APP_PRIVATE_KEY env var")?;
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(key.as_bytes()).unwrap();

    let github_client = octocrab::Octocrab::builder()
        .app(app_id.into(), key)
        .build()
        .wrap_err("failed to create client")?;

    let github_client = notification::GitHubClient::new(
        send_pings,
        github_client,
        github_owner.clone(),
        github_repo.clone(),
    )
    .await?;

    let builder = build::background_builder(db.clone(), github_client);
    let server = web::webserver(db);

    tokio::select! {
        result = builder => {
            result
        }
        result = server => {
            result
        }
    }
}
