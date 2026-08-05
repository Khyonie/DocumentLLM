use std::{collections::HashMap, fs::File, hash::{DefaultHasher, Hash, Hasher}, time::{Duration, SystemTime}};

use ::csv::Reader;
use fastembed::EmbeddingModel;
use tokio::runtime::Runtime;

use crate::{adapters::{arrow, csv::{StackOverflowAnswer, StackOverflowQuestion, StackOverflowQuestionAnswer}}, database, ingest::chunking::DocumentChunk, model};

const NUMBER_OF_QUESTIONS: usize = 15000;

pub fn trigger_stackoverflow() -> Result<(), String>
{
    let (questions, answers) = read_qa_files()?;
    
    // Embed and insert
    let question_answers = connect_answers_to_questions(questions, answers)?;
    let embeddings = embed_questions(question_answers)?;

    let runtime = Runtime::new().map_err(|e| format!("Failed to create Tokio runtime: {e}"))?;
    let arrow_data = arrow::convert_to_arrow(embeddings)?;
    runtime
        .block_on(database::create_chunk_table(arrow_data))
        .map_err(|e| format!("Failed to create database table: {e}"))?;

    println!("Oh my god its done");
    Ok(())
}

fn read_qa_files() -> Result<(Vec<StackOverflowQuestion>, Vec<StackOverflowAnswer>), String>
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
    return Ok((questions, answers))
}

fn connect_answers_to_questions(questions: Vec<StackOverflowQuestion>, answers: Vec<StackOverflowAnswer>) -> Result<Vec<StackOverflowQuestionAnswer>, String>
{
    println!("Sorting answers");
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

    Ok(complete_question_answers)
}

fn embed_questions(question_answers: Vec<StackOverflowQuestionAnswer>) -> Result<Vec<DocumentChunk>, String>
{
    let mut model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|e| format!("Failed to initialize model: {e}"))?;

    let mut embeddings = Vec::new();

    // Time display
    let mut time = SystemTime::now();
    let mut count = 0;
    let mut seconds_remaining = 100000;

    for (i, qa) in question_answers[0..NUMBER_OF_QUESTIONS].iter().enumerate()
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

    Ok(embeddings)
}
