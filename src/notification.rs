use octocrab::models::issues::IssueStateReason;
use octocrab::models::IssueState;
use rootcause::prelude::ResultExt;
use tracing::info;

use crate::db::{Db, FullBuildInfo, NotificationIssue, NotificationStatus, Status};
use crate::Result;

pub const TABLE_FILE: &str = file!();
pub const TABLE_LINE: u32 = line!() + 1;
const TARGET_NOTIFICATIONS: &[(&str, &[&str])] = &[
    ("aarch64-unknown-linux-musl", &["Gelbpunkt", "famfo"]),
    (
        "aarch64_be-unknown-linux-musl",
        &["Gelbpunkt", "neuschaefer"],
    ),
    ("aarch64_be-unknown-none-softfloat", &["Gelbpunkt"]),
    ("armv7-sony-vita-newlibeabihf", &["pheki"]),
    ("mips64-unknown-linux-muslabi64", &["Gelbpunkt"]),
    (
        "powerpc64-unknown-linux-musl",
        &["Gelbpunkt", "famfo", "neuschaefer"],
    ),
    (
        "powerpc64le-unknown-linux-musl",
        &["Gelbpunkt", "famfo", "neuschaefer"],
    ),
];

pub fn notification_pr_url() -> String {
    format!("https://github.com/Noratrieb/does-it-build/blob/main/{TABLE_FILE}#L{TABLE_LINE}")
}

pub fn maintainers_for_target(target: &str) -> Option<&'static [&'static str]> {
    TARGET_NOTIFICATIONS
        .iter()
        .find(|(target_name, _)| *target_name == target)
        .map(|(_, maintainers)| *maintainers)
}

pub struct GitHubClient {
    pub send_pings: bool,
    owner: String,
    repo: String,
    pub client: octocrab::Octocrab,
}

impl GitHubClient {
    pub async fn new(
        send_pings: bool,
        client: octocrab::Octocrab,
        owner: String,
        repo: String,
    ) -> Result<Self> {
        let installation = client
            .apps()
            .get_repository_installation(&owner, &repo)
            .await
            .context_with(|| format!("getting installation for {owner}/{repo}"))?;

        let client = client
            .installation(installation.id)
            .context("getting client for installation")?;

        Ok(Self {
            send_pings,
            owner,
            repo,
            client,
        })
    }

    pub fn issues(&self) -> octocrab::issues::IssueHandler<'_> {
        self.client.issues(&self.owner, &self.repo)
    }
}

pub async fn notify_build(
    github_client: &GitHubClient,
    db: &Db,
    build_info: &FullBuildInfo,
) -> Result<()> {
    match build_info.status {
        Status::Error => notify_build_failure(github_client, db, build_info).await,
        Status::Pass => notify_build_pass(github_client, db, build_info).await,
    }
}

pub async fn notify_build_failure(
    github_client: &GitHubClient,
    db: &Db,
    build_info: &FullBuildInfo,
) -> Result<()> {
    let FullBuildInfo {
        target,
        nightly,
        stderr,
        ..
    } = build_info;

    let Some(notify_usernames) = maintainers_for_target(target) else {
        return Ok(());
    };

    let issue = db.find_existing_notification(target).await?;

    let url = format!(
        "https://does-it-build.noratrieb.dev/build?nightly={nightly}&target={target}&mode=std"
    );

    if let Some(issue) = issue {
        // An existing issue, send a comment if it's been a month since the last update.

        if issue.last_update_date.is_none_or(|last_update_date| {
            jiff::Timestamp::from_millisecond(last_update_date).is_ok_and(|last_update_date| {
                jiff::Timestamp::now()
                    .since(last_update_date)
                    .is_ok_and(|diff| {
                        diff.total((
                            jiff::Unit::Month,
                            &last_update_date.to_zoned(jiff::tz::TimeZone::UTC),
                        ))
                        .is_ok_and(|diff_months| diff_months >= 1.0)
                    })
            })
        }) {
            info!(
                "Sending update for {target}, since enough time has elapsed since the last update"
            );

            github_client
                .issues()
                .create_comment(
                    issue.issue_number as u64,
                    format!(
                        "💥 The target {target} still fails to build on the nightly {nightly}!

<{url}>

<details><summary>full logs</summary>

```
{stderr}
```

</details>

This update is sent after a month of inactivity.
"
                    ),
                )
                .await
                .context("creating update comment")?;

            db.set_notification_last_update(issue.issue_number, jiff::Timestamp::now())
                .await
                .context("updating last_update_date in DB")?;
        } else {
            info!("Not sending update for {target}, since not enough time has elapsed since the last one");
        }

        return Ok(());
    }

    info!("Creating issue for target {target}, notifying {notify_usernames:?}");

    // Ensure the labels exist.
    let label = github_client.issues().get_label(target).await;
    match label {
        Ok(_) => {}
        Err(octocrab::Error::GitHub { source, .. }) if source.status_code.as_u16() == 404 => {
            github_client
                .issues()
                .create_label(target, "d73a4a", format!("Target: {target}"))
                .await
                .context("creating label")?;
        }
        Err(err) => {
            return Err(err)
                .context("failed to fetch label label")
                .map_err(Into::into)
        }
    }

    let pings = notify_usernames
        .iter()
        .map(|name| {
            if github_client.send_pings {
                format!("@{name}")
            } else {
                format!("@\\{name}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let issue = github_client
        .issues()
        .create(format!("{target} fails to build on {nightly}"))
        .labels(Some(vec![target.to_owned()]))
        .body(format!(
            "💥 The target {target} fails to build on the nightly {nightly}!

<{url}>

<details>
<summary>full logs</summary>

```
{stderr}
```

</details>

{pings}

This issue will be closed automatically when this target works again!"
        ))
        .send()
        .await
        .context("failed to create issue")?;

    db.insert_notification(NotificationIssue {
        first_failed_nightly: nightly.into(),
        issue_number: issue.number as i64,
        status: NotificationStatus::Open,
        target: target.into(),
        last_update_date: Some(jiff::Timestamp::now().as_millisecond()),
    })
    .await
    .context("inserting issue into DB")?;

    Ok(())
}

pub async fn notify_build_pass(
    github_client: &GitHubClient,
    db: &Db,
    build_info: &FullBuildInfo,
) -> Result<()> {
    let FullBuildInfo {
        target, nightly, ..
    } = build_info;

    let issue = db.find_existing_notification(target).await?;

    if let Some(issue) = issue {
        info!(
            "Closing issue {} for {target}, since {nightly} builds again",
            issue.issue_number
        );

        let url = format!(
            "https://does-it-build.noratrieb.dev/build?nightly={nightly}&target={target}&mode=std"
        );

        // An existing issue, send a comment.

        github_client
            .issues()
            .create_comment(
                issue.issue_number as u64,
                format!("✅ The target {target} successfully builds on nightly {nightly}, \
                thanks for playing this round of Tier 3 rustc target breakage fixing! See y'all next time :3!\n\n<{url}>"),
            )
            .await
            .context("creating update comment")?;

        github_client
            .issues()
            .update(issue.issue_number as u64)
            .state(IssueState::Closed)
            .state_reason(IssueStateReason::Completed)
            .send()
            .await
            .context("closing issue")?;

        db.finish_notification(issue.issue_number).await?;
    }

    Ok(())
}
