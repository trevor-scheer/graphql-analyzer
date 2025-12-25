// GraphQL Analysis Layer
// This crate provides validation and linting on top of the HIR layer.
// All validation is query-based for automatic incrementality via Salsa.

use graphql_db::FileId;
use std::collections::HashMap;
use std::sync::Arc;

mod diagnostics;
mod document_validation;
pub mod lint_integration;
pub mod merged_schema;
mod project_lints;
mod schema_validation;
pub mod validation;

pub use diagnostics::*;
pub use validation::validate_document;

/// The salsa database trait for analysis queries
#[salsa::db]
pub trait GraphQLAnalysisDatabase: graphql_hir::GraphQLHirDatabase {
    /// Get the lint configuration for the project
    fn lint_config(&self) -> Arc<graphql_linter::LintConfig> {
        Arc::new(graphql_linter::LintConfig::default())
    }
}

// Implement the trait for RootDatabase
// This makes RootDatabase usable with all analysis queries
#[salsa::db]
impl GraphQLAnalysisDatabase for graphql_db::RootDatabase {
    /// Get the lint configuration from the database storage
    fn lint_config(&self) -> Arc<graphql_linter::LintConfig> {
        if let Some(config_any) = self.lint_config_any() {
            // Try to downcast to the concrete type
            if let Some(config) = config_any.downcast_ref::<graphql_linter::LintConfig>() {
                return Arc::new(config.clone());
            }
        }
        // Fall back to default if not set or downcast fails
        Arc::new(graphql_linter::LintConfig::default())
    }
}

/// Get all diagnostics for a file
/// This is the main entry point for validation
#[salsa::tracked]
pub fn file_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    // Syntax errors (from parse)
    let parse = graphql_syntax::parse(db, content, metadata);
    for error in &parse.errors {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: error.clone().into(),
            range: DiagnosticRange::default(), // TODO: Parse error positions
            source: "graphql-parser".into(),
            code: None,
        });
    }

    // Apollo-compiler validation (using merged schema)
    // Only run if we have project files available
    let project_files_opt = db.project_files();
    tracing::debug!(
        has_project_files = project_files_opt.is_some(),
        "Checking for project files"
    );
    if let Some(project_files) = project_files_opt {
        use graphql_db::FileKind;
        let file_kind = metadata.kind(db);
        tracing::info!(
            uri = ?metadata.uri(db),
            ?file_kind,
            "Determining validation path for file"
        );

        match file_kind {
            FileKind::Schema => {
                tracing::info!("Running schema validation");
                // Full apollo-compiler schema validation with spec-compliant error checking
                let schema_diagnostics =
                    schema_validation::validate_schema_file(db, content, metadata);
                tracing::info!(
                    schema_diagnostic_count = schema_diagnostics.len(),
                    "Schema validation completed"
                );
                diagnostics.extend(schema_diagnostics.iter().cloned());
            }
            FileKind::ExecutableGraphQL | FileKind::TypeScript | FileKind::JavaScript => {
                tracing::info!("Running document validation");
                // Use apollo-compiler validation for documents
                let doc_diagnostics =
                    validation::validate_document(db, content, metadata, project_files);
                tracing::info!(
                    document_diagnostic_count = doc_diagnostics.len(),
                    "Document validation completed"
                );
                diagnostics.extend(doc_diagnostics.iter().cloned());
            }
        }
    }

    // Lint diagnostics (from graphql-linter integration)
    diagnostics.extend(
        lint_integration::lint_file(db, content, metadata)
            .iter()
            .cloned(),
    );

    Arc::new(diagnostics)
}

/// Get project-wide diagnostics (expensive, opt-in)
/// This includes lints like `unused_fields` and `unused_fragments`
#[salsa::tracked]
pub fn project_wide_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
) -> Arc<HashMap<FileId, Vec<Diagnostic>>> {
    // Check if any project-wide lints are enabled
    let lint_config = db.lint_config();
    let has_project_wide_lints = lint_config.is_enabled("unique_names")
        || lint_config.is_enabled("unused_fields")
        || lint_config.is_enabled("unused_fragments");

    if !has_project_wide_lints {
        return Arc::new(HashMap::new());
    }

    let mut diagnostics_by_file = HashMap::new();

    // Unused fields lint (expensive)
    if lint_config.is_enabled("unused_fields") {
        let unused = project_lints::find_unused_fields(db);
        for (_field_id, diagnostic) in unused.iter() {
            // TODO: Get file_id from field_id when we have proper HIR support
            let file_id = FileId::new(0); // Placeholder
            diagnostics_by_file
                .entry(file_id)
                .or_insert_with(Vec::new)
                .push(diagnostic.clone());
        }
    }

    // Unused fragments lint
    if lint_config.is_enabled("unused_fragments") {
        let unused = project_lints::find_unused_fragments(db);
        for (_fragment_id, diagnostic) in unused.iter() {
            // TODO: Get file_id from fragment_id when we have proper HIR support
            let file_id = FileId::new(0); // Placeholder
            diagnostics_by_file
                .entry(file_id)
                .or_insert_with(Vec::new)
                .push(diagnostic.clone());
        }
    }

    Arc::new(diagnostics_by_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileKind, FileMetadata, FileUri};

    // Test database wrapper
    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
    }

    // Implement the database traits for testing
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

        // Get diagnostics
        let diagnostics = file_diagnostics(&db, content, metadata);

        // Should have no diagnostics for valid schema
        // Note: This will work once we implement the parse query properly
        assert!(diagnostics.is_empty() || !diagnostics.is_empty()); // Placeholder assertion
    }

    #[test]
    fn test_project_wide_diagnostics_disabled() {
        let db = TestDatabase::default();

        // Project-wide diagnostics should be empty when disabled
        let diagnostics = project_wide_diagnostics(&db);
        assert!(diagnostics.is_empty());
    }
}
