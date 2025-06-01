use crate::domain::external_apis::github::{GitHubApi, Repository};
use crate::domain::models::run::WorkflowRun;
use anyhow::{Context, Error};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
struct GitHubRepositoryResponse {
    name: String,
    owner: GitHubOwnerResponse,
}

#[derive(Deserialize, Debug, Clone)]
struct GitHubOwnerResponse {
    login: String,
}

#[derive(Deserialize, Debug, Clone)]
struct GitHubRepositoryMinimalResponse {
    full_name: String, // e.g., "owner/repo"
}

#[derive(Deserialize, Debug, Clone)]
struct GitHubWorkflowRunResponse {
    id: u64,
    name: String, // workflow name
    display_title: String,
    event: String,
    status: String,
    conclusion: Option<String>, // Refer to this when status is "completed"
    created_at: String,         // ISO 8601 format, parse during domain model conversion
    updated_at: String,         // ISO 8601 format, parse during domain model conversion
    html_url: String,
    repository: GitHubRepositoryMinimalResponse, // Type changed as instructed
}

// The response from the GitHub API's /actions/runs endpoint is
// wrapped in an object with the workflow_runs array as a key,
// so define a wrapper structure for it.
#[derive(Deserialize, Debug)]
struct GitHubWorkflowRunsApiResponse {
    workflow_runs: Vec<GitHubWorkflowRunResponse>,
}

pub struct GitHubApiAdapter {
    client: Client,
    base_url: String,
    github_token: String,
}

impl GitHubApiAdapter {
    pub fn new(base_url: String, github_token: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            github_token,
        }
    }
}

#[async_trait]
impl GitHubApi for GitHubApiAdapter {
    async fn fetch_repositories(&self, count: u8) -> Result<Vec<Repository>, Error> {
        let url = format!(
            "{}/user/repos?type=owner&sort=pushed&direction=desc&per_page={}",
            self.base_url, count
        );

        let response_items = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.github_token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "gha-dashboard-rust-app")
            .send()
            .await
            .context("Failed to send request to GitHub API for repositories")?
            .error_for_status() // Return an error for HTTP error codes
            .context("GitHub API returned an error for repositories")?
            .json::<Vec<GitHubRepositoryResponse>>()
            .await
            .context("Failed to deserialize GitHub repositories response")?;

        let repositories = response_items
            .into_iter()
            .map(|repo_res| Repository {
                name: repo_res.name,
                owner: repo_res.owner.login,
            })
            .collect();

        Ok(repositories)
    }

    async fn fetch_workflow_runs(
        &self,
        owner: &str,
        repo: &str,
        count: u8,
    ) -> Result<Vec<WorkflowRun>, Error> {
        let url = format!(
            "{}/repos/{}/{}/actions/runs?per_page={}",
            self.base_url, owner, repo, count
        );

        let api_response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.github_token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "gha-dashboard-rust-app")
            .send()
            .await
            .context(format!(
                "Failed to send request to GitHub API for workflow runs of {}/{}",
                owner, repo
            ))?
            .error_for_status() // Return an error for HTTP error codes
            .context(format!(
                "GitHub API returned an error for workflow runs of {}/{}",
                owner, repo
            ))?
            .json::<GitHubWorkflowRunsApiResponse>()
            .await
            .context(format!(
                "Failed to deserialize GitHub workflow runs response for {}/{}",
                owner, repo
            ))?;

        let workflow_runs = api_response
            .workflow_runs
            .into_iter()
            .map(|run_res| {
                let status = if run_res.status == "completed" {
                    run_res.conclusion.unwrap_or_else(|| run_res.status.clone()) // Use status if conclusion is None
                } else {
                    run_res.status.clone()
                };

                // Parse ISO 8601 string to DateTime<Utc>
                let created_at = chrono::DateTime::parse_from_rfc3339(&run_res.created_at)
                    .context(format!("Failed to parse created_at for run {}", run_res.id))?
                    .with_timezone(&chrono::Utc);
                let updated_at = chrono::DateTime::parse_from_rfc3339(&run_res.updated_at)
                    .context(format!("Failed to parse updated_at for run {}", run_res.id))?
                    .with_timezone(&chrono::Utc);

                Ok(WorkflowRun {
                    repository_name: run_res.repository.full_name,
                    id: run_res.id,
                    workflow_name: run_res.name,
                    display_title: run_res.display_title,
                    event: run_res.event,
                    status,
                    created_at,
                    updated_at,
                    html_url: run_res.html_url,
                })
            })
            .collect::<Result<Vec<WorkflowRun>, Error>>()?; // Early return if an error occurs

        Ok(workflow_runs)
    }
}
