use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use apollo_compiler::diagnostic::ToCliReport;
use apollo_compiler::parser::Parser;
use apollo_compiler::validation::DiagnosticList;
use graphql_base_db::ExtractionOffset;
use std::collections::HashMap;
use std::sync::Arc;

/// Diagnostics grouped by file URI
pub type DiagnosticsByFile = Arc<HashMap<Arc<str>, Vec<Diagnostic>>>;

/// Result of merging schema files - includes both the schema (if valid) and any diagnostics
#[derive(Clone, Debug, PartialEq)]
pub struct MergedSchemaResult {
    /// The merged schema, if validation succeeded
    pub schema: Option<Arc<apollo_compiler::Schema>>,
    /// Validation diagnostics grouped by file URI
    pub diagnostics_by_file: DiagnosticsByFile,
}

/// Convert apollo-compiler diagnostics to our diagnostic format, grouped by file URI.
///
/// The `uri_offsets` map provides extraction offset information for files that contain
/// GraphQL extracted from host files (e.g., TypeScript). When a file has a non-zero offset,
/// diagnostic positions are adjusted to report correct positions in the original file.
fn collect_apollo_diagnostics(
    errors: &DiagnosticList,
    uri_offsets: &HashMap<Arc<str>, ExtractionOffset>,
) -> HashMap<Arc<str>, Vec<Diagnostic>> {
    let mut diagnostics_by_file: HashMap<Arc<str>, Vec<Diagnostic>> = HashMap::new();

    for apollo_diag in errors.iter() {
        let Some(file_uri) = apollo_diag.error.location().and_then(|location| {
            let file_id = location.file_id();
            apollo_diag
                .sources
                .get(&file_id)
                .map(|source_file| Arc::from(source_file.path().to_string_lossy().to_string()))
        }) else {
            continue;
        };

        // Get extraction offset for this file (if any)
        let offset = uri_offsets
            .get(&file_uri)
            .copied()
            .unwrap_or(ExtractionOffset::default());

        let range = if let Some(loc_range) = apollo_diag.line_column_range() {
            // Apollo-compiler uses 1-indexed lines, we use 0-indexed
            let start_line = loc_range.start.line.saturating_sub(1) as u32;
            let end_line = loc_range.end.line.saturating_sub(1) as u32;
            let start_col = loc_range.start.column.saturating_sub(1) as u32;
            let end_col = loc_range.end.column.saturating_sub(1) as u32;

            // Apply extraction offset to positions.
            // Line offset applies to all lines.
            // Column offset only applies to the first line (line 0 in the extracted content).
            DiagnosticRange {
                start: Position {
                    line: start_line + offset.line,
                    character: if start_line == 0 {
                        start_col + offset.column
                    } else {
                        start_col
                    },
                },
                end: Position {
                    line: end_line + offset.line,
                    character: if end_line == 0 {
                        end_col + offset.column
                    } else {
                        end_col
                    },
                },
            }
        } else {
            DiagnosticRange::default()
        };

        let message: Arc<str> = Arc::from(apollo_diag.error.to_string());

        diagnostics_by_file
            .entry(file_uri)
            .or_default()
            .push(Diagnostic {
                severity: Severity::Error,
                message,
                range,
                source: "apollo-compiler".into(),
                code: None,
            });
    }

    diagnostics_by_file
}

/// Merge all schema files into a single `apollo_compiler::Schema` and collect validation errors.
///
/// This is the primary function for schema merging. It returns both the merged schema
/// and any validation diagnostics, grouped by file URI for efficient per-file lookup.
///
/// This query depends ONLY on schema file IDs and their content, not document files.
/// Changing document files will not invalidate this query.
///
/// Validation includes:
/// - Interface implementation validation (types must implement all interface fields)
/// - Union member validation (union members must be object types)
/// - Type reference validation
/// - Duplicate definition detection
#[salsa::tracked]
pub fn merged_schema_with_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> MergedSchemaResult {
    tracing::debug!("merged_schema: Starting schema merge with diagnostics");
    let schema_ids = project_files.schema_file_ids(db).ids(db);
    tracing::debug!(schema_file_count = schema_ids.len(), "Found schema files");

    if schema_ids.is_empty() {
        tracing::debug!("No schema files found in project - returning empty result");
        return MergedSchemaResult {
            schema: None,
            diagnostics_by_file: Arc::new(HashMap::new()),
        };
    }

    let mut builder = apollo_compiler::schema::SchemaBuilder::new();
    let mut parser = Parser::new();
    // Track extraction offsets for files with embedded GraphQL
    let mut uri_offsets: HashMap<Arc<str>, ExtractionOffset> = HashMap::new();

    for file_id in schema_ids.iter() {
        let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        else {
            continue;
        };
        let text = content.text(db);
        let uri = metadata.uri(db);
        let extraction_offset = metadata.extraction_offset(db);

        // Store extraction offset if non-zero (for files with embedded GraphQL)
        if !extraction_offset.is_zero() {
            uri_offsets.insert(Arc::from(uri.as_str()), extraction_offset);
        }

        tracing::debug!(uri = ?uri, "Adding schema file to merge");
        parser.parse_into_schema_builder(text.as_ref(), uri.as_str(), &mut builder);
    }

    match builder.build() {
        Ok(schema) => {
            // SchemaBuilder::build() is lenient - it succeeds even with validation errors.
            // We call validate() to catch semantic issues like:
            // - Missing interface field implementations
            // - Union members that aren't object types
            // - Invalid type references
            //
            // IMPORTANT: We still return the schema even if validation fails, because
            // we need it for document validation. A schema without a Query type or with
            // minor issues should still allow fragment and operation validation.
            match schema.validate() {
                Ok(valid_schema) => {
                    tracing::debug!(
                        type_count = valid_schema.types.len(),
                        "Successfully merged and validated schema"
                    );
                    MergedSchemaResult {
                        schema: Some(Arc::new(valid_schema.into_inner())),
                        diagnostics_by_file: Arc::new(HashMap::new()),
                    }
                }
                Err(with_errors) => {
                    tracing::warn!(
                        error_count = with_errors.errors.len(),
                        "Schema validation errors found (schema still usable for document validation)"
                    );
                    let diagnostics_by_file =
                        collect_apollo_diagnostics(&with_errors.errors, &uri_offsets);
                    MergedSchemaResult {
                        schema: Some(Arc::new(with_errors.partial)),
                        diagnostics_by_file: Arc::new(diagnostics_by_file),
                    }
                }
            }
        }
        Err(with_errors) => {
            tracing::warn!(
                error_count = with_errors.errors.len(),
                "Failed to merge schema due to build errors"
            );
            let diagnostics_by_file = collect_apollo_diagnostics(&with_errors.errors, &uri_offsets);
            MergedSchemaResult {
                schema: None,
                diagnostics_by_file: Arc::new(diagnostics_by_file),
            }
        }
    }
}

/// Get merged schema diagnostics for a specific file.
///
/// Returns only the validation errors that originate from the specified file.
/// This is an O(1) lookup into the cached diagnostics `HashMap`.
pub fn merged_schema_diagnostics_for_file(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
    file_uri: &str,
) -> Vec<Diagnostic> {
    let result = merged_schema_with_diagnostics(db, project_files);
    result
        .diagnostics_by_file
        .get(file_uri)
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merged_schema_result_default_values() {
        let result = MergedSchemaResult {
            schema: None,
            diagnostics_by_file: Arc::new(HashMap::new()),
        };
        assert!(result.schema.is_none());
        assert!(result.diagnostics_by_file.is_empty());
    }

    #[test]
    fn test_merged_schema_result_with_schema() {
        let schema = apollo_compiler::Schema::builder()
            .parse("type Query { hello: String }", "schema.graphql")
            .build()
            .unwrap();

        let result = MergedSchemaResult {
            schema: Some(Arc::new(schema)),
            diagnostics_by_file: Arc::new(HashMap::new()),
        };
        assert!(result.schema.is_some());
        assert!(result.diagnostics_by_file.is_empty());
    }

    #[test]
    fn test_merged_schema_result_with_diagnostics() {
        let mut diagnostics = HashMap::new();
        diagnostics.insert(
            Arc::from("file:///schema.graphql"),
            vec![Diagnostic {
                severity: Severity::Error,
                message: Arc::from("Test error"),
                range: DiagnosticRange::default(),
                source: "test".into(),
                code: None,
            }],
        );

        let result = MergedSchemaResult {
            schema: None,
            diagnostics_by_file: Arc::new(diagnostics),
        };
        assert!(result.schema.is_none());
        assert_eq!(result.diagnostics_by_file.len(), 1);
    }

    #[test]
    fn test_merged_schema_result_equality() {
        let result1 = MergedSchemaResult {
            schema: None,
            diagnostics_by_file: Arc::new(HashMap::new()),
        };
        let result2 = MergedSchemaResult {
            schema: None,
            diagnostics_by_file: Arc::new(HashMap::new()),
        };
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_diagnostic_range_default() {
        let range = DiagnosticRange::default();
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 0);
    }

    #[test]
    fn test_position_values() {
        let pos = Position {
            line: 10,
            character: 5,
        };
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }
}
