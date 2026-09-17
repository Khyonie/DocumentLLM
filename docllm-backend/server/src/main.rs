use std::{env, net::SocketAddr, sync::Arc};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, post, put},
};
use documentllm_core::chat::ChatService;
use tower_http::services::{ServeDir, ServeFile};

mod openai;
mod rag;

const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:3001";

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
    if env::args().nth(1).as_deref() == Some("--prepare-embeddings") {
        documentllm_core::model::prepare_embedding_model()?;
        println!("Document embedding model is ready.");
        return Ok(());
    }

    let address = env::var("DOCUMENTLLM_BIND_ADDRESS")
        .unwrap_or_else(|_| DEFAULT_BIND_ADDRESS.to_owned())
        .parse::<SocketAddr>()
        .map_err(|error| format!("Invalid DOCUMENTLLM_BIND_ADDRESS: {error}"))?;

    // Frontend
    let frontend = ServeDir::new("docllm-frontend/dist")
        .not_found_service(ServeFile::new("docllm-frontend/dist/index.html"));

    // Backend
    let state = AppState {
        chat: Arc::new(ChatService),
    };
    let router = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(openai::list_models))
        .route("/v1/chat/completions", post(openai::chat_completions))
        .route("/ingest", post(rag::ingest_append))
        .route("/ingest", put(rag::ingest_new))
        .route("/ingest", delete(rag::delete))
        .route(
            "/upload",
            get(rag::list_uploads)
                .post(rag::upload)
                .layer(DefaultBodyLimit::max(rag::MAX_UPLOAD_BYTES)),
        )
        .fallback_service(frontend)
        .with_state(state);

    // Start web server
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
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("Failed to listen for shutdown signal: {error}");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => eprintln!("Failed to listen for SIGTERM: {error}"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => {},
        _ = terminate => {},
    }
}
