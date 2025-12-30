// Body extraction - extracts selection sets and field selections
// These are computed lazily and only when needed for validation
//
// Body queries are the core of fine-grained invalidation:
// - Editing an operation body only invalidates that operation's body query
// - Schema queries and other operation bodies remain cached

use apollo_compiler::executable;
use std::collections::HashSet;
use std::sync::Arc;

/// A selection in a selection set
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selection {
    Field {
        name: Arc<str>,
        alias: Option<Arc<str>>,
        arguments: Vec<(Arc<str>, Arc<str>)>,
        selection_set: Vec<Selection>,
    },
    FragmentSpread {
        name: Arc<str>,
    },
    InlineFragment {
        type_condition: Option<Arc<str>>,
        selection_set: Vec<Selection>,
    },
}

/// The body of an operation (selection set and metadata)
///
/// This is separated from `OperationStructure` to enable fine-grained invalidation:
/// editing the selection set only invalidates this body, not the operation's
/// name/variables (structure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationBody {
    /// The selections in this operation
    pub selections: Vec<Selection>,
    /// Fragment names directly referenced by this operation (not transitive)
    pub fragment_spreads: HashSet<Arc<str>>,
    /// Variable names used in this operation
    pub variable_usages: HashSet<Arc<str>>,
}

/// The body of a fragment (selection set and metadata)
///
/// This is separated from `FragmentStructure` to enable fine-grained invalidation:
/// editing the selection set only invalidates this body, not the fragment's
/// name/type condition (structure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentBody {
    /// The selections in this fragment
    pub selections: Vec<Selection>,
    /// Fragment names directly referenced by this fragment (not transitive)
    pub fragment_spreads: HashSet<Arc<str>>,
    /// Variable names used in this fragment
    pub variable_usages: HashSet<Arc<str>>,
}

/// Extract the body of an operation
///
/// This query only invalidates when the operation's selection set changes.
/// The operation's structure (name, variables) can change without invalidating this.
#[salsa::tracked]
pub fn operation_body(
    db: &dyn crate::GraphQLHirDatabase,
    file_content: graphql_db::FileContent,
    file_metadata: graphql_db::FileMetadata,
    operation_index: usize,
) -> Arc<OperationBody> {
    let parse = graphql_syntax::parse(db, file_content, file_metadata);

    // Find the operation at the given index
    let mut op_count = 0;

    // For pure GraphQL files, look in the main AST
    // For TS/JS files, look in the extracted blocks
    if parse.blocks.is_empty() {
        for definition in &parse.ast.definitions {
            if let apollo_compiler::ast::Definition::OperationDefinition(op) = definition {
                if op_count == operation_index {
                    return Arc::new(extract_operation_body_from_ast(op));
                }
                op_count += 1;
            }
        }
    } else {
        // For TypeScript/JavaScript, search through blocks
        for block in &parse.blocks {
            for definition in &block.ast.definitions {
                if let apollo_compiler::ast::Definition::OperationDefinition(op) = definition {
                    if op_count == operation_index {
                        return Arc::new(extract_operation_body_from_ast(op));
                    }
                    op_count += 1;
                }
            }
        }
    }

    // Operation not found - return empty body
    Arc::new(OperationBody {
        selections: Vec::new(),
        fragment_spreads: HashSet::new(),
        variable_usages: HashSet::new(),
    })
}

/// Extract the body of a fragment by name
///
/// This query only invalidates when the fragment's selection set changes.
#[salsa::tracked]
#[allow(clippy::needless_pass_by_value)] // Arc<str> needed for Salsa tracking
pub fn fragment_body(
    db: &dyn crate::GraphQLHirDatabase,
    file_content: graphql_db::FileContent,
    file_metadata: graphql_db::FileMetadata,
    fragment_name: Arc<str>,
) -> Arc<FragmentBody> {
    let parse = graphql_syntax::parse(db, file_content, file_metadata);

    // For pure GraphQL files, look in the main AST
    if parse.blocks.is_empty() {
        for definition in &parse.ast.definitions {
            if let apollo_compiler::ast::Definition::FragmentDefinition(frag) = definition {
                if frag.name.as_str() == fragment_name.as_ref() {
                    return Arc::new(extract_fragment_body_from_ast(frag));
                }
            }
        }
    } else {
        // For TypeScript/JavaScript, search through blocks
        for block in &parse.blocks {
            for definition in &block.ast.definitions {
                if let apollo_compiler::ast::Definition::FragmentDefinition(frag) = definition {
                    if frag.name.as_str() == fragment_name.as_ref() {
                        return Arc::new(extract_fragment_body_from_ast(frag));
                    }
                }
            }
        }
    }

    // Fragment not found - return empty body
    Arc::new(FragmentBody {
        selections: Vec::new(),
        fragment_spreads: HashSet::new(),
        variable_usages: HashSet::new(),
    })
}

/// Get all fragments transitively used by an operation
///
/// This handles circular fragment references gracefully by tracking visited fragments.
#[salsa::tracked]
pub fn operation_transitive_fragments(
    db: &dyn crate::GraphQLHirDatabase,
    file_content: graphql_db::FileContent,
    file_metadata: graphql_db::FileMetadata,
    operation_index: usize,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashSet<Arc<str>>> {
    let body = operation_body(db, file_content, file_metadata, operation_index);

    let mut visited = HashSet::new();
    let mut to_visit: Vec<Arc<str>> = body.fragment_spreads.iter().cloned().collect();

    // Use fragment_file_index for O(1) lookup of fragment files
    let fragment_index = crate::fragment_file_index(db, project_files);

    while let Some(frag_name) = to_visit.pop() {
        if !visited.insert(frag_name.clone()) {
            continue; // Already visited (handles cycles)
        }

        // Look up the fragment's file directly using the index
        if let Some((content, metadata)) = fragment_index.get(&frag_name) {
            let frag_body = fragment_body(db, *content, *metadata, frag_name.clone());

            // Add any new fragment spreads to visit
            for spread in &frag_body.fragment_spreads {
                if !visited.contains(spread) {
                    to_visit.push(spread.clone());
                }
            }
        }
    }

    Arc::new(visited)
}

/// Extract operation body from an AST operation definition
fn extract_operation_body_from_ast(
    op: &apollo_compiler::ast::OperationDefinition,
) -> OperationBody {
    let (selections, fragment_spreads) = extract_selections_from_ast(&op.selection_set);
    let variable_usages = extract_variable_usages_from_selections(&selections);

    OperationBody {
        selections,
        fragment_spreads,
        variable_usages,
    }
}

/// Extract fragment body from an AST fragment definition
fn extract_fragment_body_from_ast(frag: &apollo_compiler::ast::FragmentDefinition) -> FragmentBody {
    let (selections, fragment_spreads) = extract_selections_from_ast(&frag.selection_set);
    let variable_usages = extract_variable_usages_from_selections(&selections);

    FragmentBody {
        selections,
        fragment_spreads,
        variable_usages,
    }
}

/// Extract selections from an AST selection set (Vec<Selection>)
fn extract_selections_from_ast(
    selection_set: &[apollo_compiler::ast::Selection],
) -> (Vec<Selection>, HashSet<Arc<str>>) {
    let mut selections = Vec::new();
    let mut fragment_spreads = HashSet::new();

    for selection in selection_set {
        extract_selection_from_ast(selection, &mut selections, &mut fragment_spreads);
    }

    (selections, fragment_spreads)
}

fn extract_selection_from_ast(
    selection: &apollo_compiler::ast::Selection,
    selections: &mut Vec<Selection>,
    fragment_spreads: &mut HashSet<Arc<str>>,
) {
    match selection {
        apollo_compiler::ast::Selection::Field(field) => {
            let name = Arc::from(field.name.as_str());
            let alias = field.alias.as_ref().map(|a| Arc::from(a.as_str()));

            let arguments = field
                .arguments
                .iter()
                .map(|arg| {
                    let arg_name = Arc::from(arg.name.as_str());
                    let value = Arc::from(arg.value.to_string().as_str());
                    (arg_name, value)
                })
                .collect();

            let (nested_selections, nested_spreads) =
                extract_selections_from_ast(&field.selection_set);
            fragment_spreads.extend(nested_spreads);

            selections.push(Selection::Field {
                name,
                alias,
                arguments,
                selection_set: nested_selections,
            });
        }
        apollo_compiler::ast::Selection::FragmentSpread(spread) => {
            let name: Arc<str> = Arc::from(spread.fragment_name.as_str());
            fragment_spreads.insert(name.clone());
            selections.push(Selection::FragmentSpread { name });
        }
        apollo_compiler::ast::Selection::InlineFragment(inline) => {
            let type_condition = inline
                .type_condition
                .as_ref()
                .map(|tc| Arc::from(tc.as_str()));

            let (nested_selections, nested_spreads) =
                extract_selections_from_ast(&inline.selection_set);
            fragment_spreads.extend(nested_spreads);

            selections.push(Selection::InlineFragment {
                type_condition,
                selection_set: nested_selections,
            });
        }
    }
}

/// Extract variable usages from selections
fn extract_variable_usages_from_selections(selections: &[Selection]) -> HashSet<Arc<str>> {
    let mut usages = HashSet::new();

    for selection in selections {
        match selection {
            Selection::Field {
                arguments,
                selection_set,
                ..
            } => {
                // Check arguments for variable references (e.g., "$id")
                for (_name, value) in arguments {
                    if let Some(var_name) = value.strip_prefix('$') {
                        usages.insert(Arc::from(var_name));
                    }
                }
                usages.extend(extract_variable_usages_from_selections(selection_set));
            }
            Selection::InlineFragment { selection_set, .. } => {
                usages.extend(extract_variable_usages_from_selections(selection_set));
            }
            Selection::FragmentSpread { .. } => {
                // Variable usages in fragment spreads are handled when we
                // resolve the fragment body
            }
        }
    }

    usages
}

/// Extract selections from an executable selection set (for validation)
#[must_use]
pub fn extract_selections(
    selection_set: &executable::SelectionSet,
) -> (Vec<Selection>, HashSet<Arc<str>>) {
    let mut selections = Vec::new();
    let mut fragment_spreads = HashSet::new();

    for selection in &selection_set.selections {
        extract_selection(selection, &mut selections, &mut fragment_spreads);
    }

    (selections, fragment_spreads)
}

fn extract_selection(
    selection: &executable::Selection,
    selections: &mut Vec<Selection>,
    fragment_spreads: &mut HashSet<Arc<str>>,
) {
    match selection {
        executable::Selection::Field(field_node) => {
            let field = &**field_node;
            let name = Arc::from(field.name.as_str());
            let alias = field.alias.as_ref().map(|a| Arc::from(a.as_str()));

            let arguments = field
                .arguments
                .iter()
                .map(|arg| {
                    let arg_name = Arc::from(arg.name.as_str());
                    let value = Arc::from(arg.value.to_string().as_str());
                    (arg_name, value)
                })
                .collect();

            let selection_set = extract_selections(&field.selection_set).0;

            selections.push(Selection::Field {
                name,
                alias,
                arguments,
                selection_set,
            });
        }
        executable::Selection::FragmentSpread(spread_node) => {
            let spread = &**spread_node;
            let name: Arc<str> = Arc::from(spread.fragment_name.as_str());
            fragment_spreads.insert(name.clone());
            selections.push(Selection::FragmentSpread { name });
        }
        executable::Selection::InlineFragment(inline_node) => {
            let inline = &**inline_node;
            let type_condition = inline
                .type_condition
                .as_ref()
                .map(|tc| Arc::from(tc.as_str()));

            let selection_set = extract_selections(&inline.selection_set).0;

            selections.push(Selection::InlineFragment {
                type_condition,
                selection_set,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileId, FileKind, FileMetadata, FileUri};
    use std::collections::HashMap;

    // Test database
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
    impl crate::GraphQLHirDatabase for TestDatabase {}

    /// Helper to create ProjectFiles for tests
    fn create_project_files(
        db: &TestDatabase,
        schema_files: Vec<(FileId, FileContent, FileMetadata)>,
        document_files: Vec<(FileId, FileContent, FileMetadata)>,
    ) -> graphql_db::ProjectFiles {
        let schema_ids: Vec<FileId> = schema_files.iter().map(|(id, _, _)| *id).collect();
        let doc_ids: Vec<FileId> = document_files.iter().map(|(id, _, _)| *id).collect();

        let mut entries = HashMap::new();
        for (id, content, metadata) in &schema_files {
            entries.insert(*id, (*content, *metadata));
        }
        for (id, content, metadata) in &document_files {
            entries.insert(*id, (*content, *metadata));
        }

        let schema_file_ids = graphql_db::SchemaFileIds::new(db, Arc::new(schema_ids));
        let document_file_ids = graphql_db::DocumentFileIds::new(db, Arc::new(doc_ids));
        let file_map = graphql_db::FileMap::new(db, Arc::new(entries));

        graphql_db::ProjectFiles::new(db, schema_file_ids, document_file_ids, file_map)
    }

    #[test]
    fn test_operation_body_extraction() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from("query GetUser { user { id name } }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let body = operation_body(&db, content, metadata, 0);
        assert_eq!(body.selections.len(), 1);
        assert!(body.fragment_spreads.is_empty());
    }

    #[test]
    fn test_operation_body_with_fragment_spread() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from("query GetUser { user { ...UserFields } }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let body = operation_body(&db, content, metadata, 0);
        assert_eq!(body.selections.len(), 1);
        assert!(body.fragment_spreads.contains(&Arc::from("UserFields")));
    }

    #[test]
    fn test_fragment_body_extraction() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);
        let content = FileContent::new(
            &db,
            Arc::from("fragment UserFields on User { id name email }"),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let body = fragment_body(&db, content, metadata, Arc::from("UserFields"));
        assert_eq!(body.selections.len(), 3);
        assert!(body.fragment_spreads.is_empty());
    }

    #[test]
    fn test_fragment_body_with_nested_spread() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);
        let content = FileContent::new(
            &db,
            Arc::from("fragment UserFields on User { id ...NameFields }"),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let body = fragment_body(&db, content, metadata, Arc::from("UserFields"));
        assert_eq!(body.selections.len(), 2);
        assert!(body.fragment_spreads.contains(&Arc::from("NameFields")));
    }

    #[test]
    fn test_variable_usage_extraction() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);
        let content = FileContent::new(
            &db,
            Arc::from("query GetUser($id: ID!) { user(id: $id) { name } }"),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let body = operation_body(&db, content, metadata, 0);
        assert!(body.variable_usages.contains(&Arc::from("id")));
    }

    #[test]
    fn test_transitive_fragments() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);

        // File with operation using fragment A, which uses fragment B
        let content = FileContent::new(
            &db,
            Arc::from(
                r#"
                query GetUser { user { ...FragA } }
                fragment FragA on User { id ...FragB }
                fragment FragB on User { name }
                "#,
            ),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = create_project_files(&db, vec![], vec![(file_id, content, metadata)]);

        let transitive = operation_transitive_fragments(&db, content, metadata, 0, project_files);

        assert!(transitive.contains(&Arc::from("FragA")));
        assert!(transitive.contains(&Arc::from("FragB")));
    }
}
