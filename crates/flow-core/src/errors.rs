use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlowError {
    #[error("io error: {0}")]
    Io(std::io::Error),
    #[error("toml parse error: {0}")]
    TomlDe(toml::de::Error),
    #[error("toml serialize error: {0}")]
    TomlSer(toml::ser::Error),
    #[error("database error: {0}")]
    Db(String),
    #[error("invalid config: {0}")]
    Validation(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}
