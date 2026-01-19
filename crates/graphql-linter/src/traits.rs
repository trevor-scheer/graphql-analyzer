/// New trait-based lint rule system for Salsa integration
///
/// This module defines the trait hierarchy for lint rules that work with the
/// new Salsa-based architecture (graphql-db → graphql-syntax → graphql-hir → graphql-analysis).
use crate::diagnostics::{LintDiagnostic, LintSeverity};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use std::collections::HashMap;

/// Base trait for all lint rules
pub trait LintRule: Send + Sync {
    /// Unique identifier for this rule (e.g., `"deprecated_field"`)
    fn name(&self) -> &'static str;

    /// Human-readable description
    fn description(&self) -> &'static str;

    /// Default severity (can be overridden by config)
    fn default_severity(&self) -> LintSeverity;
}

/// Lint rule that runs on standalone documents (no schema required)
///
/// These rules can access:
/// - Parsed syntax tree via `graphql_syntax::parse(db, content, metadata)`
/// - All fragments via `graphql_hir::all_fragments(db, project_files)`
///
/// Examples: `redundant_fields`, `operation_naming`, `no_anonymous_operations`
pub trait StandaloneDocumentLintRule: LintRule {
    /// Check a single file for issues
    ///
    /// The `options` parameter contains rule-specific configuration from `.graphqlrc.yaml`.
    /// Rules should define their own options struct and deserialize from this JSON value.
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic>;
}

/// Lint rule that runs on documents with schema access
///
/// These rules can access:
/// - Parsed syntax tree
/// - All fragments
/// - Schema types via `graphql_hir::schema_types(db, project_files)`
///
/// Examples: `deprecated_field`, `require_id_field`
pub trait DocumentSchemaLintRule: LintRule {
    /// Check a single file against schema
    ///
    /// The `options` parameter contains rule-specific configuration from `.graphqlrc.yaml`.
    /// Rules should define their own options struct and deserialize from this JSON value.
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic>;
}

/// Lint rule that runs on standalone schemas (no documents)
///
/// These rules can access:
/// - Schema types
///
/// Examples: `schema_naming_conventions`, `field_naming`
pub trait StandaloneSchemaLintRule: LintRule {
    /// Check schema design
    ///
    /// The `options` parameter contains rule-specific configuration from `.graphqlrc.yaml`.
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>>;
}

/// Lint rule that runs project-wide
///
/// These rules can access:
/// - All schema types
/// - All fragments
/// - All operations
///
/// Examples: `unique_names`, `unused_fields`, `unused_fragments`
pub trait ProjectLintRule: LintRule {
    /// Check the entire project
    /// Returns diagnostics grouped by file
    ///
    /// The `options` parameter contains rule-specific configuration from `.graphqlrc.yaml`.
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>>;
}
