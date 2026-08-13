use std::sync::Mutex;

use fastembed::{EmbeddingModel, TextEmbedding};

use crate::{
    database::{self, retrieval::search_chunks},
    llm::{
        message::{ChatMessage, RoleType},
        ollama::{ChatStream, OllamaClient, SYSTEM_PROMPT},
    },
    model,
};

const RESULT_LIMIT: usize = 3;

pub struct ChatService {
    embedding_model: Mutex<TextEmbedding>,
}

impl ChatService {
    pub fn new() -> Result<Self, String> {
        let embedding_model = model::init_model(EmbeddingModel::AllMiniLML6V2)
            .map_err(|error| format!("Failed to initialize embedding model: {error}"))?;

        Ok(Self {
            embedding_model: Mutex::new(embedding_model),
        })
    }

    pub async fn answer(&self, model: &str, query: &str) -> Result<String, String> {
        let messages = self.messages_for_query(query).await?;
        OllamaClient::new(model)?.chat(messages).await
    }

    pub async fn stream_answer(&self, model: &str, query: &str) -> Result<ChatStream, String> {
        let messages = self.messages_for_query(query).await?;
        OllamaClient::new(model)?.stream_chat(messages).await
    }

    pub async fn available_models(&self) -> Result<Vec<String>, String> {
        OllamaClient::new("")?.available_models().await
    }

    async fn messages_for_query(&self, query: &str) -> Result<Vec<ChatMessage>, String> {
        let query_embedding = {
            let mut embedding_model = self
                .embedding_model
                .lock()
                .map_err(|_| String::from("Embedding model lock was poisoned"))?;
            embedding_model
                .embed(vec![query], None)
                .map_err(|error| format!("Failed to embed query: {error}"))?
                .into_iter()
                .next()
                .ok_or_else(|| String::from("Embedding model returned no query embedding"))?
        };

        let table = database::open_database()
            .await
            .map_err(|error| format!("Failed to open database table: {error}"))?;
        let sources = search_chunks(&table, &query_embedding, RESULT_LIMIT)
            .await
            .map_err(|error| format!("Failed to retrieve sources: {error}"))?;

        let mut prompt = String::from("<documentation>\n");
        for source in sources {
            prompt.push_str("<source>\n");
            prompt.push_str(&source.content);
            prompt.push_str("\n</source>\n");
        }
        prompt.push_str("</documentation>\n");
        prompt.push_str(&format!("<question>\n{query}\n</question>\n\n"));
        prompt.push_str(
            "Answer the question using the documentation above. Cite relevant sources with their document name.",
        );

        Ok(vec![
            ChatMessage::new(RoleType::System, SYSTEM_PROMPT.to_owned()),
            ChatMessage::new(RoleType::User, prompt),
        ])
    }
}
