# DocumentLLM
Web server component of a RAG-enhanced LLM stack.

HTTP endpoints:
| Endpoint             | Type   | Description                                                       |
|----------------------|--------|-------------------------------------------------------------------|
| /health              | GET    | Returns "ok" if running                                           |
| /v1/models           | GET    | Returns a list of locally installed models                        |
| /v1/chat/completions | POST   | Chat with a model                                                 |
| /ingest              | POST   | Clears the current RAG database and ingests a new document        |
| /ingest              | PUT    | Ingests a new document and appends it to the current RAG database |
| /ingest              | DELETE | Wipes the current RAG database                                    |
