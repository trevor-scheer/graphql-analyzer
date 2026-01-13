use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_hir::{FieldId, FragmentId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

type SchemaFieldsMap<'a> =
    HashMap<(Arc<str>, Arc<str>), (graphql_db::FileId, &'a graphql_hir::FieldSignature)>;

#[salsa::tracked]
pub fn find_unused_fields(db: &dyn GraphQLAnalysisDatabase) -> Arc<Vec<(FieldId, Diagnostic)>> {
    let project_files = db
        .project_files()
        .expect("project files must be set for project-wide analysis");
    let schema = graphql_hir::schema_types(db, project_files);
    let operations = graphql_hir::all_operations(db, project_files);
    let all_fragments = graphql_hir::all_fragments(db, project_files);

    // Step 1: Collect all schema fields (type_name, field_name) -> (FileId, FieldSignature)
    let mut schema_fields: SchemaFieldsMap = HashMap::new();
    for (type_name, type_def) in schema {
        for field in &type_def.fields {
            schema_fields.insert(
                (type_name.clone(), field.name.clone()),
                (type_def.file_id, field),
            );
        }
    }

    // Step 2: Collect all used fields by walking operations and fragments
    let mut used_fields: HashSet<(Arc<str>, Arc<str>)> = HashSet::new();
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let document_files: Vec<(
        graphql_db::FileId,
        graphql_db::FileContent,
        graphql_db::FileMetadata,
    )> = doc_ids
        .iter()
        .filter_map(|file_id| {
            graphql_db::file_lookup(db, project_files, *file_id)
                .map(|(content, metadata)| (*file_id, content, metadata))
        })
        .collect();

    for operation in operations.iter() {
        let root_type_name = match operation.operation_type {
            graphql_hir::OperationType::Query => "Query",
            graphql_hir::OperationType::Mutation => "Mutation",
            graphql_hir::OperationType::Subscription => "Subscription",
        };

        if let Some((_, content, metadata)) = document_files
            .iter()
            .find(|(fid, _, _)| *fid == operation.file_id)
        {
            let body = graphql_hir::operation_body(db, *content, *metadata, operation.index);

            let root_type = Arc::from(root_type_name);
            collect_used_fields_from_selections(
                &body.selections,
                &root_type,
                schema,
                all_fragments,
                db,
                &document_files,
                &mut used_fields,
                &mut HashSet::new(), // Track visited fragments to avoid cycles
            );
        }
    }

    // Step 3: Compare schema fields with used fields to find unused ones
    let mut unused = Vec::new();
    for ((type_name, field_name), (_file_id, _field_sig)) in &schema_fields {
        if !used_fields.contains(&(type_name.clone(), field_name.clone())) {
            let field_id = FieldId::new(unsafe { salsa::Id::from_index(0) });

            unused.push((
                field_id,
                Diagnostic::warning(
                    format!("Field '{type_name}.{field_name}' is never used in any operation"),
                    DiagnosticRange::default(), // Position would require tracking field locations in HIR
                ),
            ));
        }
    }

    Arc::new(unused)
}

/// Find unused fragments (project-wide analysis)
///
/// Uses HIR queries for fragment data instead of cloning ASTs.
/// This avoids massive memory allocation when processing large projects.
#[salsa::tracked]
pub fn find_unused_fragments(
    db: &dyn GraphQLAnalysisDatabase,
) -> Arc<Vec<(FragmentId, Diagnostic)>> {
    let project_files = db
        .project_files()
        .expect("project files must be set for project-wide analysis");
    let all_fragments = graphql_hir::all_fragments(db, project_files);

    // Use the fragment spreads index from HIR (cached, no AST cloning needed)
    let fragment_spreads_index = graphql_hir::fragment_spreads_index(db, project_files);

    let mut used_fragments = HashSet::new();

    let doc_ids = project_files.document_file_ids(db).ids(db);
    for file_id in doc_ids.iter() {
        let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) else {
            continue;
        };

        let file_ops = graphql_hir::file_operations(db, *file_id, content, metadata);
        for (op_index, _op) in file_ops.iter().enumerate() {
            let body = graphql_hir::operation_body(db, content, metadata, op_index);
            for spread in &body.fragment_spreads {
                collect_fragment_transitive(spread, &fragment_spreads_index, &mut used_fragments);
            }
        }
    }

    // Fragment spreads from fragment-to-fragment references are already handled
    // by the transitive collection above. The fragment_spreads_index contains
    // the direct spreads for each fragment, and collect_fragment_transitive
    // follows them recursively.

    let mut unused = Vec::new();
    for fragment_name in all_fragments.keys() {
        if !used_fragments.contains(fragment_name) {
            // Create a dummy FragmentId - in a real implementation,
            // we'd track the actual FragmentId in the HIR
            let fragment_id = FragmentId::new(unsafe { salsa::Id::from_index(0) });

            unused.push((
                fragment_id,
                Diagnostic::warning(
                    format!("Fragment '{fragment_name}' is never used"),
                    DiagnosticRange::default(), // Position would require CST traversal
                ),
            ));
        }
    }

    Arc::new(unused)
}

/// Collect a fragment and all fragments it transitively spreads
fn collect_fragment_transitive(
    fragment_name: &Arc<str>,
    fragment_spreads_index: &std::collections::HashMap<Arc<str>, HashSet<Arc<str>>>,
    used_fragments: &mut HashSet<Arc<str>>,
) {
    let mut to_process: VecDeque<Arc<str>> = VecDeque::new();
    to_process.push_back(fragment_name.clone());

    while let Some(name) = to_process.pop_front() {
        if used_fragments.contains(&name) {
            continue;
        }
        used_fragments.insert(name.clone());

        if let Some(spreads) = fragment_spreads_index.get(&name) {
            for spread in spreads {
                if !used_fragments.contains(spread) {
                    to_process.push_back(spread.clone());
                }
            }
        }
    }
}

/// Collect used fields from selections, tracking type context
#[allow(clippy::too_many_arguments)]
fn collect_used_fields_from_selections(
    selections: &[graphql_hir::Selection],
    current_type: &Arc<str>,
    schema: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    all_fragments: &HashMap<Arc<str>, graphql_hir::FragmentStructure>,
    db: &dyn GraphQLAnalysisDatabase,
    document_files: &[(
        graphql_db::FileId,
        graphql_db::FileContent,
        graphql_db::FileMetadata,
    )],
    used_fields: &mut HashSet<(Arc<str>, Arc<str>)>,
    visited_fragments: &mut HashSet<Arc<str>>,
) {
    for selection in selections {
        match selection {
            graphql_hir::Selection::Field {
                name,
                selection_set,
                ..
            } => {
                // Mark this field as used on the current type
                used_fields.insert((current_type.clone(), name.clone()));

                if let Some(type_def) = schema.get(current_type) {
                    if let Some(field) = type_def.fields.iter().find(|f| f.name == *name) {
                        // Unwrap the type (handle lists and non-null)
                        let field_type = unwrap_type_name(&field.type_ref.name);

                        // Recurse into nested selections if any
                        if !selection_set.is_empty() {
                            collect_used_fields_from_selections(
                                selection_set,
                                &field_type,
                                schema,
                                all_fragments,
                                db,
                                document_files,
                                used_fields,
                                visited_fragments,
                            );
                        }
                    }
                }
            }
            graphql_hir::Selection::FragmentSpread {
                name: fragment_name,
            } => {
                // Avoid infinite recursion with circular fragments
                if visited_fragments.contains(fragment_name) {
                    continue;
                }
                visited_fragments.insert(fragment_name.clone());

                if let Some(fragment) = all_fragments.get(fragment_name) {
                    if let Some((_, content, metadata)) = document_files
                        .iter()
                        .find(|(fid, _, _)| *fid == fragment.file_id)
                    {
                        let fragment_body = graphql_hir::fragment_body(
                            db,
                            *content,
                            *metadata,
                            fragment_name.clone(),
                        );

                        collect_used_fields_from_selections(
                            &fragment_body.selections,
                            &fragment.type_condition,
                            schema,
                            all_fragments,
                            db,
                            document_files,
                            used_fields,
                            visited_fragments,
                        );
                    }
                }
            }
            graphql_hir::Selection::InlineFragment {
                type_condition,
                selection_set,
            } => {
                // Use the type condition if specified, otherwise continue with current type
                let fragment_type = type_condition.as_ref().unwrap_or(current_type);

                collect_used_fields_from_selections(
                    selection_set,
                    fragment_type,
                    schema,
                    all_fragments,
                    db,
                    document_files,
                    used_fields,
                    visited_fragments,
                );
            }
        }
    }
}

/// Unwrap a type name (remove list/non-null wrappers)
fn unwrap_type_name(type_name: &str) -> Arc<str> {
    Arc::from(type_name.trim_matches(|c| c == '[' || c == ']' || c == '!'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};

    // Test database
    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
        project_files: std::cell::Cell<Option<ProjectFiles>>,
    }

    impl TestDatabase {
        fn set_project_files(&mut self, project_files: Option<ProjectFiles>) {
            self.project_files.set(project_files);
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDatabase {}

    #[salsa::db]
    impl graphql_syntax::GraphQLSyntaxDatabase for TestDatabase {}

    #[salsa::db]
    impl graphql_hir::GraphQLHirDatabase for TestDatabase {
        fn project_files(&self) -> Option<ProjectFiles> {
            self.project_files.get()
        }
    }

    #[salsa::db]
    impl crate::GraphQLAnalysisDatabase for TestDatabase {}

    #[test]
    fn test_unused_fields_basic() {
        let mut db = TestDatabase::default();

        // Schema with User type having id, name, and email fields
        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from(
                r"
                type Query {
                    user: User
                }

                type User {
                    id: ID!
                    name: String!
                    email: String!
                }
                ",
            ),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Operation that only uses id and name, not email
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(
            &db,
            Arc::from(
                r"
                query GetUser {
                    user {
                        id
                        name
                    }
                }
                ",
            ),
        );
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = graphql_db::test_utils::create_project_files(
            &mut db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        db.set_project_files(Some(project_files));

        let unused = find_unused_fields(&db);

        // Should find User.email as unused
        assert_eq!(unused.len(), 1);
        assert!(unused[0].1.message.contains("User.email"));
    }

    #[test]
    fn test_unused_fields_with_fragments() {
        let mut db = TestDatabase::default();

        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from(
                r"
                type Query {
                    user: User
                }

                type User {
                    id: ID!
                    name: String!
                    email: String!
                    age: Int
                }
                ",
            ),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Operation using a fragment that references email
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(
            &db,
            Arc::from(
                r"
                query GetUser {
                    user {
                        ...UserFields
                    }
                }

                fragment UserFields on User {
                    id
                    name
                    email
                }
                ",
            ),
        );
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = graphql_db::test_utils::create_project_files(
            &mut db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        db.set_project_files(Some(project_files));

        let unused = find_unused_fields(&db);

        // Should find User.age as unused (email is used via fragment)
        assert_eq!(unused.len(), 1);
        assert!(unused[0].1.message.contains("User.age"));
    }

    #[test]
    fn test_unused_fields_nested_types() {
        let mut db = TestDatabase::default();

        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from(
                r"
                type Query {
                    user: User
                }

                type User {
                    id: ID!
                    name: String!
                    posts: [Post!]!
                }

                type Post {
                    id: ID!
                    title: String!
                    content: String!
                }
                ",
            ),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Operation that queries nested posts but not all fields
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(
            &db,
            Arc::from(
                r"
                query GetUser {
                    user {
                        id
                        posts {
                            id
                            title
                        }
                    }
                }
                ",
            ),
        );
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = graphql_db::test_utils::create_project_files(
            &mut db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        db.set_project_files(Some(project_files));

        let unused = find_unused_fields(&db);

        // Should find User.name and Post.content as unused
        assert_eq!(unused.len(), 2);
        let messages: Vec<Arc<str>> = unused.iter().map(|(_, d)| d.message.clone()).collect();
        assert!(messages.iter().any(|m| m.contains("User.name")));
        assert!(messages.iter().any(|m| m.contains("Post.content")));
    }

    #[test]
    fn test_unused_fields_transitive_fragments() {
        let mut db = TestDatabase::default();

        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from(
                r"
                type Query {
                    user: User
                }

                type User {
                    id: ID!
                    name: String!
                    email: String!
                    phone: String
                }
                ",
            ),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        // Operation using fragment that spreads another fragment
        let doc_id = FileId::new(1);
        let doc_content = FileContent::new(
            &db,
            Arc::from(
                r"
                query GetUser {
                    user {
                        ...UserBasic
                    }
                }

                fragment UserBasic on User {
                    id
                    ...UserContact
                }

                fragment UserContact on User {
                    email
                }
                ",
            ),
        );
        let doc_metadata = FileMetadata::new(
            &db,
            doc_id,
            FileUri::new("query.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let project_files = graphql_db::test_utils::create_project_files(
            &mut db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        db.set_project_files(Some(project_files));

        let unused = find_unused_fields(&db);

        // Should find User.name and User.phone as unused
        assert_eq!(unused.len(), 2);
        let messages: Vec<Arc<str>> = unused.iter().map(|(_, d)| d.message.clone()).collect();
        assert!(messages.iter().any(|m| m.contains("User.name")));
        assert!(messages.iter().any(|m| m.contains("User.phone")));
    }

    #[test]
    fn test_unwrap_type_name() {
        assert_eq!(unwrap_type_name("String"), Arc::from("String"));
        assert_eq!(unwrap_type_name("String!"), Arc::from("String"));
        assert_eq!(unwrap_type_name("[String]"), Arc::from("String"));
        assert_eq!(unwrap_type_name("[String!]"), Arc::from("String"));
        assert_eq!(unwrap_type_name("[String!]!"), Arc::from("String"));
        assert_eq!(unwrap_type_name("[[String]]"), Arc::from("String"));
    }
}
