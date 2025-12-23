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
/// Note: In Phase 2, these return empty sets. `FileRegistry` will be added in a future phase.
#[salsa::db]
pub trait GraphQLHirDatabase: graphql_syntax::GraphQLSyntaxDatabase {
    /// Get all schema files in the project
    /// Returns tuples of (`FileId`, `FileContent`, `FileMetadata`)
    /// TODO: Will be properly implemented with `FileRegistry` in Phase 3
    fn schema_files(
        &self,
    ) -> Arc<Vec<(FileId, graphql_db::FileContent, graphql_db::FileMetadata)>> {
        Arc::new(Vec::new())
    }

    /// Get all document files in the project
    /// Returns tuples of (`FileId`, `FileContent`, `FileMetadata`)
    /// TODO: Will be properly implemented with `FileRegistry` in Phase 3
    fn document_files(
        &self,
    ) -> Arc<Vec<(FileId, graphql_db::FileContent, graphql_db::FileMetadata)>> {
        Arc::new(Vec::new())
    }
}

// Implement the trait for RootDatabase
// This makes RootDatabase usable with all HIR queries
#[salsa::db]
impl GraphQLHirDatabase for graphql_db::RootDatabase {}

/// Get all types in the schema
/// This query depends on all schema file structures
#[salsa::tracked]
pub fn schema_types(db: &dyn GraphQLHirDatabase) -> Arc<HashMap<Arc<str>, TypeDef>> {
    let schema_files = db.schema_files();
    let mut types = HashMap::new();

    for (file_id, content, metadata) in schema_files.iter() {
        let structure = file_structure(db, *file_id, *content, *metadata);
        for type_def in &structure.type_defs {
            types.insert(type_def.name.clone(), type_def.clone());
        }
    }

    Arc::new(types)
}

/// Get all fragments in the project
/// This query depends on all document file structures
#[salsa::tracked]
pub fn all_fragments(db: &dyn GraphQLHirDatabase) -> Arc<HashMap<Arc<str>, FragmentStructure>> {
    let document_files = db.document_files();
    let mut fragments = HashMap::new();

    for (file_id, content, metadata) in document_files.iter() {
        let structure = file_structure(db, *file_id, *content, *metadata);
        for fragment in &structure.fragments {
            fragments.insert(fragment.name.clone(), fragment.clone());
        }
    }

    Arc::new(fragments)
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
        let types = schema_types(&db);
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
