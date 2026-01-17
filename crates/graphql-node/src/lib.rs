//! Node.js bindings for GraphQL validation and linting.
//!
//! This crate provides native Node.js bindings for the GraphQL language service,
//! enabling high-performance validation and linting in Node.js applications.
//!
//! # Example
//!
//! ```javascript
//! const { GraphQLValidator } = require('@graphql-lsp/node');
//!
//! const validator = new GraphQLValidator();
//! validator.setSchema(`
//!     type Query {
//!         hello: String
//!     }
//! `);
//!
//! const result = validator.validate(`
//!     query {
//!         hello
//!     }
//! `);
//!
//! console.log(result.errors); // []
//! console.log(result.isValid()); // true
//! ```

#![deny(clippy::all)]
// napi macros generate code that triggers these warnings
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::unnecessary_wraps)]
// For napi bindings, #[must_use] doesn't make sense since JS controls return value usage
#![allow(clippy::must_use_candidate)]
// These truncations won't happen in practice (diagnostic counts < u32::MAX)
#![allow(clippy::cast_possible_truncation)]

use graphql_ide::{AnalysisHost, DiagnosticSeverity, FileKind, FilePath};
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// A diagnostic message from validation or linting.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The error/warning message.
    pub message: String,
    /// Severity: "error", "warning", "info", or "hint".
    pub severity: String,
    /// Starting line number (0-based).
    pub start_line: u32,
    /// Starting column number (0-based).
    pub start_column: u32,
    /// Ending line number (0-based).
    pub end_line: u32,
    /// Ending column number (0-based).
    pub end_column: u32,
    /// Optional diagnostic code (e.g., lint rule name).
    pub code: Option<String>,
}

impl Diagnostic {
    fn from_ide_diagnostic(d: graphql_ide::Diagnostic) -> Self {
        Diagnostic {
            message: d.message,
            severity: severity_to_string(d.severity),
            start_line: d.range.start.line,
            start_column: d.range.start.character,
            end_line: d.range.end.line,
            end_column: d.range.end.character,
            code: d.code,
        }
    }
}

/// Result of a validation or lint operation.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// All diagnostics from the validation/lint operation.
    pub diagnostics: Vec<Diagnostic>,
}

#[napi]
impl ValidationResult {
    /// Get only error diagnostics.
    #[napi]
    pub fn errors(&self) -> Vec<Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .cloned()
            .collect()
    }

    /// Get only warning diagnostics.
    #[napi]
    pub fn warnings(&self) -> Vec<Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == "warning")
            .cloned()
            .collect()
    }

    /// Get the total number of diagnostics.
    #[napi(getter)]
    pub fn count(&self) -> u32 {
        self.diagnostics.len() as u32
    }

    /// Get the number of errors.
    #[napi(getter)]
    pub fn error_count(&self) -> u32 {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .count() as u32
    }

    /// Get the number of warnings.
    #[napi(getter)]
    pub fn warning_count(&self) -> u32 {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == "warning")
            .count() as u32
    }

    /// Check if there are no errors (validation passed).
    #[napi]
    pub fn is_valid(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.severity == "error")
    }

    /// Check if there are any diagnostics.
    #[napi]
    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// A GraphQL validator that can validate documents against a schema.
#[napi]
pub struct GraphQLValidator {
    host: AnalysisHost,
    schema_loaded: bool,
}

#[napi]
impl GraphQLValidator {
    /// Create a new validator instance.
    #[napi(constructor)]
    pub fn new() -> Self {
        GraphQLValidator {
            host: AnalysisHost::new(),
            schema_loaded: false,
        }
    }

    /// Set the GraphQL schema (SDL format).
    ///
    /// This must be called before validating documents.
    #[napi]
    pub fn set_schema(&mut self, schema_sdl: String) -> Result<()> {
        let schema_path = FilePath::new("schema.graphql");
        self.host
            .add_file(&schema_path, &schema_sdl, FileKind::Schema);
        self.host.rebuild_project_files();
        self.schema_loaded = true;
        Ok(())
    }

    /// Check if a schema has been loaded.
    #[napi(getter)]
    pub fn has_schema(&self) -> bool {
        self.schema_loaded
    }

    /// Validate a GraphQL document against the schema.
    ///
    /// Returns validation errors (GraphQL spec violations).
    #[napi]
    pub fn validate(&mut self, document: String) -> Result<ValidationResult> {
        if !self.schema_loaded {
            return Err(Error::new(
                Status::GenericFailure,
                "No schema loaded. Call setSchema() first.",
            ));
        }

        let doc_path = FilePath::new("document.graphql");
        self.host
            .add_file(&doc_path, &document, FileKind::ExecutableGraphQL);

        let snapshot = self.host.snapshot();
        let diagnostics = snapshot.validation_diagnostics(&doc_path);

        Ok(ValidationResult {
            diagnostics: diagnostics
                .into_iter()
                .map(Diagnostic::from_ide_diagnostic)
                .collect(),
        })
    }

    /// Run lint rules on a GraphQL document.
    ///
    /// Returns lint diagnostics (custom rule violations).
    #[napi]
    pub fn lint(&mut self, document: String) -> Result<ValidationResult> {
        if !self.schema_loaded {
            return Err(Error::new(
                Status::GenericFailure,
                "No schema loaded. Call setSchema() first.",
            ));
        }

        let doc_path = FilePath::new("document.graphql");
        self.host
            .add_file(&doc_path, &document, FileKind::ExecutableGraphQL);

        let snapshot = self.host.snapshot();
        let diagnostics = snapshot.lint_diagnostics(&doc_path);

        Ok(ValidationResult {
            diagnostics: diagnostics
                .into_iter()
                .map(Diagnostic::from_ide_diagnostic)
                .collect(),
        })
    }

    /// Run both validation and lint rules on a GraphQL document.
    ///
    /// Returns all diagnostics (both spec violations and lint issues).
    #[napi]
    pub fn check(&mut self, document: String) -> Result<ValidationResult> {
        if !self.schema_loaded {
            return Err(Error::new(
                Status::GenericFailure,
                "No schema loaded. Call setSchema() first.",
            ));
        }

        let doc_path = FilePath::new("document.graphql");
        self.host
            .add_file(&doc_path, &document, FileKind::ExecutableGraphQL);

        let snapshot = self.host.snapshot();

        let mut all_diagnostics: Vec<Diagnostic> = Vec::new();

        for d in snapshot.validation_diagnostics(&doc_path) {
            all_diagnostics.push(Diagnostic::from_ide_diagnostic(d));
        }

        for d in snapshot.lint_diagnostics(&doc_path) {
            all_diagnostics.push(Diagnostic::from_ide_diagnostic(d));
        }

        Ok(ValidationResult {
            diagnostics: all_diagnostics,
        })
    }

    /// Configure lint rules.
    ///
    /// Pass a JSON object with rule configurations:
    /// ```javascript
    /// validator.configureLint({
    ///     "no_deprecated": "warn",
    ///     "unique_names": "error",
    ///     "unused_fields": "off"
    /// });
    /// ```
    #[napi]
    pub fn configure_lint(&mut self, config: serde_json::Value) -> Result<()> {
        let lint_config: graphql_linter::LintConfig = serde_json::from_value(config)
            .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid lint config: {e}")))?;

        self.host.set_lint_config(lint_config);
        Ok(())
    }

    /// Reset the validator to its initial state.
    #[napi]
    pub fn reset(&mut self) {
        self.host = AnalysisHost::new();
        self.schema_loaded = false;
    }
}

impl Default for GraphQLValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick validation function for one-off checks.
///
/// This is a convenience function that creates a temporary validator,
/// loads the schema, and validates the document in one call.
#[napi]
pub fn quick_validate(schema_sdl: String, document: String) -> Result<ValidationResult> {
    let mut validator = GraphQLValidator::new();
    validator.set_schema(schema_sdl)?;
    validator.validate(document)
}

/// Quick lint function for one-off checks.
#[napi]
pub fn quick_lint(schema_sdl: String, document: String) -> Result<ValidationResult> {
    let mut validator = GraphQLValidator::new();
    validator.set_schema(schema_sdl)?;
    validator.lint(document)
}

/// Quick check function for one-off checks (validation + lint).
#[napi]
pub fn quick_check(schema_sdl: String, document: String) -> Result<ValidationResult> {
    let mut validator = GraphQLValidator::new();
    validator.set_schema(schema_sdl)?;
    validator.check(document)
}

/// Parse a GraphQL schema and return validation errors.
///
/// This only validates the schema itself, not documents against it.
#[napi]
pub fn validate_schema(schema_sdl: String) -> ValidationResult {
    let mut host = AnalysisHost::new();
    let schema_path = FilePath::new("schema.graphql");
    host.add_file(&schema_path, &schema_sdl, FileKind::Schema);
    host.rebuild_project_files();

    let snapshot = host.snapshot();
    let diagnostics = snapshot.validation_diagnostics(&schema_path);

    ValidationResult {
        diagnostics: diagnostics
            .into_iter()
            .map(Diagnostic::from_ide_diagnostic)
            .collect(),
    }
}

/// Convert a diagnostic severity to a string.
fn severity_to_string(severity: DiagnosticSeverity) -> String {
    match severity {
        DiagnosticSeverity::Error => "error".to_string(),
        DiagnosticSeverity::Warning => "warning".to_string(),
        DiagnosticSeverity::Information => "info".to_string(),
        DiagnosticSeverity::Hint => "hint".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = GraphQLValidator::new();
        assert!(!validator.has_schema());
    }

    #[test]
    fn test_schema_loading() {
        let mut validator = GraphQLValidator::new();
        validator
            .set_schema("type Query { hello: String }".to_string())
            .unwrap();
        assert!(validator.has_schema());
    }

    #[test]
    fn test_valid_document() {
        let mut validator = GraphQLValidator::new();
        validator
            .set_schema("type Query { hello: String }".to_string())
            .unwrap();
        let result = validator.validate("query { hello }".to_string()).unwrap();
        assert!(result.is_valid());
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn test_invalid_document() {
        let mut validator = GraphQLValidator::new();
        validator
            .set_schema("type Query { hello: String }".to_string())
            .unwrap();
        let result = validator
            .validate("query { nonexistent }".to_string())
            .unwrap();
        assert!(!result.is_valid());
        assert!(result.error_count() > 0);
    }

    #[test]
    fn test_schema_validation() {
        let result = validate_schema("type Query { hello: String }".to_string());
        assert!(result.is_valid());

        // Syntax error - invalid schema
        let invalid_result = validate_schema("type Query {".to_string());
        assert!(!invalid_result.is_valid());
    }
}
