use std::{env, path::PathBuf, process::ExitCode};

use documentllm_core::ingest::{
    DocumentMode, replace_document_index, stackoverflow::replace_index,
};

const DEFAULT_ANSWER_LIMIT: usize = 1000;
const USAGE: &str = r#"Usage:
  documentllm-ingest document <path> [--mode <pdf|markdown>]
  documentllm-ingest stackoverflow <summary-model> [--answers <count>]

The "document" command may be omitted when the first argument is a document path.
Stack Overflow data must be in stackoverflow-qa/Questions.csv and stackoverflow-qa/Answers.csv."#;

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
        Command::StackOverflow {
            summary_model,
            answer_limit,
        } => {
            println!(
                "Ingesting the {answer_limit} highest-scoring Stack Overflow answers using {summary_model} for summaries..."
            );
            let chunk_count = replace_index(&summary_model, answer_limit).await?;
            println!("Stack Overflow ingestion complete: {chunk_count} answers indexed.");
        }
    }
    Ok(())
}

enum Command {
    Document {
        path: PathBuf,
        mode: DocumentMode,
    },
    StackOverflow {
        summary_model: String,
        answer_limit: usize,
    },
}

impl Command {
    fn parse(arguments: &[String]) -> Result<Self, String> {
        match arguments.first().map(String::as_str) {
            Some("document") => Self::parse_document(&arguments[1..]),
            Some("stackoverflow") | Some("ingest-stackoverflow") => {
                Self::parse_stackoverflow(&arguments[1..])
            }
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

    fn parse_stackoverflow(arguments: &[String]) -> Result<Self, String> {
        let mut summary_model = None;
        let mut answer_limit = None;
        let mut arguments = arguments.iter();

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--answers" => {
                    if answer_limit.is_some() {
                        return Err(format!("--answers may only be specified once.\n{USAGE}"));
                    }
                    let value = arguments
                        .next()
                        .ok_or_else(|| format!("Missing value after --answers.\n{USAGE}"))?;
                    let count = value
                        .parse::<usize>()
                        .map_err(|_| format!("Invalid answer count \"{value}\""))?;
                    if count == 0 {
                        return Err(String::from("Answer count must be greater than zero"));
                    }
                    answer_limit = Some(count);
                }
                value if value.starts_with('-') => {
                    return Err(format!("Unknown option \"{value}\".\n{USAGE}"));
                }
                value if summary_model.is_none() => summary_model = Some(value.to_owned()),
                value => return Err(format!("Unexpected argument \"{value}\".\n{USAGE}")),
            }
        }

        Ok(Self::StackOverflow {
            summary_model: summary_model
                .ok_or_else(|| format!("Missing summary model.\n{USAGE}"))?,
            answer_limit: answer_limit.unwrap_or(DEFAULT_ANSWER_LIMIT),
        })
    }
}
