use crate::application::use_cases::stream_github_actions_runs::{
    StreamGitHubActionsRunsInteractor, StreamGitHubActionsRunsUseCase,
    StreamGitHubActionsRunsUseCaseInput,
};
use crate::infrastructures::adapters::secondary::external_apis::github::GitHubApiAdapter;
use axum::extract::ws::Utf8Bytes;
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
    routing::get,
};
use futures_util::{Stream, StreamExt};
use std::convert::Infallible;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
// Since GitHubApiAdapter and StreamGitHubActionsRunsInteractor are imported in main.rs,
// only import the use_case necessary for the generic type constraint of AppState here.
// use crate::infrastructures::adapters::secondary::external_apis::github::GitHubApiAdapter;
// use crate::application::use_cases::stream_github_actions_runs::StreamGitHubActionsRunsInteractor;

// Structure to hold application state (AppState)
#[derive(Clone)]
pub struct AppState {
    pub use_case: Arc<StreamGitHubActionsRunsInteractor<GitHubApiAdapter>>,
}

#[axum::debug_handler]
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.use_case.clone()))
}

#[tracing::instrument(name = "handle_socket", skip(socket, use_case))]
async fn handle_socket(
    mut socket: WebSocket,
    use_case: Arc<StreamGitHubActionsRunsInteractor<GitHubApiAdapter>>,
) {
    tracing::info!("Client connected");
    let input = StreamGitHubActionsRunsUseCaseInput {}; // Create input
    let stream = use_case.execute(input); // Add .await
    tokio::pin!(stream);

    loop {
        tokio::select! {
            // Receive data stream from use case
            Some(result) = stream.next() => {
                match result {
                    Ok(output) => {
                        match serde_json::to_string(&output) {
                            Ok(json_string) => {
                                if socket.send(Message::Text(Utf8Bytes::from(json_string))).await.is_err() {
                                    tracing::info!("Client disconnected (failed to send message)");
                                    break; // Break loop on error
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to serialize output: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error from use case stream: {:?}", e);
                        // Consider notifying the client depending on the error content
                        if socket.send(Message::Text(Utf8Bytes::from(format!("Error: {e}")))).await.is_err() {
                            tracing::info!("Client disconnected (failed to send error notification)");
                            break;
                        }
                    }
                }
            },
            // Receive message from client (disconnection detection, etc.)
            Some(Ok(msg)) = socket.recv() => {
                match msg {
                    Message::Close(_) => {
                        tracing::info!("Client disconnected (received close message)");
                        break;
                    }
                    Message::Text(t) => {
                        tracing::debug!("Received text from client: {}", t);
                        // Process message from client (if necessary)
                    }
                    _ => {
                        // Ignore Ping/Pong and Binary messages
                    }
                }
            },
            else => {
                // Stream ended or socket error
                tracing::info!("Client or stream ended");
                break;
            }
        };
    }
    tracing::info!("Client disconnected");
}

#[axum::debug_handler]
pub async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("SSE client connected");
    let use_case = state.use_case.clone();

    let sse_stream = async_stream::stream! {
        let input = StreamGitHubActionsRunsUseCaseInput {};
        let stream = use_case.execute(input);
        tokio::pin!(stream);

        while let Some(result) = stream.next().await {
            let event = match result {
                Ok(output) => match serde_json::to_string(&output) {
                    Ok(json_string) => Event::default().data(json_string),
                    Err(e) => {
                        tracing::error!("Failed to serialize output: {:?}", e);
                        Event::default()
                            .event("error")
                            .data(format!("Serialization error: {e}"))
                    }
                },
                Err(e) => {
                    tracing::error!("Error from use case stream: {:?}", e);
                    Event::default()
                        .event("error")
                        .data(format!("Error: {e}"))
                }
            };
            yield Ok::<_, Infallible>(event);
        }
    };

    Sse::new(sse_stream)
}

#[tracing::instrument(name = "health_check")]
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

pub fn create_router(app_state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(websocket_handler))
        .route("/sse", get(sse_handler))
        .route("/health", get(health_check))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http())
}
