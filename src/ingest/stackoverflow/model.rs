// Specialized for the Stack Overflow StackSample dataset:
// https://www.kaggle.com/datasets/stackoverflow/stacksample

use html_to_markdown_rs::convert;
use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
#[allow(unused)]
pub(super) struct Question {
    pub id: usize,
    owner_user_id: usize,
    creation_date: String,
    closed_date: String,
    scope: Option<usize>,
    pub title: String,
    body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(unused)]
pub(super) struct Answer {
    id: usize,
    owner_user_id: usize,
    creation_date: String,
    pub parent_id: usize,
    score: usize,
    body: String,
}

#[allow(unused)]
pub(super) struct QuestionAnswer {
    pub id: usize,
    pub question: Question,
    pub answer: Answer,
}

impl QuestionAnswer {
    pub fn to_markdown(&self) -> Result<String, String> {
        let question_title = &self.question.title;
        let answer_score = self.answer.score;
        let answer_body = convert(&self.answer.body, None)
            .map_err(|e| e.to_string())?
            .content
            .ok_or(String::from("Failed to convert answer body to markdown"))?;

        Ok(format!(
            "\
<START QUESTION ANSWER>
Context: {question_title}
Answer score: {answer_score}
Content: {answer_body}
<END QUESTION ANSWER>
"
        ))
    }
}
