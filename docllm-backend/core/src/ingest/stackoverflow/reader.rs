use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
    env,
    fs::File,
    path::PathBuf,
};

use csv::Reader;

use super::model::{Answer, Question, RankedAnswer};

const DEFAULT_DATASET_PATH: &str = "stackoverflow-qa";

pub(super) fn read_top_answers(limit: usize) -> Result<Vec<Answer>, String> {
    let answers_path = dataset_path().join("Answers.csv");
    let answers_file = File::open(&answers_path)
        .map_err(|e| format!("Failed to open {}: {e}", answers_path.display()))?;
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
    let questions_path = dataset_path().join("Questions.csv");
    let questions_file = File::open(&questions_path)
        .map_err(|e| format!("Failed to open {}: {e}", questions_path.display()))?;
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

fn dataset_path() -> PathBuf {
    env::var("DOCUMENTLLM_STACKOVERFLOW_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_DATASET_PATH))
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
