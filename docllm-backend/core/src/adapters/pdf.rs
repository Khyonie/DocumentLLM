use pdf_inspector::process_pdf;

pub(crate) fn read_pdf_to_markdown(path: &str) -> Result<String, String> {
    let pdf = process_pdf(path).map_err(|e| format!("Failed to process PDF: {e}"))?;

    match pdf.markdown {
        Some(m) => Ok(m),
        None => Err(format!(
            "Document {path} read, but conversion to markdown intermediate failed."
        )),
    }
}
