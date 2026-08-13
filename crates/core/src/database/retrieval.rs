use anyhow::{Context, Result};
use arrow_array::{Float32Array, RecordBatch, StringArray, UInt32Array};
use futures_util::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{DistanceType, Table};

#[derive(Debug)]
pub struct SearchHit {
    pub id: String,
    pub source: String,
    pub chunk_index: u32,
    pub content: String,
    pub distance: f32,
}

pub async fn search_chunks(
    table: &Table,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let batches: Vec<RecordBatch> = table
        .query()
        .nearest_to(query_embedding)? // Get the passage nearest to the query
        .column("embedding") // Check the embedding database column
        .distance_type(DistanceType::Cosine) // What distance method to use
        .limit(limit) // How many results to return
        .execute()
        .await?
        .try_collect()
        .await?;

    let mut hits = Vec::new();

    for batch in batches {
        let ids = batch
            .column_by_name("id")
            .context("missing id column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Id column has unexpected type")?;

        let sources = batch
            .column_by_name("source")
            .context("missing source column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Source column has unexpected type")?;

        let indices = batch
            .column_by_name("index")
            .context("Missing chunk_index column")?
            .as_any()
            .downcast_ref::<UInt32Array>()
            .context("Chunk_index column has unexpected type")?;

        let contents = batch
            .column_by_name("content")
            .context("Missing content column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Content column has unexpected type")?;

        let distances = batch
            .column_by_name("_distance")
            .context("Missing _distance column")?
            .as_any()
            .downcast_ref::<Float32Array>()
            .context("_distance column has unexpected type")?;

        for row in 0..batch.num_rows() {
            hits.push(SearchHit {
                id: ids.value(row).to_owned(),
                source: sources.value(row).to_owned(),
                chunk_index: indices.value(row),
                content: contents.value(row).to_owned(),
                distance: distances.value(row),
            });
        }
    }

    Ok(hits)
}
