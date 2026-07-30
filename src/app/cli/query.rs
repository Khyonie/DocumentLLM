use fastembed::EmbeddingModel;
use tokio::runtime::Runtime;

use crate::{
    database::{self, retrieval},
    model,
};

/// How many results to return.
pub const RESULT_LIMIT: usize = 3;

pub fn trigger_query(args: &[String]) -> Result<(), String> {
    // Open table
    let runtime = Runtime::new().map_err(|e| format!("Failed to create Tokio runtime: {e}"))?;
    let table = runtime
        .block_on(database::open_database())
        .map_err(|e| format!("Failed to open database table: {e}"))?;

    // Generate query
    let mut model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|e| format!("Failed to initialize model: {e}"))?;

    let query = process_query_from_args(args)?;
    let query_embedding = model
        .embed(vec![query], None)
        .map_err(|e| format!("Failed to embed query: {e}"))?
        .into_iter()
        .next()
        .ok_or(String::from("Model gave no query embedding"))?;

    let search_results = runtime
        .block_on(retrieval::search_chunks(
            &table,
            &query_embedding,
            RESULT_LIMIT,
        ))
        .map_err(|e| format!("Failed to get search results: {e}"))?;

    for result in search_results {
        println!("────────────────────────────── Result ──────────────────────────────");
        println!(
            "File: {} ({}) @ {}",
            result.source, result.id, result.chunk_index
        );
        println!("Distance: {}", result.distance);
        println!("{}", result.content)
    }

    Ok(())
}

fn process_query_from_args(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err(String::from("No query given."));
    }

    Ok(args.join(" "))
}
