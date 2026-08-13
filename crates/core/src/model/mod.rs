use fastembed::{EmbeddingModel, Error, TextEmbedding, TextInitOptions};

pub fn init_model(model: EmbeddingModel) -> Result<TextEmbedding, Error> {
    let options = TextInitOptions::new(model).with_show_download_progress(true);

    TextEmbedding::try_new(options)
}
