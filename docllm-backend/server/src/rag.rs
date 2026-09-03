// RAG endpoints

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use axum::{
    Json,
    extract::{Multipart, State},
    http::StatusCode,
};
use documentllm_core::{
    database,
    ingest::{DocumentInput, DocumentMode, replace_documents_index},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppState, openai::ApiError};

pub const UPLOAD_DIRECTORY: &str = "./upload";
pub const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct UploadedDocument {
    id: Uuid,
    filename: String,
}

#[derive(Serialize)]
pub struct UploadListResponse {
    documents: Vec<UploadedDocument>,
}

#[derive(Serialize)]
pub struct UploadResponse {
    documents: Vec<UploadedDocument>,
}

#[derive(Deserialize)]
pub struct IngestRequest {
    #[serde(default)]
    document_ids: Vec<Uuid>,
    #[serde(default)]
    document_id: Option<Uuid>,
}

impl IngestRequest {
    fn document_ids(self) -> Vec<Uuid> {
        let mut ids = self.document_ids;

        if let Some(id) = self.document_id {
            ids.push(id);
        }

        let mut unique_ids = Vec::new();
        for id in ids {
            if !unique_ids.contains(&id) {
                unique_ids.push(id);
            }
        }

        unique_ids
    }
}

#[derive(Serialize)]
pub struct IngestResponse {
    documents: Vec<UploadedDocument>,
    chunk_count: usize,
}

struct UploadedDocumentPath {
    id: Uuid,
    filename: String,
    path: PathBuf,
}

// GET /upload
pub(super) async fn list_uploads() -> Result<Json<UploadListResponse>, ApiError> {
    let documents = read_uploaded_documents()?
        .into_iter()
        .map(|document| UploadedDocument {
            id: document.id,
            filename: document.filename,
        })
        .collect();

    Ok(Json(UploadListResponse { documents }))
}

// POST /upload
pub(super) async fn upload(
    State(_state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResponse>), ApiError> {
    let mut documents = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|error| ApiError {
        status: StatusCode::BAD_REQUEST,
        message: format!("Processing failed: {error}"),
    })? {
        let field_name = field.name().unwrap_or_default();
        if !is_document_field(field_name) {
            continue;
        }

        let filename = safe_filename(field.file_name().unwrap_or("document"));
        println!("Document upload \"{filename}\"");

        let document_id = Uuid::new_v4();
        let document_dir = upload_document_dir(document_id);
        let path = document_dir.join(&filename);

        let file_bytes = field.bytes().await.map_err(|error| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        })?;

        fs::create_dir_all(&document_dir).map_err(|error| {
            io_error(
                format!(
                    "Failed to create upload directory {}",
                    document_dir.display()
                ),
                error,
            )
        })?;
        fs::write(&path, file_bytes).map_err(|error| {
            io_error(format!("Failed to write upload {}", path.display()), error)
        })?;

        documents.push(UploadedDocument {
            id: document_id,
            filename,
        });
    }

    if documents.is_empty() {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            message: String::from(r#"Missing "documents" field in multipart upload"#),
        });
    }

    Ok((StatusCode::CREATED, Json(UploadResponse { documents })))
}

// PUT /ingest
pub(super) async fn ingest_new(
    State(_state): State<AppState>,
    Json(request): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, ApiError> {
    let document_ids = request.document_ids();

    if document_ids.is_empty() {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            message: String::from("At least one uploaded document must be selected"),
        });
    }

    let mut documents = Vec::new();
    let mut inputs = Vec::new();

    for document_id in document_ids {
        let document = find_uploaded_document(document_id)?;
        let mode = DocumentMode::from_path(&document.path).map_err(|message| ApiError {
            status: StatusCode::BAD_REQUEST,
            message,
        })?;

        documents.push(UploadedDocument {
            id: document.id,
            filename: document.filename.clone(),
        });
        inputs.push(DocumentInput::new(document.path, mode));
    }

    let chunk_count = replace_documents_index(&inputs)
        .await
        .map_err(|message| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("Ingestion failed: {message}"),
        })?;

    Ok(Json(IngestResponse {
        documents,
        chunk_count,
    }))
}

// POST /ingest
pub(super) async fn ingest_append() -> Result<StatusCode, ApiError> {
    Err(ApiError {
        status: StatusCode::NOT_IMPLEMENTED,
        message: String::from("Appending to an existing RAG database is not implemented yet"),
    })
}

// DELETE /ingest
pub(super) async fn delete() -> Result<StatusCode, ApiError> {
    database::delete_database().await.map_err(|e| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: e.to_string(),
    })?;

    Ok(StatusCode::ACCEPTED)
}

fn find_uploaded_document(id: Uuid) -> Result<UploadedDocumentPath, ApiError> {
    let document_dir = upload_document_dir(id);

    if document_dir.is_dir() {
        return uploaded_document_from_dir(id, document_dir)?.ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: String::from("Uploaded document has no stored file"),
        });
    }

    let legacy_path = PathBuf::from(UPLOAD_DIRECTORY).join(id.to_string());
    if legacy_path.is_file() {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            message: String::from(
                "Uploaded document has no original filename. Upload it again so its type can be inferred.",
            ),
        });
    }

    Err(ApiError {
        status: StatusCode::NOT_FOUND,
        message: String::from("No such uploaded document"),
    })
}

fn read_uploaded_documents() -> Result<Vec<UploadedDocumentPath>, ApiError> {
    let entries = match fs::read_dir(UPLOAD_DIRECTORY) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(io_error(
                format!("Failed to read upload directory {UPLOAD_DIRECTORY}"),
                error,
            ));
        }
    };

    let mut documents = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            io_error(
                format!("Failed to read an entry in {UPLOAD_DIRECTORY}"),
                error,
            )
        })?;
        let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|name| Uuid::parse_str(name).ok())
        else {
            continue;
        };

        if !entry.path().is_dir() {
            continue;
        }

        if let Some(document) = uploaded_document_from_dir(id, entry.path())? {
            documents.push(document);
        }
    }

    documents.sort_by(|left, right| left.filename.cmp(&right.filename));
    Ok(documents)
}

fn uploaded_document_from_dir(
    id: Uuid,
    document_dir: PathBuf,
) -> Result<Option<UploadedDocumentPath>, ApiError> {
    let entries = fs::read_dir(&document_dir).map_err(|error| {
        io_error(
            format!("Failed to read upload directory {}", document_dir.display()),
            error,
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            io_error(
                format!("Failed to read an entry in {}", document_dir.display()),
                error,
            )
        })?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let filename = entry.file_name().to_string_lossy().into_owned();
        return Ok(Some(UploadedDocumentPath { id, filename, path }));
    }

    Ok(None)
}

fn upload_document_dir(id: Uuid) -> PathBuf {
    PathBuf::from(UPLOAD_DIRECTORY).join(id.to_string())
}

fn is_document_field(field_name: &str) -> bool {
    matches!(field_name, "documents" | "document" | "files" | "file")
}

fn safe_filename(filename: &str) -> String {
    let filename = filename.replace('\\', "/");
    let filename = Path::new(&filename)
        .file_name()
        .and_then(|filename| filename.to_str())
        .map(str::trim)
        .filter(|filename| !filename.is_empty() && *filename != "." && *filename != "..")
        .unwrap_or("document");

    filename.to_owned()
}

fn io_error(message: String, error: io::Error) -> ApiError {
    ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("{message}: {error}"),
    }
}
