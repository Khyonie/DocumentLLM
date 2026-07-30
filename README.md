A small project that goes over making an LLM integrate with a RAG database based off a given document. 
Ingestion supports PDF and markdown documents.

# Ingest a document
```shell
actall-llm ingest <document> --mode [ pdf, markdown ]
```
>[!warn]
>Ingesting a new document will wipe the current database. The database is stored at "./index/database.lancedb"

## Flags:
mode: either "pdf" or "markdown". If unspecified, the mode will be guessed based on the file extension.

# Perform a query
```shell
actall-llm query <query...>
```

# Prompt an LLM
```
actall-llm prompt <model> <query...>
```
Sends a prompt to the specified model. The model will receive supporting information from ingested documents.

# Process
## Ingest
1. Given document is read. If the document is a PDF, then it is converted to markdown with `pdf-inspector2`.
2. Document is chunked into passages.

## Embed
1. An embedding model is initialized with `fastembed-rs`.
2. The model takes the chunks and generates vector embeddings containing chunk content and metadata.
3. The embeddings are converted into Apache Arrow format for use in LanceDB.
4. A table is created with LanceDB, and the converted embeddings are inserted.

## Query
1. The user query is embedded.
