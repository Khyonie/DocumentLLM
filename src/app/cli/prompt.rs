use fastembed::EmbeddingModel;
use tokio::runtime::Runtime;

use crate::{
    app::cli::query::RESULT_LIMIT, database::{
        self,
        retrieval::{SearchHit, search_chunks},
    }, llm::{
        message::{ChatMessage, RoleType},
        ollama::{self, OllamaClient},
    }, model
};

pub fn trigger_prompt(args: &[String]) -> Result<(), String> {
    // Model setting
    let model_name = args[0].clone();
    let role = ChatMessage::new(RoleType::System, String::from(ollama::SYSTEM_PROMPT));

    // Build prompt
    let runtime = Runtime::new().map_err(|e| format!("Failed to create Tokio runtime: {e}"))?;

    let prompt = args[1..].join(" ");
    let formatted_prompt = runtime.block_on(build_prompt(&prompt))?;
    let message = ChatMessage::new(RoleType::User, formatted_prompt);

    // Send prompt
    let ollama = OllamaClient::new(&model_name);
    let response = runtime.block_on(ollama.chat(vec![role, message]))?;

    println!("{}", response.message().content);
    Ok(())
}

async fn build_prompt(query: &str) -> Result<String, String> {
    let sources = retrieve_sources(query).await?;

    let mut prompt = String::from("<documentation>\n");
    for source in sources {
        let document_string = format!(
            r#"<source>
Document: {}
Index: {}

{}
</source>"#,
            source.source, source.chunk_index, source.content
        );

        prompt.push_str(&document_string);
        prompt.push('\n');
    }
    prompt.push_str("</documentation>\n");
    prompt.push_str(&format!("<question>\n{query}\n</question>\n\n"));
    prompt.push_str("Answer the question using the documentation above. Cite relevant sources with their document name.");

    Ok(prompt)
}

async fn retrieve_sources(query: &str) -> Result<Vec<SearchHit>, String> {
    let table = database::open_database()
        .await
        .map_err(|e| format!("Failed to open database table: {e}"))?;

    let mut model = model::init_model(EmbeddingModel::AllMiniLML6V2)
        .map_err(|e| format!("Failed to initialize model: {e}"))?;
    let query_embedding = model
        .embed(vec![query], None)
        .map_err(|e| format!("Failed to embed query: {e}"))?
        .into_iter()
        .next()
        .ok_or(String::from("Model gave no query embedding"))?;

    search_chunks(&table, &query_embedding, RESULT_LIMIT)
        .await
        .map_err(|e| format!("{e}")) // TODO Magic number
}
