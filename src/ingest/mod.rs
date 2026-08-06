use std::fs;

use fastembed::TextEmbedding;

use crate::{adapters::pdf, ingest::chunking::DocumentChunk};

pub mod chunking;
pub mod stackoverflow;

const CHUNK_SIZE: usize = 300;

/// Reads a PDF, turns it into a markdown intermediate, then chunks it.
pub fn process_pdf(path: &str, model: TextEmbedding) -> Result<Vec<DocumentChunk>, String> {
    let markdown = pdf::read_pdf_to_markdown(path).map_err(|e| e.to_string())?;

    chunking::chunk_markdown(model, path, markdown, CHUNK_SIZE)
        .map_err(|e| format!("Failed to embed chunks: {e}"))
}

pub fn process_markdown(path: &str, model: TextEmbedding) -> Result<Vec<DocumentChunk>, String> {
    let markdown = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read markdown document at {path}: {e}"))?;

    chunking::chunk_markdown(model, path, markdown, CHUNK_SIZE)
        .map_err(|e| format!("Failed to embed chunks: {e}"))
}
