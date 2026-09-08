use schemars::Schema;
use serde::{Deserialize, Serialize};

/// A message a user will send to an LLM, along with the system context.
#[derive(Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: RoleType, content: String) -> Self {
        ChatMessage {
            role: role.into(),
            content,
        }
    }
}

/// What role messages will use to give the LLM context
#[derive(Serialize, Deserialize)]
pub enum RoleType {
    System,
    User,
}

impl From<RoleType> for String {
    fn from(value: RoleType) -> Self {
        match value {
            RoleType::User => String::from("user"),
            RoleType::System => String::from("system"),
        }
    }
}

#[derive(Serialize)]
pub struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    options: ChatOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<Schema>,
}

impl ChatRequest {
    pub fn new(
        model: &str,
        messages: Vec<ChatMessage>,
        temperature: f32,
        think: Option<bool>,
        format: Option<Schema>,
    ) -> Self {
        ChatRequest {
            model: String::from(model),
            messages,
            stream: true,
            think,
            options: ChatOptions { temperature },
            format,
        }
    }
}

#[derive(Deserialize)]
pub struct ChatStreamResponse {
    pub message: Option<ChatMessage>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct ChatOptions {
    temperature: f32,
}
