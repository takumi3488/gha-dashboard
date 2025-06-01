use crate::domain::models::run::WorkflowRun;
use anyhow::Error;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub owner: String,
}

#[async_trait]
pub trait GitHubApi {
    async fn fetch_repositories(&self, count: u8) -> Result<Vec<Repository>, Error>;
    async fn fetch_workflow_runs(
        &self,
        owner: &str,
        repo: &str,
        count: u8,
    ) -> Result<Vec<WorkflowRun>, Error>;
}
