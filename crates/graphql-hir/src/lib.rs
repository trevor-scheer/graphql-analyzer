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

    /// Get the schema files input
    fn schema_files_input(&self) -> Option<graphql_db::SchemaFiles> {
        self.project_files().map(|pf| pf.schema_files(self))
    }

    /// Get the document files input
    fn document_files_input(&self) -> Option<graphql_db::DocumentFiles> {
        self.project_files().map(|pf| pf.document_files(self))
    }

    /// Get all schema files in the project
    /// Returns tuples of (`FileId`, `FileContent`, `FileMetadata`)
    fn schema_files(
        &self,
    ) -> Arc<Vec<(FileId, graphql_db::FileContent, graphql_db::FileMetadata)>> {
        self.schema_files_input()
            .map_or_else(|| Arc::new(Vec::new()), |sf| sf.files(self))
    }

    /// Get all document files in the project
    /// Returns tuples of (`FileId`, `FileContent`, `FileMetadata`)
    fn document_files(
        &self,
    ) -> Arc<Vec<(FileId, graphql_db::FileContent, graphql_db::FileMetadata)>> {
        self.document_files_input()
            .map_or_else(|| Arc::new(Vec::new()), |df| df.files(self))
    }
}

#[salsa::db]
impl GraphQLHirDatabase for graphql_db::RootDatabase {
    // Uses default implementation (returns None)
    // Queries should accept ProjectFiles as a parameter instead
}

/// Get all types in the schema with explicit schema files input
/// This query depends ONLY on schema files, not document files.
/// Changing document files will not invalidate this query.
#[salsa::tracked]
pub fn schema_types(
    db: &dyn GraphQLHirDatabase,
    schema_files: graphql_db::SchemaFiles,
) -> Arc<HashMap<Arc<str>, TypeDef>> {
    let files = schema_files.files(db);
    let mut types = HashMap::new();

    for (file_id, content, metadata) in files.iter() {
        let structure = file_structure(db, *file_id, *content, *metadata);
        for type_def in &structure.type_defs {
            types.insert(type_def.name.clone(), type_def.clone());
        }
    }

    Arc::new(types)
}

/// Get all types in the schema with explicit project files
/// This is a convenience wrapper that extracts `SchemaFiles` from `ProjectFiles`
#[salsa::tracked]
pub fn schema_types_with_project(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, TypeDef>> {
    schema_types(db, project_files.schema_files(db))
}

/// Get all fragments in the project with explicit document files input
/// This query depends ONLY on document files, not schema files.
/// Changing schema files will not invalidate this query.
#[salsa::tracked]
pub fn all_fragments(
    db: &dyn GraphQLHirDatabase,
    document_files: graphql_db::DocumentFiles,
) -> Arc<HashMap<Arc<str>, FragmentStructure>> {
    let files = document_files.files(db);
    let mut fragments = HashMap::new();

    for (file_id, content, metadata) in files.iter() {
        let structure = file_structure(db, *file_id, *content, *metadata);
        for fragment in &structure.fragments {
            fragments.insert(fragment.name.clone(), fragment.clone());
        }
    }

    Arc::new(fragments)
}

/// Get all fragments in the project with explicit project files
/// This is a convenience wrapper that extracts `DocumentFiles` from `ProjectFiles`
#[salsa::tracked]
pub fn all_fragments_with_project(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, FragmentStructure>> {
    all_fragments(db, project_files.document_files(db))
}

/// Index mapping fragment names to their file content and metadata (with `DocumentFiles` input)
/// This allows O(1) lookup of fragment definitions without re-parsing files.
/// Depends ONLY on document files, not schema files.
#[salsa::tracked]
pub fn fragment_file_index_with_docs(
    db: &dyn GraphQLHirDatabase,
    document_files: graphql_db::DocumentFiles,
) -> Arc<HashMap<Arc<str>, (graphql_db::FileContent, graphql_db::FileMetadata)>> {
    let files = document_files.files(db);
    let mut index = HashMap::new();

    for (_file_id, content, metadata) in files.iter() {
        let structure = file_structure(db, metadata.file_id(db), *content, *metadata);
        for fragment in &structure.fragments {
            index.insert(fragment.name.clone(), (*content, *metadata));
        }
    }

    Arc::new(index)
}

/// Index mapping fragment names to their file content and metadata
/// Convenience wrapper that extracts `DocumentFiles` from `ProjectFiles`
#[salsa::tracked]
pub fn fragment_file_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, (graphql_db::FileContent, graphql_db::FileMetadata)>> {
    fragment_file_index_with_docs(db, project_files.document_files(db))
}

/// Index mapping fragment names to the fragments they reference (with `DocumentFiles` input)
/// This allows efficient transitive fragment resolution without re-parsing.
/// Depends ONLY on document files, not schema files.
#[salsa::tracked]
pub fn fragment_spreads_index_with_docs(
    db: &dyn GraphQLHirDatabase,
    document_files: graphql_db::DocumentFiles,
) -> Arc<HashMap<Arc<str>, std::collections::HashSet<Arc<str>>>> {
    let files = document_files.files(db);
    let mut index = HashMap::new();

    for (_file_id, content, metadata) in files.iter() {
        let structure = file_structure(db, metadata.file_id(db), *content, *metadata);
        for fragment in &structure.fragments {
            // Get the fragment body to find its spreads
            let body = fragment_body(db, *content, *metadata, fragment.name.clone());
            index.insert(fragment.name.clone(), body.fragment_spreads.clone());
        }
    }

    Arc::new(index)
}

/// Index mapping fragment names to the fragments they reference (spread)
/// Convenience wrapper that extracts `DocumentFiles` from `ProjectFiles`
#[salsa::tracked]
pub fn fragment_spreads_index(
    db: &dyn GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
) -> Arc<HashMap<Arc<str>, std::collections::HashSet<Arc<str>>>> {
    fragment_spreads_index_with_docs(db, project_files.document_files(db))
}

/// Get all operations in the project
/// This query depends on all document file structures
#[salsa::tracked]
pub fn all_operations(db: &dyn GraphQLHirDatabase) -> Arc<Vec<OperationStructure>> {
    let document_files = db.document_files();
    let mut operations = Vec::new();

    for (file_id, content, metadata) in document_files.iter() {
        let structure = file_structure(db, *file_id, *content, *metadata);
        operations.extend(structure.operations.clone());
    }

    Arc::new(operations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileKind, FileMetadata, FileUri};

    // Test database that implements all required traits
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

    #[test]
    fn test_schema_types_empty() {
        let db = TestDatabase::default();
        let schema_files = graphql_db::SchemaFiles::new(&db, Arc::new(Vec::new()));
        let document_files = graphql_db::DocumentFiles::new(&db, Arc::new(Vec::new()));
        let project_files = graphql_db::ProjectFiles::new(&db, schema_files, document_files);
        let types = schema_types_with_project(&db, project_files);
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
}
