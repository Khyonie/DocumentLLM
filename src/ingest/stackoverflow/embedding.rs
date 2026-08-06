use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    time::Instant,
};

use fastembed::EmbeddingModel;

use super::model::QuestionAnswer;
use crate::{ingest::chunking::DocumentChunk, model};

pub(super) fn embed_question_answers(
    question_answers: &[QuestionAnswer],
    summaries: &HashMap<u64, String>,
) -> Result<Vec<DocumentChunk>, String> {
    let mut model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|e| format!("Failed to initialize model: {e}"))?;
    let mut embeddings = Vec::with_capacity(question_answers.len());
    let started = Instant::now();
    let total = question_answers.len();

    for (index, question_answer) in question_answers.iter().enumerate() {
        print_progress(index, total, started, question_answer);

        let summary = summaries.get(&question_answer.question.id).ok_or_else(|| {
            format!(
                "Missing summary for question {}",
                question_answer.question.id
            )
        })?;
        let content = match question_answer.to_markdown(summary) {
            Ok(content) => content,
            Err(error) => {
                eprintln!(
                    "Skipping answer {} because it could not be converted: {error}",
                    question_answer.answer.id
                );
                continue;
            }
        };

        let mut hasher = DefaultHasher::new();
        format!(
            "{} @ {}-{}",
            question_answer.question.title, question_answer.question.id, question_answer.answer.id
        )
        .hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish());

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
    }

    Ok(embeddings)
}

fn print_progress(index: usize, total: usize, started: Instant, question_answer: &QuestionAnswer) {
    let completed = index;
    let remaining_seconds = if completed == 0 {
        None
    } else {
        let seconds_per_answer = started.elapsed().as_secs_f64() / completed as f64;
        Some((seconds_per_answer * (total - completed) as f64) as u64)
    };

    println!(
        "Embedding {}/{} ({:.2}%) \"{}\"",
        index + 1,
        total,
        ((index + 1) as f64 / total as f64) * 100.0,
        question_answer.question.title
    );
    if let Some(seconds) = remaining_seconds {
        let (hours, minutes, seconds) = seconds_to_time_format(seconds);
        println!("- Estimated time remaining: {hours}h, {minutes}m, {seconds}s");
    }
}

fn seconds_to_time_format(seconds: u64) -> (u64, u64, u64) {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;

    let hours = seconds / HOUR;
    let remaining = seconds % HOUR;
    let minutes = remaining / MINUTE;
    let seconds = remaining % MINUTE;
    (hours, minutes, seconds)
}
