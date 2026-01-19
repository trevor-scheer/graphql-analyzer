use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use apollo_compiler::parser::Parser;
use apollo_compiler::validation::DiagnosticList;
use std::sync::Arc;

/// Convert apollo-compiler diagnostics to our diagnostic format
#[allow(clippy::cast_possible_truncation)]
fn collect_apollo_diagnostics(errors: &DiagnosticList) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for apollo_diag in errors.iter() {
        let range = if let Some(loc_range) = apollo_diag.line_column_range() {
            DiagnosticRange {
                start: Position {
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

    diagnostics
}

/// Result of merging schema files - includes both the schema (if valid) and any diagnostics
#[derive(Clone, Debug, PartialEq)]
pub struct MergedSchemaResult {
    /// The merged schema, if validation succeeded
    pub schema: Option<Arc<apollo_compiler::Schema>>,
    /// Validation diagnostics from the merge process
    pub diagnostics: Arc<Vec<Diagnostic>>,
}

/// Merge all schema files into a single `apollo_compiler::Schema` and collect validation errors
/// This query depends ONLY on schema file IDs and their content, not `DocumentFiles`.
/// Changing document files will not invalidate this query.
///
/// This function now performs full validation including:
/// - Interface implementation validation (types must implement all interface fields)
/// - Union member validation (union members must be object types)
/// - Type reference validation
#[salsa::tracked]
pub fn merged_schema_with_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> MergedSchemaResult {
    tracing::info!("merged_schema: Starting schema merge with diagnostics");
    let schema_ids = project_files.schema_file_ids(db).ids(db);
    tracing::info!(schema_file_count = schema_ids.len(), "Found schema files");

    if schema_ids.is_empty() {
        tracing::info!("No schema files found in project - returning empty result");
        return MergedSchemaResult {
            schema: None,
            diagnostics: Arc::new(vec![]),
        };
    }

    let mut builder = apollo_compiler::schema::SchemaBuilder::new();
    let mut parser = Parser::new();

    for file_id in schema_ids.iter() {
        // Use per-file lookup for granular caching
        let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        else {
            continue;
        };
        let text = content.text(db);
        let uri = metadata.uri(db);

        tracing::debug!(uri = ?uri, "Adding schema file to merge");

        // Parse and add to builder
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
                        diagnostics: Arc::new(vec![]),
                    }
                }
                Err(with_errors) => {
                    tracing::warn!(
                        error_count = with_errors.errors.len(),
                        "Schema validation errors found (schema still usable for document validation)"
                    );
                    let diagnostics = collect_apollo_diagnostics(&with_errors.errors);
                    // Return the schema even with validation errors so document validation can proceed
                    MergedSchemaResult {
                        schema: Some(Arc::new(with_errors.partial)),
                        diagnostics: Arc::new(diagnostics),
                    }
                }
            }
        }
        Err(with_errors) => {
            tracing::warn!(
                error_count = with_errors.errors.len(),
                "Failed to merge schema due to build errors"
            );
            let diagnostics = collect_apollo_diagnostics(&with_errors.errors);
            MergedSchemaResult {
                schema: None,
                diagnostics: Arc::new(diagnostics),
            }
        }
    }
}

/// Merge all schema files into a single `apollo_compiler::Schema`
/// This query depends ONLY on schema file IDs and their content, not `DocumentFiles`.
/// Changing document files will not invalidate this query.
///
/// **Note**: This function discards validation diagnostics. If you need schema
/// validation errors, use [`merged_schema_with_diagnostics`] instead.
#[salsa::tracked]
pub fn merged_schema_from_files(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Option<Arc<apollo_compiler::Schema>> {
    merged_schema_with_diagnostics(db, project_files).schema
}

/// Get diagnostics from merging schema files
///
/// This returns validation errors from the schema merge process, such as:
/// - Duplicate type definitions
/// - Interface implementation errors
/// - Union member validation errors
///
/// This is a separate query so callers can get diagnostics without
/// also needing the schema itself.
#[salsa::tracked]
pub fn merged_schema_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<Vec<crate::Diagnostic>> {
    merged_schema_with_diagnostics(db, project_files).diagnostics
}

/// Convenience wrapper that extracts `SchemaFiles` from `ProjectFiles`
///
/// **Note**: This function discards validation diagnostics. If you need schema
/// validation errors, use [`merged_schema_with_diagnostics`] instead.
#[salsa::tracked]
pub fn merged_schema(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Option<Arc<apollo_compiler::Schema>> {
    merged_schema_from_files(db, project_files)
}
