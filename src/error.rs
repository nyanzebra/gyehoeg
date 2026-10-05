#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Compilation {0}")]
    Compile(String),

    #[error("Custom {0}")]
    Custom(Box<dyn std::error::Error + Send + Sync>),

    #[error("JMESPath {0}")]
    JMESPath(#[from] jmespatch::JmespathError),

    #[error("IO {0}")]
    IO(#[from] std::io::Error),

    #[error("Serde json {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Serde yaml {0}")]
    SerdeYaml(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
