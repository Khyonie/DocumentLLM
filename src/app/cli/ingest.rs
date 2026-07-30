use std::path::PathBuf;

use fastembed::EmbeddingModel;
use tokio::runtime::Runtime;

use crate::{
    adapters::arrow,
    database::{self},
    ingest, model,
};

pub fn trigger_ingest(args: &[String]) -> Result<(), String> {
    let (document, mode) = process_args(args)?;

    let model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|e| format!("Failed to initialize model: {e}"))?;

    println!("Chunking document {}", document);
    let embeddings = match mode {
        DocumentMode::Pdf => ingest::process_pdf(&document, model),
        DocumentMode::Markdown => ingest::process_markdown(&document, model),
    }?;
    println!("{} chunks embedded", embeddings.len());

    let runtime = Runtime::new().map_err(|e| format!("Failed to create Tokio runtime: {e}"))?;
    let arrow_data = arrow::convert_to_arrow(embeddings)?;
    runtime
        .block_on(database::create_chunk_table(arrow_data))
        .map_err(|e| format!("Failed to create database table: {e}"))?;

    println!("Document ingested!");

    Ok(())
}

/// Verifies the specified file exists, and gets the specified or inferred document type.
#[allow(clippy::single_match)]
fn process_args(args: &[String]) -> Result<(String, DocumentMode), String> {
    // Verify file exists
    let path = args[0].clone();
    let document_path = PathBuf::from(&path);
    if !document_path.exists() {
        return Err(format!("No such file \"{path}\""));
    }

    if document_path.is_dir() {
        return Err(format!("{path} is a directory"));
    }

    // Get file type
    let mut document_mode: Option<DocumentMode> = None;

    // Read if specified
    let mut arg_iter = args.iter().skip(1);
    while let Some(arg) = arg_iter.next() {
        match arg.to_lowercase().as_str() {
            "--mode" => match arg_iter.next() {
                Some(s) => document_mode = Some(DocumentMode::try_from(s.clone())?),
                None => return Err(String::from("Mode must be specified after --mode flag.")),
            },
            _ => return Err(format!("Unknown flag {arg}")),
        }
    }

    // Otherwise attempt to guess
    let document_mode = match document_mode {
        Some(m) => m,
        None => {
            let extension = document_path.extension()
                .ok_or(String::from("Document has no file extension. Specify document mode with --mode [ pdf, markdown ]."))?
                .to_string_lossy()
                .to_string();

            DocumentMode::try_from(extension)?
        }
    };

    Ok((path, document_mode))
}

pub enum DocumentMode {
    Pdf,
    Markdown,
}

impl TryFrom<String> for DocumentMode {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "pdf" => Ok(DocumentMode::Pdf),
            "markdown" | "md" => Ok(DocumentMode::Markdown),
            _ => Err(format!("No such document type \"{value}\"")),
        }
    }
}
