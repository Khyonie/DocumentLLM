use std::{fmt, fs, path::Path};

use fastembed::{EmbeddingModel, TextEmbedding};

use crate::{
    adapters::{arrow, pdf},
    database,
    ingest::chunking::DocumentChunk,
    model,
};

pub mod chunking;
pub mod stackoverflow;

const CHUNK_SIZE: usize = 300;

#[derive(Clone, Copy)]
pub enum DocumentMode {
    Pdf,
    Markdown,
}

impl DocumentMode {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .ok_or_else(|| {
                format!(
                    "Cannot infer document type from {}. Specify --mode.",
                    path.display()
                )
            })?;
        extension.parse()
    }
}

impl std::str::FromStr for DocumentMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "pdf" => Ok(Self::Pdf),
            "markdown" | "md" => Ok(Self::Markdown),
            _ => Err(format!("Unsupported document type \"{value}\"")),
        }
    }
}

impl fmt::Display for DocumentMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pdf => formatter.write_str("PDF"),
            Self::Markdown => formatter.write_str("Markdown"),
        }
    }
}

/// Embeds a PDF or Markdown file and replaces the current document index with its chunks.
pub async fn replace_document_index(path: &Path, mode: DocumentMode) -> Result<usize, String> {
    let path = path
        .to_str()
        .ok_or_else(|| format!("Document path is not valid UTF-8: {}", path.display()))?;
    let embedding_model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|error| format!("Failed to initialize embedding model: {error}"))?;
    let chunks = match mode {
        DocumentMode::Pdf => process_pdf(path, embedding_model),
        DocumentMode::Markdown => process_markdown(path, embedding_model),
    }?;
    let chunk_count = chunks.len();
    let arrow_data = arrow::convert_to_arrow(chunks)?;
    database::create_chunk_table(arrow_data)
        .await
        .map_err(|error| format!("Failed to replace document index: {error}"))?;

    Ok(chunk_count)
}

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
