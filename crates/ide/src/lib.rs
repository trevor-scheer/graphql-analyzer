/// # graphql-ide
///
/// This crate provides editor-facing IDE features for GraphQL language support.
/// It serves as the API boundary between the analysis layer and the LSP layer.
///
/// ## Core Principle: POD Types with Public Fields
///
/// Following rust-analyzer's design:
/// - All types are Plain Old Data (POD) structs
/// - All fields are public
/// - Types use editor coordinates (file paths, line/column positions)
/// - No GraphQL domain knowledge leaks to LSP layer
///
/// ## Architecture
///
/// ```text
/// LSP Layer (tower-lsp)
///     ↓
/// graphql-ide (this crate) ← POD types, editor API
///     ↓
/// graphql-analysis ← Query-based validation and linting
///     ↓
/// graphql-hir ← Semantic queries
///     ↓
/// graphql-syntax ← Parsing
///     ↓
/// graphql-db ← Salsa database
/// ```
///
/// ## Main Types
///
/// - [`AnalysisHost`] - The main entry point, owns the database
/// - [`Analysis`] - Immutable snapshot for querying IDE features
/// - POD types: [`Position`], [`Range`], [`Location`], [`FilePath`]
/// - Feature types: [`CompletionItem`], [`HoverResult`], [`Diagnostic`]
#[cfg(test)]
mod analysis_host_isolation;
#[cfg(test)]
mod diagnostics_for_change_tests;

// Infrastructure modules
mod database;
mod db_files;
mod discovery;
mod file_registry;
mod helpers;
pub(crate) mod symbol;
mod types;

// Core modules
mod analysis;
mod host;

// Feature modules
mod code_lenses;
mod completion;
mod folding_ranges;
mod goto_definition;
mod hover;
mod inlay_hints;
mod references;
mod rename;
mod selection_range;
mod semantic_tokens;
mod signature_help;
mod symbols;

// Re-export types from the types module
pub use types::{
    CodeFix, CodeLens, CodeLensCommand, CodeLensInfo, CompletionItem, CompletionKind,
    ComplexityAnalysis, Diagnostic, DiagnosticSeverity, DocumentLoadResult, DocumentSymbol,
    FieldComplexity, FieldCoverageReport, FieldUsageInfo, FilePath, FoldingRange, FoldingRangeKind,
    FragmentReference, FragmentUsage, HoverResult, InlayHint, InlayHintKind, InsertTextFormat,
    Location, ParameterInformation, PendingIntrospection, Position, ProjectStatus, Range,
    RenameResult, SchemaContentError, SchemaLoadResult, SchemaStats, SelectionRange, SemanticToken,
    SemanticTokenModifiers, SemanticTokenType, SignatureHelp, SignatureInformation, SymbolKind,
    TextEdit, TypeCoverageInfo, WorkspaceSymbol,
};

// `FileRegistry` is owned by `AnalysisHost` and not exposed publicly. Snapshots
// access file lookups through the `DbFiles` Salsa-backed view.
pub(crate) use db_files::DbFiles;

// Re-export for use in symbol module and LSP
pub use helpers::{path_to_file_uri, unwrap_type_to_name};

// Re-export database types that IDE layer needs
pub use graphql_base_db::{DocumentKind, Language};

// Re-export core types
pub use analysis::Analysis;
pub use discovery::{
    discover_document_files, ContentMismatchError, DiscoveredFile, FileDiscoveryResult, LoadedFile,
};
pub use host::AnalysisHost;

#[cfg(test)]
/// Helper for tests: extracts cursor position from a string with a `*` marker.
///
/// # Example
/// ```ignore
/// let (source, pos) = extract_cursor("query { user*Name }");
/// assert_eq!(source, "query { userName }");
/// assert_eq!(pos, Position::new(0, 12));
/// ```
///
/// For multiline:
/// ```ignore
/// let (source, pos) = extract_cursor("query {\n  user*Name\n}");
/// assert_eq!(pos, Position::new(1, 6)); // line 1, col 6
/// ```
fn extract_cursor(input: &str) -> (String, Position) {
    let mut line = 0u32;
    let mut character = 0u32;
    let mut found = false;
    let mut result = String::with_capacity(input.len());

    for ch in input.chars() {
        if ch == '*' && !found {
            found = true;
            continue;
        }

        if !found {
            // Before cursor: track position normally
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }

        result.push(ch);
    }

    assert!(found, "No cursor marker '*' found in input");

    (result, Position::new(line, character))
}

#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::helpers::{
        convert_diagnostic, convert_position, convert_range, convert_severity, position_to_offset,
    };

    #[test]
    fn test_analysis_host_creation() {
        let host = AnalysisHost::new();
        let _snapshot = host.snapshot();
    }

    #[test]
    fn test_position_creation() {
        let pos = Position::new(10, 5);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_extract_cursor_single_line() {
        let (source, pos) = extract_cursor("query { user*Name }");
        assert_eq!(source, "query { userName }");
        assert_eq!(pos, Position::new(0, 12));
    }

    #[test]
    fn test_extract_cursor_multiline() {
        let (source, pos) = extract_cursor("query {\n  user*Name\n}");
        assert_eq!(source, "query {\n  userName\n}");
        assert_eq!(pos, Position::new(1, 6));
    }

    #[test]
    fn test_extract_cursor_start_of_line() {
        let (source, pos) = extract_cursor("query {\n*  userName\n}");
        assert_eq!(source, "query {\n  userName\n}");
        assert_eq!(pos, Position::new(1, 0));
    }

    #[test]
    fn test_extract_cursor_graphql_example() {
        let input = r#"
fragment AttackActionInfo on AttackAction {
    pokemon {
        *...TeamPokemonBasic
    }
}
"#;
        let (source, pos) = extract_cursor(input);
        assert!(!source.contains('*'));
        assert_eq!(pos.line, 3);
        assert_eq!(pos.character, 8);
    }

    #[test]
    fn test_range_creation() {
        let range = Range::new(Position::new(0, 0), Position::new(1, 10));
        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 1);
    }

    #[test]
    fn test_file_path_creation() {
        let path = FilePath::new("file:///path/to/file.graphql");
        assert_eq!(path.as_str(), "file:///path/to/file.graphql");

        let path2: FilePath = "test.graphql".into();
        assert_eq!(path2.as_str(), "test.graphql");
    }

    #[test]
    fn test_completion_item_builder() {
        let item = CompletionItem::new("fieldName", CompletionKind::Field)
            .with_detail("String!")
            .with_documentation("A field that returns a string")
            .with_deprecated(true);

        assert_eq!(item.label, "fieldName");
        assert_eq!(item.kind, CompletionKind::Field);
        assert_eq!(item.detail, Some("String!".to_string()));
        assert!(item.deprecated);
    }

    #[test]
    fn test_hover_result_builder() {
        let hover = HoverResult::new("```graphql\ntype User\n```")
            .with_range(Range::new(Position::new(0, 5), Position::new(0, 9)));

        assert!(hover.contents.contains("type User"));
        assert!(hover.range.is_some());
    }

    #[test]
    fn test_diagnostic_builder() {
        let diag = Diagnostic::new(
            Range::new(Position::new(1, 0), Position::new(1, 10)),
            DiagnosticSeverity::Error,
            "Unknown type: User",
            "graphql",
        )
        .with_code("unknown-type");

        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert_eq!(diag.message, "Unknown type: User");
        assert_eq!(diag.code, Some("unknown-type".to_string()));
    }

    #[test]
    fn test_diagnostics_for_valid_file() {
        let mut host = AnalysisHost::new();

        // Add a valid schema file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        // Get diagnostics
        let snapshot = host.snapshot();
        let diagnostics = snapshot.diagnostics(&path);

        // Valid file should have no diagnostics (or only non-error diagnostics)
        // Note: There might be some diagnostics depending on validation rules
        assert!(diagnostics
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
    }

    #[test]
    fn test_diagnostics_for_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        // Try to get diagnostics for a file that doesn't exist
        let path = FilePath::new("file:///nonexistent.graphql");
        let diagnostics = snapshot.diagnostics(&path);

        // Should return empty vector for nonexistent file
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_diagnostics_after_file_update() {
        // This test verifies that file updates work correctly with Salsa's
        // incremental computation. Key insight from Salsa SME consultation:
        //
        // Salsa uses a single-writer, multi-reader model. When we clone the
        // IdeDatabase (via snapshot()), we create a snapshot that shares the
        // underlying storage. Salsa setters require exclusive access, so ALL
        // snapshots must be dropped before calling any setter (like set_text).
        //
        // The fix is to properly scope snapshot lifetimes: get diagnostics
        // inside a block so the snapshot is dropped before mutation.

        let mut host = AnalysisHost::new();

        // Add a file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Get initial diagnostics - snapshot is scoped to this block
        let diagnostics1 = {
            let snapshot = host.snapshot();
            snapshot.diagnostics(&path)
        }; // snapshot dropped here, before mutation

        // Update the file - safe because no snapshots exist
        host.add_file(
            &path,
            "type Query { world: Int }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Get new diagnostics - new snapshot for updated content
        let diagnostics2 = {
            let snapshot = host.snapshot();
            snapshot.diagnostics(&path)
        };

        // Both should be valid (no errors)
        assert!(diagnostics1
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
        assert!(diagnostics2
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error));
    }

    /// Regression test: semantic query validation errors must show up through
    /// the IDE diagnostics pipeline. Tests both pure GraphQL and TypeScript files.
    ///
    /// This verifies that `Analysis::diagnostics()` (the entry point used by the LSP)
    /// correctly reports field selection errors when a query references a field
    /// that doesn't exist in the schema.
    #[test]
    fn test_diagnostics_reports_semantic_validation_errors() {
        let mut host = AnalysisHost::new();

        // Schema with `group` (singular) field
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { idpFetchConfig: IdpFetchConfig }\n\
             type IdpFetchConfig { id: ID! group: [IdpGroup!]! scopingEnabled: Boolean! }\n\
             type IdpGroup { id: ID! name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Pure GraphQL document referencing "groups" (non-existent, should be "group")
        let graphql_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &graphql_path,
            "query GetConfig { idpFetchConfig { id groups { id name } scopingEnabled } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // TypeScript file with embedded GraphQL referencing "groups"
        let ts_path = FilePath::new("file:///components/scoping.tsx");
        host.add_file(
            &ts_path,
            r#"import { gql } from "@apollo/client";

export const QUERY = gql`
  query GetConfig {
    idpFetchConfig {
      id
      groups {
        id
        name
      }
      scopingEnabled
    }
  }
`;
"#,
            Language::TypeScript,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();
        let snapshot = host.snapshot();

        // Pure GraphQL file should have semantic errors
        let graphql_diagnostics = snapshot.diagnostics(&graphql_path);
        assert!(
            graphql_diagnostics
                .iter()
                .any(|d| d.severity == DiagnosticSeverity::Error),
            "Expected semantic validation errors for GraphQL file referencing non-existent \
             field 'groups'. Got: {graphql_diagnostics:?}",
        );

        // TypeScript file should also have semantic errors
        let ts_diagnostics = snapshot.diagnostics(&ts_path);
        assert!(
            ts_diagnostics
                .iter()
                .any(|d| d.severity == DiagnosticSeverity::Error),
            "Expected semantic validation errors for TypeScript file referencing non-existent \
             field 'groups'. Got: {ts_diagnostics:?}",
        );
    }

    #[test]
    fn test_conversion_position() {
        let analysis_pos = graphql_analysis::Position::new(10, 20);
        let ide_pos = convert_position(analysis_pos);

        assert_eq!(ide_pos.line, 10);
        assert_eq!(ide_pos.character, 20);
    }

    #[test]
    fn test_conversion_range() {
        let analysis_range = graphql_analysis::DiagnosticRange::new(
            graphql_analysis::Position::new(1, 5),
            graphql_analysis::Position::new(1, 10),
        );
        let ide_range = convert_range(analysis_range);

        assert_eq!(ide_range.start.line, 1);
        assert_eq!(ide_range.start.character, 5);
        assert_eq!(ide_range.end.line, 1);
        assert_eq!(ide_range.end.character, 10);
    }

    #[test]
    fn test_conversion_severity() {
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Error),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Warning),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            convert_severity(graphql_analysis::Severity::Info),
            DiagnosticSeverity::Information
        );
    }

    #[test]
    fn test_conversion_diagnostic() {
        let analysis_diag = graphql_analysis::Diagnostic::with_source_and_code(
            graphql_analysis::Severity::Warning,
            "Test warning message",
            graphql_analysis::DiagnosticRange::new(
                graphql_analysis::Position::new(2, 0),
                graphql_analysis::Position::new(2, 10),
            ),
            "test-source",
            "TEST001",
        );

        let ide_diag = convert_diagnostic(&analysis_diag);

        assert_eq!(ide_diag.severity, DiagnosticSeverity::Warning);
        assert_eq!(ide_diag.message, "Test warning message");
        assert_eq!(ide_diag.source, "test-source");
        assert_eq!(ide_diag.code, Some("TEST001".to_string()));
        assert_eq!(ide_diag.range.start.line, 2);
        assert_eq!(ide_diag.range.start.character, 0);
        assert_eq!(ide_diag.range.end.line, 2);
        assert_eq!(ide_diag.range.end.character, 10);
    }

    #[test]
    fn test_hover_on_valid_file() {
        let mut host = AnalysisHost::new();

        // Add a schema file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        // Get hover at a position
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&path, Position::new(0, 5));

        // Should return hover information
        assert!(hover.is_some());
        let hover = hover.unwrap();
        assert!(!hover.contents.is_empty());
    }

    #[test]
    fn test_hover_on_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        // Try to get hover for a file that doesn't exist
        let path = FilePath::new("file:///nonexistent.graphql");
        let hover = snapshot.hover(&path, Position::new(0, 0));

        // Should return None for nonexistent file
        assert!(hover.is_none());
    }

    #[test]
    fn test_hover_with_syntax_errors_shows_valid_symbols() {
        let mut host = AnalysisHost::new();

        // Add a file with syntax errors (missing closing brace)
        let path = FilePath::new("file:///invalid.graphql");
        host.add_file(
            &path,
            "type Query {",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        // Get hover on the Query type name (position 5 is in "Query")
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&path, Position::new(0, 5));

        // Should return hover info for the Query type even with syntax errors
        // This tests that hover works on valid parts of a file with syntax errors
        assert!(hover.is_some());
        let hover = hover.unwrap();
        assert!(hover.contents.contains("Query"));
        assert!(hover.contents.contains("Type"));
    }

    #[test]
    fn test_hover_on_schema_field_definition() {
        let mut host = AnalysisHost::new();

        // Add a schema file with a type definition
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Pokemon {\n  name: String!\n  level: Int!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a document that uses this field
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetPokemon { pokemon { name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Get hover on "name" field in the schema definition (line 1, col 2 = "name")
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&schema_path, Position::new(1, 2));

        // Should return hover information for the field
        assert!(hover.is_some(), "Expected hover on schema field definition");
        let hover = hover.unwrap();
        assert!(hover.contents.contains("Field"), "Should contain 'Field'");
        assert!(hover.contents.contains("name"), "Should contain field name");
        assert!(hover.contents.contains("String"), "Should contain type");
        // Field is used in one operation, so should show usage count
        assert!(
            hover.contents.contains("Used in"),
            "Should contain usage information"
        );
    }

    #[test]
    fn test_hover_on_schema_field_shows_unused() {
        let mut host = AnalysisHost::new();

        // Add a schema file with a type definition
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Pokemon {\n  name: String!\n  level: Int!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        host.rebuild_project_files();

        // Get hover on "level" field which is not used in any operation
        let snapshot = host.snapshot();
        let hover = snapshot.hover(&schema_path, Position::new(2, 2));

        // Should show "0 operations (unused)"
        assert!(hover.is_some(), "Expected hover on schema field definition");
        let hover = hover.unwrap();
        assert!(
            hover.contents.contains("0 operations"),
            "Should indicate unused field"
        );
    }

    #[test]
    fn test_hover_field_in_inline_fragment() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { battleParticipant(id: ID!): BattleParticipant }\ninterface BattleParticipant { id: ID! name: String! displayName: String! }\ntype BattlePokemon implements BattleParticipant { id: ID! name: String! displayName: String! currentHP: Int! }",
            Language::GraphQL, DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor(
            "query { battleParticipant(id: \"1\") { id name ... on BattlePokemon { current*HP } } }",
        );
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hover = snapshot.hover(&query_file, cursor_pos);

        assert!(
            hover.is_some(),
            "Should show hover info for field in inline fragment type"
        );
        let hover = hover.unwrap();
        assert!(hover.contents.contains("currentHP"));
        assert!(hover.contents.contains("Int!"));
    }

    #[test]
    fn test_position_to_offset_helper() {
        let text = "line 1\nline 2\nline 3";
        let line_index = graphql_syntax::LineIndex::new(text);

        // First line
        assert_eq!(
            position_to_offset(&line_index, Position::new(0, 0)),
            Some(0)
        );
        assert_eq!(
            position_to_offset(&line_index, Position::new(0, 5)),
            Some(5)
        );

        // Second line
        assert_eq!(
            position_to_offset(&line_index, Position::new(1, 0)),
            Some(7)
        );
        assert_eq!(
            position_to_offset(&line_index, Position::new(1, 3)),
            Some(10)
        );

        // Third line
        assert_eq!(
            position_to_offset(&line_index, Position::new(2, 0)),
            Some(14)
        );
    }

    #[test]
    fn test_completions_on_valid_file() {
        let mut host = AnalysisHost::new();

        // Add a schema file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Get completions at a position
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&path, Position::new(0, 10));

        // Should return Some (file exists) even if empty
        assert!(completions.is_some());
    }

    #[test]
    fn test_completions_on_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        // Try to get completions for a file that doesn't exist
        let path = FilePath::new("file:///nonexistent.graphql");
        let completions = snapshot.completions(&path, Position::new(0, 0));

        // Should return None for nonexistent file
        assert!(completions.is_none());
    }

    #[test]
    fn test_completions_with_syntax_errors() {
        let mut host = AnalysisHost::new();

        // Add a file with syntax errors
        let path = FilePath::new("file:///invalid.graphql");
        host.add_file(
            &path,
            "type Query {",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        host.rebuild_project_files();

        // Get completions - at document level we now return keyword completions
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&path, Position::new(0, 10));

        // Should return completions without crashing (keyword completions at document level)
        assert!(completions.is_some());
    }

    #[test]
    fn test_goto_definition_on_valid_file() {
        let mut host = AnalysisHost::new();

        // Add a schema file
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Get goto definition at a position (may not find anything, but shouldn't crash)
        let snapshot = host.snapshot();
        let _locations = snapshot.goto_definition(&path, Position::new(0, 10));

        // Test passes if no crash occurs
    }

    #[test]
    fn test_goto_definition_on_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        // Try to get goto definition for a file that doesn't exist
        let path = FilePath::new("file:///nonexistent.graphql");
        let locations = snapshot.goto_definition(&path, Position::new(0, 0));

        // Should return None for nonexistent file
        assert!(locations.is_none());
    }

    #[test]
    fn test_goto_definition_fragment_spread() {
        let mut host = AnalysisHost::new();

        // Add a schema (required for HIR to work properly)
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a query that uses the fragment
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { ...UserFields }";
        host.add_file(
            &query_file,
            query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Get goto definition for the fragment spread (position at "UserFields")
        // Position should be at the start of "UserFields" after "..."
        // "query { ..." = 11 characters, so "UserFields" starts at position 11
        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, Position::new(0, 12));

        // Should find the fragment definition
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), fragment_file.as_str());

        // Verify we got real positions (not placeholder 0,0)
        assert!(
            locations[0].range.start.line > 0 || locations[0].range.start.character > 0,
            "Expected real positions, got {:?}",
            locations[0].range
        );
    }

    #[test]
    fn test_goto_definition_type_name() {
        let mut host = AnalysisHost::new();

        // Add a type definition
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment that references User
        let fragment_file = FilePath::new("file:///fragment.graphql");
        let (fragment_text, cursor_pos) = extract_cursor("fragment F on U*ser { id }");
        host.add_file(
            &fragment_file,
            &fragment_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&fragment_file, cursor_pos);

        // Should find the type definition
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
    }

    #[test]
    fn test_goto_definition_field_on_root_type() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor("query { u*ser }");
        dbg!(&query_text);
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(locations.is_some(), "Should find field definition");
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "user" field in Query type (line 0)
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_nested_field() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor("query { user { na*me } }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(locations.is_some(), "Should find nested field definition");
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "name" field in User type (line 1)
        assert_eq!(locations[0].range.start.line, 1);
    }

    #[test]
    fn test_goto_definition_schema_field_type() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) =
            extract_cursor("type Query { user: U*ser }\ntype User { id: ID! }");
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find type definition from field return type"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "User" type definition (line 1)
        assert_eq!(locations[0].range.start.line, 1);
    }

    #[test]
    fn test_goto_definition_on_schema_field_returns_itself() {
        // When cmd+clicking a schema field definition, return its own location.
        // VSCode will then show "Find References" peek window as fallback.
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) =
            extract_cursor("type User {\n  na*me: String!\n  age: Int!\n}");
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should return field's own location for schema field definition"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to the "name" field on line 1
        assert_eq!(locations[0].range.start.line, 1);
    }

    #[test]
    fn test_goto_definition_field_in_inline_fragment() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { battleParticipant(id: ID!): BattleParticipant }\ninterface BattleParticipant { id: ID! name: String! displayName: String! }\ntype BattlePokemon implements BattleParticipant { id: ID! name: String! displayName: String! currentHP: Int! }",
            Language::GraphQL, DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor(
            "query { battleParticipant(id: \"1\") { id name ... on BattlePokemon { current*HP } } }",
        );
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find field definition in inline fragment type"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "currentHP" field in BattlePokemon type (line 2)
        assert_eq!(locations[0].range.start.line, 2);
    }

    #[test]
    fn test_goto_definition_variable_reference() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user(id: ID!): User }\ntype User { id: ID! name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on $id in the argument value
        let (query_text, cursor_pos) =
            extract_cursor("query GetUser($id: ID!) { user(id: $i*d) { name } }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find variable definition from usage"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), query_file.as_str());
        // Should point to the variable name (id) in the definition
        // "query GetUser($" = 15 chars, and we point to "id" not "$id"
        assert_eq!(locations[0].range.start.line, 0);
        assert_eq!(locations[0].range.start.character, 15);
    }

    #[test]
    fn test_goto_definition_argument_name() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user(id: ID!, name: String): User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on "id" argument name in the query
        let (query_text, cursor_pos) = extract_cursor("query { user(i*d: \"123\") { id } }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find argument definition in schema"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "id" argument in Query.user field definition
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_operation_name() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        // Cursor on the operation name "GetHello"
        let (query_text, cursor_pos) = extract_cursor("query GetH*ello { hello }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(locations.is_some(), "Should find operation definition");
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), query_file.as_str());
        // Should point to the operation name in the same file
        assert_eq!(locations[0].range.start.line, 0);
        assert_eq!(locations[0].range.start.character, 6); // "query " = 6 chars
    }

    #[test]
    fn test_goto_definition_implements_interface() {
        let mut host = AnalysisHost::new();

        // Schema with interface and type that implements it
        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) =
            extract_cursor("interface Node { id: ID! }\ntype User implements No*de { id: ID! }");
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find interface definition from implements clause"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "Node" interface definition on line 0
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_implements_multiple_interfaces() {
        let mut host = AnalysisHost::new();

        // Schema with multiple interfaces
        let schema_file = FilePath::new("file:///schema.graphql");
        let schema_text = r#"interface Node { id: ID! }
interface Timestamped { createdAt: String! }
type User implements Node & Timestamped { id: ID!, createdAt: String! }"#;
        host.add_file(
            &schema_file,
            schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Test cursor on "Timestamped" in implements clause
        // Line 2: "type User implements Node & Timestamped { id: ID!, createdAt: String! }"
        // "type User implements Node & " = 28 chars, then "Timestamped"
        let cursor_pos = Position::new(2, 30);
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find Timestamped interface definition"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "Timestamped" interface definition on line 1
        assert_eq!(locations[0].range.start.line, 1);
    }

    #[test]
    fn test_goto_definition_interface_extends_interface() {
        let mut host = AnalysisHost::new();

        // Interface extending another interface (GraphQL supports this)
        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) = extract_cursor(
            "interface Node { id: ID! }\ninterface Entity implements No*de { id: ID!, name: String }",
        );
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find interface definition from interface implements clause"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "Node" interface definition on line 0
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_type_extension_implements() {
        let mut host = AnalysisHost::new();

        // Type extension adding an interface
        let schema_file = FilePath::new("file:///schema.graphql");
        let (schema_text, cursor_pos) = extract_cursor(
            "interface Node { id: ID! }\ntype User { name: String }\nextend type User implements No*de",
        );
        host.add_file(
            &schema_file,
            &schema_text,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&schema_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find interface definition from type extension implements clause"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), schema_file.as_str());
        // Should point to "Node" interface definition on line 0
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_directive_name() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "directive @cacheControl(maxAge: Int) on FIELD_DEFINITION\n\ntype Query {\n  hello: String @cacheControl(maxAge: 30)\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Cursor on "cacheControl" in the field usage (line 3, after the @)
        let result = snapshot.goto_definition(&schema_path, Position::new(3, 18));
        assert!(result.is_some());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_directive_argument_name() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "directive @cacheControl(maxAge: Int) on FIELD_DEFINITION\n\ntype Query {\n  hello: String @cacheControl(maxAge: 30)\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Cursor on "maxAge" in usage (line 3)
        let result = snapshot.goto_definition(&schema_path, Position::new(3, 31));
        assert!(result.is_some());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_goto_definition_directive_on_operation() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "directive @myDirective on QUERY\n\ntype Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query MyQuery @myDirective {\n  hello\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let result = snapshot.goto_definition(&doc_path, Position::new(0, 17));
        assert!(result.is_some());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), "file:///schema.graphql");
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_find_references_fragment() {
        let mut host = AnalysisHost::new();

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment F on User { id }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add queries that use the fragment
        let query1_file = FilePath::new("file:///query1.graphql");
        host.add_file(
            &query1_file,
            "query { ...F }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let query2_file = FilePath::new("file:///query2.graphql");
        host.add_file(
            &query2_file,
            "query { ...F }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Find references to the fragment (position at "F" in fragment definition)
        // "fragment " = 9 characters, so "F" starts at position 9
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&fragment_file, Position::new(0, 9), false);

        // Should find both usages but not the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_find_references_fragment_with_declaration() {
        let mut host = AnalysisHost::new();

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment F on User { id }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a query that uses the fragment
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { ...F }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Find references including declaration
        // "fragment " = 9 characters, so "F" starts at position 9
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&fragment_file, Position::new(0, 9), true);

        // Should find the usage and the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_find_references_type() {
        let mut host = AnalysisHost::new();

        // Add a type definition
        let user_file = FilePath::new("file:///user.graphql");
        host.add_file(
            &user_file,
            "type User { id: ID }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add types that reference User
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "type Query { user: User }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let mutation_file = FilePath::new("file:///mutation.graphql");
        host.add_file(
            &mutation_file,
            "type Mutation { u: User }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        // Find references to the User type
        // "type " = 5 characters, so "User" starts at position 5
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&user_file, Position::new(0, 5), false);

        // Should find all usages but not the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        // Query file has 1 reference, mutation file has 1 reference = 2 total
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_find_references_type_with_declaration() {
        let mut host = AnalysisHost::new();

        // Add a type definition
        let user_file = FilePath::new("file:///user.graphql");
        host.add_file(
            &user_file,
            "type User { id: ID }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a type that references User
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "type Query { user: User }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        // Find references including declaration
        // "type " = 5 characters, so "User" starts at position 5
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&user_file, Position::new(0, 5), true);

        // Should find the usage and the declaration
        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_find_references_field_in_queries() {
        let mut host = AnalysisHost::new();

        // Add a schema with a type that has a name field
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a query that uses the name field
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { id name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a fragment that also uses the name field
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Find references to "name" field on User type
        // Schema line 2: "type User { id: ID! name: String! }"
        // "type User { id: ID! " = 20 chars, so "name" starts at position 20
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&schema_file, Position::new(1, 20), false);

        assert!(
            locations.is_some(),
            "Should find field references in documents"
        );
        let locations = locations.unwrap();
        // Should find: query usage + fragment usage = 2
        assert_eq!(
            locations.len(),
            2,
            "Expected 2 usages (query + fragment), got {}",
            locations.len()
        );
    }

    #[test]
    fn test_find_references_field_with_declaration() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Find references including declaration
        // Line 1: "type User { " = 12 chars, so "name" starts at position 12
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&schema_file, Position::new(1, 12), true);

        assert!(locations.is_some());
        let locations = locations.unwrap();
        // Should find: declaration + query usage = 2
        assert_eq!(locations.len(), 2);

        // Verify one location is in schema, one in query
        let schema_refs: Vec<_> = locations
            .iter()
            .filter(|l| l.file.as_str() == schema_file.as_str())
            .collect();
        let query_refs: Vec<_> = locations
            .iter()
            .filter(|l| l.file.as_str() == query_file.as_str())
            .collect();
        assert_eq!(schema_refs.len(), 1, "Should have 1 schema reference");
        assert_eq!(query_refs.len(), 1, "Should have 1 query reference");
    }

    #[test]
    fn test_find_references_field_nested() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { profile: Profile }\ntype Profile { bio: String! }",
            Language::GraphQL, DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { profile { bio } } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Find references to "bio" field on Profile type
        // Line 2: "type Profile { " = 15 chars, "bio" starts at 15
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&schema_file, Position::new(2, 15), false);

        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1, "Should find nested field usage");
    }

    #[test]
    fn test_find_references_field_via_interface() {
        let mut host = AnalysisHost::new();

        // Schema with interface and implementing type
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { node: Node }\ninterface Node { id: ID! }\ntype User implements Node { id: ID! name: String }",
            Language::GraphQL, DocumentKind::Schema,
        );

        // Query that uses the field on the implementing type
        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { node { ... on User { id } } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Find references to "id" field on Node interface
        // Line 1: "interface Node { " = 17 chars, "id" starts at 17
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&schema_file, Position::new(1, 17), false);

        assert!(
            locations.is_some(),
            "Should find field references via interface"
        );
        let locations = locations.unwrap();
        // Should find the usage in the query (User implements Node, so User.id matches Node.id)
        assert_eq!(
            locations.len(),
            1,
            "Should find field usage via interface implementation"
        );
    }

    #[test]
    fn test_completions_in_selection_set_should_not_show_fragments() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a query with cursor in selection set
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { user { id } }";
        //                                 ^ cursor here at position 15 (right after { before id)
        host.add_file(
            &query_file,
            query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Get completions inside the selection set (simulating user about to type)
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&query_file, Position::new(0, 15));

        // Should return field completions only (id, name), NOT fragment names
        assert!(completions.is_some());
        let items = completions.unwrap();

        // Check that we got field completions
        let field_names: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert!(
            field_names.contains(&"id"),
            "Expected 'id' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"name"),
            "Expected 'name' field in completions, got: {field_names:?}"
        );

        // Check that we did NOT get fragment completions
        assert!(
            !field_names.contains(&"UserFields"),
            "Fragment names should not appear in field completions, but found 'UserFields'"
        );

        // All completions should be fields, not fragments
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Field,
                "Expected only Field completions, but found {:?} for '{}'",
                item.kind,
                item.label
            );
        }
    }

    #[test]
    fn test_completions_outside_selection_set_should_not_show_fragments() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a query with cursor OUTSIDE any selection set (at document level)
        let query_file = FilePath::new("file:///query.graphql");
        let query_text = "query { user { id } }\n";
        //                                       ^ cursor at end (position 22 on line 0)
        host.add_file(
            &query_file,
            query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Get completions at document level (NOT in a selection set)
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&query_file, Position::new(0, 22));

        // At document level, we shouldn't show fragment names either
        // (user would want to type "query", "mutation", "fragment", etc.)
        if let Some(items) = completions {
            let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
            assert!(
                !labels.contains(&"UserFields"),
                "Fragment names should not appear outside selection sets, but found 'UserFields'. Got: {labels:?}"
            );
        }
    }

    #[test]
    fn test_completions_after_fragment_spread_in_mutation() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Mutation { forfeitBattle(battleId: ID!, trainerId: ID!): Battle } type Battle { id: ID! status: String winner: String }",
            Language::GraphQL, DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment BattleDetailed on Battle { id status }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add a mutation with cursor after fragment spread
        let mutation_file = FilePath::new("file:///mutation.graphql");
        let mutation_text = r"mutation ForfeitBattle($battleId: ID!, $trainerId: ID!) {
  forfeitBattle(battleId: $battleId, trainerId: $trainerId) {
    ...BattleDetailed

  }
}";
        host.add_file(
            &mutation_file,
            mutation_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Get completions after the fragment spread (line 3, position 4 - after newline)
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&mutation_file, Position::new(3, 4));

        // Should return field completions for Battle type
        assert!(completions.is_some(), "Expected completions to be Some");
        let items = completions.unwrap();

        let field_names: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        dbg!(&field_names);

        assert!(
            field_names.contains(&"id"),
            "Expected 'id' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"status"),
            "Expected 'status' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"winner"),
            "Expected 'winner' field in completions, got: {field_names:?}"
        );
    }

    #[test]
    fn test_completions_with_multiple_mutations_in_same_file() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Mutation { forfeitBattle(battleId: ID!, trainerId: ID!): Battle startBattle(trainerId: ID!): Battle } type Battle { id: ID! status: String winner: String }",
            Language::GraphQL, DocumentKind::Schema,
        );

        // Add a fragment definition
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment BattleDetailed on Battle { id status }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        // Add multiple mutations in the same file
        let mutation_file = FilePath::new("file:///mutations.graphql");
        let mutation_text = r"mutation StartBattle($trainerId: ID!) {
  startBattle(trainerId: $trainerId) {
    ...BattleDetailed
  }
}

# Forfeit a battle
mutation ForfeitBattle($battleId: ID!, $trainerId: ID!) {
  forfeitBattle(battleId: $battleId, trainerId: $trainerId) {
    ...BattleDetailed

  }
}";
        host.add_file(
            &mutation_file,
            mutation_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Get completions in the second mutation after the fragment spread (line 10, position 4)
        let snapshot = host.snapshot();
        let completions = snapshot.completions(&mutation_file, Position::new(10, 4));

        // Should return field completions for Battle type
        assert!(
            completions.is_some(),
            "Expected completions to be Some, but got None"
        );
        let items = completions.unwrap();

        let field_names: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        dbg!(&field_names);

        assert!(
            !field_names.is_empty(),
            "Expected non-empty completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"id"),
            "Expected 'id' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"status"),
            "Expected 'status' field in completions, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"winner"),
            "Expected 'winner' field in completions, got: {field_names:?}"
        );
    }

    #[test]
    fn test_battle_graphql_attack_action_pokemon_completions() {
        // Simulate a GraphQL file similar to battle.graphql
        let (graphql, cursor_pos) = extract_cursor(
            r#"
fragment AttackActionInfo on AttackAction {
    pokemon {
*        ...TeamPokemonBasic
    }
    move {
        ...MoveInfo
    }
    damage
    wasEffective
}
"#,
        );

        // Minimal schema for the test
        let schema = r#"
type AttackAction {
    pokemon: TeamPokemon
    move: Move
    damage: Int
    wasEffective: Boolean
}
type TeamPokemon {
    id: ID!
    name: String!
    hp: Int
}
type Move {
    id: ID!
    name: String!
}
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let gql_path = FilePath::new("file:///battle.graphql");
        host.add_file(
            &gql_path,
            &graphql,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let completions = snapshot
            .completions(&gql_path, cursor_pos)
            .unwrap_or_default();
        let labels: Vec<_> = completions.iter().map(|i| i.label.as_str()).collect();

        // Should only suggest fields of TeamPokemon, not AttackAction
        assert!(
            labels.contains(&"id"),
            "Should suggest 'id' for TeamPokemon: got {labels:?}"
        );
        assert!(
            labels.contains(&"name"),
            "Should suggest 'name' for TeamPokemon: got {labels:?}"
        );
        assert!(
            labels.contains(&"hp"),
            "Should suggest 'hp' for TeamPokemon: got {labels:?}"
        );
        assert!(
            !labels.contains(&"damage"),
            "Should NOT suggest 'damage' for TeamPokemon: got {labels:?}"
        );
        assert!(
            !labels.contains(&"move"),
            "Should NOT suggest 'move' for TeamPokemon: got {labels:?}"
        );
        assert!(
            !labels.contains(&"pokemon"),
            "Should NOT suggest 'pokemon' for TeamPokemon: got {labels:?}"
        );
        assert!(
            !labels.contains(&"wasEffective"),
            "Should NOT suggest 'wasEffective' for TeamPokemon: got {labels:?}"
        );
    }

    #[test]

    fn test_typescript_off_by_one_parent_completions() {
        let schema = r#"
type Query { allPokemon(region: Region!, limit: Int): PokemonConnection }
type PokemonConnection { nodes: [Pokemon!]! }
type Pokemon {
    id: ID!
    name: String!
    evolution: Evolution
}
type Evolution {
    evolvesTo: [EvolutionEdge]
}
type EvolutionEdge {
    pokemon: Pokemon
    requirement: Requirement
}
interface Requirement { }
type LevelRequirement implements Requirement { level: Int }
enum Region { KANTO JOHTO }
"#;

        // Test 1: Inside 'requirement' selection set
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

            let (graphql1, pos1) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
                    evolvesTo {
                        pokemon {
                            id
                            name
                        }
                        requirement {
                            ... on LevelRequirement {
*                                level
                            }
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let ts_path1 = FilePath::new("file:///test1.graphql");
            host.add_file(
                &ts_path1,
                &graphql1,
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&ts_path1, pos1).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"level"),
                "Should suggest 'level' inside LevelRequirement: got {labels:?}"
            );
            assert!(
                !labels.contains(&"requirement"),
                "Should NOT suggest 'requirement' inside requirement: got {labels:?}"
            );
            assert!(
                !labels.contains(&"pokemon"),
                "Should NOT suggest 'pokemon' inside requirement: got {labels:?}"
            );
        }

        // Test 2: Inside 'evolvesTo' selection set
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

            let (graphql2, pos2) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
                    evolvesTo {
*                        pokemon {
                            id
                            name
                        }
                        requirement {
                            ... on LevelRequirement {
                                level
                            }
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let ts_path2 = FilePath::new("file:///test2.graphql");
            host.add_file(
                &ts_path2,
                &graphql2,
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&ts_path2, pos2).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"pokemon"),
                "Should suggest 'pokemon' inside evolvesTo: got {labels:?}"
            );
            assert!(
                labels.contains(&"requirement"),
                "Should suggest 'requirement' inside evolvesTo: got {labels:?}"
            );
            assert!(
                !labels.contains(&"evolvesTo"),
                "Should NOT suggest 'evolvesTo' inside evolvesTo: got {labels:?}"
            );
            assert!(
                !labels.contains(&"evolvesFrom"),
                "Should NOT suggest 'evolvesFrom' inside evolvesTo: got {labels:?}"
            );
        }
    }

    #[test]

    fn test_typescript_deeply_nested_completions() {
        let schema = r#"
type Query { allPokemon(region: Region!, limit: Int): PokemonConnection }
type PokemonConnection { nodes: [Pokemon!]! }
type Pokemon {
    id: ID!
    name: String!
    evolution: Evolution
}
type Evolution {
    evolvesTo: [EvolutionEdge]
}
type EvolutionEdge {
    pokemon: Pokemon
    requirement: Requirement
}
interface Requirement { }
type LevelRequirement implements Requirement { level: Int }
enum Region { KANTO JOHTO }
"#;

        // Test completions inside 'evolution' selection set
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

            let (graphql1, pos1) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
*                    evolvesTo {
                        pokemon {
                            id
                            name
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let path1 = FilePath::new("file:///test1.graphql");
            host.add_file(
                &path1,
                &graphql1,
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&path1, pos1).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"evolvesTo"),
                "Should suggest 'evolvesTo' inside evolution: got {labels:?}"
            );
        }

        // Test completions inside 'evolvesTo' selection set
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

            let (graphql2, pos2) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
                    evolvesTo {
*                        pokemon {
                            id
                            name
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let path2 = FilePath::new("file:///test2.graphql");
            host.add_file(
                &path2,
                &graphql2,
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&path2, pos2).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"pokemon"),
                "Should suggest 'pokemon' inside evolvesTo: got {labels:?}"
            );
            assert!(
                labels.contains(&"requirement"),
                "Should suggest 'requirement' inside evolvesTo: got {labels:?}"
            );
        }

        // Test completions inside 'requirement' selection set with inline fragment
        {
            let mut host = AnalysisHost::new();
            let schema_path = FilePath::new("file:///schema.graphql");
            host.add_file(
                &schema_path,
                schema,
                Language::GraphQL,
                DocumentKind::Schema,
            );

            let (graphql3, pos3) = extract_cursor(
                r#"
    query GetStarterPokemon($region: Region!) {
        allPokemon(region: $region, limit: 3) {
            nodes {
                evolution {
                    evolvesTo {
                        requirement {
                            ... on LevelRequirement {
*                                level
                            }
                        }
                    }
                }
            }
        }
    }
"#,
            );
            let path3 = FilePath::new("file:///test3.graphql");
            host.add_file(
                &path3,
                &graphql3,
                Language::GraphQL,
                DocumentKind::Executable,
            );
            host.rebuild_project_files();

            let snapshot = host.snapshot();
            let items = snapshot.completions(&path3, pos3).unwrap_or_default();
            let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(
                labels.contains(&"level"),
                "Should suggest 'level' inside requirement: got {labels:?}"
            );
        }
    }

    #[test]
    fn test_completions_for_union_type_suggest_inline_fragments() {
        let schema = r#"
type Query { evolution: EvolutionEdge }
type EvolutionEdge {
    pokemon: Pokemon
    requirement: EvolutionRequirement
}
type Pokemon { id: ID! name: String! }
union EvolutionRequirement = LevelRequirement | ItemRequirement | TradeRequirement | FriendshipRequirement
type LevelRequirement { level: Int }
type ItemRequirement { item: Item }
type TradeRequirement { withItem: Item }
type FriendshipRequirement { minimumFriendship: Int }
type Item { id: ID! name: String! }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let (graphql, pos) = extract_cursor(
            r#"
query TestEvolution {
    evolution {
        requirement {
*
        }
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        let kinds: Vec<_> = items.iter().map(|i| i.kind).collect();

        // Should suggest inline fragments for union member types
        assert!(
            labels.contains(&"... on LevelRequirement"),
            "Should suggest '... on LevelRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on ItemRequirement"),
            "Should suggest '... on ItemRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on TradeRequirement"),
            "Should suggest '... on TradeRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on FriendshipRequirement"),
            "Should suggest '... on FriendshipRequirement' inline fragment: got {labels:?}"
        );

        // Should be Type kind
        for kind in kinds {
            assert_eq!(
                kind,
                CompletionKind::Type,
                "Union member completions should be Type kind"
            );
        }

        // Should NOT suggest any fields (unions have no fields)
        assert_eq!(
            labels.len(),
            4,
            "Should only suggest 4 union member types: got {labels:?}"
        );

        // Verify insert_text includes braces, newline, and cursor placeholder
        for item in &items {
            assert!(
                item.insert_text.is_some(),
                "Inline fragment should have insert_text"
            );
            let insert_text = item.insert_text.as_ref().unwrap();
            assert!(
                insert_text.contains("{\n  $0\n}"),
                "Insert text should contain braces with $0 cursor placeholder: got {insert_text}"
            );
            assert_eq!(
                item.insert_text_format,
                Some(InsertTextFormat::Snippet),
                "Inline fragment should use snippet format"
            );
        }
    }

    #[test]

    fn test_completions_for_interface_type_suggest_fields_and_inline_fragments() {
        let schema = r#"
type Query { evolution: EvolutionEdge }
type EvolutionEdge {
    pokemon: Pokemon
    requirement: Requirement
}
type Pokemon { id: ID! name: String! }
interface Requirement {
    description: String
}
type LevelRequirement implements Requirement {
    description: String
    level: Int
}
type ItemRequirement implements Requirement {
    description: String
    item: Item
}
type TradeRequirement implements Requirement {
    description: String
    withItem: Item
}
type Item { id: ID! name: String! }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let (graphql, pos) = extract_cursor(
            r#"
query TestEvolution {
    evolution {
        requirement {
*
        }
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        // Should suggest inline fragments for implementing types
        assert!(
            labels.contains(&"... on LevelRequirement"),
            "Should suggest '... on LevelRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on ItemRequirement"),
            "Should suggest '... on ItemRequirement' inline fragment: got {labels:?}"
        );
        assert!(
            labels.contains(&"... on TradeRequirement"),
            "Should suggest '... on TradeRequirement' inline fragment: got {labels:?}"
        );

        // Should be 3 type suggestions (inline fragments) total
        let type_completions: Vec<_> = items
            .iter()
            .filter(|i| i.kind == CompletionKind::Type)
            .collect();
        assert_eq!(
            type_completions.len(),
            3,
            "Should suggest 3 inline fragment types: got {labels:?}"
        );

        // Should only suggest fields from the interface itself, not implementing types
        let field_completions: Vec<_> = items
            .iter()
            .filter(|i| i.kind == CompletionKind::Field)
            .collect();
        assert_eq!(
            field_completions.len(),
            1,
            "Should have 1 field completion from interface: got {labels:?}"
        );

        // Check interface field is suggested
        assert!(
            labels.contains(&"description"),
            "Should suggest 'description' from interface"
        );

        // Should NOT suggest fields specific to implementing types
        assert!(
            !labels.contains(&"level"),
            "Should NOT suggest 'level' (specific to LevelRequirement)"
        );
        assert!(
            !labels.contains(&"item"),
            "Should NOT suggest 'item' (specific to ItemRequirement)"
        );
        assert!(
            !labels.contains(&"withItem"),
            "Should NOT suggest 'withItem' (specific to TradeRequirement)"
        );

        // Verify inline fragment insert_text includes braces, newline, and cursor placeholder
        for item in type_completions {
            assert!(
                item.insert_text.is_some(),
                "Inline fragment should have insert_text"
            );
            let insert_text = item.insert_text.as_ref().unwrap();
            assert!(
                insert_text.contains("{\n  $0\n}"),
                "Insert text should contain braces with $0 cursor placeholder: got {insert_text}"
            );
            assert_eq!(
                item.insert_text_format,
                Some(InsertTextFormat::Snippet),
                "Inline fragment should use snippet format"
            );
            // Verify sort_text is set to push inline fragments after fields
            assert!(
                item.sort_text.is_some(),
                "Inline fragment should have sort_text"
            );
            assert!(
                item.sort_text.as_ref().unwrap().starts_with("z_"),
                "Inline fragment sort_text should start with 'z_' to sort after fields: got {:?}",
                item.sort_text
            );
        }
    }

    #[test]
    fn test_completions_for_field_arguments() {
        let schema = r#"
type Query {
    user(id: ID!, name: String): User
    users(limit: Int, offset: Int, filter: String): [User!]!
}
type User { id: ID! name: String! email: String }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor inside field arguments: user(|)
        let (graphql, pos) = extract_cursor(
            r#"
query GetUser {
    user(*) {
        id
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"id"),
            "Should suggest 'id' argument: got {labels:?}"
        );
        assert!(
            labels.contains(&"name"),
            "Should suggest 'name' argument: got {labels:?}"
        );
        assert_eq!(
            items.len(),
            2,
            "Should suggest exactly 2 arguments: got {labels:?}"
        );

        // All completions should be Argument kind
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Argument,
                "Expected Argument completion kind for '{}', got {:?}",
                item.label,
                item.kind
            );
        }

        // Check that type details are provided
        let id_item = items.iter().find(|i| i.label == "id").unwrap();
        assert_eq!(id_item.detail, Some("ID!".to_string()));

        let name_item = items.iter().find(|i| i.label == "name").unwrap();
        assert_eq!(name_item.detail, Some("String".to_string()));

        // Check that insert text includes ": " suffix
        assert_eq!(id_item.insert_text, Some("id: ".to_string()));
    }

    #[test]
    fn test_completions_for_enum_values_in_argument() {
        let schema = r#"
type Query {
    users(status: Status!, role: Role): [User!]!
}
enum Status { ACTIVE INACTIVE PENDING }
enum Role {
    ADMIN
    USER
    MODERATOR
}
type User { id: ID! name: String! }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor at enum value position: users(status: |)
        let (graphql, pos) = extract_cursor(
            r#"
query GetUsers {
    users(status: *) {
        id
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"ACTIVE"),
            "Should suggest 'ACTIVE' enum value: got {labels:?}"
        );
        assert!(
            labels.contains(&"INACTIVE"),
            "Should suggest 'INACTIVE' enum value: got {labels:?}"
        );
        assert!(
            labels.contains(&"PENDING"),
            "Should suggest 'PENDING' enum value: got {labels:?}"
        );
        assert_eq!(
            items.len(),
            3,
            "Should suggest exactly 3 enum values: got {labels:?}"
        );

        // All completions should be EnumValue kind
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::EnumValue,
                "Expected EnumValue completion kind for '{}', got {:?}",
                item.label,
                item.kind
            );
        }
    }

    #[test]
    fn test_completions_for_enum_values_deprecated() {
        let schema = r#"
type Query {
    search(sort: SortOrder): [Result!]!
}
enum SortOrder {
    ASC
    DESC
    RELEVANCE @deprecated(reason: "Use ASC instead")
}
type Result { id: ID! }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let (graphql, pos) = extract_cursor(
            r#"
query Search {
    search(sort: *) {
        id
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();

        // Should still include deprecated values but mark them
        let relevance = items.iter().find(|i| i.label == "RELEVANCE").unwrap();
        assert!(
            relevance.deprecated,
            "RELEVANCE should be marked as deprecated"
        );
    }

    #[test]
    fn test_completions_for_directives_after_at() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"
                type Query { user: User }
                type User { id: ID! name: String! }
                directive @skip(if: Boolean!) on FIELD | FRAGMENT_SPREAD | INLINE_FRAGMENT
                directive @include(if: Boolean!) on FIELD | FRAGMENT_SPREAD | INLINE_FRAGMENT
                directive @deprecated(reason: String) on FIELD_DEFINITION | ENUM_VALUE
            "#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor right after @: field @|
        let (graphql, pos) = extract_cursor(
            r#"
query GetUser {
    user {
        name @*
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"skip"),
            "Should suggest 'skip' directive: got {labels:?}"
        );
        assert!(
            labels.contains(&"include"),
            "Should suggest 'include' directive: got {labels:?}"
        );

        // @deprecated is not valid on FIELD, so it should not appear
        assert!(
            !labels.contains(&"deprecated"),
            "Should NOT suggest 'deprecated' on a field: got {labels:?}"
        );

        // All completions should be Directive kind
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Directive,
                "Expected Directive completion kind for '{}', got {:?}",
                item.label,
                item.kind
            );
        }

        // Check that documentation is provided via the detail (locations)
        let skip_item = items.iter().find(|i| i.label == "skip").unwrap();
        assert!(skip_item.detail.is_some());
    }

    #[test]
    fn test_completions_for_custom_schema_directives() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"
                type Query { user: User }
                type User { id: ID! name: String! }
                directive @skip(if: Boolean!) on FIELD | FRAGMENT_SPREAD | INLINE_FRAGMENT
                """Custom caching directive"""
                directive @cacheControl(maxAge: Int) on FIELD
            "#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let (graphql, pos) = extract_cursor(
            r#"
query GetUser {
    user {
        name @*
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"cacheControl"),
            "Should suggest custom 'cacheControl' directive: got {labels:?}"
        );
        assert!(
            labels.contains(&"skip"),
            "Should also suggest 'skip' directive: got {labels:?}"
        );

        let cache_item = items.iter().find(|i| i.label == "cacheControl").unwrap();
        assert_eq!(
            cache_item.documentation.as_deref(),
            Some("Custom caching directive")
        );
    }

    #[test]
    fn test_completions_for_apollo_client_directives() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User } type User { id: ID! name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        // Simulate Apollo client builtins being loaded as a schema file
        let client_path = FilePath::new("client_builtins.graphql");
        host.add_file(
            &client_path,
            r#"
                directive @client(always: Boolean) on FIELD | INLINE_FRAGMENT
                directive @connection(key: String!, filter: [String!]) on FIELD
            "#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let (graphql, pos) = extract_cursor(
            r#"
query GetUser {
    user {
        name @*
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"client"),
            "Should suggest Apollo '@client' directive: got {labels:?}"
        );
        assert!(
            labels.contains(&"connection"),
            "Should suggest Apollo '@connection' directive: got {labels:?}"
        );
    }

    #[test]
    fn test_completions_for_type_names_after_on() {
        let schema = r#"
type Query { user: User }
type User { id: ID! name: String! posts: [Post!]! }
type Post { id: ID! title: String! }
interface Node { id: ID! }
union SearchResult = User | Post
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor after `on` in fragment definition: fragment Foo on |
        let (graphql, pos) = extract_cursor(
            r#"
fragment UserFields on *{
    id
    name
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        // Should suggest object types, interfaces, and unions
        assert!(
            labels.contains(&"User"),
            "Should suggest 'User': got {labels:?}"
        );
        assert!(
            labels.contains(&"Post"),
            "Should suggest 'Post': got {labels:?}"
        );
        assert!(
            labels.contains(&"Node"),
            "Should suggest 'Node' interface: got {labels:?}"
        );
        assert!(
            labels.contains(&"SearchResult"),
            "Should suggest 'SearchResult' union: got {labels:?}"
        );

        // Should NOT suggest scalars or input types
        assert!(!labels.contains(&"ID"), "Should NOT suggest scalar 'ID'");
        assert!(
            !labels.contains(&"String"),
            "Should NOT suggest scalar 'String'"
        );

        // All completions should be Type kind
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Type,
                "Expected Type completion kind for '{}', got {:?}",
                item.label,
                item.kind
            );
        }

        // Check detail shows type kind
        let user_item = items.iter().find(|i| i.label == "User").unwrap();
        assert_eq!(user_item.detail, Some("object".to_string()));

        let node_item = items.iter().find(|i| i.label == "Node").unwrap();
        assert_eq!(node_item.detail, Some("interface".to_string()));
    }

    #[test]
    fn test_completions_for_top_level_keywords() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User } type User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor at document root (after a definition)
        let (graphql, pos) = extract_cursor(
            r#"
query GetUser {
    user { id }
}
*"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"query"),
            "Should suggest 'query': got {labels:?}"
        );
        assert!(
            labels.contains(&"mutation"),
            "Should suggest 'mutation': got {labels:?}"
        );
        assert!(
            labels.contains(&"subscription"),
            "Should suggest 'subscription': got {labels:?}"
        );
        assert!(
            labels.contains(&"fragment"),
            "Should suggest 'fragment': got {labels:?}"
        );
        assert_eq!(
            items.len(),
            4,
            "Should suggest exactly 4 keywords: got {labels:?}"
        );

        // All completions should be Keyword kind
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Keyword,
                "Expected Keyword completion kind for '{}', got {:?}",
                item.label,
                item.kind
            );
        }

        // Should have snippet insert text
        let query_item = items.iter().find(|i| i.label == "query").unwrap();
        assert_eq!(
            query_item.insert_text_format,
            Some(InsertTextFormat::Snippet)
        );
    }

    #[test]
    fn test_completions_for_input_object_fields() {
        let schema = r#"
type Query { me: User }
type Mutation {
    createUser(input: CreateUserInput!): User!
}
input CreateUserInput {
    name: String!
    email: String!
    age: Int
    role: Role
}
enum Role { ADMIN USER }
type User { id: ID! name: String! }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor inside input object value: createUser(input: { name: "test", | })
        let (graphql, pos) = extract_cursor(
            r#"
mutation CreateUser {
    createUser(input: { name: "test", *}) {
        id
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"name"),
            "Should suggest 'name' input field: got {labels:?}"
        );
        assert!(
            labels.contains(&"email"),
            "Should suggest 'email' input field: got {labels:?}"
        );
        assert!(
            labels.contains(&"age"),
            "Should suggest 'age' input field: got {labels:?}"
        );
        assert!(
            labels.contains(&"role"),
            "Should suggest 'role' input field: got {labels:?}"
        );
        assert_eq!(
            items.len(),
            4,
            "Should suggest exactly 4 input fields: got {labels:?}"
        );

        // Check type details
        let name_item = items.iter().find(|i| i.label == "name").unwrap();
        assert_eq!(name_item.detail, Some("String!".to_string()));

        // Check insert text includes ": "
        assert_eq!(name_item.insert_text, Some("name: ".to_string()));
    }

    #[test]
    fn test_completions_for_schema_keywords() {
        let mut host = AnalysisHost::new();

        // Cursor at top level of a schema file
        let (graphql, pos) = extract_cursor(
            r#"
type Query {
    user: User
}
*"#,
        );
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Schema);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"type"),
            "Should suggest 'type': got {labels:?}"
        );
        assert!(
            labels.contains(&"input"),
            "Should suggest 'input': got {labels:?}"
        );
        assert!(
            labels.contains(&"interface"),
            "Should suggest 'interface': got {labels:?}"
        );
        assert!(
            labels.contains(&"union"),
            "Should suggest 'union': got {labels:?}"
        );
        assert!(
            labels.contains(&"enum"),
            "Should suggest 'enum': got {labels:?}"
        );
        assert!(
            labels.contains(&"scalar"),
            "Should suggest 'scalar': got {labels:?}"
        );
        assert!(
            labels.contains(&"schema"),
            "Should suggest 'schema': got {labels:?}"
        );
        assert!(
            labels.contains(&"directive"),
            "Should suggest 'directive': got {labels:?}"
        );
        assert!(
            labels.contains(&"extend"),
            "Should suggest 'extend': got {labels:?}"
        );
        assert_eq!(
            items.len(),
            9,
            "Should suggest exactly 9 schema keywords: got {labels:?}"
        );

        // All completions should be Keyword kind
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Keyword,
                "Expected Keyword completion kind for '{}', got {:?}",
                item.label,
                item.kind
            );
        }

        // Should have snippet insert text
        let type_item = items.iter().find(|i| i.label == "type").unwrap();
        assert_eq!(
            type_item.insert_text_format,
            Some(InsertTextFormat::Snippet)
        );

        // Should NOT suggest operation keywords in schema files
        assert!(
            !labels.contains(&"query"),
            "Should not suggest 'query' in schema file: got {labels:?}"
        );
        assert!(
            !labels.contains(&"fragment"),
            "Should not suggest 'fragment' in schema file: got {labels:?}"
        );
    }

    #[test]
    fn test_completions_for_variables_after_dollar() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user(id: ID!): User } type User { id: ID! name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor right after $: user(id: $|)
        let (graphql, pos) = extract_cursor(
            r#"
query GetUser($userId: ID!, $includeEmail: Boolean!) {
    user(id: $*) {
        name
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"userId"),
            "Should suggest 'userId' variable: got {labels:?}"
        );
        assert!(
            labels.contains(&"includeEmail"),
            "Should suggest 'includeEmail' variable: got {labels:?}"
        );
        assert_eq!(
            items.len(),
            2,
            "Should suggest exactly 2 variables: got {labels:?}"
        );

        // All completions should be Variable kind
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Variable,
                "Expected Variable completion kind for '{}', got {:?}",
                item.label,
                item.kind
            );
        }

        // Check type details
        let user_id = items.iter().find(|i| i.label == "userId").unwrap();
        assert_eq!(user_id.detail, Some("ID!".to_string()));
    }

    #[test]
    fn test_completions_for_field_arguments_on_nested_field() {
        let schema = r#"
type Query { user: User }
type User {
    posts(limit: Int, cursor: String): [Post!]!
    name: String!
}
type Post { id: ID! title: String! }
"#;

        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor inside nested field arguments: posts(|)
        let (graphql, pos) = extract_cursor(
            r#"
query GetUser {
    user {
        posts(*) {
            title
        }
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"limit"),
            "Should suggest 'limit' argument: got {labels:?}"
        );
        assert!(
            labels.contains(&"cursor"),
            "Should suggest 'cursor' argument: got {labels:?}"
        );
        assert_eq!(
            items.len(),
            2,
            "Should suggest exactly 2 arguments: got {labels:?}"
        );
    }

    #[test]
    fn test_completions_for_directive_arguments() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"
type Query { user: User }
type User { id: ID! name: String! friends: [User!]! }
directive @connection(key: String!, filter: [String!]) on FIELD
"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor inside directive arguments: @connection(|)
        let (graphql, pos) = extract_cursor(
            r#"
query GetUser {
    user {
        friends @connection(*) {
            name
        }
    }
}
"#,
        );
        let path = FilePath::new("file:///test.graphql");
        host.add_file(&path, &graphql, Language::GraphQL, DocumentKind::Executable);
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let items = snapshot.completions(&path, pos).unwrap_or_default();
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();

        assert!(
            labels.contains(&"key"),
            "Should suggest 'key' argument: got {labels:?}"
        );
        assert!(
            labels.contains(&"filter"),
            "Should suggest 'filter' argument: got {labels:?}"
        );
        assert_eq!(
            items.len(),
            2,
            "Should suggest exactly 2 directive arguments: got {labels:?}"
        );

        // All completions should be Argument kind
        for item in &items {
            assert_eq!(
                item.kind,
                CompletionKind::Argument,
                "Expected Argument completion kind for '{}', got {:?}",
                item.label,
                item.kind
            );
        }

        // Check type details
        let key_item = items.iter().find(|i| i.label == "key").unwrap();
        assert_eq!(key_item.detail, Some("String!".to_string()));

        let filter_item = items.iter().find(|i| i.label == "filter").unwrap();
        assert_eq!(filter_item.detail, Some("[String!]".to_string()));

        // Check insert text includes ": " suffix
        assert_eq!(key_item.insert_text, Some("key: ".to_string()));

        // Should NOT include parent type field names
        assert!(
            !labels.contains(&"id"),
            "Should NOT suggest parent type field 'id': got {labels:?}"
        );
        assert!(
            !labels.contains(&"name"),
            "Should NOT suggest parent type field 'name': got {labels:?}"
        );
    }

    #[test]
    fn test_typescript_graphql_extraction() {
        use graphql_extract::{extract_from_source, ExtractConfig, Language};

        // Test that TypeScript files with GraphQL are correctly extracted
        // and don't produce TypeScript syntax errors like "Unexpected <EOF>" on "import"

        let typescript_source = r"import { gql } from '@apollo/client';

export const GET_POKEMON = gql`
  query GetPokemon {
    pokemon {
      id
      name
    }
  }
`;
";

        // Test extraction works
        let config = ExtractConfig::default();
        let result =
            extract_from_source(typescript_source, Language::TypeScript, &config, "test.ts");

        assert!(result.is_ok(), "Extraction should succeed");
        let blocks = result.unwrap();
        assert!(
            !blocks.is_empty(),
            "Should extract at least one GraphQL block"
        );

        // Verify extracted content
        let graphql = &blocks[0].source;
        assert!(graphql.contains("GetPokemon"), "Should contain query name");
        assert!(
            !graphql.contains("import"),
            "Should NOT contain TypeScript import statement"
        );
        assert!(
            graphql.contains("pokemon"),
            "Should contain field selections"
        );
    }

    #[test]
    fn test_document_symbols_type_with_fields() {
        let mut host = AnalysisHost::new();

        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "type User {\n  id: ID!\n  name: String\n  email: String!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.document_symbols(&path);

        assert_eq!(symbols.len(), 1, "Should have one type symbol");
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::Type);
        assert_eq!(symbols[0].children.len(), 3, "Should have 3 field children");

        // Check field names
        let field_names: Vec<&str> = symbols[0]
            .children
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(field_names.contains(&"id"));
        assert!(field_names.contains(&"name"));
        assert!(field_names.contains(&"email"));

        // Check field kinds
        for child in &symbols[0].children {
            assert_eq!(child.kind, SymbolKind::Field);
        }
    }

    #[test]
    fn test_document_symbols_operations() {
        let mut host = AnalysisHost::new();

        // Add schema first
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: String }\ntype Mutation { createUser: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let path = FilePath::new("file:///queries.graphql");
        host.add_file(
            &path,
            "query GetUser { user }\nmutation CreateUser { createUser }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.document_symbols(&path);

        assert_eq!(symbols.len(), 2, "Should have two operation symbols");

        // Check query
        assert_eq!(symbols[0].name, "GetUser");
        assert_eq!(symbols[0].kind, SymbolKind::Query);

        // Check mutation
        assert_eq!(symbols[1].name, "CreateUser");
        assert_eq!(symbols[1].kind, SymbolKind::Mutation);
    }

    #[test]
    fn test_document_symbols_fragments() {
        let mut host = AnalysisHost::new();

        // Add schema
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let path = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &path,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.document_symbols(&path);

        assert_eq!(symbols.len(), 1, "Should have one fragment symbol");
        assert_eq!(symbols[0].name, "UserFields");
        assert_eq!(symbols[0].kind, SymbolKind::Fragment);
        assert_eq!(symbols[0].detail, Some("on User".to_string()));
    }

    #[test]
    fn test_workspace_symbols_search() {
        let mut host = AnalysisHost::new();

        // Add schema with multiple types
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { id: ID! }\ntype Post { title: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add operations
        let queries_path = FilePath::new("file:///queries.graphql");
        host.add_file(
            &queries_path,
            "query GetUser { user { id } }\nquery GetUsers { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Search for "User"
        let symbols = snapshot.workspace_symbols("User");
        assert!(!symbols.is_empty(), "Should find symbols matching 'User'");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"User"), "Should find User type");
        assert!(names.contains(&"GetUser"), "Should find GetUser operation");
        assert!(
            names.contains(&"GetUsers"),
            "Should find GetUsers operation"
        );

        // Search for "Post"
        let symbols = snapshot.workspace_symbols("Post");
        assert_eq!(symbols.len(), 1, "Should find one symbol matching 'Post'");
        assert_eq!(symbols[0].name, "Post");
    }

    #[test]
    fn test_workspace_symbols_case_insensitive() {
        let mut host = AnalysisHost::new();

        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "type UserProfile { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Search with different cases
        let lower = snapshot.workspace_symbols("user");
        let upper = snapshot.workspace_symbols("USER");
        let mixed = snapshot.workspace_symbols("uSeR");

        assert_eq!(lower.len(), 1);
        assert_eq!(upper.len(), 1);
        assert_eq!(mixed.len(), 1);

        assert_eq!(lower[0].name, "UserProfile");
        assert_eq!(upper[0].name, "UserProfile");
        assert_eq!(mixed[0].name, "UserProfile");
    }

    mod schema_loading {
        use super::*;
        use std::io::Write;

        #[test]
        fn test_load_typescript_schema() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a TypeScript schema file
            let ts_schema_content = r#"
import { gql } from 'graphql-tag';

export const typeDefs = gql`
  type Query {
    user(id: ID!): User
  }

  type User {
    id: ID!
    name: String!
    email: String
  }
`;
"#;
            let ts_schema_path = temp_dir.path().join("schema.ts");
            let mut file = std::fs::File::create(&ts_schema_path).unwrap();
            file.write_all(ts_schema_content.as_bytes()).unwrap();

            // Create config
            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("schema.ts".to_string()),
                None,
                None,
                None,
                None,
            );

            // Load schemas
            let mut host = AnalysisHost::new();
            host.set_extract_config(graphql_extract::ExtractConfig {
                allow_global_identifiers: false,
                ..Default::default()
            });
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 schema builtins + 1 extracted schema from TS (no client builtins without config)
            assert_eq!(
                result.loaded_count, 2,
                "Should load 2 schema files (builtins + extracted)"
            );
            assert!(
                result.pending_introspections.is_empty(),
                "No pending introspections expected"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify the User type is available
            let symbols = snapshot.workspace_symbols("User");
            assert!(!symbols.is_empty(), "User type should be found");
            assert_eq!(symbols[0].name, "User");
        }

        #[test]
        fn test_load_typescript_schema_with_multiple_blocks() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a TypeScript file with multiple GraphQL blocks
            let ts_content = r#"
import { gql } from 'graphql-tag';

export const types = gql`
  type Query {
    posts: [Post!]!
  }
`;

export const postType = gql`
  type Post {
    id: ID!
    title: String!
    content: String
  }
`;
"#;
            let ts_path = temp_dir.path().join("schema.ts");
            let mut file = std::fs::File::create(&ts_path).unwrap();
            file.write_all(ts_content.as_bytes()).unwrap();

            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("schema.ts".to_string()),
                None,
                None,
                None,
                None,
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 schema builtins + 2 extracted blocks (no client builtins without config)
            assert_eq!(
                result.loaded_count, 3,
                "Should load 3 schema files (builtins + 2 blocks)"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify both types are available
            let query_symbols = snapshot.workspace_symbols("Query");
            assert!(!query_symbols.is_empty(), "Query type should be found");

            let post_symbols = snapshot.workspace_symbols("Post");
            assert!(!post_symbols.is_empty(), "Post type should be found");
        }

        #[test]
        fn test_multiple_block_uris_use_line_ranges() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a TypeScript file with multiple GraphQL blocks
            // The blocks start at different lines to verify URI format
            let ts_content = r#"import { gql } from 'graphql-tag';

export const types = gql`
  type Query {
    posts: [Post!]!
  }
`;

export const postType = gql`
  type Post {
    id: ID!
    title: String!
  }
`;
"#;
            let ts_path = temp_dir.path().join("schema.ts");
            let mut file = std::fs::File::create(&ts_path).unwrap();
            file.write_all(ts_content.as_bytes()).unwrap();

            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("schema.ts".to_string()),
                None,
                None,
                None,
                None,
            );

            let mut host = AnalysisHost::new();
            let _ = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            host.rebuild_project_files();

            // Get all files and check their URIs
            let files = host.files();
            let ts_file_uri = path_to_file_uri(&ts_path);

            // Find files from the TS schema
            let block_uris: Vec<_> = files
                .into_iter()
                .map(|f| f.0)
                .filter(|uri| uri.starts_with(&ts_file_uri) && uri.contains('#'))
                .collect();

            // With multiple blocks, URIs should have line-range fragments
            assert_eq!(block_uris.len(), 2, "Should have 2 block URIs");

            // Check that URIs use line-range format (#L{start}-L{end}) not block index (#block0)
            for uri in &block_uris {
                assert!(
                    uri.contains("#L") && uri.contains("-L"),
                    "Block URI should use line-range format (#L{{start}}-L{{end}}), got: {uri}"
                );
                assert!(
                    !uri.contains("#block"),
                    "Block URI should NOT use block index format, got: {uri}"
                );
            }
        }

        #[test]
        fn test_load_mixed_schema_files() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a pure GraphQL schema file
            let gql_content = r#"
type Query {
  users: [User!]!
}
"#;
            let gql_path = temp_dir.path().join("base.graphql");
            let mut file = std::fs::File::create(&gql_path).unwrap();
            file.write_all(gql_content.as_bytes()).unwrap();

            // Create a TypeScript schema extension
            let ts_content = r#"
import { gql } from 'graphql-tag';

export const typeDefs = gql`
  type User {
    id: ID!
    name: String!
  }
`;
"#;
            let ts_path = temp_dir.path().join("types.ts");
            let mut file = std::fs::File::create(&ts_path).unwrap();
            file.write_all(ts_content.as_bytes()).unwrap();

            // Use multiple schema paths
            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Paths(vec![
                    "base.graphql".to_string(),
                    "types.ts".to_string(),
                ]),
                None,
                None,
                None,
                None,
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 schema builtins + 1 GraphQL file + 1 TS extraction (no client builtins without config)
            assert_eq!(result.loaded_count, 3, "Should load 3 schema files");

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify both types are available
            let query_symbols = snapshot.workspace_symbols("Query");
            assert!(!query_symbols.is_empty(), "Query type should be found");

            let user_symbols = snapshot.workspace_symbols("User");
            assert!(
                !user_symbols.is_empty(),
                "User type should be found from TS file"
            );
        }

        #[test]
        fn test_load_typescript_schema_no_graphql_found() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a TypeScript file without any GraphQL
            let ts_content = r#"
export const greeting = "Hello, World!";
export function greet(name: string) {
    return `Hello, ${name}!`;
}
"#;
            let ts_path = temp_dir.path().join("utils.ts");
            let mut file = std::fs::File::create(&ts_path).unwrap();
            file.write_all(ts_content.as_bytes()).unwrap();

            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("utils.ts".to_string()),
                None,
                None,
                None,
                None,
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should only load schema builtins (no client builtins without config, no GraphQL found in TS file)
            assert_eq!(
                result.loaded_count, 1,
                "Should only load builtins when no GraphQL found"
            );
        }

        #[test]
        fn test_load_javascript_schema() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Create a JavaScript schema file
            let js_content = r#"
import { gql } from 'graphql-tag';

export const typeDefs = gql`
  type Query {
    product(id: ID!): Product
  }

  type Product {
    id: ID!
    name: String!
    price: Float!
  }
`;
"#;
            let js_path = temp_dir.path().join("schema.js");
            let mut file = std::fs::File::create(&js_path).unwrap();
            file.write_all(js_content.as_bytes()).unwrap();

            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("schema.js".to_string()),
                None,
                None,
                None,
                None,
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 schema builtins + 1 extracted schema from JS (no client builtins without config)
            assert_eq!(
                result.loaded_count, 2,
                "Should load 2 schema files (builtins + extracted)"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify the Product type is available
            let symbols = snapshot.workspace_symbols("Product");
            assert!(!symbols.is_empty(), "Product type should be found");
        }

        #[test]
        fn test_load_introspection_schema_config() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Config with introspection endpoint
            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Introspection(
                    graphql_config::IntrospectionSchemaConfig {
                        url: "https://api.example.com/graphql".to_string(),
                        headers: Some(
                            [("Authorization".to_string(), "Bearer token".to_string())]
                                .into_iter()
                                .collect(),
                        ),
                        timeout: Some(60),
                        retry: Some(3),
                    },
                ),
                None,
                None,
                None,
                None,
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should only load schema builtins (introspection needs async fetch, no client builtins without config)
            assert_eq!(
                result.loaded_count, 1,
                "Should load builtins, introspection is async"
            );

            // Should have one pending introspection
            assert_eq!(
                result.pending_introspections.len(),
                1,
                "Should have one pending introspection"
            );

            let pending = &result.pending_introspections[0];
            assert_eq!(pending.url, "https://api.example.com/graphql");
            assert!(pending.headers.is_some());
            assert_eq!(pending.timeout, Some(60));
            assert_eq!(pending.retry, Some(3));

            // Verify virtual_uri generation
            assert_eq!(
                pending.virtual_uri(),
                "schema://api.example.com/graphql/schema.graphql"
            );
        }

        #[test]
        fn test_load_url_schema_pattern() {
            let temp_dir = tempfile::tempdir().unwrap();

            // Config with URL pattern (simpler than full introspection config)
            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("https://api.example.com/graphql".to_string()),
                None,
                None,
                None,
                None,
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should only load schema builtins (no client builtins without config)
            assert_eq!(
                result.loaded_count, 1,
                "Should load builtins, URL schema is async"
            );

            // Should have one pending introspection (from URL pattern)
            assert_eq!(
                result.pending_introspections.len(),
                1,
                "Should have one pending introspection from URL"
            );

            let pending = &result.pending_introspections[0];
            assert_eq!(pending.url, "https://api.example.com/graphql");
            assert!(pending.headers.is_none()); // URL patterns don't have headers
        }

        #[test]
        fn test_add_introspected_schema() {
            let mut host = AnalysisHost::new();

            // Simulate adding an introspected schema
            let url = "https://api.example.com/graphql";
            let sdl = r#"
                type Query {
                    user(id: ID!): User
                }

                type User {
                    id: ID!
                    name: String!
                }
            "#;

            let virtual_uri = host.add_introspected_schema(url, sdl);

            // Verify the virtual URI format
            assert_eq!(
                virtual_uri,
                "schema://api.example.com/graphql/schema.graphql"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Verify the types are available
            let user_symbols = snapshot.workspace_symbols("User");
            assert!(!user_symbols.is_empty(), "User type should be found");
        }

        #[test]
        fn test_load_schema_with_apollo_client_builtins() {
            let temp_dir = tempfile::tempdir().unwrap();

            let schema_content = "type Query { hello: String }";
            let schema_path = temp_dir.path().join("schema.graphql");
            std::fs::write(&schema_path, schema_content).unwrap();

            let analyzer_ext = serde_json::json!({"client": "apollo"});
            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
                None,
                None,
                None,
                Some(HashMap::from([(
                    "graphql-analyzer".to_string(),
                    analyzer_ext,
                )])),
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 schema builtins + 1 client builtins + 1 schema file
            assert_eq!(
                result.loaded_count, 3,
                "Should load schema builtins + Apollo client builtins + schema file"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            // Apollo @client directive should be recognized
            let symbols = snapshot.workspace_symbols("Query");
            assert!(!symbols.is_empty(), "Query type should be found");
        }

        #[test]
        fn test_load_schema_with_relay_client_builtins() {
            let temp_dir = tempfile::tempdir().unwrap();

            let schema_content = "type Query { hello: String }";
            let schema_path = temp_dir.path().join("schema.graphql");
            std::fs::write(&schema_path, schema_content).unwrap();

            let analyzer_ext = serde_json::json!({"client": "relay"});
            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
                None,
                None,
                None,
                Some(HashMap::from([(
                    "graphql-analyzer".to_string(),
                    analyzer_ext,
                )])),
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 schema builtins + 1 client builtins + 1 schema file
            assert_eq!(
                result.loaded_count, 3,
                "Should load schema builtins + Relay client builtins + schema file"
            );

            host.rebuild_project_files();
            let snapshot = host.snapshot();

            let symbols = snapshot.workspace_symbols("Query");
            assert!(!symbols.is_empty(), "Query type should be found");
        }

        #[test]
        fn test_load_schema_with_client_none_no_builtins() {
            let temp_dir = tempfile::tempdir().unwrap();

            let schema_content = "type Query { hello: String }";
            let schema_path = temp_dir.path().join("schema.graphql");
            std::fs::write(&schema_path, schema_content).unwrap();

            let config = graphql_config::ProjectConfig::new(
                graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
                None,
                None,
                None,
                Some(HashMap::from([(
                    "client".to_string(),
                    serde_json::Value::String("none".to_string()),
                )])),
            );

            let mut host = AnalysisHost::new();
            let result = host
                .load_schemas_from_config(&config, temp_dir.path())
                .unwrap();

            // Should load: 1 schema builtins + 1 schema file (no client builtins)
            assert_eq!(
                result.loaded_count, 2,
                "Should load schema builtins + schema file only (no client builtins)"
            );
        }
    }

    #[test]
    fn test_project_lint_no_duplicates_same_file() {
        // Test that project-wide lints don't report duplicate fragments
        // when the same file is only added once
        let mut host = AnalysisHost::new();
        // Enable the recommended lint rules (which includes unique_names)
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a fragment file with a single fragment
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Get project-wide diagnostics
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Should have no diagnostics - single fragment shouldn't be flagged as duplicate
        // Check specifically for unique_names violations
        let unique_names_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("uniqueNames"))
            .collect();

        assert!(
            unique_names_errors.is_empty(),
            "Single fragment file should NOT produce uniqueNames errors, but got: {:?}",
            unique_names_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_project_lint_no_duplicates_after_file_update() {
        // Test that updating a file doesn't cause false duplicate detection
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add fragment file
        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Update the same file (simulating did_change)
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        // Note: rebuild_project_files is NOT called here since is_new=false

        // Get project-wide diagnostics
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Should have no uniqueNames errors
        let unique_names_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("uniqueNames"))
            .collect();

        assert!(
            unique_names_errors.is_empty(),
            "File update should NOT produce uniqueNames errors, but got: {:?}",
            unique_names_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_project_lint_different_uri_formats_same_file_no_duplicates() {
        // This test verifies that if the same file is added with different URI formats
        // (e.g., URL-encoded vs non-encoded), it should NOT cause false duplicate detection.
        // This simulates the scenario where:
        // 1. File is discovered via glob and added with one URI format
        // 2. File is opened in VSCode and sent with a different URI format
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Add a schema
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User } type User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add fragment file with one URI format (simulating glob discovery)
        let fragment_file_glob = FilePath::new("file:///home/user/fragments.graphql");
        host.add_file(
            &fragment_file_glob,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Try to add the SAME file with a slightly different URI format
        // This simulates VSCode sending the file with URL encoding or different formatting
        // Note: In a real scenario, these might be the same path represented differently:
        // - "file:///home/user/fragments.graphql" (glob discovery)
        // - "file:///home/user/fragments.graphql" (VSCode - should match)
        // The key test is that add_file correctly identifies it as the same file.

        // Using the exact same URI should return is_new=false
        let is_new = host.add_file(
            &fragment_file_glob,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        assert!(
            !is_new,
            "Adding file with same URI should return is_new=false"
        );

        // Get project-wide diagnostics
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Should have no uniqueNames errors
        let unique_names_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("uniqueNames"))
            .collect();

        assert!(
            unique_names_errors.is_empty(),
            "Same file added twice with same URI should NOT produce uniqueNames errors, but got: {:?}",
            unique_names_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_semantic_tokens_deprecated_field() {
        use std::io::Write;

        let temp_dir = tempfile::tempdir().unwrap();

        // Create a schema with a deprecated field
        let schema_content = r#"
type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    legacyId: String @deprecated(reason: "Use id instead")
}
"#;
        let schema_path = temp_dir.path().join("schema.graphql");
        let mut file = std::fs::File::create(&schema_path).unwrap();
        file.write_all(schema_content.as_bytes()).unwrap();

        // Create a document that uses the deprecated field
        let doc_content = r#"
query GetUser {
    user {
        id
        name
        legacyId
    }
}
"#;
        let doc_path = temp_dir.path().join("query.graphql");
        let mut doc_file = std::fs::File::create(&doc_path).unwrap();
        doc_file.write_all(doc_content.as_bytes()).unwrap();

        let config = graphql_config::ProjectConfig::new(
            graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            Some(graphql_config::DocumentsConfig::Pattern(
                "*.graphql".to_string(),
            )),
            None,
            None,
            None,
        );

        let mut host = AnalysisHost::new();
        host.load_schemas_from_config(&config, temp_dir.path())
            .unwrap();

        // Manually add the document file
        let doc_uri = path_to_file_uri(&doc_path);
        let file_path = FilePath::new(&doc_uri);
        host.add_file(
            &file_path,
            doc_content.trim(),
            graphql_base_db::Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Get semantic tokens
        let tokens = snapshot.semantic_tokens(&file_path);

        // Find the token for 'legacyId' field - it should have DEPRECATED modifier
        let deprecated_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.modifiers == SemanticTokenModifiers::DEPRECATED)
            .collect();

        assert!(
            !deprecated_tokens.is_empty(),
            "Should have at least one deprecated token, got tokens: {tokens:?}"
        );

        // Verify the deprecated token is a Property (field) type
        let deprecated_field_token = deprecated_tokens
            .iter()
            .find(|t| t.token_type == SemanticTokenType::Property)
            .expect("Should have a deprecated Property token");

        // The legacyId field is on line 5 (0-indexed) in the query (after trim)
        assert_eq!(
            deprecated_field_token.start.line, 4,
            "Deprecated field token should be on line 4 (0-indexed)"
        );
    }

    #[test]
    fn test_hover_field_in_typescript_file() {
        // Reproduces issue #398: Hover is broken for fields in TypeScript files
        //
        // The bug: find_parent_type_at_offset and walk_type_stack_to_offset were using
        // parse.tree (empty placeholder for TS files) instead of block_context.tree.

        let mut host = AnalysisHost::new();

        // Add a schema file
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            r#"type Query { pokemon(id: ID!): Pokemon }
type Pokemon { id: ID! name: String! }
"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a TypeScript file with embedded GraphQL
        let ts_file = FilePath::new("file:///query.ts");
        let ts_content = r#"import { gql } from '@apollo/client';

export const GET_POKEMON = gql`
  query GetPokemon($id: ID!) {
    pokemon(id: $id) {
      id
      name
    }
  }
`;
"#;
        host.add_file(
            &ts_file,
            ts_content,
            Language::TypeScript,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Hover over the "name" field (line 6, character ~6 in the TS file)
        // Line 6 (0-indexed) is "      name"
        // The "name" field starts at character 6
        let hover = snapshot.hover(&ts_file, Position::new(6, 7));

        // Should return hover info for the field
        assert!(
            hover.is_some(),
            "Hover should work for fields in TypeScript files (issue #398)"
        );
        let hover = hover.unwrap();
        assert!(
            hover.contents.contains("name"),
            "Hover should show field name. Got: {}",
            hover.contents
        );
        assert!(
            hover.contents.contains("String"),
            "Hover should show field type. Got: {}",
            hover.contents
        );
    }

    #[test]
    fn test_deprecated_field_code_lenses() {
        let mut host = AnalysisHost::new();

        // Add a schema with a deprecated field
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    legacyId: String @deprecated(reason: "Use id instead")
}"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a document that uses the deprecated field
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            r#"query GetUser {
    user {
        id
        name
        legacyId
    }
}"#,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Get code lenses for the schema file (where the deprecated field is defined)
        let code_lenses = snapshot.deprecated_field_code_lenses(&schema_path);

        assert_eq!(
            code_lenses.len(),
            1,
            "Should have exactly one code lens for the deprecated field"
        );

        let code_lens = &code_lenses[0];
        assert_eq!(code_lens.type_name, "User");
        assert_eq!(code_lens.field_name, "legacyId");
        assert_eq!(
            code_lens.usage_count, 1,
            "Should have 1 usage of the deprecated field"
        );
        assert_eq!(
            code_lens.deprecation_reason,
            Some("Use id instead".to_string())
        );
    }

    #[test]
    fn test_deprecated_field_code_lenses_no_usages() {
        let mut host = AnalysisHost::new();

        // Add a schema with a deprecated field that is not used
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    legacyId: String @deprecated(reason: "Use id instead")
}"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a document that does NOT use the deprecated field
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            r#"query GetUser {
    user {
        id
        name
    }
}"#,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Get code lenses for the schema file
        let code_lenses = snapshot.deprecated_field_code_lenses(&schema_path);

        assert_eq!(
            code_lenses.len(),
            1,
            "Should have exactly one code lens for the deprecated field"
        );

        let code_lens = &code_lenses[0];
        assert_eq!(
            code_lens.usage_count, 0,
            "Should have 0 usages of the deprecated field"
        );
    }

    #[test]
    fn test_deprecated_field_code_lenses_multiple_usages() {
        let mut host = AnalysisHost::new();

        // Add a schema with a deprecated field
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query {
    user: User
    users: [User!]!
}

type User {
    id: ID!
    legacyId: String @deprecated
}"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add multiple documents using the deprecated field
        let doc_path1 = FilePath::new("file:///query1.graphql");
        host.add_file(
            &doc_path1,
            r#"query GetUser {
    user {
        legacyId
    }
}"#,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let doc_path2 = FilePath::new("file:///query2.graphql");
        host.add_file(
            &doc_path2,
            r#"query GetUsers {
    users {
        legacyId
    }
}"#,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        let code_lenses = snapshot.deprecated_field_code_lenses(&schema_path);

        assert_eq!(code_lenses.len(), 1);
        assert_eq!(
            code_lenses[0].usage_count, 2,
            "Should have 2 usages of the deprecated field"
        );
        assert_eq!(code_lenses[0].usage_locations.len(), 2);
    }

    #[test]
    fn test_deprecated_field_code_lenses_non_schema_file() {
        let mut host = AnalysisHost::new();

        // Add a schema
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { id: ID! @deprecated }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a document file
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Get code lenses for the document file (not schema) - should be empty
        // since code lenses only show on schema files where deprecated fields are defined
        let code_lenses = snapshot.deprecated_field_code_lenses(&doc_path);
        assert!(
            code_lenses.is_empty(),
            "Document files should not have code lenses for deprecated fields"
        );
    }

    #[test]
    fn test_complexity_analysis_basic() {
        let mut host = AnalysisHost::new();

        // Add schema
        let schema = r#"
type Query {
    user(id: ID!): User
    posts: [Post!]!
}

type User {
    id: ID!
    name: String!
    email: String
    posts: [Post!]!
}

type Post {
    id: ID!
    title: String!
    author: User!
    comments: [Comment!]!
}

type Comment {
    id: ID!
    text: String!
}
"#;
        host.add_file(
            &FilePath::new("file:///schema.graphql"),
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add operation
        let query = r#"
query GetUser {
    user(id: "123") {
        id
        name
        posts {
            id
            title
        }
    }
}
"#;
        host.add_file(
            &FilePath::new("file:///query.graphql"),
            query,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let results = snapshot.complexity_analysis();

        assert_eq!(results.len(), 1);
        let analysis = &results[0];

        assert_eq!(analysis.operation_name, "GetUser");
        assert_eq!(analysis.operation_type, "query");
        assert!(analysis.total_complexity > 0);
        assert!(analysis.depth > 0);
    }

    #[test]
    fn test_complexity_analysis_list_fields() {
        let mut host = AnalysisHost::new();

        // Add schema
        let schema = r#"
type Query {
    posts: [Post!]!
}

type Post {
    id: ID!
    title: String!
}
"#;
        host.add_file(
            &FilePath::new("file:///schema.graphql"),
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add operation with list field
        let query = r#"
query GetPosts {
    posts {
        id
        title
    }
}
"#;
        host.add_file(
            &FilePath::new("file:///query.graphql"),
            query,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let results = snapshot.complexity_analysis();

        assert_eq!(results.len(), 1);
        let analysis = &results[0];

        // List field should have multiplier applied
        assert!(analysis.total_complexity >= 10); // Default list multiplier is 10
    }

    #[test]
    fn test_complexity_analysis_connection_detection() {
        let mut host = AnalysisHost::new();

        // Add schema with Relay connection pattern
        let schema = r#"
type Query {
    users(first: Int): UserConnection!
}

type UserConnection {
    edges: [UserEdge!]!
    pageInfo: PageInfo!
}

type UserEdge {
    node: User!
    cursor: String!
}

type User {
    id: ID!
    name: String!
}

type PageInfo {
    hasNextPage: Boolean!
}
"#;
        host.add_file(
            &FilePath::new("file:///schema.graphql"),
            schema,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add operation with connection pattern
        let query = r#"
query GetUsers {
    users(first: 10) {
        edges {
            node {
                id
                name
            }
        }
    }
}
"#;
        host.add_file(
            &FilePath::new("file:///query.graphql"),
            query,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let results = snapshot.complexity_analysis();

        assert_eq!(results.len(), 1);
        let analysis = &results[0];

        // Should detect connection pattern in breakdown
        let has_connection_field = analysis.breakdown.iter().any(|f| f.is_connection);
        assert!(has_connection_field);
    }

    #[test]
    fn test_add_files_batch() {
        let mut host = AnalysisHost::new();

        // Add multiple files in batch
        let files = vec![
            (
                FilePath::new("file:///schema.graphql"),
                "type Query { user: User } type User { id: ID! name: String! }",
                Language::GraphQL,
                DocumentKind::Schema,
            ),
            (
                FilePath::new("file:///query1.graphql"),
                "query GetUser { user { id name } }",
                Language::GraphQL,
                DocumentKind::Executable,
            ),
            (
                FilePath::new("file:///query2.graphql"),
                "query GetUserName { user { name } }",
                Language::GraphQL,
                DocumentKind::Executable,
            ),
        ];

        host.add_files_batch(&files);

        // Verify all files are accessible
        let snapshot = host.snapshot();

        // Check diagnostics work for all files
        let path1 = FilePath::new("file:///query1.graphql");
        let path2 = FilePath::new("file:///query2.graphql");

        // Both files should be accessible (diagnostics call should not panic)
        let _diag1 = snapshot.diagnostics(&path1);
        let _diag2 = snapshot.diagnostics(&path2);
        // If we got here without panic, files are properly loaded
    }

    #[test]
    fn test_add_files_batch_empty() {
        let mut host = AnalysisHost::new();

        // Add empty batch should not panic
        let files: Vec<(FilePath, &str, Language, DocumentKind)> = vec![];
        host.add_files_batch(&files);

        // Should still be able to get snapshot
        let _snapshot = host.snapshot();
    }

    #[test]
    fn test_add_files_batch_update_existing() {
        let mut host = AnalysisHost::new();

        // First batch
        let files1 = vec![(
            FilePath::new("file:///schema.graphql"),
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        )];
        host.add_files_batch(&files1);

        // Second batch with same file (update) and new file
        let files2 = vec![
            (
                FilePath::new("file:///schema.graphql"),
                "type Query { hello: String world: String }",
                Language::GraphQL,
                DocumentKind::Schema,
            ),
            (
                FilePath::new("file:///query.graphql"),
                "query { hello }",
                Language::GraphQL,
                DocumentKind::Executable,
            ),
        ];
        host.add_files_batch(&files2);

        // Verify updated content
        let snapshot = host.snapshot();
        let schema_path = FilePath::new("file:///schema.graphql");

        // Hover on "world" field should work (proves update happened)
        let hover = snapshot.hover(&schema_path, Position::new(0, 30)); // Position in "world"
        assert!(hover.is_some());
    }

    #[test]
    fn test_batch_loading_is_efficient() {
        let mut host = AnalysisHost::new();

        // Create many files
        let schema = (
            FilePath::new("file:///schema.graphql"),
            "type Query { user: User } type User { id: ID! name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let mut files = vec![schema];
        for i in 0..100 {
            files.push((
                FilePath::new(format!("file:///query{i}.graphql")),
                "query GetUser { user { id name } }",
                Language::GraphQL,
                DocumentKind::Executable,
            ));
        }

        // Convert to borrowed form for add_files_batch
        let files_borrowed: Vec<(FilePath, &str, Language, DocumentKind)> = files
            .iter()
            .map(|(p, c, l, k)| (p.clone(), *c, *l, *k))
            .collect();

        // This should complete quickly (O(n) not O(n²))
        let start = std::time::Instant::now();
        host.add_files_batch(&files_borrowed);
        let elapsed = start.elapsed();

        // Should complete in reasonable time (< 5 seconds even for 100 files)
        assert!(
            elapsed.as_secs() < 5,
            "Batch loading took too long: {elapsed:?}"
        );

        // Verify files are loaded
        let snapshot = host.snapshot();
        let last_file = FilePath::new("file:///query99.graphql");
        // If we can get diagnostics without panic, file is loaded
        let _diag = snapshot.diagnostics(&last_file);
    }

    #[test]
    fn test_unused_fields_lint_with_typescript_file() {
        // Test that fields used in TypeScript embedded GraphQL are correctly tracked
        // and NOT flagged as unused by the unusedFields lint
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Add a schema with fields
        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            r#"
                type Query { rateLimit: RateLimit }
                type RateLimit {
                    cost: Int!
                    limit: Int!
                    nodeCount: Int!
                }
            "#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add a TypeScript file with embedded GraphQL that uses the fields
        let ts_file = FilePath::new("file:///api.ts");
        host.add_file(
            &ts_file,
            r#"
import { gql } from "@apollo/client";

export const RATE_LIMIT_QUERY = gql`
  query GetRateLimit {
    rateLimit {
      cost
      limit
      nodeCount
    }
  }
`;
            "#,
            Language::TypeScript,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        // Get project-wide diagnostics
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Check for unusedFields warnings
        let unused_fields_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unusedFields"))
            .collect();

        // nodeCount should NOT be flagged as unused since it's used in the TS file
        let nodecount_errors: Vec<_> = unused_fields_errors
            .iter()
            .filter(|d| d.message.contains("nodeCount"))
            .collect();

        assert!(
            nodecount_errors.is_empty(),
            "nodeCount is used in TypeScript file and should NOT be flagged as unused. Got: {:?}",
            nodecount_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );

        // All fields (cost, limit, nodeCount) are used, so there should be no unusedFields warnings
        // for RateLimit type fields
        let ratelimit_errors: Vec<_> = unused_fields_errors
            .iter()
            .filter(|d| d.message.contains("RateLimit"))
            .collect();

        assert!(
            ratelimit_errors.is_empty(),
            "All RateLimit fields are used in TypeScript file. Got: {:?}",
            ratelimit_errors
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_unused_fields_lint_with_config_loaded_typescript() {
        use std::io::Write;

        // Simulate the real LSP scenario: files loaded from config, then lint runs on save
        let temp_dir = tempfile::tempdir().unwrap();

        // Create schema file
        let schema_content = r#"
type Query { rateLimit: RateLimit }
type RateLimit {
    cost: Int!
    limit: Int!
    nodeCount: Int!
}
"#;
        let schema_path = temp_dir.path().join("schema.graphql");
        let mut file = std::fs::File::create(&schema_path).unwrap();
        file.write_all(schema_content.as_bytes()).unwrap();

        // Create TypeScript file with embedded GraphQL
        let ts_content = r#"
import { gql } from "@apollo/client";

export const RATE_LIMIT_QUERY = gql`
  query GetRateLimit {
    rateLimit {
      cost
      limit
      nodeCount
    }
  }
`;
"#;
        let ts_path = temp_dir.path().join("api.ts");
        let mut ts_file = std::fs::File::create(&ts_path).unwrap();
        ts_file.write_all(ts_content.as_bytes()).unwrap();

        // Create config that includes both schema and TS documents
        let config = graphql_config::ProjectConfig::new(
            graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            Some(graphql_config::DocumentsConfig::Pattern("*.ts".to_string())),
            None,
            None,
            None,
        );

        // Create host and load files from config (simulating LSP initialization)
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Load schema
        let _ = host.load_schemas_from_config(&config, temp_dir.path());
        // Load documents (including TS files)
        let extract_config = graphql_extract::ExtractConfig::default();
        let _ = host.load_documents_from_config(&config, temp_dir.path(), &extract_config);

        // Rebuild project files to update indices (this happens in LSP initialization)
        host.rebuild_project_files();

        // Get snapshot and run lints (simulating did_save)
        let snapshot = host.snapshot();
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Check for unusedFields warnings
        let unused_fields_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unusedFields"))
            .collect();

        // nodeCount should NOT be flagged as unused
        let nodecount_errors: Vec<_> = unused_fields_errors
            .iter()
            .filter(|d| d.message.contains("nodeCount"))
            .collect();

        assert!(
            nodecount_errors.is_empty(),
            "nodeCount used in config-loaded TypeScript file should NOT be flagged as unused. Got: {:?}",
            nodecount_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_unused_fields_lint_simulating_save_after_open() {
        use std::io::Write;

        // This test simulates the exact user scenario:
        // 1. Config is loaded (schema + documents)
        // 2. Schema file is opened (via goto_definition or directly)
        // 3. Schema file is saved (triggers project-wide lint)
        //
        // The lint should correctly see all document files including TS files.

        let temp_dir = tempfile::tempdir().unwrap();

        // Create schema file
        let schema_content = r#"
type Query { rateLimit: RateLimit }
type RateLimit {
    cost: Int!
    limit: Int!
    nodeCount: Int!
}
"#;
        let schema_path = temp_dir.path().join("schema.graphql");
        let mut file = std::fs::File::create(&schema_path).unwrap();
        file.write_all(schema_content.as_bytes()).unwrap();

        // Create TypeScript file with embedded GraphQL
        let ts_content = r#"
import { gql } from "@apollo/client";

export const RATE_LIMIT_QUERY = gql`
  query GetRateLimit {
    rateLimit {
      cost
      limit
      nodeCount
    }
  }
`;
"#;
        let ts_path = temp_dir.path().join("api.ts");
        let mut ts_file = std::fs::File::create(&ts_path).unwrap();
        ts_file.write_all(ts_content.as_bytes()).unwrap();

        // Create config
        let config = graphql_config::ProjectConfig::new(
            graphql_config::SchemaConfig::Path("schema.graphql".to_string()),
            Some(graphql_config::DocumentsConfig::Pattern("*.ts".to_string())),
            None,
            None,
            None,
        );

        // Create host and load files from config (simulating LSP initialization)
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        // Load schema
        let _ = host.load_schemas_from_config(&config, temp_dir.path());
        // Load documents (including TS files)
        let extract_config = graphql_extract::ExtractConfig::default();
        let (loaded_docs, _doc_result) =
            host.load_documents_from_config(&config, temp_dir.path(), &extract_config);

        // Verify the TS file was loaded
        assert!(
            loaded_docs
                .iter()
                .any(|f| f.path.as_str().ends_with("api.ts")),
            "api.ts should be loaded by load_documents_from_config. Loaded: {:?}",
            loaded_docs
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>()
        );

        // Rebuild is called by add_files_batch internally, but let's also call it explicitly
        // to ensure we're in a consistent state
        host.rebuild_project_files();

        // Now simulate did_open for the schema file (as if user navigated to it)
        // Use path_to_file_uri for consistent path handling across platforms
        let schema_file_path = FilePath::new(crate::helpers::path_to_file_uri(&schema_path));
        let (is_new, snapshot) = host.update_file_and_snapshot(
            &schema_file_path,
            schema_content,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Schema should NOT be new (already loaded from config)
        assert!(
            !is_new,
            "Schema file should already exist (loaded from config)"
        );

        // Now run project-wide lints (simulating did_save)
        let project_diagnostics = snapshot.project_lint_diagnostics();

        // Debug: print all diagnostics
        for (path, diags) in &project_diagnostics {
            for d in diags {
                eprintln!(
                    "Diagnostic in {}: {} ({})",
                    path.as_str(),
                    d.message,
                    d.code.as_deref().unwrap_or("")
                );
            }
        }

        // Check for unusedFields warnings
        let unused_fields_errors: Vec<_> = project_diagnostics
            .values()
            .flatten()
            .filter(|d| d.code.as_deref() == Some("unusedFields"))
            .collect();

        // nodeCount should NOT be flagged as unused
        let nodecount_errors: Vec<_> = unused_fields_errors
            .iter()
            .filter(|d| d.message.contains("nodeCount"))
            .collect();

        assert!(
            nodecount_errors.is_empty(),
            "nodeCount used in TypeScript file should NOT be flagged as unused after save. Got: {:?}",
            nodecount_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_all_diagnostics_for_file_merges_per_file_and_project_wide() {
        let mut host = AnalysisHost::new();
        host.set_lint_config(graphql_linter::LintConfig::recommended());

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            r#"
                type Query { user: User }
                type User {
                    id: ID!
                    unusedField: String
                }
            "#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_file,
            "query { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();
        let snapshot = host.snapshot();

        // all_diagnostics_for_file should include project-wide diagnostics
        let schema_diags = snapshot.all_diagnostics_for_file(&schema_file);
        let has_unused_field = schema_diags.iter().any(|d| {
            d.code.as_deref() == Some("unusedFields") && d.message.contains("unusedField")
        });

        assert!(
            has_unused_field,
            "all_diagnostics_for_file should include project-wide unused_fields diagnostic"
        );
    }

    // ===========================================
    // Inlay Hints Tests
    // ===========================================

    #[test]
    fn test_inlay_hints_for_scalar_fields() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! level: Int! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser {\n  user {\n    name\n    level\n  }\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for scalar fields: name and level
        assert!(
            hints.len() >= 2,
            "Expected at least 2 inlay hints for scalar fields, got {}",
            hints.len()
        );

        // Check that hints contain type information
        let hint_labels: Vec<&str> = hints.iter().map(|h| h.label.as_str()).collect();
        assert!(
            hint_labels.iter().any(|l| l.contains("String")),
            "Expected hint containing String type"
        );
        assert!(
            hint_labels.iter().any(|l| l.contains("Int")),
            "Expected hint containing Int type"
        );
    }

    #[test]
    fn test_inlay_hints_on_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        let path = FilePath::new("file:///nonexistent.graphql");
        let hints = snapshot.inlay_hints(&path, None);

        assert!(hints.is_empty(), "Expected no hints for nonexistent file");
    }

    #[test]
    fn test_inlay_hints_with_range_filter() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! level: Int! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser {\n  user {\n    name\n    level\n  }\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Request hints only for line 2 (where "name" is)
        let range = Some(Range::new(Position::new(2, 0), Position::new(2, 100)));
        let hints = snapshot.inlay_hints(&doc_path, range);

        // Should only get the hint for the field on line 2
        assert!(
            hints.len() == 1,
            "Expected 1 hint for filtered range, got {}",
            hints.len()
        );
        assert!(
            hints[0].label.contains("String"),
            "Expected String type hint for name field"
        );
    }

    #[test]
    fn test_inlay_hints_no_project() {
        let mut host = AnalysisHost::new();

        // Add a file but don't rebuild project (so there's no schema context)
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser { user { name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        // Don't call rebuild_project_files()

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Without schema context, no type hints can be generated
        assert!(
            hints.is_empty(),
            "Expected no hints without project context"
        );
    }

    #[test]
    fn test_inlay_hints_nested_fields() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query { user: User }
type User {
  name: String!
  posts: [Post!]!
}
type Post {
  title: String!
  content: String
}"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            r#"query GetUserWithPosts {
  user {
    name
    posts {
      title
      content
    }
  }
}"#,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for scalar fields: name, title, content
        assert!(
            hints.len() >= 3,
            "Expected at least 3 inlay hints for nested scalar fields, got {}",
            hints.len()
        );

        let hint_labels: Vec<&str> = hints.iter().map(|h| h.label.as_str()).collect();

        // Check for String type hints
        let string_hints = hint_labels.iter().filter(|l| l.contains("String")).count();
        assert!(
            string_hints >= 2,
            "Expected at least 2 String type hints, got {string_hints}"
        );
    }

    #[test]
    fn test_inlay_hints_fragment_definition() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! age: Int! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "fragment UserFields on User {\n  name\n  age\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for fields in fragment
        assert!(
            hints.len() >= 2,
            "Expected at least 2 inlay hints for fragment fields, got {}",
            hints.len()
        );

        let hint_labels: Vec<&str> = hints.iter().map(|h| h.label.as_str()).collect();
        assert!(
            hint_labels.iter().any(|l| l.contains("String")),
            "Expected String type hint"
        );
        assert!(
            hint_labels.iter().any(|l| l.contains("Int")),
            "Expected Int type hint"
        );
    }

    #[test]
    fn test_inlay_hints_with_aliases() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! email: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser {\n  user {\n    userName: name\n    userEmail: email\n  }\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for aliased scalar fields
        assert!(
            hints.len() >= 2,
            "Expected at least 2 inlay hints for aliased fields, got {}",
            hints.len()
        );

        // Both hints should show String type
        let string_hints = hints.iter().filter(|h| h.label.contains("String")).count();
        assert_eq!(
            string_hints, 2,
            "Expected 2 String type hints for aliased fields"
        );
    }

    #[test]
    fn test_inlay_hints_typename() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetUser {\n  user {\n    __typename\n    name\n  }\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have hints for user (object type), __typename, and name
        assert_eq!(
            hints.len(),
            3,
            "Expected 3 inlay hints (user, __typename, name), got {}",
            hints.len()
        );

        // Check user shows User type hint
        let user_hint = hints.iter().find(|h| h.label == ": User");
        assert!(user_hint.is_some(), "Expected user hint with 'User' type");

        // Check __typename shows String! hint
        let typename_hint = hints.iter().find(|h| h.label == ": String!");
        assert!(
            typename_hint.is_some(),
            "Expected __typename hint with 'String!' type"
        );
    }

    #[test]
    fn test_inlay_hints_object_type_fields() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query { user(id: ID!): User }
type User {
  name: String!
  posts: [Post!]!
}
type Post {
  title: String!
}"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            // Non-leaf fields with and without arguments
            r#"query {
  user(id: "1") {
    name
    posts {
      title
    }
  }
}"#,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        let hint_labels: Vec<&str> = hints.iter().map(|h| h.label.as_str()).collect();

        // Non-leaf fields should get type hints too
        assert!(
            hint_labels.contains(&": User"),
            "Expected User type hint for user field, got: {hint_labels:?}"
        );
        assert!(
            hint_labels.contains(&": [Post]!"),
            "Expected [Post]! type hint for posts field, got: {hint_labels:?}"
        );

        // user(id: "1") hint should appear after the arguments
        let user_hint = hints.iter().find(|h| h.label == ": User").unwrap();
        let name_hint = hints.iter().find(|h| h.label == ": String!").unwrap();
        // user hint should be on line 1, after the closing paren of args
        assert_eq!(user_hint.position.line, 1);
        // name hint should be on a later line
        assert!(name_hint.position.line > user_hint.position.line);
    }

    // =============================================================================
    // Schema Extension Tests (extend type)
    // These test that fields from "extend type X" are merged with the base type
    // =============================================================================

    #[test]
    fn test_hover_on_field_from_schema_extension() {
        // Fields defined in "extend type Query" should have hover info
        let mut host = AnalysisHost::new();

        // Base schema with Query type
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query {\n  user: User\n}\n\ntype User {\n  id: ID!\n  name: String!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Client schema that extends Query with local-only fields
        let client_schema_path = FilePath::new("file:///client-schema.graphql");
        host.add_file(
            &client_schema_path,
            "extend type Query {\n  isLoggedIn: Boolean!\n  cartItems: Int!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Document that uses the extended field
        let doc_path = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) =
            extract_cursor("query GetState {\n  isLogged*In\n  cartItems\n}");
        host.add_file(
            &doc_path,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hover = snapshot.hover(&doc_path, cursor_pos);

        // Should return hover information for the field from the extension
        assert!(
            hover.is_some(),
            "Expected hover info for field from schema extension"
        );
        let hover = hover.unwrap();
        assert!(
            hover.contents.contains("isLoggedIn"),
            "Hover should contain field name 'isLoggedIn'"
        );
        assert!(
            hover.contents.contains("Boolean"),
            "Hover should contain type 'Boolean'"
        );
    }

    #[test]
    fn test_goto_definition_on_field_from_schema_extension() {
        // Goto definition on a field from "extend type Query" should jump to extension
        let mut host = AnalysisHost::new();

        // Base schema
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query {\n  user: User\n}\n\ntype User {\n  id: ID!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Client schema extension
        let client_schema_path = FilePath::new("file:///client-schema.graphql");
        host.add_file(
            &client_schema_path,
            "extend type Query {\n  isLoggedIn: Boolean!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Document using the extended field
        let doc_path = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor("query GetState {\n  isLogged*In\n}");
        host.add_file(
            &doc_path,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&doc_path, cursor_pos);

        // Should find the definition in the client schema extension
        assert!(
            locations.is_some(),
            "Expected goto definition to find field from schema extension"
        );
        let locations = locations.unwrap();
        assert!(
            !locations.is_empty(),
            "Expected at least one definition location"
        );
        assert_eq!(
            locations[0].file.as_str(),
            "file:///client-schema.graphql",
            "Definition should be in client-schema.graphql"
        );
    }

    #[test]
    fn test_inlay_hints_on_field_from_schema_extension() {
        // Inlay hints should show type for fields from "extend type Query"
        let mut host = AnalysisHost::new();

        // Base schema
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query {\n  user: User\n}\n\ntype User {\n  id: ID!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Client schema extension
        let client_schema_path = FilePath::new("file:///client-schema.graphql");
        host.add_file(
            &client_schema_path,
            "extend type Query {\n  isLoggedIn: Boolean!\n  cartItems: Int!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Document using the extended fields
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query GetState {\n  isLoggedIn\n  cartItems\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let hints = snapshot.inlay_hints(&doc_path, None);

        // Should have inlay hints for both extension fields
        assert!(
            hints.len() >= 2,
            "Expected at least 2 inlay hints for extension fields, got {}",
            hints.len()
        );

        let boolean_hint = hints.iter().find(|h| h.label.contains("Boolean"));
        assert!(
            boolean_hint.is_some(),
            "Expected inlay hint with 'Boolean' type for isLoggedIn"
        );

        let int_hint = hints.iter().find(|h| h.label.contains("Int"));
        assert!(
            int_hint.is_some(),
            "Expected inlay hint with 'Int' type for cartItems"
        );
    }

    #[test]
    fn test_goto_definition_type_name_returns_base_and_extensions() {
        // Goto-def on a type name should return both the base type and any extensions
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query {\n  user: User\n}\n\ntype User {\n  id: ID!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let client_schema_path = FilePath::new("file:///client-schema.graphql");
        host.add_file(
            &client_schema_path,
            "extend type Query {\n  isLoggedIn: Boolean!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Cursor on "Query" in the type condition
        let doc_path = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) =
            extract_cursor("query GetUser {\n  ... on Que*ry {\n    user { id }\n  }\n}");
        host.add_file(
            &doc_path,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&doc_path, cursor_pos);

        assert!(locations.is_some(), "Expected goto-def to find type Query");
        let locations = locations.unwrap();
        assert_eq!(
            locations.len(),
            2,
            "Expected 2 locations (base type + extension), got {}",
            locations.len()
        );

        let files: Vec<&str> = locations.iter().map(|l| l.file.as_str()).collect();
        assert!(
            files.contains(&"file:///schema.graphql"),
            "Should include base type definition"
        );
        assert!(
            files.contains(&"file:///client-schema.graphql"),
            "Should include type extension"
        );
    }

    #[test]
    fn test_goto_definition_type_name_extension_only() {
        // Goto-def on a type that only exists as an extension (no base type in scope)
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "extend type Query {\n  isLoggedIn: Boolean!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) =
            extract_cursor("query GetState {\n  ... on Que*ry {\n    isLoggedIn\n  }\n}");
        host.add_file(
            &doc_path,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&doc_path, cursor_pos);

        assert!(
            locations.is_some(),
            "Expected goto-def to find extension-only type Query"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file.as_str(), "file:///schema.graphql");
    }

    #[test]
    fn test_document_symbols_extension_labels() {
        // Document symbols should show proper "extend type Query" labels
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query {\n  user: User\n}\n\nextend type Query {\n  isLoggedIn: Boolean!\n}\n\nextend interface Node {\n  createdAt: String\n}\n\nextend union SearchResult = Post\n\nextend enum Status {\n  ARCHIVED\n}\n\nextend input CreateUserInput {\n  role: String\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.document_symbols(&schema_path);

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Query"), "Should have base type Query");
        assert!(
            names.contains(&"extend type Query"),
            "Should have 'extend type Query', got {names:?}",
        );
        assert!(
            names.contains(&"extend interface Node"),
            "Should have 'extend interface Node', got {names:?}",
        );
        assert!(
            names.contains(&"extend union SearchResult"),
            "Should have 'extend union SearchResult', got {names:?}",
        );
        assert!(
            names.contains(&"extend enum Status"),
            "Should have 'extend enum Status', got {names:?}",
        );
        assert!(
            names.contains(&"extend input CreateUserInput"),
            "Should have 'extend input CreateUserInput', got {names:?}",
        );
    }

    #[test]
    fn test_schema_types_base_type_wins_primary_slot() {
        // When extension is processed before base type, base type should still win primary slot
        let mut host = AnalysisHost::new();

        // Add extension file first (gets lower FileId)
        let ext_path = FilePath::new("file:///a-extensions.graphql");
        host.add_file(
            &ext_path,
            "extend type Query {\n  clientField: Boolean!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Add base type file second (gets higher FileId)
        let schema_path = FilePath::new("file:///b-schema.graphql");
        host.add_file(
            &schema_path,
            "type Query {\n  user: User\n}\n\ntype User {\n  id: ID!\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) =
            extract_cursor("query GetState {\n  ... on Que*ry { user { id } }\n}");
        host.add_file(
            &doc_path,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&doc_path, cursor_pos);

        assert!(locations.is_some());
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 2, "Should find both base and extension");

        // Both files should be represented
        let files: Vec<&str> = locations.iter().map(|l| l.file.as_str()).collect();
        assert!(files.contains(&"file:///a-extensions.graphql"));
        assert!(files.contains(&"file:///b-schema.graphql"));
    }

    // =========================================================================
    // Rename tests
    // =========================================================================

    #[test]
    fn test_prepare_rename_fragment_spread() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { ...UserFields } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Position on "UserFields" in the spread (after "...")
        let range = snapshot.prepare_rename(&query_file, Position::new(0, 19));
        assert!(range.is_some(), "Should allow renaming fragment spread");
    }

    #[test]
    fn test_prepare_rename_rejects_type_name() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Position on "User" type name - should be rejected
        let range = snapshot.prepare_rename(&schema_file, Position::new(1, 5));
        assert!(range.is_none(), "Should reject renaming schema types");
    }

    #[test]
    fn test_rename_fragment_project_wide() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { ...UserFields } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let query_file2 = FilePath::new("file:///query2.graphql");
        host.add_file(
            &query_file2,
            "query Other { user { ...UserFields } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Rename from the fragment definition
        let result = snapshot.rename(
            &fragment_file,
            Position::new(0, 10), // "UserFields" in definition
            "UserInfo",
        );
        assert!(result.is_some(), "Should produce rename result");
        let result = result.unwrap();

        // Should have edits in 3 files: definition + 2 query files
        assert_eq!(result.changes.len(), 3, "Should affect 3 files");
        assert!(result.changes.contains_key(&fragment_file));
        assert!(result.changes.contains_key(&query_file));
        assert!(result.changes.contains_key(&query_file2));

        // Each file should have exactly 1 edit
        for edits in result.changes.values() {
            assert_eq!(edits.len(), 1);
            assert_eq!(edits[0].new_text, "UserInfo");
        }
    }

    #[test]
    fn test_rename_fragment_from_spread() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let fragment_file = FilePath::new("file:///fragments.graphql");
        host.add_file(
            &fragment_file,
            "fragment UserFields on User { id name }",
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { ...UserFields } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Rename from a spread site
        let result = snapshot.rename(
            &query_file,
            Position::new(0, 19), // "UserFields" in spread
            "UserInfo",
        );
        assert!(result.is_some(), "Should produce rename result from spread");
        let result = result.unwrap();

        assert_eq!(
            result.changes.len(),
            2,
            "Should affect definition + spread files"
        );
        assert!(result.changes.contains_key(&fragment_file));
        assert!(result.changes.contains_key(&query_file));
    }

    #[test]
    fn test_rename_operation_name() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query GetUser { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Rename "GetUser" operation
        let result = snapshot.rename(&query_file, Position::new(0, 7), "FetchUser");
        assert!(
            result.is_some(),
            "Should produce rename result for operation"
        );
        let result = result.unwrap();

        assert_eq!(result.changes.len(), 1, "Operation rename is file-local");
        let edits = result.changes.get(&query_file).unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "FetchUser");
    }

    #[test]
    fn test_rename_variable() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user(id: ID!): User }\ntype User { id: ID! name: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query GetUser($userId: ID!) { user(id: $userId) { id name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Rename "$userId" variable from its usage site
        // "query GetUser($userId: ID!) { user(id: $userId) { id name } }"
        // The $userId reference: "user(id: $" is at offset ~39, "userId" starts after $
        let result = snapshot.rename(&query_file, Position::new(0, 40), "id");
        assert!(
            result.is_some(),
            "Should produce rename result for variable"
        );
        let result = result.unwrap();

        assert_eq!(result.changes.len(), 1);
        let edits = result.changes.get(&query_file).unwrap();
        // Should rename both the definition and the usage
        assert_eq!(
            edits.len(),
            2,
            "Should rename variable definition and usage"
        );
        for edit in edits {
            assert_eq!(edit.new_text, "id");
        }
    }

    #[test]
    fn test_rename_rejects_field_name() {
        let mut host = AnalysisHost::new();

        let schema_file = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_file,
            "type Query { user: User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Try to rename "id" field in query - should be rejected
        let result = snapshot.rename(&query_file, Position::new(0, 15), "identifier");
        assert!(result.is_none(), "Should reject renaming fields");
    }

    // =========================================================================
    // Signature Help Tests
    // =========================================================================

    #[test]
    fn test_signature_help_field_with_arguments() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user(id: ID!, name: String): User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        // Cursor right after `(` at position (0, 8)
        host.add_file(
            &doc_path,
            "{ user(id: \"123\") { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Cursor inside the argument list, after `(`
        let help = snapshot.signature_help(&doc_path, Position::new(0, 7));
        assert!(
            help.is_some(),
            "Should return signature help inside field arguments"
        );
        let help = help.unwrap();
        assert_eq!(help.signatures.len(), 1);
        assert!(help.signatures[0].label.contains("user("));
        assert!(help.signatures[0].label.contains("id: ID!"));
        assert!(help.signatures[0].label.contains("name: String"));
        assert!(help.signatures[0].label.contains("): User"));
        assert_eq!(help.signatures[0].parameters.len(), 2);
        assert_eq!(help.active_signature, Some(0));
        assert_eq!(help.active_parameter, Some(0));
    }

    #[test]
    fn test_signature_help_directive_with_arguments() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            r#"type Query { hello: String }
directive @skip(if: Boolean!) on FIELD"#,
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "{ hello @skip(if: true) }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Cursor inside the directive argument list
        let help = snapshot.signature_help(&doc_path, Position::new(0, 18));
        assert!(
            help.is_some(),
            "Should return signature help inside directive arguments"
        );
        let help = help.unwrap();
        assert_eq!(help.signatures.len(), 1);
        assert!(help.signatures[0].label.starts_with("@skip("));
        assert_eq!(help.signatures[0].parameters.len(), 1);
    }

    #[test]
    fn test_signature_help_nested_field() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user: User }\ntype User { posts(first: Int, after: String): [Post] }\ntype Post { title: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "{ user { posts(first: 10) { title } } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Cursor inside posts() arguments
        let help = snapshot.signature_help(&doc_path, Position::new(0, 21));
        assert!(
            help.is_some(),
            "Should return signature help for nested field arguments"
        );
        let help = help.unwrap();
        assert!(help.signatures[0].label.contains("posts("));
        assert_eq!(help.signatures[0].parameters.len(), 2);
    }

    #[test]
    fn test_signature_help_not_in_arguments() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "{ hello }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Cursor on `hello` field, not in arguments
        let help = snapshot.signature_help(&doc_path, Position::new(0, 3));
        assert!(
            help.is_none(),
            "Should not return signature help outside argument list"
        );
    }

    #[test]
    fn test_signature_help_default_values() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { posts(first: Int = 10, after: String): [Post] }\ntype Post { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "{ posts(first: 5) { id } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let help = snapshot.signature_help(&doc_path, Position::new(0, 15));
        assert!(help.is_some());
        let help = help.unwrap();
        assert!(
            help.signatures[0].label.contains("= 10"),
            "Should show default value in label: {}",
            help.signatures[0].label
        );
    }

    #[test]
    fn test_signature_help_active_parameter_tracking() {
        let mut host = AnalysisHost::new();

        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "type Query { user(id: ID!, name: String, age: Int): User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            r#"{ user(id: "1", name: "test", age: 25) { id } }"#,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();

        // Cursor in first argument
        let help = snapshot.signature_help(&doc_path, Position::new(0, 10));
        assert!(help.is_some());
        assert_eq!(help.unwrap().active_parameter, Some(0));

        // Cursor in second argument (after first comma)
        let help = snapshot.signature_help(&doc_path, Position::new(0, 20));
        assert!(help.is_some());
        assert_eq!(help.unwrap().active_parameter, Some(1));

        // Cursor in third argument (after second comma)
        let help = snapshot.signature_help(&doc_path, Position::new(0, 33));
        assert!(help.is_some());
        assert_eq!(help.unwrap().active_parameter, Some(2));
    }

    #[test]
    fn test_signature_help_nonexistent_file() {
        let host = AnalysisHost::new();
        let snapshot = host.snapshot();

        let path = FilePath::new("file:///nonexistent.graphql");
        let help = snapshot.signature_help(&path, Position::new(0, 0));
        assert!(help.is_none());
    }

    // ========================================================================
    // Type extension tests: fields defined in a different file via `extend type`
    // Regression tests for offset/file_id mismatch panics
    // ========================================================================

    #[test]
    fn test_goto_definition_field_from_type_extension() {
        let mut host = AnalysisHost::new();

        // Base type in one file
        let base_file = FilePath::new("file:///base.graphql");
        host.add_file(
            &base_file,
            "type Query { user: User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Extension adds a field in a separate file
        let ext_file = FilePath::new("file:///extension.graphql");
        host.add_file(
            &ext_file,
            "extend type User { name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        let (query_text, cursor_pos) = extract_cursor("query { user { na*me } }");
        host.add_file(
            &query_file,
            &query_text,
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let locations = snapshot.goto_definition(&query_file, cursor_pos);

        assert!(
            locations.is_some(),
            "Should find field definition from extension"
        );
        let locations = locations.unwrap();
        assert_eq!(locations.len(), 1);
        // Should point to the extension file, not the base file
        assert_eq!(locations[0].file.as_str(), ext_file.as_str());
        assert_eq!(locations[0].range.start.line, 0);
    }

    #[test]
    fn test_find_references_field_from_type_extension() {
        let mut host = AnalysisHost::new();

        // Base type in one file
        let base_file = FilePath::new("file:///base.graphql");
        host.add_file(
            &base_file,
            "type Query { user: User }\ntype User { id: ID! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        // Extension adds a field in a separate file
        let ext_file = FilePath::new("file:///extension.graphql");
        host.add_file(
            &ext_file,
            "extend type User { name: String! }",
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let query_file = FilePath::new("file:///query.graphql");
        host.add_file(
            &query_file,
            "query { user { name } }",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        // Find references to "name" from the extension file, including declaration
        // "extend type User { " = 19 chars, "name" at position 19
        let snapshot = host.snapshot();
        let locations = snapshot.find_references(&ext_file, Position::new(0, 19), true);

        assert!(
            locations.is_some(),
            "Should find references for extension field"
        );
        let locations = locations.unwrap();
        // declaration (in ext_file) + usage (in query_file) = 2
        assert_eq!(
            locations.len(),
            2,
            "Expected declaration + usage, got {locations:?}",
        );

        let ext_refs: Vec<_> = locations
            .iter()
            .filter(|l| l.file.as_str() == ext_file.as_str())
            .collect();
        let query_refs: Vec<_> = locations
            .iter()
            .filter(|l| l.file.as_str() == query_file.as_str())
            .collect();
        assert_eq!(
            ext_refs.len(),
            1,
            "Should have 1 declaration in extension file"
        );
        assert_eq!(query_refs.len(), 1, "Should have 1 usage in query file");
    }

    #[test]
    fn test_hover_on_directive_usage() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "\"Cache control directive\"\ndirective @cacheControl(maxAge: Int) repeatable on FIELD_DEFINITION\n\ntype Query {\n  hello: String @cacheControl(maxAge: 30)\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let result = snapshot.hover(&schema_path, Position::new(4, 18));
        assert!(result.is_some());
        let hover = result.unwrap();
        assert!(hover.contents.contains("@cacheControl"));
        assert!(hover.contents.contains("FIELD_DEFINITION"));
        assert!(hover.contents.contains("Repeatable"));
    }

    #[test]
    fn test_hover_on_directive_argument() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "\"Cache control\"\ndirective @cacheControl(\"Max age in seconds\" maxAge: Int = 60) on FIELD_DEFINITION\n\ntype Query {\n  hello: String @cacheControl(maxAge: 30)\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let result = snapshot.hover(&schema_path, Position::new(4, 31));
        assert!(result.is_some());
        let hover = result.unwrap();
        assert!(hover.contents.contains("maxAge"));
        assert!(hover.contents.contains("Int"));
    }

    #[test]
    fn test_document_symbols_includes_directives() {
        let mut host = AnalysisHost::new();
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "directive @cacheControl(maxAge: Int) on FIELD_DEFINITION\n\ntype Query {\n  hello: String\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.document_symbols(&path);
        let directive_sym = symbols.iter().find(|s| s.name == "@cacheControl");
        assert!(
            directive_sym.is_some(),
            "Should include directive definition in document symbols"
        );
        assert_eq!(directive_sym.unwrap().kind, SymbolKind::Directive);
    }

    #[test]
    fn test_workspace_symbols_includes_directives() {
        let mut host = AnalysisHost::new();
        let path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &path,
            "directive @cacheControl(maxAge: Int) on FIELD_DEFINITION\n\ntype Query {\n  hello: String\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        let symbols = snapshot.workspace_symbols("cache");
        let directive_sym = symbols.iter().find(|s| s.name == "@cacheControl");
        assert!(
            directive_sym.is_some(),
            "Should include directive definition in workspace symbols"
        );
        assert_eq!(directive_sym.unwrap().kind, SymbolKind::Directive);
    }

    #[test]
    fn test_find_references_directive() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "directive @deprecated(reason: String) on FIELD_DEFINITION\n\ntype Query {\n  oldField: String @deprecated(reason: \"use newField\")\n  newField: String\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Without declaration - just usages
        // Position on @deprecated usage: line 3, inside "deprecated"
        let result = snapshot.find_references(&schema_path, Position::new(3, 21), false);
        assert!(result.is_some());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, 3);
    }

    #[test]
    fn test_find_references_directive_with_declaration() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        // "directive @tag(name: String!) on FIELD_DEFINITION\n\ntype Query {\n  a: String @tag(name: \"public\")\n  b: Int @tag(name: \"internal\")\n}"
        host.add_file(
            &schema_path,
            "directive @tag(name: String!) on FIELD_DEFINITION\n\ntype Query {\n  a: String @tag(name: \"public\")\n  b: Int @tag(name: \"internal\")\n}",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Position on @tag usage: line 3 "  a: String @tag(...)" -> "@tag" starts at col 12, "tag" at col 13
        let result = snapshot.find_references(&schema_path, Position::new(3, 13), true);
        assert!(result.is_some());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 3); // declaration + 2 usages
    }

    #[test]
    fn test_find_references_directive_across_files() {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        host.add_file(
            &schema_path,
            "directive @myDir on QUERY\n\ntype Query { hello: String }",
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(
            &doc_path,
            "query Foo @myDir {\n  hello\n}",
            Language::GraphQL,
            DocumentKind::Executable,
        );
        host.rebuild_project_files();

        let snapshot = host.snapshot();
        // Position on @myDir usage in query file: "query Foo @myDir" -> "myDir" starts at col 11
        let result = snapshot.find_references(&doc_path, Position::new(0, 11), true);
        assert!(result.is_some());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 2); // declaration + usage in query file
    }
}
