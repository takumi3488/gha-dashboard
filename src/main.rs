use gha_dashboard::application::use_cases::stream_github_actions_runs::StreamGitHubActionsRunsInteractor;
use gha_dashboard::infrastructures::adapters::primary::web::{AppState, create_router};
use gha_dashboard::infrastructures::adapters::secondary::external_apis::github::GitHubApiAdapter;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    info!("Application starting");

    // GitHub Token の読み込み
    let github_token = env::var("GITHUB_TOKEN")
        .map_err(|e| anyhow::anyhow!("Failed to read GITHUB_TOKEN: {e}"))?;

    // Build dependencies
    let github_api_adapter = Arc::new(GitHubApiAdapter::new(
        "https://api.github.com".to_string(),
        github_token,
    ));
    let stream_use_case = Arc::new(StreamGitHubActionsRunsInteractor::new(github_api_adapter));
    let app_state = Arc::new(AppState {
        use_case: stream_use_case,
    });

    // Create router
    let app = create_router(app_state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?; // Added
    axum::serve(listener, app.into_make_service()).await?; // Modified

    Ok(())
}
