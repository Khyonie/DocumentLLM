use std::{env, path::PathBuf, process::ExitCode};

use documentllm_core::ingest::{DocumentMode, replace_document_index};

const USAGE: &str = r#"Usage:
  documentllm-ingest document <path> [--mode <pdf|markdown>]
  documentllm-ingest <path> [--mode <pdf|markdown>]

The "document" command may be omitted when the first argument is a document path."#;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let command = Command::parse(&arguments)?;

    eprintln!(
        "Warning: this testing command replaces the existing document index at ./index/database.lancedb."
    );
    match command {
        Command::Document { path, mode } => {
            println!("Ingesting {} as {mode}...", path.display());
            let chunk_count = replace_document_index(&path, mode).await?;
            println!("Ingestion complete: {chunk_count} chunks indexed.");
        }
    }
    Ok(())
}

enum Command {
    Document { path: PathBuf, mode: DocumentMode },
}

impl Command {
    fn parse(arguments: &[String]) -> Result<Self, String> {
        match arguments.first().map(String::as_str) {
            Some("document") => Self::parse_document(&arguments[1..]),
            Some(_) => Self::parse_document(arguments),
            None => Err(format!("Missing ingestion command.\n{USAGE}")),
        }
    }

    fn parse_document(arguments: &[String]) -> Result<Self, String> {
        let mut path = None;
        let mut mode = None;
        let mut arguments = arguments.iter();

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--mode" => {
                    if mode.is_some() {
                        return Err(format!("--mode may only be specified once.\n{USAGE}"));
                    }
                    let value = arguments
                        .next()
                        .ok_or_else(|| format!("Missing value after --mode.\n{USAGE}"))?;
                    mode = Some(value.parse()?);
                }
                value if value.starts_with('-') => {
                    return Err(format!("Unknown option \"{value}\".\n{USAGE}"));
                }
                value if path.is_none() => path = Some(PathBuf::from(value)),
                value => return Err(format!("Unexpected argument \"{value}\".\n{USAGE}")),
            }
        }

        let path = path.ok_or_else(|| format!("Missing document path.\n{USAGE}"))?;
        if !path.exists() {
            return Err(format!("Document does not exist: {}", path.display()));
        }
        if !path.is_file() {
            return Err(format!("Document is not a file: {}", path.display()));
        }

        let mode = match mode {
            Some(mode) => mode,
            None => DocumentMode::from_path(&path)?,
        };
        Ok(Self::Document { path, mode })
    }
}
