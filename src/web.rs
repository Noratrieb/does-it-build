use std::{cmp::Reverse, collections::HashMap};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use color_eyre::{eyre::Context, Result};
use serde::Deserialize;
use tracing::{error, info};

use crate::{
    db::{BuildInfo, BuildMode, BuildStats, Db, Status},
    notification,
};

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
        .route("/index.js", get(index_js))
        .with_state(AppState { db });

    info!("Serving website on port 3000 (commit {})", crate::VERSION);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.wrap_err("failed to serve")
}

#[derive(Debug, Clone, Copy)]
pub struct LegacyBuildMode(BuildMode);

impl<'de> serde::Deserialize<'de> for LegacyBuildMode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "core" => Ok(LegacyBuildMode(BuildMode::Std)),
            "std" => Ok(LegacyBuildMode(BuildMode::Std)),
            // This mode used to be called "miri-std" but it has been renamed to "std" using build-std.
            // Allow the old value to keep links working but map it to std.
            "miri-std" => Ok(LegacyBuildMode(BuildMode::Std)),
            _ => Err(serde::de::Error::custom(
                "invalid build mode, expected 'core', 'std', or 'miri-std'".to_owned(),
            )),
        }
    }
}

#[derive(Deserialize)]
struct BuildQuery {
    nightly: String,
    target: String,
    mode: Option<LegacyBuildMode>,
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
        build_date: Option<String>,
        build_duration_s: Option<f32>,
        does_it_build_version: Option<String>,
    }

    match state
        .db
        .build_status_full(
            &query.nightly,
            &query.target,
            query.mode.map(|mode| mode.0).unwrap_or(BuildMode::Std),
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
                build_date: build.build_date.and_then(|build_date| {
                    jiff::Timestamp::from_millisecond(build_date)
                        .ok()
                        .map(|build_date| build_date.to_string())
                }),
                build_duration_s: build
                    .build_duration_ms
                    .map(|build_duration_ms| (build_duration_ms as f32) / 1000.0),
                does_it_build_version: build.does_it_build_version,
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
        notification_pr_url: String,
        maintainers: Option<&'static [&'static str]>,
    }

    let filter_failures = query.failures.unwrap_or(false);

    match state.db.history_for_target(&query.target).await {
        Ok(builds) => {
            let latest_std = builds
                .iter()
                .filter(|build| build.mode == BuildMode::Std)
                .max_by_key(|elem| elem.nightly.clone());

            let status = match latest_std {
                Some(one) => one.status.to_string(),
                None => "missing".to_owned(),
            };

            let mut builds_grouped =
                HashMap::<String, (Option<BuildInfo>, Option<BuildInfo>)>::new();
            for build in builds {
                let v = builds_grouped.entry(build.nightly.clone()).or_default();
                match build.mode {
                    BuildMode::Std => v.1 = Some(build),
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

            let maintainers = notification::maintainers_for_target(&query.target);

            let page = TargetPage {
                status,
                target: query.target,
                version: crate::VERSION,
                builds,
                showing_failures: filter_failures,
                notification_pr_url: notification::notification_pr_url(),
                maintainers,
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
        std_failures: usize,
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
                        BuildMode::Std => v.1 = Some(build.clone()),
                    }
                }

                let mut std_failures = 0;
                for build in builds {
                    if build.status == Status::Error {
                        match build.mode {
                            BuildMode::Std => std_failures += 1,
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

                let std_broken = info
                    .iter()
                    .find(|info| info.mode == BuildMode::Std && info.is_broken)
                    .and_then(|info| info.broken_error.clone());

                let page = NightlyPage {
                    nightly: query.nightly,
                    version: crate::VERSION,
                    builds,
                    std_failures,
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

    async fn render(state: AppState) -> Result<Response> {
        #[derive(askama::Template)]
        #[template(path = "index.html")]
        struct RootPage {
            targets: Vec<String>,
            nightlies: Vec<String>,
            version: &'static str,
            build_count: BuildStats,
            notification_pr_url: String,
        }

        let targets = state.db.target_list().await?;
        let nightlies = state.db.nightly_list().await?;
        let build_count = state.db.build_count().await?;

        let page = RootPage {
            targets,
            nightlies,
            version: crate::VERSION,
            build_count,
            notification_pr_url: notification::notification_pr_url(),
        };

        Ok(Html(page.render().unwrap()).into_response())
    }

    render(state)
        .await
        .unwrap_or_else(|err: color_eyre::eyre::Error| {
            error!(?err, "Error loading data for root page");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })
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
            axum::http::HeaderValue::from_static("application/javascript; charset=utf-8"),
        )],
        include_str!("../static/index.js"),
    )
}

impl Status {
    fn to_emoji(self) -> &'static str {
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
