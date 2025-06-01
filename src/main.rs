use gha_dashboard::application::use_cases::stream_github_actions_runs::StreamGitHubActionsRunsInteractor;
use gha_dashboard::infrastructures::adapters::primary::web::{AppState, create_router};
use gha_dashboard::infrastructures::adapters::secondary::external_apis::github::GitHubApiAdapter;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, info_span};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .expect("Failed to create OTLP exporter");
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(otlp_exporter)
        .build();
    let tracer = provider.tracer("gha-dashboard");

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(telemetry)
        .with(fmt_layer)
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let initialize_span = info_span!("initialize");
    let _enter = initialize_span.enter();
    info!("Application starting");

    // GitHub Token の読み込み
    let github_token = env::var("GITHUB_TOKEN")
        .map_err(|e| anyhow::anyhow!("Failed to read GITHUB_TOKEN: {}", e))?;

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
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?; // Added
    axum::serve(listener, app.into_make_service()).await?; // Modified

    Ok(())
}
