use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlowError {
    #[error("io error: {0}")]
    Io(std::io::Error),
    #[error("toml parse error: {0}")]
    TomlDe(toml::de::Error),
    #[error("database error: {0}")]
    Db(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}
