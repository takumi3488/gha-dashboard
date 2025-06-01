use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRun {
    #[serde(rename = "repositoryName")]
    pub repository_name: String,
    pub id: u64,
    #[serde(rename = "workflowName")]
    pub workflow_name: String,
    #[serde(rename = "displayTitle")]
    pub display_title: String,
    pub event: String,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "htmlUrl")]
    pub html_url: String,
}
