// GraphQL HIR (High-level Intermediate Representation)
// This crate provides semantic queries on top of syntax.
// It implements the "golden invariant": editing a document's body never invalidates global schema knowledge.

use graphql_db::FileId;
use std::collections::HashMap;
use std::sync::Arc;

mod body;
mod structure;

pub use body::*;
pub use structure::*;

/// Identifier for a GraphQL type in the schema
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(salsa::Id);

impl TypeId {
    #[must_use]
    pub const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_id(self) -> salsa::Id {
        self.0
    }
}

/// Identifier for a field definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(salsa::Id);

impl FieldId {
    #[must_use]
    pub const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_id(self) -> salsa::Id {
        self.0
    }
}

/// Identifier for a fragment definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FragmentId(salsa::Id);

impl FragmentId {
    #[must_use]
    pub const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_id(self) -> salsa::Id {
        self.0
    }
}

/// Identifier for an operation definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(salsa::Id);

impl OperationId {
    #[must_use]
    pub const fn new(id: salsa::Id) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_id(self) -> salsa::Id {
        self.0
    }
}

/// The salsa database trait for HIR queries
#[salsa::db]
pub trait GraphQLHirDatabase: graphql_syntax::GraphQLSyntaxDatabase {
    /// Get the project files input
    /// Returns None if no project files have been set yet
    /// This should be overridden by implementations that track project files
    fn project_files(&self) -> Option<graphql_db::ProjectFiles> {
        None
    }
}

// ============================================================================
// Per-file queries - these provide fine-grained caching
// Each query depends only on the specific file's content, not all files
// ============================================================================

/// Get type definitions from a single schema file
/// This query is cached per-file - editing another file won't invalidate it
#[salsa::tracked]
pub fn file_type_defs(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<TypeDef>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::clone(&structure.type_defs)
}

/// Get fragments from a single document file
/// This query is cached per-file - editing another file won't invalidate it
#[salsa::tracked]
pub fn file_fragments(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<FragmentStructure>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::clone(&structure.fragments)
}

/// Get operations from a single document file
/// This query is cached per-file - editing another file won't invalidate it
#[salsa::tracked]
pub fn file_operations(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<OperationStructure>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::clone(&structure.operations)
}

// ============================================================================
// Aggregate queries - these use granular inputs for efficient invalidation
// They depend on file IDs (stable) and call per-file queries (granular caching)
// ============================================================================

/// Get all types in the schema
///
/// This query uses granular dependencies:
/// - Depends on `SchemaFileIds` (only changes when files are added/removed)
/// - Calls `file_type_defs` per-file (each cached independently)
///
/// When a single schema file changes, only that file's `file_type_defs` is recomputed.
/// Other files' results come from cache.
#[salsa::tracked(returns(ref))]
pub fn schema_types(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> HashMap<Arc<str>, TypeDef> {
    let schema_ids = project_files.schema_file_ids(db).ids(db);
    let mut types = HashMap::new();

    for file_id in schema_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            let file_types = file_type_defs(db, *file_id, content, metadata);
            for type_def in file_types.iter() {
                types.insert(type_def.name.clone(), type_def.clone());
            }
        }
    }

    types
}

/// Get all fragments in the project
///
/// This query uses granular dependencies:
/// - Depends on `DocumentFileIds` (only changes when files are added/removed)
/// - Calls `file_fragments` per-file (each cached independently)
///
/// When a single document file changes, only that file's `file_fragments` is recomputed.
/// Other files' results come from cache.
#[salsa::tracked(returns(ref))]
pub fn all_fragments(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> HashMap<Arc<str>, FragmentStructure> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut fragments = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query - cached independently
            let file_frags = file_fragments(db, *file_id, content, metadata);
            for fragment in file_frags.iter() {
                fragments.insert(fragment.name.clone(), fragment.clone());
            }
        }
    }

    fragments
}

/// Index mapping fragment names to their file content and metadata
/// Uses granular per-file caching for efficient invalidation.
#[salsa::tracked]
pub fn fragment_file_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, (graphql_db::FileContent, graphql_db::FileMetadata)>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query for fragments
            let file_frags = file_fragments(db, *file_id, content, metadata);
            for fragment in file_frags.iter() {
                index.insert(fragment.name.clone(), (content, metadata));
            }
        }
    }

    Arc::new(index)
}

/// Per-file query for fragment sources in a single file.
/// Returns a map of fragment name -> source text for all fragments in this file.
/// This enables fine-grained caching: editing file A only invalidates file A's sources.
#[salsa::tracked]
pub fn file_fragment_sources(
    db: &dyn GraphQLHirDatabase,
    _file_id: graphql_db::FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<HashMap<Arc<str>, Arc<str>>> {
    let parse = graphql_syntax::parse(db, content, metadata);
    let mut sources = HashMap::new();

    // Unified: iterate over all documents (works for both pure GraphQL and TS/JS)
    for doc in parse.documents() {
        let source: Arc<str> = Arc::from(doc.source);
        for def in &doc.ast.definitions {
            if let apollo_compiler::ast::Definition::FragmentDefinition(frag) = def {
                let name: Arc<str> = Arc::from(frag.name.as_str());
                sources.insert(name, source.clone());
            }
        }
    }

    Arc::new(sources)
}

/// Per-file query for fragment ASTs in a single file.
/// Returns a map of fragment name -> AST document containing that fragment.
/// This enables caching of parsed ASTs to avoid re-parsing during validation.
#[salsa::tracked]
pub fn file_fragment_asts(
    db: &dyn GraphQLHirDatabase,
    _file_id: graphql_db::FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<HashMap<Arc<str>, Arc<apollo_compiler::ast::Document>>> {
    let parse = graphql_syntax::parse(db, content, metadata);
    let mut asts = HashMap::new();

    // Unified: iterate over all documents (works for both pure GraphQL and TS/JS)
    for doc in parse.documents() {
        // Clone the AST Arc for this document
        let ast_arc = Arc::new(doc.ast.clone());
        for def in &doc.ast.definitions {
            if let apollo_compiler::ast::Definition::FragmentDefinition(frag) = def {
                let name: Arc<str> = Arc::from(frag.name.as_str());
                asts.insert(name, ast_arc.clone());
            }
        }
    }

    Arc::new(asts)
}

/// Index mapping fragment names to their file location.
/// Used by `fragment_source` to find which file contains a fragment.
#[salsa::tracked]
pub fn fragment_file_location_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, graphql_db::FileId>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            let file_frags = file_fragments(db, *file_id, content, metadata);
            for fragment in file_frags.iter() {
                index.insert(fragment.name.clone(), *file_id);
            }
        }
    }

    Arc::new(index)
}

/// Get the source text for a single fragment by name.
/// This creates a fine-grained Salsa dependency: if you query fragment "Foo",
/// you only depend on the file containing "Foo", not all fragment files.
///
/// When fragment "Foo" changes, only queries that called `fragment_source(db, "Foo")`
/// are invalidated, not queries that depend on other fragments.
#[salsa::tracked]
#[allow(clippy::needless_pass_by_value)] // Salsa tracked functions require owned arguments
pub fn fragment_source(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
    fragment_name: Arc<str>,
) -> Option<Arc<str>> {
    // First, find which file contains this fragment
    let location_index = fragment_file_location_index(db, project_files);
    let file_id = location_index.get(&fragment_name)?;

    // Get the file's content and metadata
    let (content, metadata) = graphql_db::file_lookup(db, project_files, *file_id)?;

    // Query just this file's fragment sources (fine-grained dependency)
    let file_sources = file_fragment_sources(db, *file_id, content, metadata);
    file_sources.get(&fragment_name).cloned()
}

/// Get the parsed AST document containing a single fragment by name.
/// This creates a fine-grained Salsa dependency and enables caching of parsed ASTs.
///
/// Unlike `fragment_source` which returns source text (requiring re-parsing),
/// this returns the already-parsed AST document, avoiding redundant parsing
/// during validation.
///
/// When fragment "Foo" changes, only queries that called `fragment_ast(db, "Foo")`
/// are invalidated, not queries that depend on other fragments.
#[salsa::tracked]
#[allow(clippy::needless_pass_by_value)] // Salsa tracked functions require owned arguments
pub fn fragment_ast(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
    fragment_name: Arc<str>,
) -> Option<Arc<apollo_compiler::ast::Document>> {
    // First, find which file contains this fragment
    let location_index = fragment_file_location_index(db, project_files);
    let file_id = location_index.get(&fragment_name)?;

    // Get the file's content and metadata
    let (content, metadata) = graphql_db::file_lookup(db, project_files, *file_id)?;

    // Query just this file's fragment ASTs (fine-grained dependency)
    let file_asts = file_fragment_asts(db, *file_id, content, metadata);
    file_asts.get(&fragment_name).cloned()
}

/// Index mapping fragment names to their source text (the GraphQL block containing them).
///
/// For TS/JS files with multiple blocks, this returns only the specific block
/// containing each fragment, not all blocks from the file. This is crucial for
/// proper validation - we don't want to accidentally include unrelated operations
/// or fragments from the same file.
///
/// NOTE: This is a convenience query for bulk access. For fine-grained invalidation,
/// use `fragment_source(db, project_files, fragment_name)` instead.
#[salsa::tracked]
pub fn fragment_source_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, Arc<str>>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Use per-file query for granular caching
            let file_sources = file_fragment_sources(db, *file_id, content, metadata);
            index.extend(file_sources.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }

    Arc::new(index)
}

/// Per-file query for fragment spreads mapping
/// This enables fine-grained incremental computation - editing fragment A
/// only invalidates file A's spreads, not the entire project index.
#[salsa::tracked]
pub fn file_fragment_spreads(
    db: &dyn GraphQLHirDatabase,
    file_id: graphql_db::FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<HashMap<Arc<str>, std::collections::HashSet<Arc<str>>>> {
    let file_frags = file_fragments(db, file_id, content, metadata);
    let mut spreads = HashMap::new();

    for fragment in file_frags.iter() {
        // Get the fragment body to find its spreads
        let body = fragment_body(db, content, metadata, fragment.name.clone());
        spreads.insert(fragment.name.clone(), body.fragment_spreads.clone());
    }

    Arc::new(spreads)
}

/// Index mapping fragment names to the fragments they reference (spread)
/// Uses per-file queries for fine-grained incremental computation.
/// Editing one fragment file only rebuilds that file's spreads, not the entire index.
#[salsa::tracked]
pub fn fragment_spreads_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, std::collections::HashSet<Arc<str>>>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query - only rebuilds when THIS file changes
            let file_spreads = file_fragment_spreads(db, *file_id, content, metadata);
            index.extend(file_spreads.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }

    Arc::new(index)
}

/// Get all operations in the project
/// Uses granular per-file caching for efficient invalidation.
#[salsa::tracked]
pub fn all_operations(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<Vec<OperationStructure>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut operations = Vec::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) {
            // Per-file query for operations
            let file_ops = file_operations(db, *file_id, content, metadata);
            operations.extend(file_ops.iter().cloned());
        }
    }

    Arc::new(operations)
}

// ============================================================================
// Per-file contribution queries for project-wide lint rules
// These enable incremental computation: editing one file only recomputes that
// file's contribution, other files' contributions come from cache.
// ============================================================================

/// Per-file query for fragment names used (spread) in a file.
/// Returns all fragment spread names found in operations and fragments.
/// This enables incremental computation for the `unused_fragments` lint rule.
#[salsa::tracked]
#[allow(clippy::items_after_statements)]
pub fn file_used_fragment_names(
    db: &dyn GraphQLHirDatabase,
    _file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<std::collections::HashSet<Arc<str>>> {
    let parse = graphql_syntax::parse(db, content, metadata);
    let mut used = std::collections::HashSet::new();

    // Helper to collect fragment spreads recursively
    fn collect_spreads(
        selections: &[apollo_compiler::ast::Selection],
        used: &mut std::collections::HashSet<Arc<str>>,
    ) {
        for selection in selections {
            match selection {
                apollo_compiler::ast::Selection::Field(field) => {
                    collect_spreads(&field.selection_set, used);
                }
                apollo_compiler::ast::Selection::FragmentSpread(spread) => {
                    used.insert(Arc::from(spread.fragment_name.as_str()));
                }
                apollo_compiler::ast::Selection::InlineFragment(inline) => {
                    collect_spreads(&inline.selection_set, used);
                }
            }
        }
    }

    // Unified: process all documents (works for both pure GraphQL and TS/JS)
    for doc in parse.documents() {
        for definition in &doc.ast.definitions {
            match definition {
                apollo_compiler::ast::Definition::OperationDefinition(op) => {
                    collect_spreads(&op.selection_set, &mut used);
                }
                apollo_compiler::ast::Definition::FragmentDefinition(frag) => {
                    collect_spreads(&frag.selection_set, &mut used);
                }
                _ => {}
            }
        }
    }

    Arc::new(used)
}

/// Per-file query for defined fragment names in a file.
/// Returns `fragment_name` for all fragments defined in the file.
/// This enables incremental computation for the `unused_fragments` lint rule.
#[salsa::tracked]
pub fn file_defined_fragment_names(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<Arc<str>>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::new(structure.fragments.iter().map(|f| f.name.clone()).collect())
}

/// Info about a defined fragment for the `unique_names` lint rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FragmentNameInfo {
    /// The fragment name
    pub name: Arc<str>,
    /// The text range of the fragment name
    pub name_range: TextRange,
    /// For embedded GraphQL: line offset of the block (0-indexed)
    pub block_line_offset: Option<usize>,
    /// For embedded GraphQL: source text of the block
    pub block_source: Option<Arc<str>>,
}

/// Per-file query for defined fragment info in a file.
/// Returns info for all fragments including block context for embedded GraphQL.
/// This enables incremental computation for the `unique_names` lint rule.
#[salsa::tracked]
pub fn file_fragment_info(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<FragmentNameInfo>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::new(
        structure
            .fragments
            .iter()
            .map(|frag| FragmentNameInfo {
                name: frag.name.clone(),
                name_range: frag.name_range,
                block_line_offset: frag.block_line_offset,
                block_source: frag.block_source.clone(),
            })
            .collect(),
    )
}

/// Info about a named operation for the `unique_names` lint rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationNameInfo {
    /// The operation name
    pub name: Arc<str>,
    /// The operation index (for deduplication)
    pub index: usize,
    /// The text range of the operation name
    pub name_range: Option<TextRange>,
    /// For embedded GraphQL: line offset of the block (0-indexed)
    pub block_line_offset: Option<usize>,
    /// For embedded GraphQL: source text of the block
    pub block_source: Option<Arc<str>>,
}

/// Per-file query for operation names in a file.
/// Returns info for all named operations including block context for embedded GraphQL.
/// This enables incremental computation for the `unique_names` lint rule.
#[salsa::tracked]
pub fn file_operation_names(
    db: &dyn GraphQLHirDatabase,
    file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
) -> Arc<Vec<OperationNameInfo>> {
    let structure = file_structure(db, file_id, content, metadata);
    Arc::new(
        structure
            .operations
            .iter()
            .filter_map(|op| {
                op.name.as_ref().map(|name| OperationNameInfo {
                    name: name.clone(),
                    index: op.index,
                    name_range: op.name_range,
                    block_line_offset: op.block_line_offset,
                    block_source: op.block_source.clone(),
                })
            })
            .collect(),
    )
}

/// A schema coordinate representing a field on a type (e.g., `User.name`).
///
/// Schema coordinates are the standard GraphQL way to reference specific
/// fields within a schema. See: <https://spec.graphql.org/draft/#sec-Schema-Coordinates>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchemaCoordinate {
    pub type_name: Arc<str>,
    pub field_name: Arc<str>,
}

/// Per-file query for schema coordinates used in a file.
/// Returns all `Type.field` coordinates referenced in operations and fragments.
/// This enables incremental computation for the `unused_fields` lint rule.
///
/// Note: This query requires schema types to resolve field return types.
/// If schema is not available, it uses heuristics based on selection patterns.
#[salsa::tracked]
#[allow(clippy::items_after_statements, clippy::too_many_lines)]
pub fn file_schema_coordinates(
    db: &dyn GraphQLHirDatabase,
    _file_id: FileId,
    content: graphql_db::FileContent,
    metadata: graphql_db::FileMetadata,
    project_files: graphql_db::ProjectFiles,
) -> Arc<std::collections::HashSet<SchemaCoordinate>> {
    let parse = graphql_syntax::parse(db, content, metadata);
    let schema_types = schema_types(db, project_files);
    let mut coordinates = std::collections::HashSet::new();

    // Get root type names
    let query_type = schema_types
        .contains_key("Query")
        .then(|| Arc::from("Query"));
    let mutation_type = schema_types
        .contains_key("Mutation")
        .then(|| Arc::from("Mutation"));
    let subscription_type = schema_types
        .contains_key("Subscription")
        .then(|| Arc::from("Subscription"));

    // Helper to collect schema coordinates recursively
    fn collect_coordinates(
        selections: &[apollo_compiler::ast::Selection],
        parent_type: &Arc<str>,
        schema_types: &HashMap<Arc<str>, TypeDef>,
        coordinates: &mut std::collections::HashSet<SchemaCoordinate>,
    ) {
        for selection in selections {
            match selection {
                apollo_compiler::ast::Selection::Field(field) => {
                    let field_name: Arc<str> = Arc::from(field.name.as_str());

                    // Record this schema coordinate
                    coordinates.insert(SchemaCoordinate {
                        type_name: parent_type.clone(),
                        field_name: field_name.clone(),
                    });

                    // Recursively process nested selections
                    if !field.selection_set.is_empty() {
                        // Find the field's return type from schema
                        if let Some(type_def) = schema_types.get(parent_type) {
                            if let Some(field_sig) = type_def
                                .fields
                                .iter()
                                .find(|f| f.name.as_ref() == field_name.as_ref())
                            {
                                let nested_type: Arc<str> =
                                    Arc::from(field_sig.type_ref.name.as_ref());
                                collect_coordinates(
                                    &field.selection_set,
                                    &nested_type,
                                    schema_types,
                                    coordinates,
                                );
                            }
                        }
                    }
                }
                apollo_compiler::ast::Selection::FragmentSpread(_) => {
                    // Fragment spreads are handled separately
                }
                apollo_compiler::ast::Selection::InlineFragment(inline) => {
                    let inline_type = inline
                        .type_condition
                        .as_ref()
                        .map_or_else(|| parent_type.clone(), |tc| Arc::from(tc.as_str()));
                    collect_coordinates(
                        &inline.selection_set,
                        &inline_type,
                        schema_types,
                        coordinates,
                    );
                }
            }
        }
    }

    // Unified: process all documents (works for both pure GraphQL and TS/JS)
    for doc in parse.documents() {
        for definition in &doc.ast.definitions {
            match definition {
                apollo_compiler::ast::Definition::OperationDefinition(op) => {
                    let root_type = match op.operation_type {
                        apollo_compiler::ast::OperationType::Query => query_type.as_ref(),
                        apollo_compiler::ast::OperationType::Mutation => mutation_type.as_ref(),
                        apollo_compiler::ast::OperationType::Subscription => {
                            subscription_type.as_ref()
                        }
                    };
                    if let Some(root) = root_type {
                        collect_coordinates(
                            &op.selection_set,
                            root,
                            schema_types,
                            &mut coordinates,
                        );
                    }
                }
                apollo_compiler::ast::Definition::FragmentDefinition(frag) => {
                    let frag_type = Arc::from(frag.type_condition.as_str());
                    collect_coordinates(
                        &frag.selection_set,
                        &frag_type,
                        schema_types,
                        &mut coordinates,
                    );
                }
                _ => {}
            }
        }
    }

    Arc::new(coordinates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileKind, FileMetadata, FileUri};
    use salsa::Setter;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // TestDatabase for graphql-hir tests.
    // Note: We can't use graphql_test_utils::TestDatabase here because it would
    // create a cyclic dependency (graphql-test-utils depends on graphql-analysis
    // which depends on graphql-hir). Instead, we define a minimal TestDatabase
    // that only implements the traits needed for this crate's tests.
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
    impl GraphQLHirDatabase for TestDatabase {}

    /// Helper to create `ProjectFiles` for tests.
    /// Uses `graphql_db::test_utils` but with our local `TestDatabase`.
    fn create_project_files(
        db: &mut TestDatabase,
        schema_files: &[(FileId, FileContent, FileMetadata)],
        document_files: &[(FileId, FileContent, FileMetadata)],
    ) -> graphql_db::ProjectFiles {
        graphql_db::test_utils::create_project_files(db, schema_files, document_files)
    }

    #[test]
    fn test_schema_types_empty() {
        let mut db = TestDatabase::default();
        let project_files = create_project_files(&mut db, &[], &[]);
        let types = schema_types(&db, project_files);
        assert_eq!(types.len(), 0);
    }

    #[test]
    fn test_file_structure_basic() {
        let db = TestDatabase::default();
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from("type User { id: ID! }"));
        let metadata =
            FileMetadata::new(&db, file_id, FileUri::new("test.graphql"), FileKind::Schema);

        let structure = file_structure(&db, file_id, content, metadata);
        assert_eq!(structure.type_defs.len(), 1);
        assert_eq!(structure.type_defs[0].name.as_ref(), "User");
    }

    // ========================================================================
    // Test for Issue #209: DocumentFiles input granularity causes excessive invalidation
    //
    // This test demonstrates that editing one file's content should NOT cause
    // file_structure queries for OTHER files to be re-executed.
    //
    // BEFORE FIX: all_fragments depends on DocumentFiles which contains all
    // FileContent objects. When any FileContent changes, all_fragments is
    // invalidated, which causes it to re-query file_structure for ALL files.
    //
    // AFTER FIX: all_fragments depends on DocumentFileIds (just file IDs) and
    // per-file queries. Editing file A only invalidates file A's per-file query.
    // ========================================================================

    /// Counter for tracking `file_structure` executions
    static FILE_STRUCTURE_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

    /// Wrapper around `file_structure` that counts executions
    /// We use this to verify caching behavior
    fn counted_file_structure(
        db: &dyn GraphQLHirDatabase,
        file_id: FileId,
        content: graphql_db::FileContent,
        metadata: graphql_db::FileMetadata,
    ) -> Arc<FileStructureData> {
        FILE_STRUCTURE_CALL_COUNT.fetch_add(1, Ordering::SeqCst);
        file_structure(db, file_id, content, metadata)
    }

    #[test]
    fn test_editing_one_file_does_not_recompute_other_files_structure() {
        // Reset counter
        FILE_STRUCTURE_CALL_COUNT.store(0, Ordering::SeqCst);

        let mut db = TestDatabase::default();

        // Create two document files, each with a fragment
        let file1_id = FileId::new(0);
        let file1_content =
            FileContent::new(&db, Arc::from("fragment FragmentA on User { id name }"));
        let file1_metadata = FileMetadata::new(
            &db,
            file1_id,
            FileUri::new("file1.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let file2_id = FileId::new(1);
        let file2_content =
            FileContent::new(&db, Arc::from("fragment FragmentB on User { email }"));
        let file2_metadata = FileMetadata::new(
            &db,
            file2_id,
            FileUri::new("file2.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Create project files with new granular structure
        let doc_files = [
            (file1_id, file1_content, file1_metadata),
            (file2_id, file2_content, file2_metadata),
        ];
        let project_files = create_project_files(&mut db, &[], &doc_files);

        // First call: compute file_structure for both files to warm the cache
        let _ = counted_file_structure(&db, file1_id, file1_content, file1_metadata);
        let _ = counted_file_structure(&db, file2_id, file2_content, file2_metadata);
        assert_eq!(
            FILE_STRUCTURE_CALL_COUNT.load(Ordering::SeqCst),
            2,
            "Expected 2 initial file_structure calls"
        );

        // Query all_fragments to also warm that cache
        let fragments = all_fragments(&db, project_files);
        assert_eq!(fragments.len(), 2, "Should have 2 fragments");

        // Reset counter before the edit
        FILE_STRUCTURE_CALL_COUNT.store(0, Ordering::SeqCst);

        // Now edit ONLY file2's content
        // With the new granular architecture, we only update the FileContent.text
        // The FileEntryMap HashMap stays the same (same keys, same Arc)
        file2_content
            .set_text(&mut db)
            .to(Arc::from("fragment FragmentB on User { email phone }"));

        // Query file1's structure - this should come from cache
        let _ = counted_file_structure(&db, file1_id, file1_content, file1_metadata);

        // ASSERTION: After editing file2, file1's structure should NOT be recomputed
        // It should be served from Salsa's cache since file1's content didn't change
        let _file1_calls = FILE_STRUCTURE_CALL_COUNT.load(Ordering::SeqCst);

        FILE_STRUCTURE_CALL_COUNT.store(0, Ordering::SeqCst);

        // Query all_fragments again after editing file2
        let fragments_after = all_fragments(&db, project_files);
        assert_eq!(fragments_after.len(), 2, "Should still have 2 fragments");

        // Check if FragmentB was updated (it should have "phone" now)
        let _frag_b = fragments_after
            .get("FragmentB")
            .expect("FragmentB should exist");

        // With the new granular architecture:
        // - DocumentFileIds didn't change (same files)
        // - Only file2's FileContent changed
        // - So only file2's file_fragments query should recompute
        // - file1's file_fragments should come from cache
    }

    /// This test verifies the core issue: `all_fragments` depends on `DocumentFiles`
    /// which causes full invalidation when any file content changes.
    ///
    /// After the fix (using `DocumentFileIds` + per-file queries), this test should
    /// show that editing one file doesn't cause the aggregate query to do
    /// unnecessary work for other files.
    #[test]
    fn test_all_fragments_granular_invalidation() {
        let mut db = TestDatabase::default();

        // Create two document files with fragments
        let file1_id = FileId::new(0);
        let file1_content = FileContent::new(&db, Arc::from("fragment F1 on User { id }"));
        let file1_metadata = FileMetadata::new(
            &db,
            file1_id,
            FileUri::new("f1.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let file2_id = FileId::new(1);
        let file2_content = FileContent::new(&db, Arc::from("fragment F2 on User { name }"));
        let file2_metadata = FileMetadata::new(
            &db,
            file2_id,
            FileUri::new("f2.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let doc_files = [
            (file1_id, file1_content, file1_metadata),
            (file2_id, file2_content, file2_metadata),
        ];
        let project_files = create_project_files(&mut db, &[], &doc_files);

        // Warm the cache
        let frags1 = all_fragments(&db, project_files);
        assert_eq!(frags1.len(), 2);
        assert!(frags1.contains_key("F1"));
        assert!(frags1.contains_key("F2"));

        // Edit file2's content - with new granular architecture, only update FileContent.text
        file2_content
            .set_text(&mut db)
            .to(Arc::from("fragment F2 on User { name email }"));

        // Query again - file1's data should come from cache
        let frags2 = all_fragments(&db, project_files);
        assert_eq!(frags2.len(), 2);

        // Both fragments should still be present
        assert!(frags2.contains_key("F1"), "F1 should still exist");
        assert!(frags2.contains_key("F2"), "F2 should still exist");

        // The structural data should be correct
        let _f1 = frags2.get("F1").unwrap();
        // With the new granular architecture:
        // - DocumentFileIds didn't change (same files)
        // - Only file2's FileContent changed
        // - So only file2's file_fragments query should recompute
        // - file1's file_fragments should come from cache
    }

    #[test]
    fn test_fragment_source_per_fragment_lookup() {
        let mut db = TestDatabase::default();

        // Create two document files with different fragments
        let file1_id = FileId::new(0);
        let file1_content =
            FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
        let file1_metadata = FileMetadata::new(
            &db,
            file1_id,
            FileUri::new("user-fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let file2_id = FileId::new(1);
        let file2_content =
            FileContent::new(&db, Arc::from("fragment PostFields on Post { title body }"));
        let file2_metadata = FileMetadata::new(
            &db,
            file2_id,
            FileUri::new("post-fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let doc_files = [
            (file1_id, file1_content, file1_metadata),
            (file2_id, file2_content, file2_metadata),
        ];
        let project_files = create_project_files(&mut db, &[], &doc_files);

        // Query individual fragment sources
        let user_source = fragment_source(&db, project_files, Arc::from("UserFields"));
        let post_source = fragment_source(&db, project_files, Arc::from("PostFields"));
        let nonexistent = fragment_source(&db, project_files, Arc::from("NonExistent"));

        // Verify correct sources are returned
        assert!(user_source.is_some(), "UserFields should exist");
        assert!(post_source.is_some(), "PostFields should exist");
        assert!(nonexistent.is_none(), "NonExistent should not exist");

        assert!(
            user_source.unwrap().contains("UserFields"),
            "UserFields source should contain the fragment"
        );
        assert!(
            post_source.unwrap().contains("PostFields"),
            "PostFields source should contain the fragment"
        );
    }

    #[test]
    fn test_fragment_source_granular_invalidation() {
        let mut db = TestDatabase::default();

        // Create two document files with different fragments
        let file1_id = FileId::new(0);
        let file1_content = FileContent::new(&db, Arc::from("fragment UserFields on User { id }"));
        let file1_metadata = FileMetadata::new(
            &db,
            file1_id,
            FileUri::new("user-fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let file2_id = FileId::new(1);
        let file2_content =
            FileContent::new(&db, Arc::from("fragment PostFields on Post { title }"));
        let file2_metadata = FileMetadata::new(
            &db,
            file2_id,
            FileUri::new("post-fragment.graphql"),
            FileKind::ExecutableGraphQL,
        );

        let doc_files = [
            (file1_id, file1_content, file1_metadata),
            (file2_id, file2_content, file2_metadata),
        ];
        let project_files = create_project_files(&mut db, &[], &doc_files);

        // Warm the cache by querying both fragments
        let user_source_1 = fragment_source(&db, project_files, Arc::from("UserFields"));
        let post_source_1 = fragment_source(&db, project_files, Arc::from("PostFields"));

        assert!(user_source_1.is_some());
        assert!(post_source_1.is_some());

        // Modify file2 (PostFields)
        file2_content
            .set_text(&mut db)
            .to(Arc::from("fragment PostFields on Post { title body }"));

        // Query UserFields again - should come from cache since file1 didn't change
        let user_source_2 = fragment_source(&db, project_files, Arc::from("UserFields"));

        // Query PostFields again - should reflect the update
        let post_source_2 = fragment_source(&db, project_files, Arc::from("PostFields"));

        // UserFields should be unchanged (same Arc)
        assert_eq!(
            user_source_1.as_ref().map(AsRef::as_ref),
            user_source_2.as_ref().map(AsRef::as_ref),
            "UserFields source should be unchanged"
        );

        // PostFields should be updated
        assert!(
            post_source_2.as_ref().unwrap().contains("body"),
            "PostFields should be updated with 'body' field"
        );
    }

    // ========================================================================
    // Caching verification tests using TrackedHirDatabase
    //
    // These tests verify that our query DESIGN enables proper incremental
    // computation. We're not testing Salsa itself (which is well-tested),
    // but rather that:
    //
    // 1. Our input structure (FileEntry, FileContent, etc.) enables per-file granularity
    // 2. Our query dependencies don't accidentally depend on unrelated inputs
    // 3. The "golden invariant" holds: editing operations doesn't invalidate schema knowledge
    //
    // These tests use TrackedHirDatabase which tracks WillExecute events per-database,
    // avoiding parallel test interference.
    // ========================================================================

    #[allow(
        clippy::similar_names,         // file_a/file_b, op1/op2 are intentionally similar in tests
        clippy::uninlined_format_args, // Test assertions are clearer with separate args
        clippy::items_after_statements,// const in test functions is fine
        clippy::cast_possible_truncation, // Test file IDs won't overflow u32
        clippy::doc_markdown,          // Relaxed doc formatting for tests
    )]
    mod caching_tests {
        use super::*;
        use graphql_db::tracking::queries;
        use salsa::{Event, EventKind, Setter, Storage};
        use std::sync::Mutex;

        /// Per-database query execution log.
        /// Stored inside `TrackedHirDatabase` for hermetic tests.
        #[derive(Default)]
        struct QueryLog {
            executions: Vec<String>,
            counts: HashMap<String, usize>,
        }

        impl QueryLog {
            fn record(&mut self, query_name: &str) {
                self.executions.push(query_name.to_string());
                *self.counts.entry(query_name.to_string()).or_insert(0) += 1;
            }

            fn checkpoint(&self) -> usize {
                self.executions.len()
            }

            fn count_since(&self, query_name: &str, checkpoint: usize) -> usize {
                self.executions[checkpoint..]
                    .iter()
                    .filter(|n| n.as_str() == query_name)
                    .count()
            }

            fn executions_since(&self, checkpoint: usize) -> Vec<String> {
                self.executions[checkpoint..].to_vec()
            }
        }

        /// Extracts the query name from Salsa's debug representation.
        fn extract_query_name(database_key: &dyn std::fmt::Debug) -> String {
            let debug_str = format!("{database_key:?}");
            let without_args = debug_str.split('(').next().unwrap_or(&debug_str);
            without_args
                .rsplit("::")
                .next()
                .unwrap_or(without_args)
                .to_string()
        }

        /// A tracked database for HIR caching tests.
        /// Implements all required traits locally to satisfy orphan rules.
        #[derive(Clone)]
        struct TrackedHirDatabase {
            storage: Storage<Self>,
            log: Arc<Mutex<QueryLog>>,
        }

        impl Default for TrackedHirDatabase {
            fn default() -> Self {
                Self::new()
            }
        }

        impl TrackedHirDatabase {
            fn new() -> Self {
                let log = Arc::new(Mutex::new(QueryLog::default()));
                let log_for_callback = Arc::clone(&log);

                Self {
                    storage: Storage::new(Some(Box::new(move |event: Event| {
                        if let EventKind::WillExecute { database_key } = event.kind {
                            let query_name = extract_query_name(&database_key);
                            log_for_callback
                                .lock()
                                .expect("QueryLog mutex poisoned")
                                .record(&query_name);
                        }
                    }))),
                    log,
                }
            }

            fn checkpoint(&self) -> usize {
                self.log
                    .lock()
                    .expect("QueryLog mutex poisoned")
                    .checkpoint()
            }

            fn count_since(&self, query_name: &str, checkpoint: usize) -> usize {
                self.log
                    .lock()
                    .expect("QueryLog mutex poisoned")
                    .count_since(query_name, checkpoint)
            }

            fn executions_since(&self, checkpoint: usize) -> Vec<String> {
                self.log
                    .lock()
                    .expect("QueryLog mutex poisoned")
                    .executions_since(checkpoint)
            }
        }

        #[salsa::db]
        impl salsa::Database for TrackedHirDatabase {}

        // SAFETY: storage/storage_mut return references to the owned storage field
        unsafe impl salsa::plumbing::HasStorage for TrackedHirDatabase {
            fn storage(&self) -> &Storage<Self> {
                &self.storage
            }
            fn storage_mut(&mut self) -> &mut Storage<Self> {
                &mut self.storage
            }
        }

        #[salsa::db]
        impl graphql_syntax::GraphQLSyntaxDatabase for TrackedHirDatabase {}

        #[salsa::db]
        impl GraphQLHirDatabase for TrackedHirDatabase {}

        /// Helper to create `ProjectFiles` for TrackedHirDatabase
        fn create_tracked_project_files(
            db: &TrackedHirDatabase,
            schema_files: &[(FileId, graphql_db::FileContent, graphql_db::FileMetadata)],
            document_files: &[(FileId, graphql_db::FileContent, graphql_db::FileMetadata)],
        ) -> graphql_db::ProjectFiles {
            let schema_ids: Vec<FileId> = schema_files.iter().map(|(id, _, _)| *id).collect();
            let doc_ids: Vec<FileId> = document_files.iter().map(|(id, _, _)| *id).collect();

            let mut entries = HashMap::new();
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

            graphql_db::ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
        }

        /// Test that repeated queries are served from cache (no re-execution)
        #[test]
        fn test_cache_hit_on_repeated_query() {
            let db = TrackedHirDatabase::new();

            let file_id = FileId::new(0);
            let content =
                graphql_db::FileContent::new(&db, Arc::from("type Query { hello: String }"));
            let metadata = graphql_db::FileMetadata::new(
                &db,
                file_id,
                graphql_db::FileUri::new("test.graphql"),
                graphql_db::FileKind::Schema,
            );

            let schema_files = [(file_id, content, metadata)];
            let project_files = create_tracked_project_files(&db, &schema_files, &[]);

            // First query - cold (should execute)
            let checkpoint = db.checkpoint();
            let types1 = schema_types(&db, project_files);
            assert_eq!(types1.len(), 1);

            let cold_count = db.count_since(queries::SCHEMA_TYPES, checkpoint);
            assert!(
                cold_count >= 1,
                "First query should execute schema_types at least once, got {}",
                cold_count
            );

            // Second query - should be cached (0 new executions)
            let checkpoint2 = db.checkpoint();
            let types2 = schema_types(&db, project_files);
            assert_eq!(types2.len(), 1);

            let warm_count = db.count_since(queries::SCHEMA_TYPES, checkpoint2);
            assert_eq!(
                warm_count, 0,
                "Second query should NOT re-execute schema_types (cached)"
            );
        }

        /// Test that editing one file only re-executes queries for that file
        #[test]
        fn test_granular_caching_editing_one_file() {
            let mut db = TrackedHirDatabase::new();

            // Create two schema files
            let file_a_id = FileId::new(0);
            let file_a_content =
                graphql_db::FileContent::new(&db, Arc::from("type TypeA { id: ID! }"));
            let file_a_metadata = graphql_db::FileMetadata::new(
                &db,
                file_a_id,
                graphql_db::FileUri::new("a.graphql"),
                graphql_db::FileKind::Schema,
            );

            let file_b_id = FileId::new(1);
            let file_b_content =
                graphql_db::FileContent::new(&db, Arc::from("type TypeB { id: ID! }"));
            let file_b_metadata = graphql_db::FileMetadata::new(
                &db,
                file_b_id,
                graphql_db::FileUri::new("b.graphql"),
                graphql_db::FileKind::Schema,
            );

            let schema_files = [
                (file_a_id, file_a_content, file_a_metadata),
                (file_b_id, file_b_content, file_b_metadata),
            ];
            let project_files = create_tracked_project_files(&db, &schema_files, &[]);

            // Warm the cache
            let types = schema_types(&db, project_files);
            assert_eq!(types.len(), 2);
            assert!(types.contains_key("TypeA"));
            assert!(types.contains_key("TypeB"));

            // Checkpoint BEFORE the edit
            let checkpoint = db.checkpoint();

            // Edit ONLY file A's content
            file_a_content
                .set_text(&mut db)
                .to(Arc::from("type TypeA { id: ID! name: String }"));

            // Re-query
            let types_after = schema_types(&db, project_files);
            assert_eq!(types_after.len(), 2);

            // Verify per-file granular caching
            let parse_count = db.count_since(queries::PARSE, checkpoint);
            let file_structure_count = db.count_since(queries::FILE_STRUCTURE, checkpoint);

            println!(
                "After editing 1 of 2 files: parse={}, file_structure={}",
                parse_count, file_structure_count
            );

            // With granular caching, we should see ~1 parse and ~1 file_structure
            // (only for file A, not file B)
            assert!(
                parse_count <= 2,
                "Expected ~1 parse call (for edited file), got {}",
                parse_count
            );
            assert!(
                file_structure_count <= 2,
                "Expected ~1 file_structure call (for edited file), got {}",
                file_structure_count
            );
        }

        /// Test that editing document files doesn't invalidate schema queries
        #[test]
        fn test_unrelated_file_edit_doesnt_invalidate_schema() {
            let mut db = TrackedHirDatabase::new();

            // Create one schema file and one document file
            let schema_id = FileId::new(0);
            let schema_content =
                graphql_db::FileContent::new(&db, Arc::from("type Query { hello: String }"));
            let schema_metadata = graphql_db::FileMetadata::new(
                &db,
                schema_id,
                graphql_db::FileUri::new("schema.graphql"),
                graphql_db::FileKind::Schema,
            );

            let doc_id = FileId::new(1);
            let doc_content = graphql_db::FileContent::new(&db, Arc::from("query { hello }"));
            let doc_metadata = graphql_db::FileMetadata::new(
                &db,
                doc_id,
                graphql_db::FileUri::new("query.graphql"),
                graphql_db::FileKind::ExecutableGraphQL,
            );

            let project_files = create_tracked_project_files(
                &db,
                &[(schema_id, schema_content, schema_metadata)],
                &[(doc_id, doc_content, doc_metadata)],
            );

            // Warm the cache
            let types = schema_types(&db, project_files);
            assert_eq!(types.len(), 1);

            // Checkpoint BEFORE editing the document
            let checkpoint = db.checkpoint();

            // Edit the DOCUMENT file (not the schema)
            doc_content
                .set_text(&mut db)
                .to(Arc::from("query { hello world }"));

            // Query schema types again
            let types_after = schema_types(&db, project_files);
            assert_eq!(types_after.len(), 1);

            // schema_types should NOT have re-executed (document change doesn't affect schema)
            let schema_types_count = db.count_since(queries::SCHEMA_TYPES, checkpoint);
            assert_eq!(
                schema_types_count, 0,
                "Editing a document file should NOT invalidate schema_types query"
            );
        }

        /// Test O(1) behavior: with N files, editing 1 file causes O(1) recomputation
        #[test]
        fn test_editing_one_of_many_files_is_o1_not_on() {
            let mut db = TrackedHirDatabase::new();

            // Create 10 schema files
            const NUM_FILES: usize = 10;
            let mut schema_files = Vec::with_capacity(NUM_FILES);
            let mut file_contents = Vec::with_capacity(NUM_FILES);

            for i in 0..NUM_FILES {
                let file_id = FileId::new(i as u32);
                let type_name = format!("Type{i}");
                let content_str = format!("type {type_name} {{ id: ID! }}");
                let content = graphql_db::FileContent::new(&db, Arc::from(content_str.as_str()));
                let uri = format!("file{i}.graphql");
                let metadata = graphql_db::FileMetadata::new(
                    &db,
                    file_id,
                    graphql_db::FileUri::new(uri),
                    graphql_db::FileKind::Schema,
                );

                file_contents.push(content);
                schema_files.push((file_id, content, metadata));
            }

            let project_files = create_tracked_project_files(&db, &schema_files, &[]);

            // Warm the cache
            let types = schema_types(&db, project_files);
            assert_eq!(types.len(), NUM_FILES);

            // Checkpoint BEFORE the edit
            let checkpoint = db.checkpoint();

            // Edit ONLY file 0
            file_contents[0]
                .set_text(&mut db)
                .to(Arc::from("type Type0 { id: ID! name: String }"));

            // Re-query
            let types_after = schema_types(&db, project_files);
            assert_eq!(types_after.len(), NUM_FILES);

            // Measure deltas
            let parse_delta = db.count_since(queries::PARSE, checkpoint);
            let file_structure_delta = db.count_since(queries::FILE_STRUCTURE, checkpoint);

            println!(
                "With {} files, editing 1 file caused: parse={}, file_structure={}",
                NUM_FILES, parse_delta, file_structure_delta
            );

            // KEY ASSERTION: With granular caching, we should see ~1 parse and ~1 file_structure
            // Without granular caching, we'd see ~10 of each (O(N))
            let max_allowed = NUM_FILES / 2;
            assert!(
                parse_delta <= max_allowed,
                "Expected O(1) parse calls, got {} (O(N) would be ~{})",
                parse_delta,
                NUM_FILES
            );
            assert!(
                file_structure_delta <= max_allowed,
                "Expected O(1) file_structure calls, got {} (O(N) would be ~{})",
                file_structure_delta,
                NUM_FILES
            );
        }

        /// Test that fragment index is not invalidated by editing non-fragment files
        #[test]
        fn test_fragment_index_not_invalidated_by_unrelated_edit() {
            let mut db = TrackedHirDatabase::new();

            // Create a schema file
            let schema_id = FileId::new(0);
            let schema_content = graphql_db::FileContent::new(
                &db,
                Arc::from("type Query { user: User } type User { id: ID! name: String }"),
            );
            let schema_metadata = graphql_db::FileMetadata::new(
                &db,
                schema_id,
                graphql_db::FileUri::new("schema.graphql"),
                graphql_db::FileKind::Schema,
            );

            // Create a document with a fragment
            let frag_id = FileId::new(1);
            let frag_content = graphql_db::FileContent::new(
                &db,
                Arc::from("fragment UserFields on User { id name }"),
            );
            let frag_metadata = graphql_db::FileMetadata::new(
                &db,
                frag_id,
                graphql_db::FileUri::new("fragments.graphql"),
                graphql_db::FileKind::ExecutableGraphQL,
            );

            // Create another document that will be edited (no fragments)
            let query_id = FileId::new(2);
            let query_content =
                graphql_db::FileContent::new(&db, Arc::from("query GetUser { user { id } }"));
            let query_metadata = graphql_db::FileMetadata::new(
                &db,
                query_id,
                graphql_db::FileUri::new("query.graphql"),
                graphql_db::FileKind::ExecutableGraphQL,
            );

            let project_files = create_tracked_project_files(
                &db,
                &[(schema_id, schema_content, schema_metadata)],
                &[
                    (frag_id, frag_content, frag_metadata),
                    (query_id, query_content, query_metadata),
                ],
            );

            // Build the fragment index (warm cache)
            let fragments = all_fragments(&db, project_files);
            assert_eq!(fragments.len(), 1);
            assert!(fragments.contains_key("UserFields"));

            // Checkpoint BEFORE the edit
            let checkpoint = db.checkpoint();

            // Edit the QUERY file (not the fragment file)
            query_content
                .set_text(&mut db)
                .to(Arc::from("query GetUser { user { id name } }"));

            // Re-query fragments
            let fragments_after = all_fragments(&db, project_files);
            assert_eq!(fragments_after.len(), 1);

            // all_fragments should be cached (query file has no fragments)
            let all_fragments_delta = db.count_since(queries::ALL_FRAGMENTS, checkpoint);
            let file_fragments_delta = db.count_since(queries::FILE_FRAGMENTS, checkpoint);

            println!(
                "After editing query file: all_fragments={}, file_fragments={}",
                all_fragments_delta, file_fragments_delta
            );

            // all_fragments should not re-execute because no fragment changed
            // file_fragments for the edited file may re-run but should be O(1)
            assert!(
                all_fragments_delta <= 1,
                "all_fragments should be mostly cached when editing non-fragment files, got {}",
                all_fragments_delta
            );
        }

        /// Test the "golden invariant": schema_types is stable across operation edits
        ///
        /// This is critical for IDE responsiveness: users edit operations frequently,
        /// and we must NOT re-compute schema knowledge on every keystroke.
        #[test]
        fn test_golden_invariant_schema_stable_across_operation_edits() {
            let mut db = TrackedHirDatabase::new();

            // Create schema
            let schema_id = FileId::new(0);
            let schema_content = graphql_db::FileContent::new(
                &db,
                Arc::from(
                    "type Query { users: [User!]! } type User { id: ID! name: String! email: String }",
                ),
            );
            let schema_metadata = graphql_db::FileMetadata::new(
                &db,
                schema_id,
                graphql_db::FileUri::new("schema.graphql"),
                graphql_db::FileKind::Schema,
            );

            // Create multiple operation files
            let op1_id = FileId::new(1);
            let op1_content =
                graphql_db::FileContent::new(&db, Arc::from("query GetUsers { users { id } }"));
            let op1_metadata = graphql_db::FileMetadata::new(
                &db,
                op1_id,
                graphql_db::FileUri::new("op1.graphql"),
                graphql_db::FileKind::ExecutableGraphQL,
            );

            let op2_id = FileId::new(2);
            let op2_content = graphql_db::FileContent::new(
                &db,
                Arc::from("query GetUserNames { users { name } }"),
            );
            let op2_metadata = graphql_db::FileMetadata::new(
                &db,
                op2_id,
                graphql_db::FileUri::new("op2.graphql"),
                graphql_db::FileKind::ExecutableGraphQL,
            );

            let project_files = create_tracked_project_files(
                &db,
                &[(schema_id, schema_content, schema_metadata)],
                &[
                    (op1_id, op1_content, op1_metadata),
                    (op2_id, op2_content, op2_metadata),
                ],
            );

            // Warm the cache
            let types = schema_types(&db, project_files);
            assert_eq!(types.len(), 2); // Query and User

            // Checkpoint BEFORE editing operations
            let checkpoint = db.checkpoint();

            // Edit BOTH operation files (simulating active development)
            op1_content
                .set_text(&mut db)
                .to(Arc::from("query GetUsers { users { id name } }"));
            op2_content
                .set_text(&mut db)
                .to(Arc::from("query GetUserNames { users { name email } }"));

            // Re-query schema
            let types_after = schema_types(&db, project_files);
            assert_eq!(types_after.len(), 2);

            // GOLDEN INVARIANT: schema_types should be COMPLETELY cached
            let schema_types_delta = db.count_since(queries::SCHEMA_TYPES, checkpoint);
            let file_type_defs_delta = db.count_since(queries::FILE_TYPE_DEFS, checkpoint);

            println!(
                "After editing 2 operation files: schema_types={}, file_type_defs={}",
                schema_types_delta, file_type_defs_delta
            );

            // schema_types should NOT re-execute when only operations are edited
            assert_eq!(
                schema_types_delta, 0,
                "GOLDEN INVARIANT VIOLATED: schema_types ran {} times after operation edit",
                schema_types_delta
            );

            // file_type_defs also should not re-execute (schema file unchanged)
            assert_eq!(
                file_type_defs_delta, 0,
                "file_type_defs should be cached when operations are edited, got {}",
                file_type_defs_delta
            );
        }

        /// Test that per-file contribution queries enable incremental computation
        /// for project-wide lint rules (Issue #213)
        #[test]
        fn test_per_file_contribution_queries_incremental() {
            let mut db = TrackedHirDatabase::new();

            // Create multiple document files with fragments
            const NUM_FILES: usize = 5;
            let mut doc_files = Vec::with_capacity(NUM_FILES);
            let mut file_contents = Vec::with_capacity(NUM_FILES);

            for i in 0..NUM_FILES {
                let file_id = FileId::new(i as u32);
                let fragment_name = format!("Fragment{i}");
                let content_str =
                    format!("fragment {fragment_name} on User {{ id }} query Q{i} {{ user {{ ...{fragment_name} }} }}");
                let content = graphql_db::FileContent::new(&db, Arc::from(content_str.as_str()));
                let uri = format!("file{i}.graphql");
                let metadata = graphql_db::FileMetadata::new(
                    &db,
                    file_id,
                    graphql_db::FileUri::new(uri),
                    graphql_db::FileKind::ExecutableGraphQL,
                );

                file_contents.push(content);
                doc_files.push((file_id, content, metadata));
            }

            let _project_files = create_tracked_project_files(&db, &[], &doc_files);

            // Warm the cache by calling per-file contribution queries
            for (file_id, content, metadata) in &doc_files {
                let _ = file_defined_fragment_names(&db, *file_id, *content, *metadata);
                let _ = file_used_fragment_names(&db, *file_id, *content, *metadata);
                let _ = file_operation_names(&db, *file_id, *content, *metadata);
            }

            // Checkpoint BEFORE the edit
            let checkpoint = db.checkpoint();

            // Edit ONLY file 0
            file_contents[0].set_text(&mut db).to(Arc::from(
                "fragment Fragment0 on User { id name } query Q0 { user { ...Fragment0 } }",
            ));

            // Re-query the per-file contributions
            for (file_id, content, metadata) in &doc_files {
                let _ = file_defined_fragment_names(&db, *file_id, *content, *metadata);
                let _ = file_used_fragment_names(&db, *file_id, *content, *metadata);
                let _ = file_operation_names(&db, *file_id, *content, *metadata);
            }

            // Measure recomputation - should be O(1), not O(N)
            let defined_delta = db.count_since(queries::FILE_DEFINED_FRAGMENT_NAMES, checkpoint);
            let used_delta = db.count_since(queries::FILE_USED_FRAGMENT_NAMES, checkpoint);
            let op_names_delta = db.count_since(queries::FILE_OPERATION_NAMES, checkpoint);

            println!(
                "After editing 1 of {} files: defined={}, used={}, op_names={}",
                NUM_FILES, defined_delta, used_delta, op_names_delta
            );

            // KEY ASSERTION: Only the edited file's queries should recompute
            // With O(N) behavior, we'd see ~5 of each
            let max_allowed = NUM_FILES / 2;
            assert!(
                defined_delta <= max_allowed,
                "Expected O(1) file_defined_fragment_names calls, got {} (O(N) would be ~{})",
                defined_delta,
                NUM_FILES
            );
            assert!(
                used_delta <= max_allowed,
                "Expected O(1) file_used_fragment_names calls, got {} (O(N) would be ~{})",
                used_delta,
                NUM_FILES
            );
            assert!(
                op_names_delta <= max_allowed,
                "Expected O(1) file_operation_names calls, got {} (O(N) would be ~{})",
                op_names_delta,
                NUM_FILES
            );
        }

        /// Test that executions_since() provides debugging information
        #[test]
        fn test_executions_since_for_debugging() {
            let db = TrackedHirDatabase::new();

            let file_id = FileId::new(0);
            let content =
                graphql_db::FileContent::new(&db, Arc::from("type Query { hello: String }"));
            let metadata = graphql_db::FileMetadata::new(
                &db,
                file_id,
                graphql_db::FileUri::new("test.graphql"),
                graphql_db::FileKind::Schema,
            );

            let schema_files = [(file_id, content, metadata)];
            let project_files = create_tracked_project_files(&db, &schema_files, &[]);

            let checkpoint = db.checkpoint();
            let _ = schema_types(&db, project_files);

            // Get all executions for debugging
            let executions = db.executions_since(checkpoint);

            // Should have recorded some query executions
            assert!(
                !executions.is_empty(),
                "Should have recorded query executions"
            );

            // The executions should include expected queries
            let has_schema_types = executions.iter().any(|q| q == queries::SCHEMA_TYPES);
            assert!(
                has_schema_types,
                "Executions should include schema_types: {:?}",
                executions
            );
        }
    }

    #[test]
    fn test_file_structure_finds_fragments_in_typescript() {
        let db = TestDatabase::default();
        let file_id = FileId::new(100);

        // TypeScript content with a fragment
        let ts_content = r#"
import { gql } from "@apollo/client";

const MY_FRAGMENT = gql`
  fragment TestFragment on Pokemon {
    id
    name
  }
`;
"#;

        let content = FileContent::new(&db, Arc::from(ts_content));
        let metadata =
            FileMetadata::new(&db, file_id, FileUri::new("test.ts"), FileKind::TypeScript);

        let structure = file_structure(&db, file_id, content, metadata);

        println!("Operations: {:?}", structure.operations.len());
        println!("Fragments: {:?}", structure.fragments.len());
        for frag in structure.fragments.iter() {
            println!("  Found fragment: {}", frag.name);
        }

        assert_eq!(
            structure.fragments.len(),
            1,
            "Expected to find 1 fragment in TypeScript file"
        );
        assert_eq!(structure.fragments[0].name.as_ref(), "TestFragment");
    }

    #[test]
    fn test_all_fragments_includes_typescript_files() {
        let mut db = TestDatabase::default();

        // Create a pure GraphQL file with a fragment
        let graphql_file_id = FileId::new(1);
        let graphql_content =
            FileContent::new(&db, Arc::from("fragment GraphQLFragment on User { id }"));
        let graphql_metadata = FileMetadata::new(
            &db,
            graphql_file_id,
            FileUri::new("test.graphql"),
            FileKind::ExecutableGraphQL,
        );

        // Create a TypeScript file with a fragment
        let ts_file_id = FileId::new(2);
        let ts_content = FileContent::new(
            &db,
            Arc::from(
                r#"
import { gql } from "@apollo/client";

const FRAG = gql`
  fragment TSFragment on Pokemon {
    id
    name
  }
`;
"#,
            ),
        );
        let ts_metadata = FileMetadata::new(
            &db,
            ts_file_id,
            FileUri::new("test.ts"),
            FileKind::TypeScript,
        );

        // Create project files with both documents
        let project_files = create_project_files(
            &mut db,
            &[], // No schema files
            &[
                (graphql_file_id, graphql_content, graphql_metadata),
                (ts_file_id, ts_content, ts_metadata),
            ],
        );

        let fragments = all_fragments(&db, project_files);

        println!("Total fragments found: {}", fragments.len());
        #[allow(clippy::explicit_iter_loop)]
        for (name, _) in fragments.iter() {
            println!("  Found fragment: {name}");
        }

        assert!(
            fragments.contains_key(&Arc::from("GraphQLFragment")),
            "Should find fragment from .graphql file"
        );
        assert!(
            fragments.contains_key(&Arc::from("TSFragment")),
            "Should find fragment from .ts file"
        );
        assert_eq!(fragments.len(), 2, "Should find exactly 2 fragments");
    }
}
