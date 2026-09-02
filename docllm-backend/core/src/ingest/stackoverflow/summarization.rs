use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::model::Question;
use crate::llm::{
    message::{ChatMessage, RoleType},
    ollama::OllamaClient,
};

const DEFAULT_SUMMARY_CACHE_PATH: &str = "./index/stackoverflow-summaries";
const SUMMARY_CACHE_VERSION: u32 = 1;
const SYSTEM_PROMPT: &str = r#"You summarize Stack Overflow questions for semantic retrieval.

Preserve the concrete problem, language and framework details, exact errors, relevant identifiers,
configuration values, and constraints. Do not answer the question, recommend a solution, or add
facts that are not present. Keep the summary below 150 words."#;

#[derive(Deserialize, Serialize)]
struct CachedSummary {
    cache_version: u32,
    question_id: u64,
    model: String,
    summary: String,
}

pub(super) struct QuestionSummarizer {
    client: OllamaClient,
    model: String,
    cache_path: PathBuf,
}

impl QuestionSummarizer {
    pub fn new(model: &str) -> Result<Self, String> {
        let cache_path = env::var("DOCUMENTLLM_SUMMARY_CACHE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_SUMMARY_CACHE_PATH));
        fs::create_dir_all(&cache_path)
            .map_err(|e| format!("Failed to create summary cache directory: {e}"))?;

        Ok(Self {
            client: OllamaClient::new(model)?,
            model: model.to_owned(),
            cache_path,
        })
    }

    pub async fn summarize(&self, question: &Question) -> Result<String, String> {
        let cache_file = self.cache_path.join(format!("{}.json", question.id));
        if let Some(summary) = self.cached_summary(question.id) {
            return Ok(summary);
        }

        let body = question.body_as_markdown()?;
        let prompt = format!("Title:\n{}\n\nQuestion:\n{}", question.title, body);
        let messages = vec![
            ChatMessage::new(RoleType::System, SYSTEM_PROMPT.to_owned()),
            ChatMessage::new(RoleType::User, prompt),
        ];
        let response = self.client.chat_without_thinking(messages).await?;
        let summary = response.message().content.trim().to_owned();
        if summary.is_empty() {
            return Err(format!(
                "Summary model returned an empty response for question {}",
                question.id
            ));
        }

        self.write_cache(&cache_file, question.id, &summary)?;
        Ok(summary)
    }

    pub fn cached_summary(&self, question_id: u64) -> Option<String> {
        let cache_file = self.cache_path.join(format!("{question_id}.json"));
        self.read_cache(&cache_file, question_id)
    }

    fn read_cache(&self, path: &Path, question_id: u64) -> Option<String> {
        let contents = fs::read_to_string(path).ok()?;
        let cached: CachedSummary = match serde_json::from_str(&contents) {
            Ok(cached) => cached,
            Err(error) => {
                eprintln!("Ignoring invalid summary cache {}: {error}", path.display());
                return None;
            }
        };

        (cached.cache_version == SUMMARY_CACHE_VERSION
            && cached.question_id == question_id
            && cached.model == self.model)
            .then_some(cached.summary)
    }

    fn write_cache(&self, path: &Path, question_id: u64, summary: &str) -> Result<(), String> {
        let cached = CachedSummary {
            cache_version: SUMMARY_CACHE_VERSION,
            question_id,
            model: self.model.clone(),
            summary: summary.to_owned(),
        };
        let serialized = serde_json::to_string_pretty(&cached)
            .map_err(|e| format!("Failed to serialize question {question_id} summary: {e}"))?;
        let temporary_path = path.with_extension("json.tmp");

        fs::write(&temporary_path, serialized)
            .map_err(|e| format!("Failed to write summary cache {}: {e}", path.display()))?;
        fs::rename(&temporary_path, path)
            .map_err(|e| format!("Failed to finalize summary cache {}: {e}", path.display()))
    }
}
