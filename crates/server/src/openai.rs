use std::{convert::Infallible, time::SystemTime};

use async_stream::stream;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse, sse::Event},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Deserialize)]
pub(super) struct ChatCompletionRequest {
    model: String,
    messages: Vec<RequestMessage>,
    #[serde(default)]
    stream: bool,
}

#[derive(Deserialize)]
struct RequestMessage {
    role: String,
    content: String,
}

pub(super) async fn chat_completions(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    let query = request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| ApiError::bad_request("A non-empty user message is required"))?;
    let id = completion_id();
    let created = unix_timestamp();

    if request.stream {
        let mut answer = state
            .chat
            .stream_answer(&request.model, query)
            .await
            .map_err(ApiError::upstream)?;
        let model = request.model;
        let events = stream! {
            while let Some(fragment) = answer.next().await {
                match fragment {
                    Ok(content) => {
                        let chunk = StreamChunk::content(&id, created, &model, content);
                        yield Ok::<Event, Infallible>(json_event(&chunk));
                    }
                    Err(error) => {
                        let body = ErrorEnvelope::new(error);
                        yield Ok(json_event(&body));
                        yield Ok(Event::default().data("[DONE]"));
                        return;
                    }
                }
            }

            let chunk = StreamChunk::finished(&id, created, &model);
            yield Ok(json_event(&chunk));
            yield Ok(Event::default().data("[DONE]"));
        };

        return Ok(Sse::new(events).into_response());
    }

    let content = state
        .chat
        .answer(&request.model, query)
        .await
        .map_err(ApiError::upstream)?;
    Ok(Json(CompletionResponse::new(id, created, request.model, content)).into_response())
}

pub(super) async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<ModelList>, ApiError> {
    let models = state
        .chat
        .available_models()
        .await
        .map_err(ApiError::upstream)?
        .into_iter()
        .map(|id| Model {
            id,
            object: "model",
            owned_by: "ollama",
        })
        .collect();

    Ok(Json(ModelList {
        object: "list",
        data: models,
    }))
}

fn json_event(value: &impl Serialize) -> Event {
    Event::default().data(
        serde_json::to_string(value)
            .unwrap_or_else(|_| String::from(r#"{"error":{"message":"Serialization failed"}}"#)),
    )
}

fn completion_id() -> String {
    format!("chatcmpl-{}", unix_timestamp_micros())
}

fn unix_timestamp() -> u64 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .map_or(0, |time| time.as_secs())
}

fn unix_timestamp_micros() -> u128 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .map_or(0, |time| time.as_micros())
}

#[derive(Serialize)]
pub(super) struct ModelList {
    object: &'static str,
    data: Vec<Model>,
}

#[derive(Serialize)]
struct Model {
    id: String,
    object: &'static str,
    owned_by: &'static str,
}

#[derive(Serialize)]
struct CompletionResponse {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<CompletionChoice>,
}

impl CompletionResponse {
    fn new(id: String, created: u64, model: String, content: String) -> Self {
        Self {
            id,
            object: "chat.completion",
            created,
            model,
            choices: vec![CompletionChoice {
                index: 0,
                message: ResponseMessage {
                    role: "assistant",
                    content,
                },
                finish_reason: "stop",
            }],
        }
    }
}

#[derive(Serialize)]
struct CompletionChoice {
    index: u8,
    message: ResponseMessage,
    finish_reason: &'static str,
}

#[derive(Serialize)]
struct ResponseMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct StreamChunk<'a> {
    id: &'a str,
    object: &'static str,
    created: u64,
    model: &'a str,
    choices: Vec<StreamChoice>,
}

impl<'a> StreamChunk<'a> {
    fn content(id: &'a str, created: u64, model: &'a str, content: String) -> Self {
        Self {
            id,
            object: "chat.completion.chunk",
            created,
            model,
            choices: vec![StreamChoice {
                index: 0,
                delta: StreamDelta {
                    content: Some(content),
                },
                finish_reason: None,
            }],
        }
    }

    fn finished(id: &'a str, created: u64, model: &'a str) -> Self {
        Self {
            id,
            object: "chat.completion.chunk",
            created,
            model,
            choices: vec![StreamChoice {
                index: 0,
                delta: StreamDelta { content: None },
                finish_reason: Some("stop"),
            }],
        }
    }
}

#[derive(Serialize)]
struct StreamChoice {
    index: u8,
    delta: StreamDelta,
    finish_reason: Option<&'static str>,
}

#[derive(Serialize)]
struct StreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

pub(super) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn upstream(message: String) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(ErrorEnvelope::new(self.message))).into_response()
    }
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

impl ErrorEnvelope {
    fn new(message: String) -> Self {
        Self {
            error: ErrorBody { message },
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    message: String,
}
