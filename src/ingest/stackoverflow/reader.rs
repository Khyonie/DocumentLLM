use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
    fs::File,
};

use csv::Reader;

use super::model::{Answer, Question, RankedAnswer};

const QUESTIONS_PATH: &str = "stackoverflow-qa/Questions.csv";
const ANSWERS_PATH: &str = "stackoverflow-qa/Answers.csv";

pub(super) fn read_top_answers(limit: usize) -> Result<Vec<Answer>, String> {
    let answers_file =
        File::open(ANSWERS_PATH).map_err(|e| format!("Failed to open {ANSWERS_PATH}: {e}"))?;
    let mut selected: BinaryHeap<Reverse<RankedAnswer>> = BinaryHeap::with_capacity(limit + 1);
    let mut rejected = 0usize;
    let mut first_error = None;

    println!("Selecting the {limit} highest-scoring answers");
    for record in Reader::from_reader(answers_file).into_deserialize::<Answer>() {
        match record {
            Ok(answer) => {
                selected.push(Reverse(RankedAnswer(answer)));
                if selected.len() > limit {
                    selected.pop();
                }
            }
            Err(error) => {
                rejected += 1;
                first_error.get_or_insert_with(|| error.to_string());
            }
        }
    }

    report_rejected("answer", rejected, first_error.as_deref());

    let mut answers: Vec<Answer> = selected
        .into_iter()
        .map(|Reverse(ranked)| ranked.0)
        .collect();
    answers.sort_unstable_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });

    println!("Selected {} answers", answers.len());
    Ok(answers)
}

pub(super) fn read_questions(
    question_ids: &HashSet<u64>,
) -> Result<HashMap<u64, Question>, String> {
    let questions_file =
        File::open(QUESTIONS_PATH).map_err(|e| format!("Failed to open {QUESTIONS_PATH}: {e}"))?;
    let mut questions = HashMap::with_capacity(question_ids.len());
    let mut rejected = 0usize;
    let mut first_error = None;

    println!("Loading {} related questions", question_ids.len());
    for record in Reader::from_reader(questions_file).into_deserialize::<Question>() {
        match record {
            Ok(question) if question_ids.contains(&question.id) => {
                questions.insert(question.id, question);
                if questions.len() == question_ids.len() {
                    break;
                }
            }
            Ok(_) => {}
            Err(error) => {
                rejected += 1;
                first_error.get_or_insert_with(|| error.to_string());
            }
        }
    }

    report_rejected("question", rejected, first_error.as_deref());
    println!("Loaded {} related questions", questions.len());
    Ok(questions)
}

fn report_rejected(record_type: &str, count: usize, first_error: Option<&str>) {
    if count == 0 {
        return;
    }

    eprintln!("Skipped {count} malformed {record_type} records");
    if let Some(error) = first_error {
        eprintln!("First {record_type} parsing error: {error}");
    }
}
