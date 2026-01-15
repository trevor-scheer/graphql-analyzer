use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use graphql_db::{FileContent, FileMetadata};
use std::sync::Arc;

/// Validate a document file using apollo-compiler
/// Returns apollo-compiler diagnostics converted to our Diagnostic type
///
/// This provides comprehensive validation including:
/// - Field selection validation against schema types
/// - Argument validation (required args, correct types)
/// - Fragment spread resolution and type checking
/// - Variable usage and type validation
/// - Circular fragment detection
/// - Type coercion validation
#[allow(clippy::too_many_lines)]
#[salsa::tracked]
pub fn validate_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: graphql_db::ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    let Some(schema) = crate::merged_schema::merged_schema(db, project_files) else {
        // Without a schema, we can't validate documents
        // Return empty diagnostics (syntax errors are handled elsewhere)
        return Arc::new(diagnostics);
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    let doc_uri = metadata.uri(db);
    let metadata_line_offset = metadata.line_offset(db) as usize;

    // Unified: process all documents (works for both pure GraphQL and TS/JS)
    for doc in parse.documents() {
        // Combine document's line offset with metadata's line offset
        // For pure GraphQL files: doc.line_offset = 0, total = metadata_line_offset
        // For embedded GraphQL: doc.line_offset from extraction, metadata_line_offset typically 0
        let line_offset_val = doc.line_offset + metadata_line_offset;

        // Collect fragment names referenced by this document (transitively across files)
        // Uses the already-parsed tree to avoid redundant parsing
        let referenced_fragments =
            collect_referenced_fragments_transitive(doc.tree, project_files, db);

        let valid_schema = apollo_compiler::validation::Valid::assume_valid_ref(schema.as_ref());
        let mut errors = apollo_compiler::validation::DiagnosticList::new(Arc::default());
        let mut builder =
            apollo_compiler::ExecutableDocument::builder(Some(valid_schema), &mut errors);

        // The AST is cached via graphql_syntax::parse()
        // Clone the AST Arc for tracking purposes
        let doc_ast = Arc::new(doc.ast.clone());
        builder.add_ast_document(&doc_ast, true);

        // This ensures that changing fragment A only invalidates files that actually use A
        // Using fragment_ast instead of fragment_source avoids re-parsing
        let mut added_fragments = std::collections::HashSet::new();
        let mut added_ast_ptrs = std::collections::HashSet::new();
        // Pre-populate with current document's AST to avoid adding it again when fragments
        // in the same document reference each other
        added_ast_ptrs.insert(Arc::as_ptr(&doc_ast) as usize);
        for fragment_name in &referenced_fragments {
            let key: Arc<str> = Arc::from(fragment_name.as_str());
            if !added_fragments.insert(key.clone()) {
                continue;
            }
            // Fine-grained query: only creates dependency on this specific fragment
            // Uses cached AST instead of re-parsing source text
            if let Some(fragment_ast) = graphql_hir::fragment_ast(db, project_files, key) {
                // Multiple fragments may share the same AST document
                let ptr = Arc::as_ptr(&fragment_ast) as usize;
                if added_ast_ptrs.insert(ptr) {
                    builder.add_ast_document(&fragment_ast, false);
                }
            }
        }

        let doc_result = builder.build();
        match if errors.is_empty() {
            doc_result
                .validate(valid_schema)
                .map(|_| ())
                .map_err(|with_errors| with_errors.errors)
        } else {
            Err(errors)
        } {
            Ok(_valid_document) => {}
            Err(error_list) => {
                for apollo_diag in error_list.iter() {
                    use apollo_compiler::diagnostic::ToCliReport;
                    if let Some(location) = apollo_diag.error.location() {
                        let file_id = location.file_id();
                        if let Some(source_file) = apollo_diag.sources.get(&file_id) {
                            let diag_file_path = source_file.path();
                            if diag_file_path != doc_uri.as_str() {
                                continue;
                            }
                        }
                    }
                    // Line offset adjusts positions since the AST was parsed without source offset
                    #[allow(clippy::cast_possible_truncation)]
                    let range = apollo_diag.line_column_range().map_or_else(
                        DiagnosticRange::default,
                        |loc_range| DiagnosticRange {
                            start: Position {
                                line: (loc_range.start.line.saturating_sub(1) + line_offset_val)
                                    as u32,
                                character: loc_range.start.column.saturating_sub(1) as u32,
                            },
                            end: Position {
                                line: (loc_range.end.line.saturating_sub(1) + line_offset_val)
                                    as u32,
                                character: loc_range.end.column.saturating_sub(1) as u32,
                            },
                        },
                    );
                    let message: Arc<str> = Arc::from(apollo_diag.error.to_string());
                    if message.contains("must be used in an operation") {
                        continue;
                    }
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
    }

    Arc::new(diagnostics)
}

/// Collect all fragment names referenced by a document transitively across files
/// This resolves fragment dependencies by following fragment spreads to their definitions
///
/// Uses the `fragment_spreads_index` for O(1) lookup instead of scanning all files
fn collect_referenced_fragments_transitive(
    tree: &apollo_parser::SyntaxTree,
    project_files: graphql_db::ProjectFiles,
    db: &dyn GraphQLAnalysisDatabase,
) -> std::collections::HashSet<String> {
    use std::collections::{HashSet, VecDeque};

    let spreads_index = graphql_hir::fragment_spreads_index(db, project_files);

    let mut all_referenced = collect_referenced_fragments_from_tree(tree);
    let mut to_process: VecDeque<String> = all_referenced.iter().cloned().collect();
    let mut processed = HashSet::new();

    while let Some(fragment_name) = to_process.pop_front() {
        if processed.contains(&fragment_name) {
            continue;
        }
        processed.insert(fragment_name.clone());

        // Look up the fragment's spreads in the index (O(1) instead of O(n) file scan)
        let key: Arc<str> = Arc::from(fragment_name.as_str());
        if let Some(fragment_spreads) = spreads_index.get(&key) {
            for spread_name in fragment_spreads {
                let spread_str = spread_name.as_ref().to_string();
                if all_referenced.insert(spread_str.clone()) {
                    to_process.push_back(spread_str);
                }
            }
        }
    }

    all_referenced
}

/// Collect all fragment names referenced by a document (in the same file only)
/// This includes fragments directly referenced in operations and fragments referenced by other fragments
/// Uses an already-parsed syntax tree to avoid redundant parsing
///
/// Note: We always attempt to collect fragments regardless of parse errors because
/// apollo-parser is error-tolerant and produces a usable CST even with syntax errors.
/// This ensures cross-file fragment resolution works even when files have parse errors.
fn collect_referenced_fragments_from_tree(
    tree: &apollo_parser::SyntaxTree,
) -> std::collections::HashSet<String> {
    use std::collections::HashSet;

    // Always collect fragments, even with parse errors.
    // apollo-parser is error-tolerant and produces a CST that may contain
    // valid fragment spreads even when other parts of the document have errors.
    let mut referenced = HashSet::new();
    let document = tree.document();

    for definition in document.definitions() {
        match definition {
            apollo_parser::cst::Definition::OperationDefinition(op) => {
                collect_fragment_spreads_from_selection_set(op.selection_set(), &mut referenced);
            }
            apollo_parser::cst::Definition::FragmentDefinition(frag) => {
                collect_fragment_spreads_from_selection_set(frag.selection_set(), &mut referenced);
            }
            _ => {}
        }
    }

    referenced
}

/// Recursively collect fragment spreads from a selection set
fn collect_fragment_spreads_from_selection_set(
    selection_set: Option<apollo_parser::cst::SelectionSet>,
    referenced: &mut std::collections::HashSet<String>,
) {
    let Some(selection_set) = selection_set else {
        return;
    };

    for selection in selection_set.selections() {
        match selection {
            apollo_parser::cst::Selection::Field(field) => {
                collect_fragment_spreads_from_selection_set(field.selection_set(), referenced);
            }
            apollo_parser::cst::Selection::FragmentSpread(spread) => {
                if let Some(name) = spread.fragment_name() {
                    if let Some(name_node) = name.name() {
                        referenced.insert(name_node.text().to_string());
                    }
                }
            }
            apollo_parser::cst::Selection::InlineFragment(inline_frag) => {
                collect_fragment_spreads_from_selection_set(
                    inline_frag.selection_set(),
                    referenced,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileId, FileKind, FileUri, ProjectFiles};
    use salsa::Setter;

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
    impl crate::GraphQLAnalysisDatabase for TestDatabase {}

    /// Helper to create `ProjectFiles` for tests using the new granular structure
    fn create_project_files(
        db: &TestDatabase,
        schema_files: &[(FileId, FileContent, FileMetadata)],
        document_files: &[(FileId, FileContent, FileMetadata)],
    ) -> ProjectFiles {
        let schema_ids: Vec<FileId> = schema_files.iter().map(|(id, _, _)| *id).collect();
        let doc_ids: Vec<FileId> = document_files.iter().map(|(id, _, _)| *id).collect();

        let mut entries = std::collections::HashMap::new();
        for (id, content, metadata) in schema_files {
            let entry = graphql_db::FileEntry::new(db, *content, *metadata);
            entries.insert(*id, entry);
        }
        for (id, content, metadata) in document_files {
            let entry = graphql_db::FileEntry::new(db, *content, *metadata);
            entries.insert(*id, entry);
        }

        let schema_file_ids = graphql_db::SchemaFileIds::new(db, Arc::new(schema_ids));
        let document_file_ids = graphql_db::DocumentFileIds::new(db, Arc::new(doc_ids));
        let file_entry_map = graphql_db::FileEntryMap::new(db, Arc::new(entries));

        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_validate_file_no_schema() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);

        let content = FileContent::new(&db, Arc::from("query { hello }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Empty project files (no schema)
        let project_files = create_project_files(&db, &[], &[]);

        let diagnostics = validate_file(&db, content, metadata, project_files);
        assert_eq!(
            diagnostics.len(),
            0,
            "Expected no diagnostics when schema is missing"
        );
    }

    #[test]
    fn test_validate_file_with_valid_fragment() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content =
            FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with only fragment definitions (no operations)
        // This is a valid document - fragments don't need to be used in operations
        // within the same file, they may be used in other files
        let doc_id = FileId::new(1);
        let doc_content =
            FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
        assert_eq!(
            diagnostics.len(),
            0,
            "Expected no diagnostics for valid fragment. Fragments don't need operations in the same file."
        );
    }

    #[test]
    fn test_validate_file_with_invalid_fragment() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content =
            FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with fragment that has an invalid field
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(
            &db,
            Arc::from("fragment UserFields on User { invalidField }"),
        );
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for fragment with invalid field"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("invalidField") || d.message.contains("field")),
            "Expected error message about invalid field 'invalidField'"
        );
    }

    #[test]
    fn test_validate_file_invalid_field() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with invalid field
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { world }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for invalid field selection"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("world") || d.message.contains("field")),
            "Expected error message about invalid field 'world'"
        );
    }

    #[test]
    fn test_validate_file_valid_query() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create valid document
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { hello }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
        assert_eq!(
            diagnostics.len(),
            0,
            "Expected no diagnostics for valid query"
        );
    }

    #[test]
    fn test_validate_file_missing_required_argument() {
        let db = TestDatabase::default();

        // Create schema with required argument
        let schema_id = FileId::new(0);
        let schema_content =
            FileContent::new(&db, Arc::from("type Query { user(id: ID!): String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document missing required argument
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { user }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for missing required argument"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("id") || d.message.contains("argument")),
            "Expected error message about missing argument 'id'"
        );
    }

    #[test]
    fn test_validate_file_invalid_variable_type() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with invalid variable type
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query($var: UnknownType) { hello }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
        assert!(
            !diagnostics.is_empty(),
            "Expected diagnostics for invalid variable type"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("UnknownType") || d.message.contains("type")),
            "Expected error message about unknown type 'UnknownType'"
        );
    }

    #[test]
    fn test_cross_file_fragment_resolution() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from("type Query { user: User } type User { id: ID! name: String! }"),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create fragment file
        let frag_id = FileId::new(1);
        let frag_content =
            FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
        let frag_metadata = FileMetadata::new(
            &db,
            frag_id,
            FileUri::new("fragments.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Create query that uses the fragment from another file
        let query_id = FileId::new(2);
        let query_content = FileContent::new(&db, Arc::from("query { user { ...UserFields } }"));
        let query_metadata = FileMetadata::new(
            &db,
            query_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[
                (frag_id, frag_content, frag_metadata),
                (query_id, query_content, query_metadata),
            ],
        );

        // Validate the query - it should find the fragment from the other file
        let diagnostics = validate_file(&db, query_content, query_metadata, project_files);
        assert_eq!(
            diagnostics.len(),
            0,
            "Expected no diagnostics when fragment is defined in another file. Got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_line_offset_adjustment() {
        let mut db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create document with invalid query and line_offset of 10
        // This simulates extracted GraphQL from TypeScript/JavaScript at line 10
        // The content is already extracted GraphQL, so we mark it as ExecutableGraphQL
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(&db, Arc::from("query { invalidField }"));
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.ts"),
            FileKind::ExecutableGraphQL,
        );
        // Set line offset to simulate extraction from line 10 in TypeScript file
        doc_metadata.set_line_offset(&mut db).to(10);

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
        assert!(!diagnostics.is_empty(), "Expected validation errors");

        // Verify that line numbers are adjusted by the line offset
        for diag in diagnostics.iter() {
            // The error should be at line 10 or later (accounting for the offset)
            // The GraphQL error is at line 0 in the extracted text, but should be reported at line 10
            assert!(
                diag.range.start.line >= 10,
                "Expected diagnostic line to be adjusted by offset. Got line: {}",
                diag.range.start.line
            );
        }
    }

    #[test]
    fn test_fragment_collection_with_parse_errors() {
        let db = TestDatabase::default();

        // Create schema
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from("type Query { user: User } type User { id: ID! name: String! }"),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Create fragment file
        let frag_id = FileId::new(1);
        let frag_content =
            FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
        let frag_metadata = FileMetadata::new(
            &db,
            frag_id,
            FileUri::new("fragments.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Create query that uses the fragment but has a syntax error elsewhere
        // The fragment spread is valid, but the document has a parse error (missing closing brace)
        let query_id = FileId::new(2);
        let query_content = FileContent::new(
            &db,
            Arc::from("query GetUser {\n  user {\n    ...UserFields\n    invalidSyntax{\n}\n"),
        );
        let query_metadata = FileMetadata::new(
            &db,
            query_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[
                (frag_id, frag_content, frag_metadata),
                (query_id, query_content, query_metadata),
            ],
        );

        // Validate the query - it should have syntax errors but NOT "unknown fragment" errors
        // Because apollo-parser is error-tolerant and we should still collect fragment references
        let diagnostics = validate_file(&db, query_content, query_metadata, project_files);

        // We expect syntax errors, but NOT "unknown fragment UserFields" error
        // The fragment should resolve correctly despite the parse errors
        let has_unknown_fragment_error = diagnostics.iter().any(|d| {
            d.message.contains("unknown fragment") || d.message.contains("Unknown fragment")
        });

        assert!(
            !has_unknown_fragment_error,
            "Fragment 'UserFields' should resolve even with parse errors in the document. Diagnostics: {diagnostics:?}"
        );
    }
}
