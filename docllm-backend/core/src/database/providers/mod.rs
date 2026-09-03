use std::{env, future::Future, str::FromStr};

use anyhow::Result;
use lancedb::Table;

use crate::{database::retrieval::SearchHit, llm::ollama::OllamaClient};

use self::{basic::BasicRagRetriever, multiquery::MultiQueryRagProvider};

pub mod basic;
pub mod multiquery;

const DEFAULT_REFORMULATED_QUESTIONS: usize = 3;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RetrievalType {
    /// One question is embedded with no further processing.
    #[default]
    Basic,
    /// Initial question is reformulated by the given LLM multiple times, which is then embedded.
    /// Responses are deduped and reranked by an LLM.
    MultiQueryReranked,
}

impl RetrievalType {
    pub fn from_env() -> Self {
        env::var("DOCUMENTLLM_RAG_PROVIDER")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_default()
    }
}

impl FromStr for RetrievalType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "basic" => Ok(Self::Basic),
            "multi-query"
            | "multi_query"
            | "multiquery"
            | "multiquery-reranked"
            | "multiquery_reranked" => Ok(Self::MultiQueryReranked),
            _ => Err(format!("Unsupported RAG provider: {value}")),
        }
    }
}

pub trait RagRetrievalProvider {
    fn retrieve(&self, table: &Table, query: &str) -> impl Future<Output = Result<Vec<SearchHit>>>;
}

pub async fn retrieve(
    provider: RetrievalType,
    table: &Table,
    query: &str,
    chat_model: &str,
    result_limit: usize,
) -> Result<Vec<SearchHit>, String> {
    match provider {
        RetrievalType::Basic => BasicRagRetriever::new(result_limit)
            .retrieve(table, query)
            .await
            .map_err(|error| error.to_string()),
        RetrievalType::MultiQueryReranked => {
            let client = OllamaClient::new(chat_model)?;
            MultiQueryRagProvider::new(client, DEFAULT_REFORMULATED_QUESTIONS, result_limit)
                .retrieve(table, query)
                .await
                .map_err(|error| error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RetrievalType;

    #[test]
    fn parses_basic_provider_name() {
        assert_eq!("basic".parse(), Ok(RetrievalType::Basic));
    }

    #[test]
    fn parses_multiquery_provider_aliases() {
        assert_eq!(
            "multiquery-reranked".parse(),
            Ok(RetrievalType::MultiQueryReranked)
        );
        assert_eq!("multi_query".parse(), Ok(RetrievalType::MultiQueryReranked));
    }
}
