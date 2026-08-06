use std::fs::File;

use csv::Reader;

use super::model::{Answer, Question};

const QUESTIONS_PATH: &str = "stackoverflow-qa/Questions.csv";
const ANSWERS_PATH: &str = "stackoverflow-qa/Answers.csv";

pub(super) fn read_qa_files() -> Result<(Vec<Question>, Vec<Answer>), String> {
    let questions_file =
        File::open(QUESTIONS_PATH).map_err(|e| format!("Failed to open {QUESTIONS_PATH}: {e}"))?;
    let answers_file =
        File::open(ANSWERS_PATH).map_err(|e| format!("Failed to open {ANSWERS_PATH}: {e}"))?;

    println!("Deserializing questions");
    let questions: Vec<Question> = Reader::from_reader(questions_file)
        .into_deserialize()
        .filter_map(|record| record.ok())
        .collect();
    println!("Read {} questions", questions.len());

    println!("Deserializing answers");
    let answers: Vec<Answer> = Reader::from_reader(answers_file)
        .into_deserialize()
        .filter_map(|record| record.ok())
        .collect();
    println!("Read {} answers", answers.len());

    Ok((questions, answers))
}
