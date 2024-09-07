mod build;
mod db;
mod nightlies;
mod web;

use color_eyre::{eyre::WrapErr, Result};
use db::Db;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    let db = Db::open(&std::env::var("DB_PATH").unwrap_or("db.sqlite".into())).await?;
    db::MIGRATOR
        .run(&db.conn)
        .await
        .wrap_err("running migrations")?;

    let builder = build::background_builder(db.clone());
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
