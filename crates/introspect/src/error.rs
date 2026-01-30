use thiserror::Error;

pub type Result<T> = std::result::Result<T, IntrospectionError>;

#[derive(Debug, Error)]
pub enum IntrospectionError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("HTTP error {0}: {1}")]
    Http(u16, String),

    #[error("Failed to parse introspection response: {0}")]
    Parse(String),

    #[error("Invalid introspection response: {0}")]
    Invalid(String),
}
