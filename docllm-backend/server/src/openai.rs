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

use documentllm_core::{database::providers::RetrievalType, llm::ollama::ChatStream};

use crate::AppState;

#[derive(Deserialize)]
pub(super) struct ChatCompletionRequest {
    model: String,
    messages: Vec<RequestMessage>,
    #[serde(default)]
    rag_provider: Option<String>,
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
    let rag_provider = request
        .rag_provider
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(RetrievalType::from_env);

    let answer = state
        .chat
        .stream_answer(&request.model, rag_provider, query)
        .await
        .map_err(ApiError::upstream)?;
    Ok(stream_response(answer, id, created, request.model))
}

fn stream_response(mut answer: ChatStream, id: String, created: u64, model: String) -> Response {
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

    Sse::new(events).into_response()
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
    pub status: StatusCode,
    pub message: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use futures_util::stream;
    use serde_json::{Value, json};

    async fn events(fragments: Vec<Result<String, String>>) -> Vec<String> {
        let response = stream_response(
            Box::pin(stream::iter(fragments)),
            "chatcmpl-test".to_owned(),
            123,
            "test-model".to_owned(),
        );
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        String::from_utf8(body.to_vec())
            .unwrap()
            .split("\n\n")
            .filter_map(|event| event.strip_prefix("data: ").map(str::to_owned))
            .collect()
    }

    #[tokio::test]
    async fn streams_content_then_finish_and_done() {
        let events = events(vec![Ok("Hello".to_owned()), Ok(" world".to_owned())]).await;
        assert_eq!(events.len(), 4);
        for (event, content) in events[..2].iter().zip(["Hello", " world"]) {
            let chunk: Value = serde_json::from_str(event).unwrap();
            assert_eq!(chunk["object"], "chat.completion.chunk");
            assert_eq!(chunk["choices"][0]["delta"]["content"], content);
        }
        let finished: Value = serde_json::from_str(&events[2]).unwrap();
        assert_eq!(finished["choices"][0]["finish_reason"], "stop");
        assert_eq!(events[3], "[DONE]");
    }

    #[tokio::test]
    async fn stream_error_ends_response_without_success_chunk() {
        let events = events(vec![
            Ok("Partial".to_owned()),
            Err("Ollama failed".to_owned()),
            Ok("Must not be sent".to_owned()),
        ])
        .await;
        assert_eq!(events.len(), 3);
        assert_eq!(
            serde_json::from_str::<Value>(&events[1]).unwrap(),
            json!({"error": {"message": "Ollama failed"}})
        );
        assert_eq!(events[2], "[DONE]");
    }
}
