use std::path::PathBuf;
use std::sync::Arc;

use crate::embed::EmbedderFactory;

#[derive(Clone)]
pub struct ApiState {
    pub db_path: PathBuf,
    pub embedder_factory: Arc<dyn EmbedderFactory>,
}

impl ApiState {
    pub fn new(db_path: PathBuf, embedder_factory: Arc<dyn EmbedderFactory>) -> Self {
        Self {
            db_path,
            embedder_factory,
        }
    }
}
