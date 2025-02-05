use std::{cmp::Reverse, collections::HashMap};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use color_eyre::{eyre::Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::db::{BuildInfo, BuildMode, Db, Status};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
}

pub async fn webserver(db: Db) -> Result<()> {
    let app = Router::new()
        .route("/", get(web_root))
        .route("/build", get(web_build))
        .route("/target", get(web_target))
        .route("/full-table", get(web_full_table))
        .route("/index.css", get(index_css))
        .route("/index.js", get(index_js))
        .route("/full-mega-monster", get(full_mega_monster))
        .route("/trigger-build", post(trigger_build))
        .with_state(AppState { db });

    info!("Serving website on port 3000 (commit {})", crate::VERSION);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.wrap_err("failed to serve")
}

#[derive(Deserialize)]
struct BuildQuery {
    nightly: String,
    target: String,
    mode: Option<BuildMode>,
}

async fn web_build(State(state): State<AppState>, Query(query): Query<BuildQuery>) -> Response {
    match state
        .db
        .build_status_full(
            &query.nightly,
            &query.target,
            query.mode.unwrap_or(BuildMode::Core),
        )
        .await
    {
        Ok(Some(build)) => {
            let page = include_str!("../static/build.html")
                .replace("{{nightly}}", &query.nightly)
                .replace("{{target}}", &query.target)
                .replace("{{stderr}}", &build.stderr)
                .replace("{{mode}}", &build.mode.to_string())
                .replace("{{version}}", crate::VERSION)
                .replace("{{status}}", &build.status.to_string());

            Html(page).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            error!(?err, "Error loading target state");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
struct TargetQuery {
    target: String,
}

async fn web_target(State(state): State<AppState>, Query(query): Query<TargetQuery>) -> Response {
    use askama::Template;
    #[derive(askama::Template)]
    #[template(path = "target.html")]
    struct TargetPage {
        target: String,
        status: String,
        version: &'static str,
        builds: Vec<(String, Option<BuildInfo>, Option<BuildInfo>)>,
    }

    match state.db.history_for_target(&query.target).await {
        Ok(builds) => {
            let latest_core = builds
                .iter()
                .filter(|build| build.mode == BuildMode::Core)
                .max_by_key(|elem| elem.nightly.clone());
            let latest_miri = builds
                .iter()
                .filter(|build| build.mode == BuildMode::Core)
                .max_by_key(|elem| elem.nightly.clone());

            let status = match (latest_core, latest_miri) {
                (Some(core), Some(miri)) => {
                    if core.status == Status::Error || miri.status == Status::Error {
                        Status::Error
                    } else {
                        Status::Pass
                    }
                    .to_string()
                }
                (Some(one), None) | (None, Some(one)) => one.status.to_string(),
                (None, None) => "missing".to_owned(),
            };

            let mut builds_grouped =
                HashMap::<String, (Option<BuildInfo>, Option<BuildInfo>)>::new();
            for build in builds {
                let v = builds_grouped.entry(build.nightly.clone()).or_default();
                match build.mode {
                    BuildMode::Core => v.0 = Some(build),
                    BuildMode::MiriStd => v.1 = Some(build),
                }
            }

            let mut builds = builds_grouped
                .into_iter()
                .map(|(k, (v1, v2))| (k, v1, v2))
                .collect::<Vec<_>>();
            builds.sort_by_cached_key(|build| Reverse(build.0.clone()));

            let page = TargetPage {
                status,
                target: query.target,
                version: crate::VERSION,
                builds,
            };

            Html(page.render().unwrap()).into_response()
        }
        Err(err) => {
            error!(?err, "Error loading target state");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn web_root(State(state): State<AppState>) -> impl IntoResponse {
    use askama::Template;
    #[derive(askama::Template)]
    #[template(path = "index.html")]
    struct RootPage {
        targets: Vec<String>,
        version: &'static str,
    }

    match state.db.target_list().await {
        Ok(targets) => {
            let page = RootPage {
                targets,
                version: crate::VERSION,
            };

            Html(page.render().unwrap()).into_response()
        }
        Err(err) => {
            error!(?err, "Error loading target state");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn web_full_table() -> impl IntoResponse {
    Html(include_str!("../static/full-table.html").replace("{{version}}", crate::VERSION))
}
async fn index_css() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("text/css; charset=utf-8"),
        )],
        include_str!("../static/index.css"),
    )
}
async fn index_js() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("text/javascript"),
        )],
        include_str!("../static/index.js"),
    )
}

async fn full_mega_monster(State(state): State<AppState>) -> impl IntoResponse {
    state.db.full_mega_monster().await.map(Json).map_err(|err| {
        error!(?err, "Error loading target state");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(Serialize, Deserialize)]
struct TriggerBuildBody {
    nightly: String,
}

#[axum::debug_handler]
async fn trigger_build(
    State(_state): State<AppState>,
    _body: Json<TriggerBuildBody>,
) -> StatusCode {
    return StatusCode::BAD_REQUEST;
    // tokio::spawn(async move {
    //     let result = build::build_every_target_for_toolchain(&state.db, &body.nightly).await;
    //     if let Err(err) = result {
    //         error!(?err, "Error while building");
    //     }
    // });
    //
    // StatusCode::ACCEPTED
}

impl Status {
    fn to_emoji(&self) -> &'static str {
        match self {
            Status::Pass => "✅",
            Status::Error => "❌",
        }
    }
}

impl BuildInfo {
    fn link(&self) -> String {
        format!(
            "build?nightly={}&target={}&mode={}",
            self.nightly, self.target, self.mode
        )
    }
}
