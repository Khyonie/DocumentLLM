mod embedding;
mod model;
mod reader;
mod summarization;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use self::{model::QuestionAnswer, summarization::QuestionSummarizer};
use crate::ingest::chunking::DocumentChunk;

const ANSWER_LIMIT: usize = 1000;

pub async fn build_chunks(summary_model: &str) -> Result<Vec<DocumentChunk>, String> {
    let answers = reader::read_top_answers(ANSWER_LIMIT)?;
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
    let unique_count = question_answers
        .iter()
        .map(|item| item.question.id)
        .collect::<HashSet<_>>()
        .len();
    let mut summaries = HashMap::with_capacity(unique_count);

    for question_answer in question_answers {
        let question = &question_answer.question;
        if summaries.contains_key(&question.id) {
            continue;
        }

        println!(
            "Summarizing question {}/{}: {}",
            summaries.len() + 1,
            unique_count,
            question.title
        );
        let summary = summarizer.summarize(question).await?;
        summaries.insert(question.id, summary);
    }

    Ok(summaries)
}
