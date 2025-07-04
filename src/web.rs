use std::{cmp::Reverse, collections::HashMap};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
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
        .route("/nightly", get(web_nightly))
        .route("/index.css", get(index_css))
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
    use askama::Template;
    #[derive(askama::Template)]
    #[template(path = "build.html")]
    struct BuildPage {
        nightly: String,
        target: String,
        stderr: String,
        mode: BuildMode,
        rustflags: Option<String>,
        version: &'static str,
        status: Status,
    }

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
            let page = BuildPage {
                nightly: query.nightly,
                target: query.target,
                stderr: build.stderr,
                mode: build.mode,
                rustflags: build.rustflags,
                version: crate::VERSION,
                status: build.status,
            };
            Html(page.render().unwrap()).into_response()
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
    failures: Option<bool>,
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
        showing_failures: bool,
    }

    let filter_failures = query.failures.unwrap_or(false);

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
                .filter(|(_, core_build, std_build)| {
                    filter_build(filter_failures, core_build)
                        || filter_build(filter_failures, std_build)
                })
                .collect::<Vec<_>>();
            builds.sort_by_cached_key(|build| Reverse(build.0.clone()));

            let page = TargetPage {
                status,
                target: query.target,
                version: crate::VERSION,
                builds,
                showing_failures: filter_failures,
            };

            Html(page.render().unwrap()).into_response()
        }
        Err(err) => {
            error!(?err, "Error loading target state");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
struct NightlyQuery {
    nightly: String,
    failures: Option<bool>,
}

async fn web_nightly(State(state): State<AppState>, Query(query): Query<NightlyQuery>) -> Response {
    use askama::Template;
    #[derive(askama::Template)]
    #[template(path = "nightly.html")]
    struct NightlyPage {
        nightly: String,
        version: &'static str,
        builds: Vec<(String, Option<BuildInfo>, Option<BuildInfo>)>,
        core_failures: usize,
        std_failures: usize,
        core_broken: Option<String>,
        std_broken: Option<String>,
        showing_failures: bool,
    }

    let filter_failures = query.failures.unwrap_or(false);

    match state.db.history_for_nightly(&query.nightly).await {
        Ok(builds) => match state.db.nightly_info(&query.nightly).await {
            Ok(info) => {
                let mut builds_grouped =
                    HashMap::<String, (Option<BuildInfo>, Option<BuildInfo>)>::new();
                for build in &builds {
                    let v = builds_grouped.entry(build.target.clone()).or_default();
                    match build.mode {
                        BuildMode::Core => v.0 = Some(build.clone()),
                        BuildMode::MiriStd => v.1 = Some(build.clone()),
                    }
                }

                let mut std_failures = 0;
                let mut core_failures = 0;
                for build in builds {
                    if build.status == Status::Error {
                        match build.mode {
                            BuildMode::Core => core_failures += 1,
                            BuildMode::MiriStd => std_failures += 1,
                        }
                    }
                }

                let mut builds = builds_grouped
                    .into_iter()
                    .map(|(k, (v1, v2))| (k, v1, v2))
                    .filter(|(_, core_build, std_build)| {
                        filter_build(filter_failures, core_build)
                            || filter_build(filter_failures, std_build)
                    })
                    .collect::<Vec<_>>();
                builds.sort_by_cached_key(|build| build.0.clone());

                let core_broken = info
                    .iter()
                    .find(|info| info.mode == BuildMode::Core && info.is_broken)
                    .and_then(|info| info.broken_error.clone());
                let std_broken = info
                    .iter()
                    .find(|info| info.mode == BuildMode::MiriStd && info.is_broken)
                    .and_then(|info| info.broken_error.clone());

                let page = NightlyPage {
                    nightly: query.nightly,
                    version: crate::VERSION,
                    builds,
                    std_failures,
                    core_failures,
                    core_broken,
                    std_broken,
                    showing_failures: filter_failures,
                };

                Html(page.render().unwrap()).into_response()
            }
            Err(err) => {
                error!(?err, "Error loading target state");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        },
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
        nightlies: Vec<String>,
        version: &'static str,
    }

    match state.db.target_list().await {
        Ok(targets) => match state.db.nightly_list().await {
            Ok(nightlies) => {
                let page = RootPage {
                    targets,
                    nightlies,
                    version: crate::VERSION,
                };

                Html(page.render().unwrap()).into_response()
            }
            Err(err) => {
                error!(?err, "Error loading nightly state");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        },
        Err(err) => {
            error!(?err, "Error loading target state");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
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

#[derive(Serialize, Deserialize)]
struct TriggerBuildBody {
    nightly: String,
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

fn filter_build(filter_failures: bool, build: &Option<BuildInfo>) -> bool {
    !filter_failures
        || build
            .as_ref()
            .is_some_and(|build| build.status == Status::Error)
}
