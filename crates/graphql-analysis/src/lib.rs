// GraphQL Analysis Layer
// This crate provides validation and linting on top of the HIR layer.
// All validation is query-based for automatic incrementality via Salsa.

use std::sync::Arc;

mod diagnostics;
mod document_validation;
pub mod lint_integration;
pub mod merged_schema;
mod project_lints;
mod schema_validation;
pub mod validation;

pub use diagnostics::*;
pub use merged_schema::{
    merged_schema_diagnostics, merged_schema_with_diagnostics, MergedSchemaResult,
};
pub use project_lints::{analyze_field_usage, FieldCoverageReport, FieldUsage, TypeCoverage};
pub use validation::validate_file;

#[salsa::db]
pub trait GraphQLAnalysisDatabase: graphql_hir::GraphQLHirDatabase {
    fn lint_config(&self) -> Arc<graphql_linter::LintConfig> {
        Arc::new(graphql_linter::LintConfig::default())
    }
}

#[salsa::db]
impl GraphQLAnalysisDatabase for graphql_db::RootDatabase {}

/// Get validation diagnostics for a file, including syntax errors and
/// validation errors.
///
/// This is the public API that accepts an optional `ProjectFiles`.
/// When `project_files` is `None`, only syntax errors are returned.
#[allow(clippy::cast_possible_truncation)] // Line and column numbers won't exceed u32::MAX
pub fn file_validation_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
    project_files: Option<graphql_db::ProjectFiles>,
) -> Arc<Vec<Diagnostic>> {
    // Without project files, we can only report syntax errors
    project_files.map_or_else(
        || syntax_diagnostics(db, content, metadata),
        |pf| file_validation_diagnostics_impl(db, content, metadata, pf),
    )
}

/// Get only syntax errors for a file (no validation against schema)
#[salsa::tracked]
#[allow(clippy::cast_possible_truncation)]
fn syntax_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
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
#[allow(clippy::cast_possible_truncation)]
fn file_validation_diagnostics_impl(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
    project_files: graphql_db::ProjectFiles,
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

    let file_kind = metadata.kind(db);
    tracing::info!(
        uri = ?metadata.uri(db),
        ?file_kind,
        "Determining validation path for file"
    );

    if file_kind.is_schema() {
        tracing::info!("Running schema validation");
        let schema_diagnostics = schema_validation::validate_schema_file(db, content, metadata);
        tracing::info!(
            schema_diagnostic_count = schema_diagnostics.len(),
            "Schema validation completed"
        );
        diagnostics.extend(schema_diagnostics.iter().cloned());
    } else if file_kind.is_document() {
        tracing::info!("Running document validation");
        let doc_diagnostics = validation::validate_file(db, content, metadata, project_files);
        tracing::info!(
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
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
    project_files: Option<graphql_db::ProjectFiles>,
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
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
    project_files: graphql_db::ProjectFiles,
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

#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileId, FileKind, FileMetadata, FileUri};

    // TestDatabase for graphql-analysis tests.
    // Note: We can't use graphql_test_utils::TestDatabase here because it would
    // create a cyclic dependency (graphql-test-utils depends on graphql-analysis).
    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
    }

    #[salsa::db]
    impl salsa::Database for TestDatabase {}

    #[salsa::db]
    impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabase {}

    #[salsa::db]
    impl graphql_hir::GraphQLHirDatabase for TestDatabase {}

    #[salsa::db]
    impl GraphQLAnalysisDatabase for TestDatabase {}

    #[test]
    fn test_file_diagnostics_empty() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);

        // Create a valid schema file
        let content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::Schema,
        );

        // Get diagnostics (no project_files, so only syntax errors would be reported)
        let diagnostics = file_diagnostics(&db, content, metadata, None);

        // Valid schema should have no syntax errors
        assert!(
            diagnostics.is_empty(),
            "Valid schema should have no diagnostics, got: {diagnostics:?}"
        );
    }
}
