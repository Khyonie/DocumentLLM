use std::{
    hash::{DefaultHasher, Hash, Hasher},
    time::{Duration, SystemTime},
};

use fastembed::EmbeddingModel;

use super::model::QuestionAnswer;
use crate::{ingest::chunking::DocumentChunk, model};

const NUMBER_OF_QUESTIONS: usize = 1000;
const MINUTE: usize = 60;
const HOUR: usize = 60 * MINUTE;

pub(super) fn embed_question_answers(
    question_answers: &[QuestionAnswer],
) -> Result<Vec<DocumentChunk>, String> {
    let mut model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|e| format!("Failed to initialize model: {e}"))?;
    let mut embeddings = Vec::new();

    let mut time = SystemTime::now();
    let mut count = 0;
    let mut seconds_remaining = 100000;

    for (index, question_answer) in question_answers[0..NUMBER_OF_QUESTIONS].iter().enumerate() {
        if SystemTime::now().duration_since(time).unwrap() >= Duration::from_secs(1) {
            time = SystemTime::now();

            let remaining = NUMBER_OF_QUESTIONS - index;
            seconds_remaining = remaining / count;
            count = 0;
        }

        print_progress(index, seconds_remaining, question_answer);

        let mut hasher = DefaultHasher::new();
        format!(
            "{} @ {}",
            question_answer.question.title, question_answer.id
        )
        .hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish());

        let Ok(content) = question_answer.to_markdown() else {
            continue;
        };

        let embedding = model
            .embed(vec![&content], None)
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or("Failed to embed chunk")?;

        embeddings.push(DocumentChunk {
            id,
            source: question_answer.question.title.clone(),
            index: question_answer.question.id as u32,
            content,
            embedding,
        });
        count += 1;
    }

    Ok(embeddings)
}

fn print_progress(index: usize, seconds_remaining: usize, question_answer: &QuestionAnswer) {
    let (hours, minutes, seconds) = seconds_to_time_format(seconds_remaining);
    println!(
        "Embedding {index}/{} ({:.2}%) \"{}\"",
        NUMBER_OF_QUESTIONS,
        (index as f32 / NUMBER_OF_QUESTIONS as f32) * 100.0,
        question_answer.question.title
    );
    println!("- Time remaining: {hours}h, {minutes}m, {seconds}s");
}

fn seconds_to_time_format(seconds: usize) -> (usize, usize, usize) {
    let hours = seconds / HOUR;
    let remaining = seconds % HOUR;
    let minutes = remaining / MINUTE;
    let seconds = remaining % MINUTE;
    (hours, minutes, seconds)
}
