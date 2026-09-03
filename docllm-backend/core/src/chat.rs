use std::collections::BTreeSet;

use async_stream::try_stream;
use futures_util::StreamExt;

use crate::{
    database::{
        self,
        providers::{RetrievalType, retrieve},
    },
    llm::{
        message::{ChatMessage, RoleType},
        ollama::{ChatStream, OllamaClient, SYSTEM_PROMPT},
    },
};

const RESULT_LIMIT: usize = 3;

#[derive(Default)]
pub struct ChatService;

impl ChatService {
    pub async fn answer(
        &self,
        model: &str,
        rag_provider: RetrievalType,
        query: &str,
    ) -> Result<String, String> {
        let context = self.context_for_query(model, rag_provider, query).await?;
        let mut answer = OllamaClient::new(model)?.chat(context.messages).await?;
        append_sources_if_supported(&mut answer, &context.sources_section);

        Ok(answer)
    }

    pub async fn stream_answer(
        &self,
        model: &str,
        rag_provider: RetrievalType,
        query: &str,
    ) -> Result<ChatStream, String> {
        let context = self.context_for_query(model, rag_provider, query).await?;
        let mut answer = OllamaClient::new(model)?
            .stream_chat(context.messages)
            .await?;
        let sources_section = context.sources_section;
        let stream = try_stream! {
            let mut answer_text = String::new();

            while let Some(fragment) = answer.next().await {
                let fragment = fragment?;
                answer_text.push_str(&fragment);
                yield fragment;
            }

            if should_include_sources(&answer_text) {
                yield sources_section;
            }
        };

        Ok(Box::pin(stream))
    }

    pub async fn available_models(&self) -> Result<Vec<String>, String> {
        OllamaClient::new("")?.available_models().await
    }

    async fn context_for_query(
        &self,
        model: &str,
        rag_provider: RetrievalType,
        query: &str,
    ) -> Result<ChatContext, String> {
        let table = database::open_database()
            .await
            .map_err(|error| format!("Failed to open database table: {error}"))?;

        let sources = retrieve(rag_provider, &table, query, model, RESULT_LIMIT)
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

        Ok(ChatContext {
            messages: vec![
                ChatMessage::new(RoleType::System, SYSTEM_PROMPT.to_owned()),
                ChatMessage::new(RoleType::User, prompt),
            ],
            sources_section: format_sources_section(source_labels),
        })
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

fn append_sources_if_supported(answer: &mut String, sources_section: &str) {
    if should_include_sources(answer) {
        answer.push_str(sources_section);
    }
}

fn should_include_sources(answer: &str) -> bool {
    let normalized = answer.to_ascii_lowercase();
    let cannot_answer_phrases = [
        "available documents cannot",
        "cannot sufficiently answer",
        "can't sufficiently answer",
        "do not contain enough",
        "don't contain enough",
        "does not contain enough",
        "not enough information",
        "insufficient information",
        "cannot answer",
        "can't answer",
        "unable to answer",
        "not provided",
        "not covered",
    ];

    !cannot_answer_phrases
        .iter()
        .any(|phrase| normalized.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::should_include_sources;

    #[test]
    fn includes_sources_for_supported_answer() {
        assert!(should_include_sources(
            "The project supports PDF and Markdown ingestion."
        ));
    }

    #[test]
    fn skips_sources_for_insufficient_answer() {
        assert!(!should_include_sources(
            "The available documents cannot sufficiently answer this question."
        ));
    }
}
