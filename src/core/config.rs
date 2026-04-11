use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

const DEFAULT_DB_PATH: &str = "~/.mempal/palace.db";
const DEFAULT_EMBED_BACKEND: &str = "model2vec";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub db_path: String,
    pub embed: EmbedConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: DEFAULT_DB_PATH.to_string(),
            embed: EmbedConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&default_config_path())
    }

    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(contents) => Ok(toml::from_str(&contents)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct EmbedConfig {
    pub backend: String,
    /// Model identifier (e.g., "minishlab/potion-multilingual-128M" for model2vec).
    pub model: Option<String>,
    pub api_endpoint: Option<String>,
    pub api_model: Option<String>,
}

impl Default for EmbedConfig {
    fn default() -> Self {
        Self {
            backend: DEFAULT_EMBED_BACKEND.to_string(),
            model: None,
            api_endpoint: None,
            api_model: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config from {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config TOML")]
    Parse(#[from] toml::de::Error),
}

fn default_config_path() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".mempal").join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("~/.mempal/config.toml"))
}
