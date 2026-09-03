use std::{collections::BTreeSet, sync::Mutex};

use async_stream::try_stream;
use fastembed::{EmbeddingModel, TextEmbedding};
use futures_util::StreamExt;

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
    embedding_model: Mutex<Option<TextEmbedding>>,
}

impl ChatService {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            embedding_model: Mutex::new(None),
        })
    }

    pub async fn answer(&self, model: &str, query: &str) -> Result<String, String> {
        let context = self.context_for_query(query).await?;
        let mut answer = OllamaClient::new(model)?.chat(context.messages).await?;
        answer.push_str(&context.sources_section);

        Ok(answer)
    }

    pub async fn stream_answer(&self, model: &str, query: &str) -> Result<ChatStream, String> {
        let context = self.context_for_query(query).await?;
        let mut answer = OllamaClient::new(model)?
            .stream_chat(context.messages)
            .await?;
        let sources_section = context.sources_section;
        let stream = try_stream! {
            while let Some(fragment) = answer.next().await {
                yield fragment?;
            }

            yield sources_section;
        };

        Ok(Box::pin(stream))
    }

    pub async fn available_models(&self) -> Result<Vec<String>, String> {
        OllamaClient::new("")?.available_models().await
    }

    async fn context_for_query(&self, query: &str) -> Result<ChatContext, String> {
        let query_embedding = self.query_embedding(query)?;

        let table = database::open_database()
            .await
            .map_err(|error| format!("Failed to open database table: {error}"))?;
        let sources = search_chunks(&table, &query_embedding, RESULT_LIMIT)
            .await
            .map_err(|error| format!("Failed to retrieve sources: {error}"))?;

        let mut source_labels = BTreeSet::new();
        let mut prompt = String::from("<documents>\n");
        for source in sources {
            source_labels.insert(source.source.clone());

            prompt.push_str("<excerpt>\n");
            prompt.push_str(&format!("<document>{}</document>\n", source.source));
            prompt.push_str(&format!("<chunk>{}</chunk>\n", source.chunk_index));
            prompt.push_str("<content>\n");
            prompt.push_str(&source.content);
            prompt.push_str("\n</content>\n");
            prompt.push_str("</excerpt>\n");
        }
        prompt.push_str("</documents>\n");
        prompt.push_str("<available_sources>\n");
        for source_label in &source_labels {
            prompt.push_str("- ");
            prompt.push_str(source_label);
            prompt.push('\n');
        }
        prompt.push_str("</available_sources>\n");
        prompt.push_str(&format!("<question>\n{query}\n</question>\n\n"));
        prompt.push_str("Answer the question using the document excerpts above. Write only the answer body. Do not include citations, footnotes, or a Sources section.");

        println!("{prompt}");

        Ok(ChatContext {
            messages: vec![
                ChatMessage::new(RoleType::System, SYSTEM_PROMPT.to_owned()),
                ChatMessage::new(RoleType::User, prompt),
            ],
            sources_section: format_sources_section(source_labels),
        })
    }

    fn query_embedding(&self, query: &str) -> Result<Vec<f32>, String> {
        let mut embedding_model = self
            .embedding_model
            .lock()
            .map_err(|_| String::from("Embedding model lock was poisoned"))?;

        if embedding_model.is_none() {
            *embedding_model = Some(
                model::init_model(EmbeddingModel::AllMiniLML6V2)
                    .map_err(|error| format!("Failed to initialize embedding model: {error}"))?,
            );
        }

        embedding_model
            .as_mut()
            .ok_or_else(|| String::from("Embedding model was not initialized"))?
            .embed(vec![query], None)
            .map_err(|error| format!("Failed to embed query: {error}"))?
            .into_iter()
            .next()
            .ok_or_else(|| String::from("Embedding model returned no query embedding"))
    }
}

struct ChatContext {
    messages: Vec<ChatMessage>,
    sources_section: String,
}

fn format_sources_section(sources: BTreeSet<String>) -> String {
    if sources.is_empty() {
        return String::new();
    }

    let mut section = String::from("\n\n**Sources**\n");
    for source in sources {
        section.push_str("- ");
        section.push_str(&source);
        section.push('\n');
    }

    section
}
