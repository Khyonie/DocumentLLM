use anyhow::{Result, anyhow};
use fastembed::EmbeddingModel;
use lancedb::Table;

use crate::{
    database::{
        self,
        providers::RagRetrievalProvider,
        retrieval::{SearchHit, search_by_embedding},
    },
    model,
};

pub struct BasicRagRetriever {
    search_limit: usize,
}

impl BasicRagRetriever {
    pub fn new(search_limit: usize) -> Self {
        Self { search_limit }
    }
}

impl RagRetrievalProvider for BasicRagRetriever {
    async fn retrieve(&self, table: &Table, query: &str) -> Result<Vec<SearchHit>> {
        let mut model = model::init_model(EmbeddingModel::AllMiniLML6V2)?;
        let query_embedding = database::embed_query(query, &mut model)?
            .ok_or_else(|| anyhow!("Query model returned no embedding"))?;

        search_by_embedding(table, &query_embedding, self.search_limit).await
    }
}
