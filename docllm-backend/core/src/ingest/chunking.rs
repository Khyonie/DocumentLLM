use std::hash::{DefaultHasher, Hash, Hasher};

use fastembed::TextEmbedding;
use markdown_chunk::chunk;

pub struct DocumentChunk {
    pub id: String,
    pub source: String,
    pub index: u32,
    pub content: String,
    pub embedding: Vec<f32>,
}

pub(crate) fn chunk_markdown(
    mut model: TextEmbedding,
    file: &str,
    markdown: String,
    chunk_size: usize,
) -> Result<Vec<DocumentChunk>, String> {
    let markdown_chunks = chunk(&markdown, chunk_size);
    let mut document_chunks: Vec<DocumentChunk> = Vec::new();

    for (index, chunk) in markdown_chunks.iter().enumerate() {
        let mut hasher = DefaultHasher::new();
        // Generate chunk ID
        // TODO This isn't stable between changing filenames
        format!("{file} @ {index}").hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish()); // Hash number as padded hex

        // RAG embedding
        let embedding = model
            .embed(vec![chunk], None)
            .map_err(|e| format!("{e}"))?
            .into_iter()
            .next()
            .ok_or("Failed to embed chunk")?;

        let chunk = DocumentChunk {
            id,
            source: String::from(file),
            index: index as u32,
            content: String::from(chunk),
            embedding,
        };

        document_chunks.push(chunk);
    }

    if document_chunks.is_empty() {
        return Err(String::from(
            "No document chunks, the document may be empty",
        ));
    }

    Ok(document_chunks)
}
