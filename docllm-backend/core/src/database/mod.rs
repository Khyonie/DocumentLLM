use arrow_array::RecordBatch;
use fastembed::TextEmbedding;
use lancedb::{Error, Table, connect, database::CreateTableMode};
use std::env;

pub mod providers;
pub mod retrieval;

const DEFAULT_DATABASE_PATH: &str = "./index/database.lancedb";
const DATABASE_NAME: &str = "document_chunks";

/// Creates a new LanceDB database and opens it.
/// If the database exists, it will be overwritten.
pub async fn create_chunk_table(arrow_data: RecordBatch) -> Result<Table, Error> {
    let path = database_path();
    let database = connect(&path).execute().await?;

    let table = database
        .create_table(DATABASE_NAME, arrow_data)
        .mode(CreateTableMode::Overwrite)
        .execute()
        .await?;

    Ok(table)
}

/// Connects to an existing LanceDB database.
pub async fn open_database() -> Result<Table, Error> {
    let path = database_path();
    let database = connect(&path).execute().await?;

    database.open_table(DATABASE_NAME).execute().await
}

pub async fn delete_database() -> Result<(), Error> {
    let path = database_path();
    let database = connect(&path).execute().await?;
    database.drop_all_tables(&[]).await?;

    Ok(())
}

fn database_path() -> String {
    env::var("DOCUMENTLLM_DATABASE_PATH").unwrap_or_else(|_| DEFAULT_DATABASE_PATH.to_owned())
}

pub fn embed_query(
    query: &str,
    embedding_model: &mut TextEmbedding,
) -> Result<Option<Vec<f32>>, fastembed::Error> {
    let value = embedding_model.embed(vec![query], None)?.into_iter().next();

    Ok(value)
}
