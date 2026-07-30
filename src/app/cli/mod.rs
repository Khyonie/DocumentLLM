pub mod ingest;
pub mod query;

pub enum DocumentMode {
    Pdf,
    Markdown,
}

impl TryFrom<String> for DocumentMode {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "pdf" => Ok(DocumentMode::Pdf),
            "markdown" | "md" => Ok(DocumentMode::Markdown),
            _ => Err(format!("No such document type \"{value}\"")),
        }
    }
}
