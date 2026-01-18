//! Integration tests for the GraphQL linter
//!
//! These tests verify end-to-end linting functionality across
//! the graphql-linter, graphql-hir, and graphql-syntax crates.

use graphql_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, RootDatabase};
use graphql_linter::{standalone_document_rules, LintConfig};
use std::sync::Arc;

/// Create empty project files for standalone document linting
fn create_empty_project_files(db: &RootDatabase) -> graphql_db::ProjectFiles {
    let schema_file_ids = graphql_db::SchemaFileIds::new(db, Arc::new(vec![]));
    let document_file_ids = graphql_db::DocumentFileIds::new(db, Arc::new(vec![]));
    let file_entry_map =
        graphql_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
    graphql_db::ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
}

/// Run all standalone document rules on a source
fn run_standalone_rules(
    db: &RootDatabase,
    source: &str,
    config: &LintConfig,
) -> Vec<graphql_linter::LintDiagnostic> {
    let file_id = FileId::new(0);
    let content = FileContent::new(db, Arc::from(source));
    let metadata = FileMetadata::new(
        db,
        file_id,
        FileUri::new("test.graphql"),
        FileKind::ExecutableGraphQL,
    );
    let project_files = create_empty_project_files(db);

    let rules = standalone_document_rules();
    let mut all_diagnostics = Vec::new();

    for rule in rules {
        if config.is_enabled(rule.name()) {
            let diagnostics = rule.check(db, file_id, content, metadata, project_files);
            all_diagnostics.extend(diagnostics);
        }
    }

    all_diagnostics
}

#[test]
fn test_lint_anonymous_operation() {
    let db = RootDatabase::default();
    let source = "query { user { id } }";
    let config = LintConfig::recommended();

    let diagnostics = run_standalone_rules(&db, source, &config);

    let anon_warning = diagnostics
        .iter()
        .find(|d| d.message.contains("Anonymous"));

    assert!(
        anon_warning.is_some(),
        "Expected warning about anonymous operation. Got: {diagnostics:?}"
    );
}

#[test]
fn test_lint_named_operation_passes() {
    let db = RootDatabase::default();
    let source = "query GetUser { user { id } }";
    let config = LintConfig::recommended();

    let diagnostics = run_standalone_rules(&db, source, &config);

    let anon_warning = diagnostics
        .iter()
        .find(|d| d.message.contains("Anonymous"));

    assert!(
        anon_warning.is_none(),
        "Named operations should not trigger anonymous warning. Got: {diagnostics:?}"
    );
}

#[test]
fn test_lint_config_recommended() {
    let config = LintConfig::recommended();

    // Recommended preset should enable no_anonymous_operations
    assert!(
        config.is_enabled("no_anonymous_operations"),
        "no_anonymous_operations should be enabled in recommended"
    );
}

#[test]
fn test_rule_registry_returns_rules() {
    let rules = standalone_document_rules();

    let rule_names: Vec<&str> = rules.iter().map(|r| r.name()).collect();

    assert!(
        rule_names.contains(&"no_anonymous_operations"),
        "Registry should include no_anonymous_operations"
    );
    assert!(
        rule_names.contains(&"unused_variables"),
        "Registry should include unused_variables"
    );
}

#[test]
fn test_multiple_anonymous_operations() {
    let db = RootDatabase::default();
    let source = r#"
query { user { name } }
mutation { updateUser { id } }
"#;
    let config = LintConfig::recommended();

    let diagnostics = run_standalone_rules(&db, source, &config);

    let anon_count = diagnostics
        .iter()
        .filter(|d| d.message.contains("Anonymous"))
        .count();

    assert!(
        anon_count >= 2,
        "Expected at least 2 anonymous operation warnings. Got: {diagnostics:?}"
    );
}

#[test]
fn test_shorthand_query_syntax() {
    let db = RootDatabase::default();
    let source = "{ user { id } }";
    let config = LintConfig::recommended();

    let diagnostics = run_standalone_rules(&db, source, &config);

    let anon_warning = diagnostics
        .iter()
        .find(|d| d.message.contains("Anonymous"));

    assert!(
        anon_warning.is_some(),
        "Shorthand query syntax should trigger anonymous warning. Got: {diagnostics:?}"
    );
}
