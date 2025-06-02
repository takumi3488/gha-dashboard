use crate::domain::external_apis::github::GitHubApi;
use crate::domain::models::run::WorkflowRun;
use anyhow::{Context, Error};
use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::Stream;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct StreamGitHubActionsRunsUseCaseInput {}

#[derive(Serialize, Debug, Clone)]
pub struct StreamGitHubActionsRunsUseCaseOutput {
    pub runs: Vec<WorkflowRun>,
}

pub trait StreamGitHubActionsRunsUseCase {
    fn execute(
        &self,
        input: StreamGitHubActionsRunsUseCaseInput,
    ) -> impl Stream<Item = Result<StreamGitHubActionsRunsUseCaseOutput, Error>> + Send;
}

pub struct StreamGitHubActionsRunsInteractor<G: GitHubApi + Send + Sync + 'static> {
    github_api: Arc<G>,
}

impl<G: GitHubApi + Send + Sync + 'static> StreamGitHubActionsRunsInteractor<G> {
    pub fn new(github_api: Arc<G>) -> Self {
        Self { github_api }
    }
}

#[async_trait]
impl<G: GitHubApi + Send + Sync + 'static> StreamGitHubActionsRunsUseCase
    for StreamGitHubActionsRunsInteractor<G>
{
    fn execute(
        &self,
        _input: StreamGitHubActionsRunsUseCaseInput,
    ) -> impl Stream<Item = Result<StreamGitHubActionsRunsUseCaseOutput, anyhow::Error>> + Send
    {
        let github_api = self.github_api.clone();

        try_stream! {
            loop {
                tracing::info!("Fetching repositories...");
                let repositories = github_api.fetch_repositories(5).await
                    .context("Failed to fetch repositories")?;
                tracing::info!("Fetched {} repositories", repositories.len());

                if repositories.is_empty() {
                    tracing::warn!("No repositories found, waiting before retrying...");
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }

                for i in 0..2 {
                    tracing::info!("Fetching workflow runs (iteration {}/2)...", i + 1);
                    let mut all_runs: Vec<WorkflowRun> = Vec::new();

                    for repo in &repositories {
                        tracing::debug!("Fetching runs for {}/{}", repo.owner, repo.name);
                        let runs = github_api.fetch_workflow_runs(&repo.owner, &repo.name, 2).await
                            .with_context(|| format!("Failed to fetch workflow runs for {}/{}", repo.owner, repo.name))?;
                        all_runs.extend(runs);
                    }

                    // created_at でソート
                    all_runs.sort_by_key(|run| run.created_at.timestamp_millis());
                    all_runs.reverse(); // 最新のものを先頭に

                    tracing::info!("Yielding {} workflow runs", all_runs.len());
                    yield StreamGitHubActionsRunsUseCaseOutput { runs: all_runs };

                    tracing::debug!("Waiting for 30 seconds...");
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            }
        }
    }
}
