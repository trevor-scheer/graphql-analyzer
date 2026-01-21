//! Exit codes for the GraphQL CLI.
//!
//! This module defines distinct exit codes for different error types,
//! allowing scripts and CI systems to distinguish between different
//! failure modes.

/// Exit codes used by the CLI.
///
/// These follow standard Unix conventions where 0 indicates success
/// and non-zero values indicate different types of failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    /// Success - no errors
    Success = 0,
    /// Validation or lint errors found in GraphQL documents
    ValidationError = 1,
    /// Configuration error (missing or invalid config file)
    ConfigError = 2,
    /// Schema load error (introspection failed, file not found)
    SchemaError = 3,
    /// I/O error (file read/write failure)
    IoError = 4,
    /// Parse error (invalid GraphQL syntax in config or schema)
    ParseError = 5,
}

impl ExitCode {
    /// Exit the process with this exit code.
    pub fn exit(self) -> ! {
        std::process::exit(self as i32)
    }

    /// Get the numeric value of this exit code.
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }
}

impl std::fmt::Display for ExitCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::ValidationError => write!(f, "validation error"),
            Self::ConfigError => write!(f, "configuration error"),
            Self::SchemaError => write!(f, "schema load error"),
            Self::IoError => write!(f, "I/O error"),
            Self::ParseError => write!(f, "parse error"),
        }
    }
}
