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

use crate::db::{BuildMode, Db};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
}

pub async fn webserver(db: Db) -> Result<()> {
    let app = Router::new()
        .route("/", get(root))
        .route("/build", get(build))
        .route("/index.css", get(index_css))
        .route("/index.js", get(index_js))
        .route("/target-state", get(target_state))
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

async fn build(State(state): State<AppState>, Query(query): Query<BuildQuery>) -> Response {
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

async fn root() -> impl IntoResponse {
    Html(include_str!("../static/index.html").replace("{{version}}", crate::VERSION))
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

async fn target_state(State(state): State<AppState>) -> impl IntoResponse {
    state.db.build_status().await.map(Json).map_err(|err| {
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
