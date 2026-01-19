// Schema validation queries using apollo-compiler

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use apollo_compiler::parser::Parser;
use graphql_base_db::{FileContent, FileMetadata};
use std::sync::Arc;

/// Validate a schema file using apollo-compiler
/// This provides comprehensive GraphQL spec validation including:
/// - Syntax validation
/// - Duplicate type names
/// - Type reference validation
/// - Interface implementation validation
/// - Union member validation
/// - Directive validation
/// - And all other GraphQL schema validation rules
#[salsa::tracked]
pub fn validate_schema_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let text = content.text(db);
    let uri = metadata.uri(db);

    // Use apollo-compiler's SchemaBuilder for validation
    // This provides full spec-compliant schema validation
    let mut builder = apollo_compiler::schema::SchemaBuilder::new();
    let mut parser = Parser::new();

    parser.parse_into_schema_builder(text.as_ref(), uri.as_str(), &mut builder);

    match builder.build() {
        Ok(_schema) => {
            tracing::debug!(uri = ?uri, "Schema file validated successfully");
        }
        Err(with_errors) => {
            tracing::debug!(
                uri = ?uri,
                error_count = with_errors.errors.len(),
                "Schema validation failed"
            );

            #[allow(clippy::cast_possible_truncation, clippy::option_if_let_else)]
            for apollo_diag in with_errors.errors.iter() {
                let range = if let Some(loc_range) = apollo_diag.line_column_range() {
                    DiagnosticRange {
                        start: Position {
                            // apollo-compiler uses 1-indexed, we use 0-indexed
                            line: loc_range.start.line.saturating_sub(1) as u32,
                            character: loc_range.start.column.saturating_sub(1) as u32,
                        },
                        end: Position {
                            line: loc_range.end.line.saturating_sub(1) as u32,
                            character: loc_range.end.column.saturating_sub(1) as u32,
                        },
                    }
                } else {
                    DiagnosticRange::default()
                };

                let message: Arc<str> = Arc::from(apollo_diag.error.to_string());

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message,
                    range,
                    source: "apollo-compiler".into(),
                    code: None,
                });
            }
        }
    }

    Arc::new(diagnostics)
}
