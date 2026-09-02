use std::sync::Arc;

use arrow_array::{FixedSizeListArray, RecordBatch, StringArray, UInt32Array, types::Float32Type};
use arrow_schema::{DataType, Field, Schema};

use crate::ingest::chunking::DocumentChunk;

/// Converts DocumentChunks to Apache Arrow format to be used by LanceDb.
pub fn convert_to_arrow(chunks: Vec<DocumentChunk>) -> Result<RecordBatch, String> {
    if chunks.is_empty() {
        return Err(String::from("No document chunks, cannot convert"));
    }

    validate_embeddings(&chunks)?;

    let dimension = chunks[0].embedding.len() as i32;

    // Database schema to use
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("source", DataType::Utf8, false),
        Field::new("index", DataType::UInt32, false),
        Field::new("content", DataType::Utf8, false),
        Field::new(
            "embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimension,
            ),
            false,
        ),
    ]));

    let ids = StringArray::from_iter_values(chunks.iter().map(|chunk| chunk.id.as_str()));

    let sources = StringArray::from_iter_values(chunks.iter().map(|chunk| chunk.source.as_str()));

    let chunk_indices = UInt32Array::from_iter_values(chunks.iter().map(|chunk| chunk.index));

    let contents = StringArray::from_iter_values(chunks.iter().map(|chunk| chunk.content.as_str()));

    let embeddings = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        chunks.iter().map(|chunk| {
            Some(
                chunk
                    .embedding
                    .iter()
                    .copied()
                    .map(Some)
                    .collect::<Vec<_>>(),
            )
        }),
        dimension,
    );

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(ids),
            Arc::new(sources),
            Arc::new(chunk_indices),
            Arc::new(contents),
            Arc::new(embeddings),
        ],
    )
    .map_err(|e| e.to_string())
}

fn validate_embeddings(chunks: &[DocumentChunk]) -> Result<(), String> {
    let dimension = chunks[0].embedding.len();
    if dimension == 0 {
        return Err(String::from("Chunk dimension size is zero"));
    }

    for chunk in chunks {
        if chunk.embedding.len() != dimension {
            return Err(format!(
                "Inconsistent chunk dimension size, expected {dimension}, got {}",
                chunk.embedding.len()
            ));
        }
    }

    Ok(())
}
