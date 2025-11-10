use color_eyre::{eyre::Context, Result};
use octocrab::issues;

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
            .wrap_err_with(|| format!("getting installation for {owner}/{repo}"))?;

        let client = client
            .installation(installation.id)
            .wrap_err("getting client for installation")?;

        Ok(Self {
            send_pings,
            owner,
            repo,
            client,
        })
    }

    pub fn issues(&self) -> issues::IssueHandler<'_> {
        self.client.issues(&self.owner, &self.repo)
    }
}
