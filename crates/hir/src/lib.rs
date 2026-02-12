// GraphQL HIR (High-level Intermediate Representation)
// This crate provides semantic queries on top of syntax.
// It implements the "golden invariant": editing a document's body never invalidates global schema knowledge.

use graphql_base_db::FileId;
use std::collections::HashMap;
use std::sync::Arc;

mod body;
mod structure;

pub use body::*;
pub use structure::*;

// Type aliases for commonly used HashMap types.
// These improve readability in function signatures and provide
// a single point of change if the underlying type needs modification.

/// Map from type name to type definition.
pub type TypeDefMap = HashMap<Arc<str>, TypeDef>;

/// Map from fragment name to fragment structure.
pub type FragmentMap = HashMap<Arc<str>, FragmentStructure>;

/// Map from name to count (used for uniqueness validation).
pub type NameCountMap = HashMap<Arc<str>, usize>;

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
    fn project_files(&self) -> Option<graphql_base_db::ProjectFiles> {
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
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
    project_files: graphql_base_db::ProjectFiles,
) -> TypeDefMap {
    let schema_ids = project_files.schema_file_ids(db).ids(db);
    let mut types = HashMap::new();

    for file_id in schema_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
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
    project_files: graphql_base_db::ProjectFiles,
) -> FragmentMap {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut fragments = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
            // Per-file query - cached independently
            let file_frags = file_fragments(db, *file_id, content, metadata);
            for fragment in file_frags.iter() {
                fragments.insert(fragment.name.clone(), fragment.clone());
            }
        }
    }

    fragments
}

/// Index mapping fragment names to the number of fragments with that name.
///
/// This query provides O(1) lookup for fragment name uniqueness validation,
/// replacing the O(n*m) pattern of calling `all_fragments()` for every fragment.
///
/// Uses granular per-file caching:
/// - Depends on `DocumentFileIds` (only changes when files are added/removed)
/// - Calls `file_defined_fragment_names` per-file (each cached independently)
///
/// When a single file changes, only that file's contribution is recomputed.
#[salsa::tracked]
pub fn project_fragment_name_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, usize>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut name_counts: HashMap<Arc<str>, usize> = HashMap::new();

    for file_id in doc_ids.iter() {
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
            let frag_names = file_defined_fragment_names(db, *file_id, content, metadata);
            for name in frag_names.iter() {
                *name_counts.entry(name.clone()).or_insert(0) += 1;
            }
        }
    }

    Arc::new(name_counts)
}

/// Index mapping fragment names to their file content and metadata
/// Uses granular per-file caching for efficient invalidation.
#[salsa::tracked]
pub fn fragment_file_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, (graphql_base_db::FileContent, graphql_base_db::FileMetadata)>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
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
    _file_id: graphql_base_db::FileId,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
    _file_id: graphql_base_db::FileId,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, graphql_base_db::FileId>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
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
    project_files: graphql_base_db::ProjectFiles,
    fragment_name: Arc<str>,
) -> Option<Arc<str>> {
    // First, find which file contains this fragment
    let location_index = fragment_file_location_index(db, project_files);
    let file_id = location_index.get(&fragment_name)?;

    // Get the file's content and metadata
    let (content, metadata) = graphql_base_db::file_lookup(db, project_files, *file_id)?;

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
    project_files: graphql_base_db::ProjectFiles,
    fragment_name: Arc<str>,
) -> Option<Arc<apollo_compiler::ast::Document>> {
    // First, find which file contains this fragment
    let location_index = fragment_file_location_index(db, project_files);
    let file_id = location_index.get(&fragment_name)?;

    // Get the file's content and metadata
    let (content, metadata) = graphql_base_db::file_lookup(db, project_files, *file_id)?;

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
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, Arc<str>>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
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
    file_id: graphql_base_db::FileId,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, std::collections::HashSet<Arc<str>>>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut index = HashMap::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
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
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<Vec<OperationStructure>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut operations = Vec::new();

    for file_id in doc_ids.iter() {
        // Use per-file lookup for granular caching
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
            // Per-file query for operations
            let file_ops = file_operations(db, *file_id, content, metadata);
            operations.extend(file_ops.iter().cloned());
        }
    }

    Arc::new(operations)
}

/// Index mapping operation names to the number of operations with that name.
///
/// This query provides O(1) lookup for operation name uniqueness validation,
/// replacing the O(n*m) pattern of calling `all_operations()` for every operation.
///
/// Uses granular per-file caching:
/// - Depends on `DocumentFileIds` (only changes when files are added/removed)
/// - Calls `file_operation_names` per-file (each cached independently)
///
/// When a single file changes, only that file's contribution is recomputed.
#[salsa::tracked]
pub fn project_operation_name_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, usize>> {
    let doc_ids = project_files.document_file_ids(db).ids(db);
    let mut name_counts: HashMap<Arc<str>, usize> = HashMap::new();

    for file_id in doc_ids.iter() {
        if let Some((content, metadata)) = graphql_base_db::file_lookup(db, project_files, *file_id)
        {
            let op_names = file_operation_names(db, *file_id, content, metadata);
            for op_info in op_names.iter() {
                *name_counts.entry(op_info.name.clone()).or_insert(0) += 1;
            }
        }
    }

    Arc::new(name_counts)
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
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
    pub block_line_offset: Option<u32>,
    /// For embedded GraphQL: byte offset of the block in the original file
    pub block_byte_offset: Option<usize>,
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
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
                block_byte_offset: frag.block_byte_offset,
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
    pub block_line_offset: Option<u32>,
    /// For embedded GraphQL: byte offset of the block in the original file
    pub block_byte_offset: Option<usize>,
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
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
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
                    block_byte_offset: op.block_byte_offset,
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
///
/// Fragment spreads are resolved transitively: if a file uses `...FragmentA`,
/// and `FragmentA` is defined elsewhere and uses fields, those fields are
/// included in this file's coordinates.
#[salsa::tracked]
#[allow(clippy::items_after_statements)]
pub fn file_schema_coordinates(
    db: &dyn GraphQLHirDatabase,
    _file_id: FileId,
    content: graphql_base_db::FileContent,
    metadata: graphql_base_db::FileMetadata,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<std::collections::HashSet<SchemaCoordinate>> {
    let parse = graphql_syntax::parse(db, content, metadata);
    let schema_types = schema_types(db, project_files);
    let fragments = all_fragments(db, project_files);

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

    // Context for collecting coordinates - allows passing db and project_files to helper
    struct CollectContext<'a> {
        db: &'a dyn GraphQLHirDatabase,
        project_files: graphql_base_db::ProjectFiles,
        schema_types: &'a TypeDefMap,
        fragments: &'a FragmentMap,
        visited_fragments: std::collections::HashSet<Arc<str>>,
        coordinates: std::collections::HashSet<SchemaCoordinate>,
    }

    impl CollectContext<'_> {
        fn collect_from_selections(
            &mut self,
            selections: &[apollo_compiler::ast::Selection],
            parent_type: &Arc<str>,
        ) {
            for selection in selections {
                match selection {
                    apollo_compiler::ast::Selection::Field(field) => {
                        let field_name: Arc<str> = Arc::from(field.name.as_str());

                        // Record this schema coordinate
                        self.coordinates.insert(SchemaCoordinate {
                            type_name: parent_type.clone(),
                            field_name: field_name.clone(),
                        });

                        // Recursively process nested selections
                        if !field.selection_set.is_empty() {
                            // Find the field's return type from schema
                            if let Some(type_def) = self.schema_types.get(parent_type) {
                                if let Some(field_sig) = type_def
                                    .fields
                                    .iter()
                                    .find(|f| f.name.as_ref() == field_name.as_ref())
                                {
                                    let nested_type: Arc<str> =
                                        Arc::from(field_sig.type_ref.name.as_ref());
                                    self.collect_from_selections(
                                        &field.selection_set,
                                        &nested_type,
                                    );
                                }
                            }
                        }
                    }
                    apollo_compiler::ast::Selection::FragmentSpread(spread) => {
                        let frag_name: Arc<str> = Arc::from(spread.fragment_name.as_str());

                        // Prevent infinite recursion from circular fragment references
                        if self.visited_fragments.contains(&frag_name) {
                            continue;
                        }

                        // Look up the fragment structure for type condition
                        if let Some(frag_structure) = self.fragments.get(&frag_name) {
                            self.visited_fragments.insert(frag_name.clone());

                            // Use fragment_ast to get the parsed AST (handles both embedded and pure GraphQL)
                            if let Some(ast) =
                                fragment_ast(self.db, self.project_files, frag_name.clone())
                            {
                                // Find the fragment definition in the AST
                                for def in &ast.definitions {
                                    if let apollo_compiler::ast::Definition::FragmentDefinition(
                                        frag,
                                    ) = def
                                    {
                                        if frag.name.as_str() == frag_name.as_ref() {
                                            self.collect_from_selections(
                                                &frag.selection_set,
                                                &frag_structure.type_condition,
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    apollo_compiler::ast::Selection::InlineFragment(inline) => {
                        let inline_type = inline
                            .type_condition
                            .as_ref()
                            .map_or_else(|| parent_type.clone(), |tc| Arc::from(tc.as_str()));
                        self.collect_from_selections(&inline.selection_set, &inline_type);
                    }
                }
            }
        }
    }

    let mut ctx = CollectContext {
        db,
        project_files,
        schema_types,
        fragments,
        visited_fragments: std::collections::HashSet::new(),
        coordinates: std::collections::HashSet::new(),
    };

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
                        ctx.collect_from_selections(&op.selection_set, root);
                    }
                }
                apollo_compiler::ast::Definition::FragmentDefinition(frag) => {
                    let frag_type = Arc::from(frag.type_condition.as_str());
                    ctx.collect_from_selections(&frag.selection_set, &frag_type);
                }
                _ => {}
            }
        }
    }

    Arc::new(ctx.coordinates)
}
