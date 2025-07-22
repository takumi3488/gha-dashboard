use crate::domain::external_apis::github::{GitHubApi, Repository};
use crate::domain::models::run::WorkflowRun;
use anyhow::{Context, Error};
use async_trait::async_trait;
use reqwest::{Client, Response};
use serde::Deserialize;
use std::future::Future;
use tokio::time::{Duration, sleep};

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

    async fn execute_with_retry<T, F, Fut>(
        &self,
        operation_name: &str,
        request_fn: F,
    ) -> Result<T, Error>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<Response, reqwest::Error>>,
        T: serde::de::DeserializeOwned,
    {
        const MAX_RETRIES: u32 = 10;
        const INITIAL_WAIT_SECS: f64 = 1.0;
        const BACKOFF_MULTIPLIER: f64 = 1.5;

        let mut retries = 0;
        let mut wait_time = INITIAL_WAIT_SECS;

        loop {
            match request_fn().await {
                Ok(response) => match response.error_for_status() {
                    Ok(response) => match response.json::<T>().await {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            if retries >= MAX_RETRIES {
                                return Err(e).context(format!(
                                    "Failed to deserialize response for {operation_name} after {MAX_RETRIES} retries"
                                ));
                            }
                            tracing::warn!(
                                "Failed to deserialize response for {}, retry {} of {}: {}",
                                operation_name,
                                retries + 1,
                                MAX_RETRIES,
                                e
                            );
                        }
                    },
                    Err(e) => {
                        if retries >= MAX_RETRIES {
                            return Err(e).context(format!(
                                "API returned an error for {operation_name} after {MAX_RETRIES} retries"
                            ));
                        }
                        tracing::warn!(
                            "API error for {}, retry {} of {}: {}",
                            operation_name,
                            retries + 1,
                            MAX_RETRIES,
                            e
                        );
                    }
                },
                Err(e) => {
                    if retries >= MAX_RETRIES {
                        return Err(e).context(format!(
                            "Failed to send request for {operation_name} after {MAX_RETRIES} retries"
                        ));
                    }
                    tracing::warn!(
                        "Request failed for {}, retry {} of {}: {}",
                        operation_name,
                        retries + 1,
                        MAX_RETRIES,
                        e
                    );
                }
            }

            retries += 1;
            sleep(Duration::from_secs_f64(wait_time)).await;
            wait_time *= BACKOFF_MULTIPLIER;
        }
    }
}

#[async_trait]
impl GitHubApi for GitHubApiAdapter {
    #[tracing::instrument(name = "GitHubApiAdapter::fetch_repositories", skip(self))]
    async fn fetch_repositories(&self, count: u8) -> Result<Vec<Repository>, Error> {
        let url = format!(
            "{}/user/repos?type=owner&sort=pushed&direction=desc&per_page={}",
            self.base_url, count
        );

        let response_items: Vec<GitHubRepositoryResponse> = self
            .execute_with_retry("fetch_repositories", || {
                self.client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", self.github_token))
                    .header("Accept", "application/vnd.github.v3+json")
                    .header("User-Agent", "gha-dashboard-rust-app")
                    .send()
            })
            .await?;

        let repositories = response_items
            .into_iter()
            .map(|repo_res| Repository {
                name: repo_res.name,
                owner: repo_res.owner.login,
            })
            .collect();

        Ok(repositories)
    }

    #[tracing::instrument(name = "GitHubApiAdapter::fetch_repository", skip(self))]
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

        let api_response: GitHubWorkflowRunsApiResponse = self
            .execute_with_retry(&format!("workflow runs for {owner}/{repo}"), || {
                self.client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", self.github_token))
                    .header("Accept", "application/vnd.github.v3+json")
                    .header("User-Agent", "gha-dashboard-rust-app")
                    .send()
            })
            .await?;

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
