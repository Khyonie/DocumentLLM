// Specialized for the StackOverflow dataset I found
// https://www.kaggle.com/datasets/stackoverflow/stacksample?select=Questions.csv

use html_to_markdown_rs::convert;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(unused)]
pub struct StackOverflowQuestion
{
    pub id: usize,
    owner_user_id: usize,
    creation_date: String,
    closed_date: String,
    scope: Option<usize>,
    pub title: String,
    body: String
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(unused)]
pub struct StackOverflowAnswer
{
    id: usize,
    owner_user_id: usize,
    creation_date: String,
    pub parent_id: usize,
    score: usize,
    body: String
}

#[allow(unused)]
pub struct StackOverflowQuestionAnswer
{
    pub id: usize,
    pub question: StackOverflowQuestion,
    pub answers: Vec<StackOverflowAnswer>
}

impl StackOverflowQuestionAnswer
{
    pub fn to_markdown(&self) -> String
    {
        let mut markdown_string = String::new();

        // Question
        markdown_string.push_str(&format!("# Question: {}\n", self.question.title));

        let markdown_body = convert(&self.question.body, None)
            .unwrap()
            .content
            .unwrap();
        markdown_string.push_str(&format!("{markdown_body}\n"));

        // Answers
        markdown_string.push_str("**START STACKOVERFLOW USER ANSWERS**:\n");
        for answer in &self.answers
        {
            let markdown_body = convert(&answer.body, None)
                .unwrap()
                .content
                .unwrap();

            markdown_string.push_str("**START ANSWER**\n");
            markdown_string.push_str(&format!("{}\n", markdown_body));
            markdown_string.push_str("**END ANSWER**\n");
        }
        markdown_string.push_str("**END STACKOVERFLOW USER ANSWERS**:\n");

        markdown_string
    }
}
