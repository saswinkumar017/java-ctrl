use serde::Serialize;

/// Every fallible operation in the backend returns this error. Tauri commands
/// serialise it as a human-readable string so the UI can surface it directly.
#[derive(Debug, thiserror::Error)]
pub enum JccError {
    #[error("{0}")]
    Message(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("could not parse tool output: {0}")]
    Parse(String),
    #[error("{0}")]
    Tool(String),
    #[error("{0}")]
    Attach(String),
    #[error("{0}")]
    Http(String),
    #[error("timed out after {0} ms: {1}")]
    Timeout(u64, String),
    #[error("{0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl JccError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    pub fn tool(message: impl Into<String>) -> Self {
        Self::Tool(message.into())
    }

    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse(message.into())
    }

    pub fn attach(message: impl Into<String>) -> Self {
        Self::Attach(message.into())
    }

    pub fn http(message: impl Into<String>) -> Self {
        Self::Http(message.into())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }
}

impl Serialize for JccError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, JccError>;

/// Convert a `serde_json` failure into a backend error without losing context.
impl From<serde_json::Error> for JccError {
    fn from(value: serde_json::Error) -> Self {
        Self::Parse(value.to_string())
    }
}
