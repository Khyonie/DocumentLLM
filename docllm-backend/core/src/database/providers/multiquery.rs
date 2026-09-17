use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use fastembed::EmbeddingModel;
use lancedb::Table;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    database::{
        self,
        providers::RagRetrievalProvider,
        retrieval::{SearchHit, search_by_embedding},
    },
    interpolate_str,
    llm::{
        message::{ChatMessage, RoleType},
        ollama::OllamaClient,
    },
    model,
};

pub struct MultiQueryRagProvider {
    client: OllamaClient,
    reformulated_questions: usize,
    search_limit: usize,
}

impl MultiQueryRagProvider {
    pub fn new(client: OllamaClient, reformulated_questions: usize, search_limit: usize) -> Self {
        Self {
            client,
            reformulated_questions,
            search_limit,
        }
    }
}

impl RagRetrievalProvider for MultiQueryRagProvider {
    async fn retrieve(
        &self,
        table: &Table,
        query: &str,
        report: &(dyn Fn(&'static str) + Send + Sync),
    ) -> Result<Vec<SearchHit>> {
        report("Reformulating query...");
        let mut questions = reformulate_question(&self.client, query, self.reformulated_questions)
            .await?
            .variants;
        questions.push(query.to_owned());

        report("Searching documents and deduplicating...");
        tokio::task::yield_now().await;
        let hits = query_multiple(table, &questions, self.search_limit).await?;

        report("Reranking...");
        let reranked = match rerank(&self.client, query, hits).await {
            Ok(hits) => hits,
            Err(error) => {
                eprintln!("Reranking failed; falling back to vector search order: {error}");
                report("Retrying document search...");
                tokio::task::yield_now().await;
                query_multiple(table, &questions, self.search_limit).await?
            }
        };

        Ok(reranked.into_iter().take(self.search_limit).collect())
    }
}

#[derive(Deserialize, JsonSchema)]
struct ReformulateResponse {
    variants: Vec<String>,
}

const REFORMULTE_PROMPT: &str = include_str!("../../../../../prompts/REFORMULATE_SYSTEM_PROMPT.md");

async fn reformulate_question(
    client: &OllamaClient,
    question: &str,
    question_count: usize,
) -> Result<ReformulateResponse> {
    let prompt = interpolate_str!(
        REFORMULTE_PROMPT,
        question = question,
        count = question_count
    );

    let message = ChatMessage::new(RoleType::System, prompt);
    client
        .send_structured_chat(vec![message], 1.5, Some(false))
        .await
        .map_err(|error| anyhow!(error))
}

async fn query_multiple(
    table: &Table,
    questions: &[String],
    search_limit: usize,
) -> Result<Vec<SearchHit>> {
    let mut model = model::init_model(EmbeddingModel::AllMiniLML6V2)?;
    let mut hits = HashMap::new();

    for question in questions {
        let embedding = database::embed_query(question, &mut model)?
            .ok_or_else(|| anyhow!("Query model returned no embedding for {question:?}"))?;
        let initial_hits = search_by_embedding(table, &embedding, search_limit).await?;

        for hit in initial_hits {
            keep_closest_hit(&mut hits, hit);
        }
    }

    Ok(hits.into_values().collect())
}

fn keep_closest_hit(hits: &mut HashMap<String, SearchHit>, hit: SearchHit) {
    match hits.get(&hit.id) {
        Some(existing) if existing.distance <= hit.distance => {}
        _ => {
            hits.insert(hit.id.clone(), hit);
        }
    }
}

const RERANK_PROMPT: &str = include_str!("../../../../../prompts/RERANK_SYSTEM_PROMPT.md");

#[derive(Deserialize, JsonSchema)]
struct RerankResponse {
    results: Vec<RerankEntry>,
}

#[derive(Deserialize, JsonSchema)]
struct RerankEntry {
    id: String,
    relevance: usize,
}

async fn rerank(
    client: &OllamaClient,
    question: &str,
    hits: Vec<SearchHit>,
) -> Result<Vec<SearchHit>> {
    let mut prompt = String::from(RERANK_PROMPT);

    prompt.push_str(&format!(
        "<USER QUESTION>\n{question}\n</USER QUESTION>\n\n"
    ));

    for hit in &hits {
        prompt.push_str(&format!(
            "<chunk id=\"{}\">\n{}\n</chunk>\n",
            hit.id, hit.content
        ));
    }

    let message = ChatMessage::new(RoleType::System, prompt);
    let reranked: RerankResponse = client
        .send_structured_chat(vec![message], 0.1, Some(false))
        .await
        .map_err(|error| anyhow!(error))?;

    let mut hits: HashMap<_, _> = hits.into_iter().map(|hit| (hit.id.clone(), hit)).collect();

    let mut seen_ids = HashSet::new();
    let mut ranked_hits = Vec::new();

    for result in reranked.results {
        if !seen_ids.insert(result.id.clone()) {
            continue;
        }

        let hit = hits.remove(&result.id).ok_or_else(|| {
            anyhow!(
                "Reranker returned nonexistent ID: {} with relevance {}",
                result.id,
                result.relevance
            )
        })?;

        ranked_hits.push(hit);
    }

    Ok(ranked_hits)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::database::{providers::multiquery::keep_closest_hit, retrieval::SearchHit};

    #[test]
    fn keeps_closest_duplicate_hit() {
        let mut hits = HashMap::new();

        keep_closest_hit(&mut hits, search_hit("same-id", 0.4));
        keep_closest_hit(&mut hits, search_hit("same-id", 0.2));
        keep_closest_hit(&mut hits, search_hit("same-id", 0.8));

        assert_eq!(hits.len(), 1);
        assert_eq!(hits["same-id"].distance, 0.2);
    }

    fn search_hit(id: &str, distance: f32) -> SearchHit {
        SearchHit {
            id: id.to_owned(),
            source: "document.md".to_owned(),
            chunk_index: 0,
            content: "content".to_owned(),
            distance,
        }
    }
}
