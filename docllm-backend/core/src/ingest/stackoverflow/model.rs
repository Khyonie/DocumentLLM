// Specialized for the Stack Overflow StackSample dataset:
// https://www.kaggle.com/datasets/stackoverflow/stacksample

use std::{cmp::Ordering, sync::Arc};

use html_to_markdown_rs::convert;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Question {
    pub id: u64,
    pub title: String,
    body: String,
}

impl Question {
    pub fn body_as_markdown(&self) -> Result<String, String> {
        convert(&self.body, None)
            .map_err(|e| e.to_string())?
            .content
            .ok_or_else(|| format!("Failed to convert question {} to markdown", self.id))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Answer {
    pub id: u64,
    pub parent_id: u64,
    pub score: i64,
    body: String,
}

impl Answer {
    fn body_as_markdown(&self) -> Result<String, String> {
        convert(&self.body, None)
            .map_err(|e| e.to_string())?
            .content
            .ok_or_else(|| format!("Failed to convert answer {} to markdown", self.id))
    }
}

/// Orders answers by score and then ID so they can be retained in a bounded heap.
pub(super) struct RankedAnswer(pub Answer);

impl PartialEq for RankedAnswer {
    fn eq(&self, other: &Self) -> bool {
        self.0.score == other.0.score && self.0.id == other.0.id
    }
}

impl Eq for RankedAnswer {}

impl PartialOrd for RankedAnswer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedAnswer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .score
            .cmp(&other.0.score)
            .then_with(|| self.0.id.cmp(&other.0.id))
    }
}

pub(super) struct QuestionAnswer {
    pub question: Arc<Question>,
    pub answer: Answer,
}

impl QuestionAnswer {
    pub fn to_markdown(&self, question_summary: &str) -> Result<String, String> {
        let question_id = self.question.id;
        let answer_id = self.answer.id;
        let question_title = &self.question.title;
        let answer_score = self.answer.score;
        let answer_body = self.answer.body_as_markdown()?;

        Ok(format!(
            "\
<START QUESTION ANSWER>
Question: {question_title}
Question URL: https://stackoverflow.com/questions/{question_id}
Question summary: {question_summary}
Answer score: {answer_score}
Answer URL: https://stackoverflow.com/a/{answer_id}
Answer: {answer_body}
<END QUESTION ANSWER>
"
        ))
    }
}
