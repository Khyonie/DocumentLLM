use std::{env, net::SocketAddr, sync::Arc};

use axum::{
    Router,
    routing::{get, post},
};
use documentllm_core::chat::ChatService;

mod openai;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:3001";

#[derive(Clone)]
struct AppState {
    chat: Arc<ChatService>,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let address = env::var("DOCUMENTLLM_BIND_ADDRESS")
        .unwrap_or_else(|_| DEFAULT_BIND_ADDRESS.to_owned())
        .parse::<SocketAddr>()
        .map_err(|error| format!("Invalid DOCUMENTLLM_BIND_ADDRESS: {error}"))?;
    let state = AppState {
        chat: Arc::new(ChatService::new()?),
    };
    let router = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(openai::list_models))
        .route("/v1/chat/completions", post(openai::chat_completions))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("Failed to bind server to {address}: {error}"))?;

    println!("documentllm-server listening on http://{address}");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| format!("Server failed: {error}"))
}

async fn health() -> &'static str {
    "ok"
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("Failed to listen for shutdown signal: {error}");
    }
}
