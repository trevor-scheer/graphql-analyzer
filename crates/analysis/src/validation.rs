use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use graphql_base_db::{FileContent, FileMetadata};
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

#[salsa::tracked]
pub fn validate_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    let Some(schema) =
        crate::merged_schema::merged_schema_with_diagnostics(db, project_files).schema
    else {
        // Without a schema, we can't validate documents
        // Return empty diagnostics (syntax errors are handled elsewhere)
        return Arc::new(diagnostics);
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    let doc_uri = metadata.uri(db);

    // Unified: process all documents (works for both pure GraphQL and TS/JS)
    for doc in parse.documents() {
        // Use document's line offset from extraction (0 for pure GraphQL files)
        let line_offset_val = doc.line_offset;

        // Collect fragment names referenced by this document (transitively across files)
        // Uses the already-parsed tree to avoid redundant parsing
        let referenced_fragments =
            collect_referenced_fragments_transitive(doc.tree, project_files, db);

        // Collect fragment names defined in this document block
        // We need to skip these when adding referenced fragments to avoid duplicates
        let local_fragments = collect_local_fragment_names(doc.ast);

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
        for fragment_name in &referenced_fragments {
            // Skip fragments that are already in the current document block
            // This prevents duplicate definition errors when fragments in the same file
            // reference each other (the document AST already contains all local fragments)
            if local_fragments.contains(fragment_name) {
                continue;
            }
            let key: Arc<str> = Arc::from(fragment_name.as_str());
            if !added_fragments.insert(key.clone()) {
                continue;
            }
            // Fine-grained query: only creates dependency on this specific fragment
            // Uses cached AST instead of re-parsing source text
            if let Some(fragment_ast) = graphql_hir::fragment_ast(db, project_files, key) {
                builder.add_ast_document(&fragment_ast, false);
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
    project_files: graphql_base_db::ProjectFiles,
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

/// Collect fragment names defined in a document AST
/// Used to skip adding fragments that are already in the current document block
fn collect_local_fragment_names(
    ast: &apollo_compiler::ast::Document,
) -> std::collections::HashSet<String> {
    ast.definitions
        .iter()
        .filter_map(|def| {
            if let apollo_compiler::ast::Definition::FragmentDefinition(frag) = def {
                Some(frag.name.to_string())
            } else {
                None
            }
        })
        .collect()
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
