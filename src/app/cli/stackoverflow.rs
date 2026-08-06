use tokio::runtime::Runtime;

use crate::{adapters::arrow, database, ingest::stackoverflow};

pub fn trigger_ingest() -> Result<(), String> {
    let embeddings = stackoverflow::build_chunks()?;

    let runtime = Runtime::new().map_err(|e| format!("Failed to create Tokio runtime: {e}"))?;
    let arrow_data = arrow::convert_to_arrow(embeddings)?;
    runtime
        .block_on(database::create_chunk_table(arrow_data))
        .map_err(|e| format!("Failed to create database table: {e}"))?;

    println!("Stack Overflow ingestion complete");
    Ok(())
}
