use std::{collections::HashMap, fs::File, hash::{DefaultHasher, Hash, Hasher}, path::PathBuf, time::{Duration, SystemTime}};

use csv::Reader;
use fastembed::EmbeddingModel;
use tokio::runtime::Runtime;

use crate::{
    adapters::{arrow, csv::{StackOverflowAnswer, StackOverflowQuestion, StackOverflowQuestionAnswer}},
    database::{self},
    ingest::{self, chunking::DocumentChunk}, model,
};

const NUMBER_OF_QUESTIONS: usize = 15000;

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

pub fn trigger_stackoverflow() -> Result<(), String>
{
    let questions_file = File::open("stackoverflow-qa/Questions.csv")
        .map_err(|e| e.to_string())?;
    let answers_file = File::open("stackoverflow-qa/Answers.csv")
        .map_err(|e| e.to_string())?;

    // Deserialize questions, filtering out errors
    println!("Deserializing questions");
    let questions: Vec<StackOverflowQuestion> = Reader::from_reader(questions_file)
        .into_deserialize()
        .filter_map(| v | v.ok())
        .collect();
    println!("Read {} questions", questions.len());

    println!(r#"Deserializing answers"#);
    let answers: Vec<StackOverflowAnswer> = Reader::from_reader(answers_file)
        .into_deserialize()
        .filter_map(| v | v.ok())
        .collect();
    println!("Read {} answers", answers.len());

    // Collect answers into map by question ID
    let mut answers_by_id: HashMap<usize, Vec<StackOverflowAnswer>> = HashMap::new();
    for a in answers
    {
        let id = a.parent_id;
        if !answers_by_id.contains_key(&id)
        {
            answers_by_id.insert(id, Vec::new());
        }

        answers_by_id.get_mut(&id)
            .expect(&format!("No such answer ID {id} in answers map"))
            .push(a);
    }
    
    println!("Zippering questions and answers together");
    let mut complete_question_answers: Vec<StackOverflowQuestionAnswer> = Vec::new();
    for question in questions
    {
        let id = question.id;
        let question_answers = match answers_by_id.remove(&id) {
            Some(v) => v,
            None => continue
        };

        let qa = StackOverflowQuestionAnswer {
            id: id,
            question,
            answers: question_answers
        };

        complete_question_answers.push(qa);
    }

    // Embed and insert
    let mut model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|e| format!("Failed to initialize model: {e}"))?;

    let mut embeddings = Vec::new();
    let mut time = SystemTime::now();
    let mut count = 0;
    let mut seconds_remaining = 100000;
    for (i, qa) in complete_question_answers[0..NUMBER_OF_QUESTIONS].iter().enumerate()
    {
        // Time delta to calculate time
        if SystemTime::now().duration_since(time).unwrap() >= Duration::from_secs(1)
        {
            time = SystemTime::now();

            let remaining = NUMBER_OF_QUESTIONS - i;
            seconds_remaining = remaining / count;
            count = 0;
        }

        println!("Embedding {i}/{} ({}%) \"{}\"", NUMBER_OF_QUESTIONS, ((i / NUMBER_OF_QUESTIONS) * 100), qa.question.title);
        println!("- Time remaining: {seconds_remaining}s");
        let mut hasher = DefaultHasher::new();
        // Generate chunk ID
        format!("{} @ {}", qa.question.title, qa.id).hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish()); // Hash number as padded hex
        let content = qa.to_markdown();

        // RAG embedding
        let embedding = model
            .embed(vec![&content], None)
            .map_err(|e| format!("{e}"))?
            .into_iter()
            .next()
            .ok_or("Failed to embed chunk")?;

        let chunk = DocumentChunk {
            id,
            source: String::from(&qa.question.title),
            index: qa.question.id as u32,
            content: content,
            embedding,
        };

        embeddings.push(chunk);
        count += 1;
    }

    let runtime = Runtime::new().map_err(|e| format!("Failed to create Tokio runtime: {e}"))?;
    let arrow_data = arrow::convert_to_arrow(embeddings)?;
    runtime
        .block_on(database::create_chunk_table(arrow_data))
        .map_err(|e| format!("Failed to create database table: {e}"))?;

    println!("Oh my god its done");
    Ok(())
}
