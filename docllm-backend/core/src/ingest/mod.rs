use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use fastembed::{EmbeddingModel, TextEmbedding};

use crate::{
    adapters::{arrow, pdf},
    database,
    ingest::chunking::DocumentChunk,
    model,
};

pub mod chunking;

const CHUNK_SIZE: usize = 300;

#[derive(Clone, Copy)]
pub enum DocumentMode {
    Pdf,
    Markdown,
}

pub struct DocumentInput {
    pub path: PathBuf,
    pub source_label: String,
    pub mode: DocumentMode,
}

impl DocumentInput {
    pub fn new(path: impl Into<PathBuf>, mode: DocumentMode) -> Self {
        let path = path.into();
        let source_label = path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .unwrap_or("Unknown document")
            .to_owned();

        Self {
            path,
            source_label,
            mode,
        }
    }

    pub fn with_source_label(
        path: impl Into<PathBuf>,
        source_label: impl Into<String>,
        mode: DocumentMode,
    ) -> Self {
        Self {
            path: path.into(),
            source_label: source_label.into(),
            mode,
        }
    }
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
    let documents = [DocumentInput::new(path, mode)];
    replace_documents_index(&documents).await
}

/// Embeds one or more PDF/Markdown files and replaces the current document index.
pub async fn replace_documents_index(documents: &[DocumentInput]) -> Result<usize, String> {
    if documents.is_empty() {
        return Err(String::from("At least one document is required"));
    }

    let mut embedding_model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|error| format!("Failed to initialize embedding model: {error}"))?;
    let mut chunks = Vec::new();

    for document in documents {
        chunks.extend(process_document(document, &mut embedding_model)?);
    }

    let chunk_count = chunks.len();
    let arrow_data = arrow::convert_to_arrow(chunks)?;
    database::create_chunk_table(arrow_data)
        .await
        .map_err(|error| format!("Failed to replace document index: {error}"))?;

    Ok(chunk_count)
}

fn process_document(
    document: &DocumentInput,
    embedding_model: &mut TextEmbedding,
) -> Result<Vec<DocumentChunk>, String> {
    let path = document.path.to_str().ok_or_else(|| {
        format!(
            "Document path is not valid UTF-8: {}",
            document.path.display()
        )
    })?;

    match document.mode {
        DocumentMode::Pdf => process_pdf(path, &document.source_label, embedding_model),
        DocumentMode::Markdown => process_markdown(path, &document.source_label, embedding_model),
    }
}

/// Reads a PDF, turns it into a markdown intermediate, then chunks it.
pub fn process_pdf(
    path: &str,
    source_label: &str,
    model: &mut TextEmbedding,
) -> Result<Vec<DocumentChunk>, String> {
    let markdown = pdf::read_pdf_to_markdown(path).map_err(|e| e.to_string())?;

    chunking::chunk_markdown(model, source_label, markdown, CHUNK_SIZE)
        .map_err(|e| format!("Failed to embed chunks: {e}"))
}

pub fn process_markdown(
    path: &str,
    source_label: &str,
    model: &mut TextEmbedding,
) -> Result<Vec<DocumentChunk>, String> {
    let markdown = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read markdown document at {path}: {e}"))?;

    chunking::chunk_markdown(model, source_label, markdown, CHUNK_SIZE)
        .map_err(|e| format!("Failed to embed chunks: {e}"))
}
