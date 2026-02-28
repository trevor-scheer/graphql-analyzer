// GraphQL Analysis Layer
// This crate provides validation and linting on top of the HIR layer.
// All validation is query-based for automatic incrementality via Salsa.

use std::sync::Arc;

mod diagnostics;
mod document_validation;
pub mod lint_integration;
pub mod merged_schema;
mod project_lints;
pub mod validation;

pub use diagnostics::*;
pub use document_validation::validate_document_file;
pub use merged_schema::{
    merged_schema_diagnostics_for_file, merged_schema_with_diagnostics, DiagnosticsByFile,
    MergedSchemaResult,
};
pub use project_lints::{
    analyze_field_usage, field_usage_for_type, find_unused_fields, find_unused_fragments,
    FieldCoverageReport, FieldUsage, TypeCoverage,
};
pub use validation::validate_file;

#[salsa::db]
pub trait GraphQLAnalysisDatabase: graphql_hir::GraphQLHirDatabase {
    fn lint_config(&self) -> Arc<graphql_linter::LintConfig> {
        Arc::new(graphql_linter::LintConfig::default())
    }
}

/// Get validation diagnostics for a file, including syntax errors and
/// validation errors.
///
/// This is the public API that accepts an optional `ProjectFiles`.
/// When `project_files` is `None`, only syntax errors are returned.
pub fn file_validation_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
    project_files: Option<graphql_base_db::ProjectFiles>,
) -> Arc<Vec<Diagnostic>> {
    // Without project files, we can only report syntax errors
    project_files.map_or_else(
        || syntax_diagnostics(db, content, metadata),
        |pf| file_validation_diagnostics_impl(db, content, metadata, pf),
    )
}

/// Get only syntax errors for a file (no validation against schema)
#[salsa::tracked]
fn syntax_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    let parse = graphql_syntax::parse(db, content, metadata);
    let line_index = graphql_syntax::line_index(db, content);

    for error in parse.errors() {
        let (line, col) = line_index.line_col(error.offset);

        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: error.message.clone().into(),
            range: DiagnosticRange {
                start: Position {
                    line: line as u32,
                    character: col as u32,
                },
                end: Position {
                    line: line as u32,
                    character: col as u32,
                },
            },
            source: "graphql-parser".into(),
            code: None,
        });
    }

    Arc::new(diagnostics)
}

/// Internal tracked function for validation with project files
#[salsa::tracked]
fn file_validation_diagnostics_impl(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    let parse = graphql_syntax::parse(db, content, metadata);
    let line_index = graphql_syntax::line_index(db, content);

    for error in parse.errors() {
        let (line, col) = line_index.line_col(error.offset);

        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: error.message.clone().into(),
            range: DiagnosticRange {
                start: Position {
                    line: line as u32,
                    character: col as u32,
                },
                end: Position {
                    line: line as u32,
                    character: col as u32,
                },
            },
            source: "graphql-parser".into(),
            code: None,
        });
    }

    let document_kind = metadata.document_kind(db);
    tracing::debug!(
        uri = ?metadata.uri(db),
        ?document_kind,
        "Determining validation path for file"
    );

    if metadata.is_schema(db) {
        // Schema files only need syntax validation (handled above) plus merged schema diagnostics.
        // Individual schema files don't need to be spec-valid on their own - only the
        // merged schema needs spec validation. Filter to only show errors from this file.
        let file_uri = metadata.uri(db);
        let schema_diagnostics =
            merged_schema::merged_schema_diagnostics_for_file(db, project_files, file_uri.as_str());
        diagnostics.extend(schema_diagnostics);
    } else if metadata.is_document(db) {
        tracing::debug!("Running document validation");
        let doc_diagnostics = validation::validate_file(db, content, metadata, project_files);
        tracing::debug!(
            document_diagnostic_count = doc_diagnostics.len(),
            "Document validation completed"
        );
        diagnostics.extend(doc_diagnostics.iter().cloned());
    }

    Arc::new(diagnostics)
}

/// Get all diagnostics for a file (validation + linting)
///
/// This is the public API that accepts an optional `ProjectFiles`.
/// When `project_files` is `None`, only syntax errors are returned.
/// Memoization happens at the tracked `file_diagnostics_impl` function.
pub fn file_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
    project_files: Option<graphql_base_db::ProjectFiles>,
) -> Arc<Vec<Diagnostic>> {
    project_files.map_or_else(
        || syntax_diagnostics(db, content, metadata),
        |pf| file_diagnostics_impl(db, content, metadata, pf),
    )
}

/// Internal tracked function that combines validation and linting
#[salsa::tracked]
fn file_diagnostics_impl(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    diagnostics.extend(
        file_validation_diagnostics_impl(db, content, metadata, project_files)
            .iter()
            .cloned(),
    );

    diagnostics.extend(
        lint_integration::lint_file_with_project(db, content, metadata, project_files)
            .iter()
            .cloned(),
    );

    Arc::new(diagnostics)
}
