# DocumentLLM

Self-contained RAG chat app for PDF and Markdown documents, with multi-query rewriting, dedupe, and reranking.

HTTP endpoints:
| Endpoint             | Type   | Description                                                       |
|----------------------|--------|-------------------------------------------------------------------|
| /health              | GET    | Returns "ok" if running                                           |
| /v1/models           | GET    | Returns a list of locally installed models                        |
| /v1/chat/completions | POST   | Chat with a model                                                 |
| /ingest              | PUT    | Replaces the RAG database with the selected uploaded documents    |
| /ingest              | POST   | Appends the selected document to the RAG database                 |
| /ingest              | DELETE | Wipes the current RAG database                                    |
| /upload              | GET    | Lists uploaded documents                                          |
| /upload              | POST   | Uploads one or more documents to the server                       |

Chat responses always stream as OpenAI-style SSE chunks and end with `data: [DONE]`.
The request does not need a `stream` flag; any supplied value is ignored.
Structured utility calls also stream from Ollama, then collect the JSON before parsing it.

## Docker

Build and run the app plus Ollama:

```sh
docker compose up --build
```

The app listens on port `3001` by default. Set `DOCUMENTLLM_HOST=127.0.0.1`
to bind only to localhost.

Persistent runtime data:

- `./index` stores the LanceDB RAG index.
- `./upload` stores uploaded documents.
- `./fastembed-cache` stores FastEmbed's downloaded embedding model files.

FastEmbed downloads the embedding model the first time ingestion or RAG chat
needs it. After that, it runs from `./fastembed-cache`. If the deployment
machine cannot reach Hugging Face, warm this cache on a machine with internet
access first, then copy `./fastembed-cache` to the deployment machine.
