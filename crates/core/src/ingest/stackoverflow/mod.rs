mod embedding;
mod model;
mod reader;
mod summarization;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use self::{model::QuestionAnswer, summarization::QuestionSummarizer};
use crate::{adapters::arrow, database, ingest::chunking::DocumentChunk};

/// Builds Stack Overflow question/answer chunks and replaces the current document index.
pub async fn replace_index(summary_model: &str, answer_limit: usize) -> Result<usize, String> {
    if answer_limit == 0 {
        return Err(String::from("Answer count must be greater than zero"));
    }

    let chunks = build_chunks(summary_model, answer_limit).await?;
    let chunk_count = chunks.len();
    let arrow_data = arrow::convert_to_arrow(chunks)?;
    database::create_chunk_table(arrow_data)
        .await
        .map_err(|error| format!("Failed to replace document index: {error}"))?;

    Ok(chunk_count)
}

pub async fn build_chunks(
    summary_model: &str,
    answer_limit: usize,
) -> Result<Vec<DocumentChunk>, String> {
    let answers = reader::read_top_answers(answer_limit)?;
    let question_ids: HashSet<u64> = answers.iter().map(|answer| answer.parent_id).collect();
    let questions = reader::read_questions(&question_ids)?;
    let question_answers = connect_answers_to_questions(questions, answers);
    let summaries = summarize_questions(&question_answers, summary_model).await?;

    embedding::embed_question_answers(&question_answers, &summaries)
}

fn connect_answers_to_questions(
    questions: HashMap<u64, model::Question>,
    answers: Vec<model::Answer>,
) -> Vec<QuestionAnswer> {
    let questions: HashMap<u64, Arc<model::Question>> = questions
        .into_iter()
        .map(|(id, question)| (id, Arc::new(question)))
        .collect();
    let answer_count = answers.len();

    let connected: Vec<QuestionAnswer> = answers
        .into_iter()
        .filter_map(|answer| {
            let question = questions.get(&answer.parent_id)?.clone();
            Some(QuestionAnswer { question, answer })
        })
        .collect();

    let missing = answer_count - connected.len();
    if missing > 0 {
        eprintln!("Skipped {missing} answers whose parent questions were unavailable");
    }
    connected
}

async fn summarize_questions(
    question_answers: &[QuestionAnswer],
    summary_model: &str,
) -> Result<HashMap<u64, String>, String> {
    let summarizer = QuestionSummarizer::new(summary_model)?;
    let mut seen = HashSet::new();
    let unique_questions: Vec<Arc<model::Question>> = question_answers
        .iter()
        .filter(|item| seen.insert(item.question.id))
        .map(|item| item.question.clone())
        .collect();
    let unique_count = unique_questions.len();
    let mut summaries = HashMap::with_capacity(unique_count);
    let mut uncached_questions = Vec::new();

    for question in unique_questions {
        if let Some(summary) = summarizer.cached_summary(question.id) {
            summaries.insert(question.id, summary);
        } else {
            uncached_questions.push(question);
        }
    }

    let cached_count = summaries.len();
    let generation_count = uncached_questions.len();
    println!(
        "Question summaries: {cached_count} cached, {generation_count} to generate ({unique_count} total)"
    );
    let started = Instant::now();

    for (index, question) in uncached_questions.into_iter().enumerate() {
        println!(
            "Summarizing question {}/{}: {}",
            index + 1,
            generation_count,
            question.title
        );
        let summary = summarizer.summarize(&question).await?;
        summaries.insert(question.id, summary);
        print_summary_eta(started, index + 1, generation_count);
    }

    Ok(summaries)
}

fn print_summary_eta(started: Instant, completed: usize, total: usize) {
    if completed == 0 || completed >= total {
        return;
    }

    let elapsed = started.elapsed().as_secs_f64();
    let average_seconds = elapsed / completed as f64;
    let remaining_seconds = (average_seconds * (total - completed) as f64) as u64;
    let (hours, minutes, seconds) = seconds_to_time_format(remaining_seconds);

    println!(
        "- Summary ETA: {hours}h, {minutes}m, {seconds}s remaining ({average_seconds:.1}s average)"
    );
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
