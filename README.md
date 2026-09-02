# DocumentLLM
Web server component of a RAG-enhanced LLM stack.

HTTP endpoints:
| Endpoint             | Type   | Description                                                       |
|----------------------|--------|-------------------------------------------------------------------|
| /health              | GET    | Returns "ok" if running                                           |
| /v1/models           | GET    | Returns a list of locally installed models                        |
| /v1/chat/completions | POST   | Chat with a model                                                 |
| /ingest              | PUT    | Clears the current RAG database and ingests the selected document |
| /ingest              | POST   | Appends the selected document to the RAG database                 |
| /ingest              | DELETE | Wipes the current RAG database                                    |
| /upload              | POST   | Uploads a document to the server                                  |
