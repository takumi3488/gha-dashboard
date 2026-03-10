use crate::domain::external_apis::github::GitHubApi;
use crate::domain::models::run::WorkflowRun;
use anyhow::{Context, Error};
use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::Stream;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

/// リポジトリの最大取得数
const MAX_REPOSITORIES_TO_FETCH: u8 = 5;

/// リポジトリが見つからない場合の待機時間（秒）
const RETRY_WAIT_SECONDS: u64 = 60;

/// ワークフローランの取得イテレーション回数
const FETCH_ITERATIONS: usize = 2;

/// 各リポジトリから取得するワークフローランの最大数
const MAX_WORKFLOW_RUNS_PER_REPO: u8 = 2;

/// イテレーション間の待機時間（秒）
const ITERATION_WAIT_SECONDS: u64 = 30;

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
                let repositories = github_api.fetch_repositories(MAX_REPOSITORIES_TO_FETCH).await
                    .context("Failed to fetch repositories")?;
                tracing::info!("Fetched {} repositories", repositories.len());

                if repositories.is_empty() {
                    tracing::warn!("No repositories found, waiting before retrying...");
                    tokio::time::sleep(Duration::from_secs(RETRY_WAIT_SECONDS)).await;
                    continue;
                }

                for i in 0..FETCH_ITERATIONS {
                    tracing::info!("Fetching workflow runs (iteration {}/{})...", i + 1, FETCH_ITERATIONS);
                    let mut all_runs: Vec<WorkflowRun> = Vec::new();

                    for repo in &repositories {
                        tracing::debug!("Fetching runs for {}/{}", repo.owner, repo.name);
                        let runs = github_api.fetch_workflow_runs(&repo.owner, &repo.name, MAX_WORKFLOW_RUNS_PER_REPO).await
                            .with_context(|| format!("Failed to fetch workflow runs for {}/{}", repo.owner, repo.name))?;
                        all_runs.extend(runs);
                    }

                    // sort runs by created_at in descending order
                    all_runs.sort_by_key(|run| run.created_at.timestamp_millis());
                    all_runs.reverse();

                    tracing::info!("Yielding {} workflow runs", all_runs.len());
                    yield StreamGitHubActionsRunsUseCaseOutput { runs: all_runs };

                    tracing::debug!("Waiting for {} seconds...", ITERATION_WAIT_SECONDS);
                    tokio::time::sleep(Duration::from_secs(ITERATION_WAIT_SECONDS)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GitHub APIのレート制限（認証済みリクエストの場合）
    const GITHUB_API_RATE_LIMIT_PER_HOUR: u32 = 5_000;
    const GITHUB_API_RATE_LIMIT_ENTERPRISE_PER_HOUR: u32 = 15_000;

    /// `FETCH_ITERATIONS` を u64 として扱うための定数（u8/u32 範囲内に収まることが保証されている）
    const FETCH_ITERATIONS_U64: u64 = FETCH_ITERATIONS as u64;

    /// 1時間あたりのAPI呼び出し回数を計算する（すべて u64 で計算）
    const fn calc_api_calls_per_hour_u64() -> u64 {
        let api_calls_per_iteration = 1u64 + (MAX_REPOSITORIES_TO_FETCH as u64);
        let api_calls_per_loop = api_calls_per_iteration * FETCH_ITERATIONS_U64;
        let seconds_per_loop = FETCH_ITERATIONS_U64 * ITERATION_WAIT_SECONDS;
        let loops_per_hour = 3600 / seconds_per_loop;
        api_calls_per_loop * loops_per_hour
    }

    #[test]
    fn test_api_calls_do_not_exceed_rate_limit() {
        let max_api_calls_per_hour = calc_api_calls_per_hour_u64();
        let rate_limit = u64::from(GITHUB_API_RATE_LIMIT_PER_HOUR);

        // 標準のレート制限（5,000リクエスト/時間）を超えないことを確認
        assert!(
            max_api_calls_per_hour <= rate_limit,
            "API呼び出し回数（{max_api_calls_per_hour}回/時間）がGitHubのレート制限（{rate_limit}回/時間）を超えています",
        );

        // デバッグ情報を出力
        let api_calls_per_iteration = 1u64 + u64::from(MAX_REPOSITORIES_TO_FETCH);
        let api_calls_per_loop = api_calls_per_iteration * FETCH_ITERATIONS_U64;
        let seconds_per_loop = FETCH_ITERATIONS_U64 * ITERATION_WAIT_SECONDS;
        let loops_per_hour = 3600 / seconds_per_loop;
        println!("=== GitHub APIレート制限チェック ===");
        println!("1イテレーションあたりのAPI呼び出し: {api_calls_per_iteration}回");
        println!("1ループあたりのAPI呼び出し: {api_calls_per_loop}回");
        println!("1ループの所要時間: {seconds_per_loop}秒");
        println!("1時間あたりのループ回数: {loops_per_hour}回");
        println!("1時間あたりの最大API呼び出し: {max_api_calls_per_hour}回");
        println!("GitHubレート制限（標準）: {rate_limit}回/時間");
        let margin = rate_limit - max_api_calls_per_hour;
        let margin_pct = margin * 100 / rate_limit;
        println!("安全マージン: {margin}回/時間（{margin_pct}%）");
    }

    #[test]
    fn test_api_calls_efficiency() {
        // 効率性の確認：レート制限の80%以下の使用率を推奨
        // 80% = 4/5 なので整数演算で計算
        let recommended_max = u64::from(GITHUB_API_RATE_LIMIT_PER_HOUR) * 4 / 5;

        let max_api_calls_per_hour = calc_api_calls_per_hour_u64();

        assert!(
            max_api_calls_per_hour <= recommended_max,
            "API呼び出し回数（{max_api_calls_per_hour}回/時間）が推奨される使用率（レート制限の80%: {recommended_max}回/時間）を超えています",
        );
    }

    #[test]
    fn test_api_calls_calculation_breakdown() {
        // 計算の内訳を詳細に検証
        assert_eq!(
            MAX_REPOSITORIES_TO_FETCH, 5,
            "MAX_REPOSITORIES_TO_FETCHが変更されています"
        );
        assert_eq!(FETCH_ITERATIONS, 2, "FETCH_ITERATIONSが変更されています");
        assert_eq!(
            ITERATION_WAIT_SECONDS, 30,
            "ITERATION_WAIT_SECONDSが変更されています"
        );

        // 期待される値
        let expected_api_calls_per_iteration = 6u64; // 1 + 5
        let expected_api_calls_per_loop = 12u64; // 6 * 2
        let expected_seconds_per_loop = 60u64; // 2 * 30
        let expected_loops_per_hour = 60u64; // 3600 / 60
        let expected_max_api_calls_per_hour = 720u64; // 12 * 60

        // 実際の計算
        let api_calls_per_iteration = 1u64 + u64::from(MAX_REPOSITORIES_TO_FETCH);
        let api_calls_per_loop = api_calls_per_iteration * FETCH_ITERATIONS_U64;
        let seconds_per_loop = FETCH_ITERATIONS_U64 * ITERATION_WAIT_SECONDS;
        let loops_per_hour = 3600 / seconds_per_loop;
        let max_api_calls_per_hour = api_calls_per_loop * loops_per_hour;

        // 検証
        assert_eq!(api_calls_per_iteration, expected_api_calls_per_iteration);
        assert_eq!(api_calls_per_loop, expected_api_calls_per_loop);
        assert_eq!(seconds_per_loop, expected_seconds_per_loop);
        assert_eq!(loops_per_hour, expected_loops_per_hour);
        assert_eq!(max_api_calls_per_hour, expected_max_api_calls_per_hour);

        // 最終確認：720回/時間 << 5,000回/時間
        assert!(max_api_calls_per_hour < u64::from(GITHUB_API_RATE_LIMIT_PER_HOUR));
    }

    #[test]
    fn test_enterprise_rate_limit_compliance() {
        // Enterprise Cloudのレート制限（15,000リクエスト/時間）でも問題ないことを確認
        let max_api_calls_per_hour = calc_api_calls_per_hour_u64();
        let enterprise_limit = u64::from(GITHUB_API_RATE_LIMIT_ENTERPRISE_PER_HOUR);

        assert!(
            max_api_calls_per_hour <= enterprise_limit,
            "API呼び出し回数（{max_api_calls_per_hour}回/時間）がGitHub Enterprise Cloudのレート制限（{enterprise_limit}回/時間）を超えています",
        );
    }
}
