use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_hir::{FieldId, FragmentId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

type SchemaFieldsMap<'a> =
    HashMap<(Arc<str>, Arc<str>), (graphql_base_db::FileId, &'a graphql_hir::FieldSignature)>;

/// Information about how a schema field is used across all operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldUsage {
    /// The type that contains this field
    pub type_name: Arc<str>,
    /// The name of the field
    pub field_name: Arc<str>,
    /// How many times the field is used (across all operations)
    pub usage_count: usize,
    /// Names of operations that use this field
    pub operations: Vec<Arc<str>>,
}

/// Summary of field usage coverage for the entire project
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldCoverageReport {
    /// Total number of schema fields
    pub total_fields: usize,
    /// Number of fields that are used in at least one operation
    pub used_fields: usize,
    /// Field usage details keyed by (`type_name`, `field_name`)
    pub field_usages: HashMap<(Arc<str>, Arc<str>), FieldUsage>,
    /// Coverage statistics by type
    pub type_coverage: HashMap<Arc<str>, TypeCoverage>,
}

impl FieldCoverageReport {
    /// Calculate coverage as a percentage (0.0 to 100.0)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn coverage_percentage(&self) -> f64 {
        if self.total_fields == 0 {
            100.0
        } else {
            (self.used_fields as f64 / self.total_fields as f64) * 100.0
        }
    }
}

/// Coverage statistics for a single type
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypeCoverage {
    /// Total number of fields on this type
    pub total_fields: usize,
    /// Number of fields that are used
    pub used_fields: usize,
}

impl TypeCoverage {
    /// Calculate coverage as a percentage (0.0 to 100.0)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn coverage_percentage(&self) -> f64 {
        if self.total_fields == 0 {
            100.0
        } else {
            (self.used_fields as f64 / self.total_fields as f64) * 100.0
        }
    }
}

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
        graphql_base_db::FileId,
        graphql_base_db::FileContent,
        graphql_base_db::FileMetadata,
    )> = doc_ids
        .iter()
        .filter_map(|file_id| {
            graphql_base_db::file_lookup(db, project_files, *file_id)
                .map(|(content, metadata)| (*file_id, content, metadata))
        })
        .collect();

    for operation in operations.iter() {
        #[allow(clippy::match_same_arms)]
        let root_type_name = match operation.operation_type {
            graphql_hir::OperationType::Query => "Query",
            graphql_hir::OperationType::Mutation => "Mutation",
            graphql_hir::OperationType::Subscription => "Subscription",
            _ => "Query", // fallback for future operation types
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
        let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        else {
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

/// Analyze field usage across all operations in the project
///
/// Returns detailed usage information for every schema field,
/// including which operations use each field and how many times.
#[salsa::tracked]
#[allow(clippy::too_many_lines)]
pub fn analyze_field_usage(db: &dyn GraphQLAnalysisDatabase) -> Arc<FieldCoverageReport> {
    let project_files = db
        .project_files()
        .expect("project files must be set for project-wide analysis");
    let schema = graphql_hir::schema_types(db, project_files);
    let operations = graphql_hir::all_operations(db, project_files);
    let all_fragments = graphql_hir::all_fragments(db, project_files);

    // Build document files lookup
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let document_files: Vec<(
        graphql_base_db::FileId,
        graphql_base_db::FileContent,
        graphql_base_db::FileMetadata,
    )> = doc_ids
        .iter()
        .filter_map(|file_id| {
            graphql_base_db::file_lookup(db, project_files, *file_id)
                .map(|(content, metadata)| (*file_id, content, metadata))
        })
        .collect();

    // Initialize field usage map with all schema fields
    // Only include Object and Interface types - InputObject, Scalar, Enum, Union don't have
    // selectable fields in the same sense (InputObject fields are provided, not selected)
    let mut field_usages: HashMap<(Arc<str>, Arc<str>), FieldUsage> = HashMap::new();
    let mut type_coverage: HashMap<Arc<str>, TypeCoverage> = HashMap::new();
    let mut total_fields = 0;

    for (type_name, type_def) in schema {
        // Skip non-selectable types
        if !matches!(
            type_def.kind,
            graphql_hir::TypeDefKind::Object | graphql_hir::TypeDefKind::Interface
        ) {
            continue;
        }

        let field_count = type_def.fields.len();
        type_coverage.insert(
            type_name.clone(),
            TypeCoverage {
                total_fields: field_count,
                used_fields: 0,
            },
        );
        total_fields += field_count;

        for field in &type_def.fields {
            field_usages.insert(
                (type_name.clone(), field.name.clone()),
                FieldUsage {
                    type_name: type_name.clone(),
                    field_name: field.name.clone(),
                    usage_count: 0,
                    operations: Vec::new(),
                },
            );
        }
    }

    // Track field usages per operation to support usage_count and operations list
    for operation in operations.iter() {
        #[allow(clippy::match_same_arms)]
        let root_type_name = match operation.operation_type {
            graphql_hir::OperationType::Query => "Query",
            graphql_hir::OperationType::Mutation => "Mutation",
            graphql_hir::OperationType::Subscription => "Subscription",
            _ => "Query", // fallback for future operation types
        };

        let operation_name = operation
            .name
            .as_ref()
            .map_or_else(|| Arc::from("<anonymous>"), Arc::clone);

        if let Some((_, content, metadata)) = document_files
            .iter()
            .find(|(fid, _, _)| *fid == operation.file_id)
        {
            let body = graphql_hir::operation_body(db, *content, *metadata, operation.index);

            // Collect fields used in this operation
            let mut operation_fields: HashSet<(Arc<str>, Arc<str>)> = HashSet::new();
            let root_type = Arc::from(root_type_name);
            collect_field_usages_from_selections(
                &body.selections,
                &root_type,
                schema,
                all_fragments,
                db,
                &document_files,
                &mut operation_fields,
                &mut HashSet::new(),
            );

            // Update field usage counts
            for (type_name, field_name) in operation_fields {
                if let Some(usage) = field_usages.get_mut(&(type_name.clone(), field_name.clone()))
                {
                    usage.usage_count += 1;
                    if !usage.operations.contains(&operation_name) {
                        usage.operations.push(operation_name.clone());
                    }
                }
            }
        }
    }

    // Calculate type coverage (count used fields per type)
    let mut used_fields_count = 0;
    for usage in field_usages.values() {
        if usage.usage_count > 0 {
            used_fields_count += 1;
            if let Some(type_cov) = type_coverage.get_mut(&usage.type_name) {
                type_cov.used_fields += 1;
            }
        }
    }

    Arc::new(FieldCoverageReport {
        total_fields,
        used_fields: used_fields_count,
        field_usages,
        type_coverage,
    })
}

/// Helper to collect field usages from selections (for field usage analysis)
#[allow(clippy::too_many_arguments)]
fn collect_field_usages_from_selections(
    selections: &[graphql_hir::Selection],
    current_type: &Arc<str>,
    schema: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    all_fragments: &HashMap<Arc<str>, graphql_hir::FragmentStructure>,
    db: &dyn GraphQLAnalysisDatabase,
    document_files: &[(
        graphql_base_db::FileId,
        graphql_base_db::FileContent,
        graphql_base_db::FileMetadata,
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
                // Record this field usage
                used_fields.insert((current_type.clone(), name.clone()));

                // Get the field's return type to recurse into nested selections
                if let Some(type_def) = schema.get(current_type) {
                    if let Some(field) = type_def.fields.iter().find(|f| f.name == *name) {
                        let field_type = unwrap_type_name(&field.type_ref.name);

                        if !selection_set.is_empty() {
                            collect_field_usages_from_selections(
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

                        collect_field_usages_from_selections(
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
                let fragment_type = type_condition.as_ref().unwrap_or(current_type);

                collect_field_usages_from_selections(
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
        graphql_base_db::FileId,
        graphql_base_db::FileContent,
        graphql_base_db::FileMetadata,
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
    use graphql_base_db::{
        DocumentFileIds, FileContent, FileEntry, FileEntryMap, FileId, FileKind, FileMetadata,
        FileUri, ProjectFiles, SchemaFileIds,
    };

    #[salsa::db]
    #[derive(Clone)]
    struct TestDatabase {
        storage: salsa::Storage<Self>,
        project_files: std::cell::Cell<Option<ProjectFiles>>,
    }

    impl Default for TestDatabase {
        fn default() -> Self {
            Self {
                storage: salsa::Storage::default(),
                project_files: std::cell::Cell::new(None),
            }
        }
    }

    impl TestDatabase {
        fn set_project_files(&self, project_files: Option<ProjectFiles>) {
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

    fn create_project_files(
        db: &TestDatabase,
        schema_files: &[(FileId, FileContent, FileMetadata)],
        document_files: &[(FileId, FileContent, FileMetadata)],
    ) -> ProjectFiles {
        let schema_ids: Vec<FileId> = schema_files.iter().map(|(id, _, _)| *id).collect();
        let doc_ids: Vec<FileId> = document_files.iter().map(|(id, _, _)| *id).collect();

        let mut entries = std::collections::HashMap::new();
        for (id, content, metadata) in schema_files {
            let entry = FileEntry::new(db, *content, *metadata);
            entries.insert(*id, entry);
        }
        for (id, content, metadata) in document_files {
            let entry = FileEntry::new(db, *content, *metadata);
            entries.insert(*id, entry);
        }

        let schema_file_ids = SchemaFileIds::new(db, Arc::new(schema_ids));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(doc_ids));
        let file_entry_map = FileEntryMap::new(db, Arc::new(entries));

        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_unused_fields_basic() {
        let db = TestDatabase::default();

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

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        db.set_project_files(Some(project_files));

        let unused = find_unused_fields(&db);

        assert_eq!(unused.len(), 1);
        assert!(unused[0].1.message.contains("User.email"));
    }

    #[test]
    fn test_unused_fields_with_fragments() {
        let db = TestDatabase::default();

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

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        db.set_project_files(Some(project_files));

        let unused = find_unused_fields(&db);

        assert_eq!(unused.len(), 1);
        assert!(unused[0].1.message.contains("User.age"));
    }

    #[test]
    fn test_unused_fields_nested_types() {
        let db = TestDatabase::default();

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

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        db.set_project_files(Some(project_files));

        let unused = find_unused_fields(&db);

        assert_eq!(unused.len(), 2);
        let messages: Vec<Arc<str>> = unused.iter().map(|(_, d)| d.message.clone()).collect();
        assert!(messages.iter().any(|m| m.contains("User.name")));
        assert!(messages.iter().any(|m| m.contains("Post.content")));
    }

    #[test]
    fn test_unused_fields_transitive_fragments() {
        let db = TestDatabase::default();

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

        let project_files = create_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        db.set_project_files(Some(project_files));

        let unused = find_unused_fields(&db);

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
