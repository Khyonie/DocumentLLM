use std::{env, pin::Pin, time::Duration};

use async_stream::try_stream;
use futures_util::{Stream, StreamExt};
use reqwest::Client;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, de::DeserializeOwned};

use crate::llm::message::{ChatMessage, ChatRequest, ChatStreamResponse};

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
pub const MODEL_TEMPERATURE: f32 = 0.1;
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<String, String>> + Send>>;
pub const SYSTEM_PROMPT: &str = include_str!("../../../../prompts/ASSISTANT_SYSTEM_PROMPT.md");

/// Wrapper around ollama which takes an HTTP client and an LLM name.
pub struct OllamaClient {
    client: Client,
    model: String,
    base_url: String,
}

impl OllamaClient {
    pub fn new(model: &str) -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_mins(5))
            .build()
            .map_err(|error| format!("Failed to build Ollama HTTP client: {error}"))?;
        Ok(Self {
            client,
            model: String::from(model),
            base_url: env::var("DOCUMENTLLM_OLLAMA_URL")
                .unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_owned())
                .trim_end_matches('/')
                .to_owned(),
        })
    }

    pub async fn stream_chat(&self, messages: Vec<ChatMessage>) -> Result<ChatStream, String> {
        let request = ChatRequest::new(&self.model, messages, MODEL_TEMPERATURE, None, None);
        self.send_streaming_request(&request).await
    }

    async fn send_streaming_request(&self, request: &ChatRequest) -> Result<ChatStream, String> {
        let response = self.send_request(request).await?;
        let mut bytes = response.bytes_stream();
        let stream = try_stream! {
            let mut buffer = Vec::new();
            let mut has_content = false;

            loop {
                let chunk = bytes.next().await;
                let eof = chunk.is_none();
                if let Some(chunk) = chunk {
                    buffer.extend_from_slice(
                        &chunk.map_err(|error| format!("Failed while reading Ollama response: {error}"))?,
                    );
                } else {
                    // Parse a final JSON record even when it has no trailing newline.
                    buffer.push(b'\n');
                }

                while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = buffer.drain(..=newline).collect();
                    if let Some(response) = Self::decode_stream_line(&line)? {
                        if let Some(message) = response.message && !message.content.is_empty(){
                            has_content |= !message.content.trim().is_empty();
                            yield message.content;
                        }
                        if response.done {
                            if !has_content {
                                eprintln!("Ollama returned no answer text (done_reason: {}).", response.done_reason.as_deref().unwrap_or("not supplied"));
                            }
                            if response.done_reason.as_deref() == Some("length") {
                                Err("The model reached its output limit before completing the response.".to_owned())?;
                            }
                            return;
                        }
                    }
                }
                if eof {
                    Err("The Ollama stream ended before confirming that the response was complete.".to_owned())?;
                }
            }
        };

        Ok(Box::pin(stream))
    }

    pub async fn available_models(&self) -> Result<Vec<String>, String> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|error| format!("Failed to connect to Ollama: {error}"))?
            .error_for_status()
            .map_err(|error| format!("Ollama responded with an error: {error}"))?
            .json::<ModelList>()
            .await
            .map_err(|error| format!("Failed to deserialize Ollama models: {error}"))?;

        Ok(response
            .models
            .into_iter()
            .map(|model| model.name)
            .collect())
    }

    pub async fn send_structured_chat<T>(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f32,
        think: Option<bool>,
    ) -> Result<T, String>
    where
        T: DeserializeOwned + JsonSchema,
    {
        let request = ChatRequest::new(
            &self.model,
            messages,
            temperature,
            think,
            Some(schema_for!(T)),
        );

        // Schema-constrained JSON must be complete before it can be deserialized.
        let mut fragments = self.send_streaming_request(&request).await?;
        let mut content = String::new();
        while let Some(fragment) = fragments.next().await {
            content.push_str(&fragment?);
        }

        serde_json::from_str(content.trim())
            .map_err(|e| format!("Failed to deserialize structured LLM response: {e}"))
    }

    async fn send_request(&self, request: &ChatRequest) -> Result<reqwest::Response, String> {
        self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(request)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Ollama: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Ollama responded with an error: {e}"))
    }

    fn decode_stream_line(line: &[u8]) -> Result<Option<ChatStreamResponse>, String> {
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            return Ok(None);
        }

        let response: ChatStreamResponse = serde_json::from_slice(line)
            .map_err(|e| format!("Failed to deserialize streamed response: {e}"))?;
        if let Some(error) = &response.error {
            return Err(format!("Ollama responded with an error: {error}"));
        }
        Ok(Some(response))
    }
}

#[derive(Deserialize)]
struct ModelList {
    models: Vec<ModelDetails>,
}

#[derive(Deserialize)]
struct ModelDetails {
    name: String,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::{Value, json};
    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
        task::JoinHandle,
    };

    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct UtilityResponse {
        queries: Vec<String>,
    }

    pub(crate) async fn mock_ollama(bodies: Vec<String>) -> (OllamaClient, JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for body in bodies {
                let (socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
                let mut socket = BufReader::new(socket);
                let mut line = String::new();
                socket.read_line(&mut line).await.unwrap();
                assert_eq!(line, "POST /api/chat HTTP/1.1\r\n");
                let mut content_length = None;
                loop {
                    line.clear();
                    assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        content_length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
                let mut request = vec![0; content_length.unwrap()];
                socket.read_exact(&mut request).await.unwrap();
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(headers.as_bytes()).await.unwrap();
                socket.write_all(body.as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
                requests.push(serde_json::from_slice(&request).unwrap());
            }
            requests
        });
        let client = OllamaClient {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            model: "test-model".to_owned(),
            base_url,
        };
        (client, server)
    }

    #[tokio::test]
    async fn structured_chat_collects_streamed_json_and_sends_schema() {
        let first = json!({"message": {"role": "assistant", "content": " {\"queries\":["}});
        let last = json!({"message": {"role": "assistant", "content": "\"PDF\",\"Markdown\"]} "}, "done": true});
        let (client, server) = mock_ollama(vec![format!("{first}\r\n\n{last}")]).await;
        let result = client
            .send_structured_chat::<UtilityResponse>(vec![], 0.2, Some(false))
            .await;
        let requests = server.await.unwrap();
        let request = &requests[0];
        assert_eq!(result.unwrap().queries, vec!["PDF", "Markdown"]);
        assert_eq!(request["stream"], true);
        assert_eq!(request["think"], false);
        assert_eq!(request["model"], "test-model");
        assert_eq!(
            request["format"],
            serde_json::to_value(schema_for!(UtilityResponse)).unwrap()
        );
    }

    #[tokio::test]
    async fn structured_chat_propagates_stream_errors() {
        let first = json!({"message": {"role": "assistant", "content": "{\"queries\":["}});
        let (client, server) =
            mock_ollama(vec![format!("{first}\n{{\"error\":\"model failed\"}}\n")]).await;
        let result = client
            .send_structured_chat::<UtilityResponse>(vec![], 0.1, Some(false))
            .await;
        server.await.unwrap();
        assert_eq!(
            result.unwrap_err(),
            "Ollama responded with an error: model failed"
        );
    }

    #[tokio::test]
    async fn structured_chat_rejects_valid_json_without_completion() {
        let body =
            json!({"message": {"role": "assistant", "content": "{\"queries\":[]}"}, "done": false});
        let (client, server) = mock_ollama(vec![body.to_string()]).await;
        let result = client
            .send_structured_chat::<UtilityResponse>(vec![], 0.1, Some(false))
            .await;
        server.await.unwrap();
        assert!(result.unwrap_err().contains("before confirming"));
    }

    #[tokio::test]
    async fn reports_output_limit_after_partial_content() {
        let body = json!({"message": {"role": "assistant", "content": "Partial"}, "done": true, "done_reason": "length"});
        let (client, server) = mock_ollama(vec![body.to_string()]).await;
        let mut stream = client.stream_chat(vec![]).await.unwrap();
        assert_eq!(stream.next().await.unwrap().unwrap(), "Partial");
        assert!(
            stream
                .next()
                .await
                .unwrap()
                .unwrap_err()
                .contains("output limit")
        );
        assert!(stream.next().await.is_none());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn stops_at_completion_marker() {
        let body = json!({"message": {"role": "assistant", "content": "Answer"}, "done": true});
        let (client, server) = mock_ollama(vec![format!("{body}\nnot another response\n")]).await;
        let result: Vec<_> = client.stream_chat(vec![]).await.unwrap().collect().await;
        server.await.unwrap();
        assert_eq!(result, vec![Ok("Answer".to_owned())]);
    }
}
