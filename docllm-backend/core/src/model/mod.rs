use std::{env, path::PathBuf};

use fastembed::{EmbeddingModel, Error, TextEmbedding, TextInitOptions};

pub fn init_model(model: EmbeddingModel) -> Result<TextEmbedding, Error> {
    let mut options = TextInitOptions::new(model).with_show_download_progress(true);
    if let Ok(cache_path) = env::var("DOCUMENTLLM_EMBEDDING_CACHE_PATH") {
        options = options.with_cache_dir(PathBuf::from(cache_path));
    }

    TextEmbedding::try_new(options)
}
