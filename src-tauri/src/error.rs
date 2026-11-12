use serde::{Serialize, Serializer};

/// An error returned to the frontend as a plain, human-readable message.
///
/// Internally everything uses `anyhow`; command handlers convert at the boundary
/// so the UI always receives a string it can show in a toast.
#[derive(Debug)]
pub struct AppError(pub String);

impl AppError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        // `{:#}` keeps the context chain ("Couldn't install X: disk full").
        Self(format!("{error:#}"))
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

pub type CmdResult<T> = Result<T, AppError>;

/// An HTTP failure with its status code, so callers can react to 404s and rate limits.
#[derive(Debug)]
pub struct HttpStatusError {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for HttpStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HttpStatusError {}

/// True when the error chain contains an HTTP 404.
pub fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| matches!(cause.downcast_ref::<HttpStatusError>(), Some(e) if e.status == 404))
}
