A small project that goes over making an LLM integrate with a RAG database based off a given document. 
Ingestion supports PDF and markdown documents.

# Ingest a document
```shell
llm-project ingest <document> --mode [ pdf, markdown ]
```
>[!warn]
>Ingesting a new document will wipe the current database. The database is stored at "./index/database.lancedb"

## Flags:
mode: either "pdf" or "markdown". If unspecified, the mode will be guessed based on the file extension.

# Perform a query
```shell
llm-project query <query...>
```

# Prompt an LLM
```
llm-project prompt <model> <query...>
```
Sends a prompt to the specified model. The model will receive supporting information from ingested documents.

# Ingest Stack Overflow answers
```shell
llm-project ingest-stackoverflow <summary-model>
```
Streams the answer dataset, retains the 1,000 highest-scoring answers, summarizes each related
question with the selected local Ollama model, and caches summaries under `./index/stackoverflow-summaries`.

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
