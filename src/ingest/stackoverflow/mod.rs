mod embedding;
mod model;
mod reader;

use std::collections::HashMap;

use self::model::{Answer, Question, QuestionAnswer};
use crate::ingest::chunking::DocumentChunk;

pub fn build_chunks() -> Result<Vec<DocumentChunk>, String> {
    let (questions, answers) = reader::read_qa_files()?;
    let question_answers = connect_answers_to_questions(questions, answers);
    embedding::embed_question_answers(&question_answers)
}

fn connect_answers_to_questions(
    questions: Vec<Question>,
    answers: Vec<Answer>,
) -> Vec<QuestionAnswer> {
    println!("Sorting questions by ID");
    let questions_by_id: HashMap<usize, Question> = questions
        .into_iter()
        .map(|question| (question.id, question))
        .collect();

    println!("Connecting answers to their parent question");
    answers
        .into_iter()
        .filter_map(|answer| {
            let question = questions_by_id.get(&answer.parent_id)?.clone();
            Some(QuestionAnswer {
                id: answer.parent_id,
                answer,
                question,
            })
        })
        .collect()
}
