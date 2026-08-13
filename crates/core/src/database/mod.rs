use arrow_array::RecordBatch;
use lancedb::{Error, Table, connect, database::CreateTableMode};

pub mod retrieval;

const DATABASE_PATH: &str = "./index/database.lancedb";
const DATABASE_NAME: &str = "document_chunks";

/// Creates a new LanceDB database and opens it.
/// If the database exists, it will be overwritten.
pub async fn create_chunk_table(arrow_data: RecordBatch) -> Result<Table, Error> {
    let database = connect(DATABASE_PATH).execute().await?;

    let table = database
        .create_table(DATABASE_NAME, arrow_data)
        .mode(CreateTableMode::Overwrite)
        .execute()
        .await?;

    Ok(table)
}

/// Connects to an existing LanceDB database.
pub async fn open_database() -> Result<Table, Error> {
    let database = connect(DATABASE_PATH).execute().await?;

    database.open_table(DATABASE_NAME).execute().await
}
