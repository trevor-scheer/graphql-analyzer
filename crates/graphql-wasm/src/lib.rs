//! WebAssembly bindings for GraphQL validation and linting.
//!
//! This crate provides JavaScript bindings for the GraphQL language service,
//! enabling browser-based validation and linting without a server.
//!
//! # Example
//!
//! ```javascript
//! import init, { GraphQLValidator, ValidationResult } from '@graphql-lsp/wasm';
//!
//! async function main() {
//!     await init();
//!
//!     const validator = new GraphQLValidator();
//!     validator.setSchema(`
//!         type Query {
//!             hello: String
//!         }
//!     `);
//!
//!     const result = validator.validate(`
//!         query {
//!             hello
//!         }
//!     `);
//!
//!     console.log(result.errors); // []
//!     console.log(result.isValid()); // true
//! }
//! ```

// For WASM bindings, #[must_use] doesn't make sense since JS controls return value usage
#![allow(clippy::must_use_candidate)]
// These are public WASM bindings, not library APIs
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
// wasm_bindgen generates unsafe code in impls, but serde derive is safe for our use case
#![allow(clippy::unsafe_derive_deserialize)]

use graphql_ide::{AnalysisHost, DiagnosticSeverity, FileKind, FilePath};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Initialize the WASM module with panic hooks for better error messages.
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// A diagnostic message from validation or linting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen(getter_with_clone)]
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

#[wasm_bindgen]
impl Diagnostic {
    /// Create a new diagnostic.
    #[wasm_bindgen(constructor)]
    pub fn new(
        message: String,
        severity: String,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        code: Option<String>,
    ) -> Diagnostic {
        Diagnostic {
            message,
            severity,
            start_line,
            start_column,
            end_line,
            end_column,
            code,
        }
    }

    /// Check if this is an error.
    #[wasm_bindgen(js_name = isError)]
    pub fn is_error(&self) -> bool {
        self.severity == "error"
    }

    /// Check if this is a warning.
    #[wasm_bindgen(js_name = isWarning)]
    pub fn is_warning(&self) -> bool {
        self.severity == "warning"
    }
}

/// Result of a validation or lint operation.
#[wasm_bindgen]
pub struct ValidationResult {
    diagnostics: Vec<Diagnostic>,
}

#[wasm_bindgen]
impl ValidationResult {
    /// Get all diagnostics as a JavaScript array.
    #[wasm_bindgen(getter)]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.clone()
    }

    /// Get only error diagnostics.
    #[wasm_bindgen(getter)]
    pub fn errors(&self) -> Vec<Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.is_error())
            .cloned()
            .collect()
    }

    /// Get only warning diagnostics.
    #[wasm_bindgen(getter)]
    pub fn warnings(&self) -> Vec<Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.is_warning())
            .cloned()
            .collect()
    }

    /// Get the total number of diagnostics.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.diagnostics.len()
    }

    /// Get the number of errors.
    #[wasm_bindgen(js_name = errorCount, getter)]
    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }

    /// Get the number of warnings.
    #[wasm_bindgen(js_name = warningCount, getter)]
    pub fn warning_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_warning()).count()
    }

    /// Check if there are no errors (validation passed).
    #[wasm_bindgen(js_name = isValid)]
    pub fn is_valid(&self) -> bool {
        !self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// Check if there are any diagnostics.
    #[wasm_bindgen(js_name = hasDiagnostics)]
    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// A GraphQL validator that can validate documents against a schema.
#[wasm_bindgen]
pub struct GraphQLValidator {
    host: AnalysisHost,
    schema_loaded: bool,
}

#[wasm_bindgen]
impl GraphQLValidator {
    /// Create a new validator instance.
    #[wasm_bindgen(constructor)]
    pub fn new() -> GraphQLValidator {
        GraphQLValidator {
            host: AnalysisHost::new(),
            schema_loaded: false,
        }
    }

    /// Set the GraphQL schema (SDL format).
    ///
    /// This must be called before validating documents.
    #[wasm_bindgen(js_name = setSchema)]
    pub fn set_schema(&mut self, schema_sdl: &str) -> Result<(), JsValue> {
        let schema_path = FilePath::new("schema.graphql");
        self.host
            .add_file(&schema_path, schema_sdl, FileKind::Schema);
        self.host.rebuild_project_files();
        self.schema_loaded = true;
        Ok(())
    }

    /// Check if a schema has been loaded.
    #[wasm_bindgen(js_name = hasSchema, getter)]
    pub fn has_schema(&self) -> bool {
        self.schema_loaded
    }

    /// Validate a GraphQL document against the schema.
    ///
    /// Returns validation errors (GraphQL spec violations).
    #[wasm_bindgen]
    pub fn validate(&mut self, document: &str) -> Result<ValidationResult, JsValue> {
        if !self.schema_loaded {
            return Err(JsValue::from_str(
                "No schema loaded. Call setSchema() first.",
            ));
        }

        let doc_path = FilePath::new("document.graphql");
        self.host
            .add_file(&doc_path, document, FileKind::ExecutableGraphQL);

        let snapshot = self.host.snapshot();
        let diagnostics = snapshot.validation_diagnostics(&doc_path);

        let result_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(|d| Diagnostic {
                message: d.message,
                severity: severity_to_string(d.severity),
                start_line: d.range.start.line,
                start_column: d.range.start.character,
                end_line: d.range.end.line,
                end_column: d.range.end.character,
                code: d.code,
            })
            .collect();

        Ok(ValidationResult {
            diagnostics: result_diagnostics,
        })
    }

    /// Run lint rules on a GraphQL document.
    ///
    /// Returns lint diagnostics (custom rule violations).
    #[wasm_bindgen]
    pub fn lint(&mut self, document: &str) -> Result<ValidationResult, JsValue> {
        if !self.schema_loaded {
            return Err(JsValue::from_str(
                "No schema loaded. Call setSchema() first.",
            ));
        }

        let doc_path = FilePath::new("document.graphql");
        self.host
            .add_file(&doc_path, document, FileKind::ExecutableGraphQL);

        let snapshot = self.host.snapshot();
        let diagnostics = snapshot.lint_diagnostics(&doc_path);

        let result_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(|d| Diagnostic {
                message: d.message,
                severity: severity_to_string(d.severity),
                start_line: d.range.start.line,
                start_column: d.range.start.character,
                end_line: d.range.end.line,
                end_column: d.range.end.character,
                code: d.code,
            })
            .collect();

        Ok(ValidationResult {
            diagnostics: result_diagnostics,
        })
    }

    /// Run both validation and lint rules on a GraphQL document.
    ///
    /// Returns all diagnostics (both spec violations and lint issues).
    #[wasm_bindgen]
    pub fn check(&mut self, document: &str) -> Result<ValidationResult, JsValue> {
        if !self.schema_loaded {
            return Err(JsValue::from_str(
                "No schema loaded. Call setSchema() first.",
            ));
        }

        let doc_path = FilePath::new("document.graphql");
        self.host
            .add_file(&doc_path, document, FileKind::ExecutableGraphQL);

        let snapshot = self.host.snapshot();

        let mut all_diagnostics = Vec::new();

        // Validation diagnostics
        for d in snapshot.validation_diagnostics(&doc_path) {
            all_diagnostics.push(Diagnostic {
                message: d.message,
                severity: severity_to_string(d.severity),
                start_line: d.range.start.line,
                start_column: d.range.start.character,
                end_line: d.range.end.line,
                end_column: d.range.end.character,
                code: d.code,
            });
        }

        // Lint diagnostics
        for d in snapshot.lint_diagnostics(&doc_path) {
            all_diagnostics.push(Diagnostic {
                message: d.message,
                severity: severity_to_string(d.severity),
                start_line: d.range.start.line,
                start_column: d.range.start.character,
                end_line: d.range.end.line,
                end_column: d.range.end.character,
                code: d.code,
            });
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
    #[wasm_bindgen(js_name = configureLint)]
    pub fn configure_lint(&mut self, config: JsValue) -> Result<(), JsValue> {
        let config_value: serde_json::Value = serde_wasm_bindgen::from_value(config)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let lint_config: graphql_linter::LintConfig = serde_json::from_value(config_value)
            .map_err(|e| JsValue::from_str(&format!("Invalid lint config: {e}")))?;

        self.host.set_lint_config(lint_config);
        Ok(())
    }

    /// Reset the validator to its initial state.
    #[wasm_bindgen]
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

/// Convert a diagnostic severity to a string.
fn severity_to_string(severity: DiagnosticSeverity) -> String {
    match severity {
        DiagnosticSeverity::Error => "error".to_string(),
        DiagnosticSeverity::Warning => "warning".to_string(),
        DiagnosticSeverity::Information => "info".to_string(),
        DiagnosticSeverity::Hint => "hint".to_string(),
    }
}

/// Quick validation function for one-off checks.
///
/// This is a convenience function that creates a temporary validator,
/// loads the schema, and validates the document in one call.
#[wasm_bindgen(js_name = quickValidate)]
pub fn quick_validate(schema_sdl: &str, document: &str) -> Result<ValidationResult, JsValue> {
    let mut validator = GraphQLValidator::new();
    validator.set_schema(schema_sdl)?;
    validator.validate(document)
}

/// Quick lint function for one-off checks.
#[wasm_bindgen(js_name = quickLint)]
pub fn quick_lint(schema_sdl: &str, document: &str) -> Result<ValidationResult, JsValue> {
    let mut validator = GraphQLValidator::new();
    validator.set_schema(schema_sdl)?;
    validator.lint(document)
}

/// Quick check function for one-off checks (validation + lint).
#[wasm_bindgen(js_name = quickCheck)]
pub fn quick_check(schema_sdl: &str, document: &str) -> Result<ValidationResult, JsValue> {
    let mut validator = GraphQLValidator::new();
    validator.set_schema(schema_sdl)?;
    validator.check(document)
}

/// Parse a GraphQL schema and return validation errors.
///
/// This only validates the schema itself, not documents against it.
#[wasm_bindgen(js_name = validateSchema)]
pub fn validate_schema(schema_sdl: &str) -> ValidationResult {
    let mut host = AnalysisHost::new();
    let schema_path = FilePath::new("schema.graphql");
    host.add_file(&schema_path, schema_sdl, FileKind::Schema);
    host.rebuild_project_files();

    let snapshot = host.snapshot();
    let diagnostics = snapshot.validation_diagnostics(&schema_path);

    let result_diagnostics: Vec<Diagnostic> = diagnostics
        .into_iter()
        .map(|d| Diagnostic {
            message: d.message,
            severity: severity_to_string(d.severity),
            start_line: d.range.start.line,
            start_column: d.range.start.character,
            end_line: d.range.end.line,
            end_column: d.range.end.character,
            code: d.code,
        })
        .collect();

    ValidationResult {
        diagnostics: result_diagnostics,
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
            .set_schema("type Query { hello: String }")
            .unwrap();
        assert!(validator.has_schema());
    }

    #[test]
    fn test_valid_document() {
        let mut validator = GraphQLValidator::new();
        validator
            .set_schema("type Query { hello: String }")
            .unwrap();
        let result = validator.validate("query { hello }").unwrap();
        assert!(result.is_valid());
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn test_invalid_document() {
        let mut validator = GraphQLValidator::new();
        validator
            .set_schema("type Query { hello: String }")
            .unwrap();
        let result = validator.validate("query { nonexistent }").unwrap();
        assert!(!result.is_valid());
        assert!(result.error_count() > 0);
    }

    #[test]
    fn test_schema_validation() {
        let result = validate_schema("type Query { hello: String }");
        assert!(result.is_valid());

        // Syntax error - invalid schema
        let invalid_result = validate_schema("type Query {");
        assert!(!invalid_result.is_valid());
    }
}
