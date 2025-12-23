// GraphQL HIR (High-level Intermediate Representation)
// This crate provides semantic queries on top of syntax.
// It implements the "golden invariant": editing a document's body never invalidates global schema knowledge.

use apollo_parser::ast;
use graphql_db::FileId;
use std::collections::{HashMap, HashSet};
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
/// Note: In Phase 2, these return empty sets. FileRegistry will be added in a future phase.
#[salsa::db]
pub trait GraphQLHirDatabase: graphql_syntax::GraphQLSyntaxDatabase {
    /// Get all schema files in the project
    /// Returns tuples of (FileId, FileContent, FileMetadata)
    /// TODO: Will be properly implemented with FileRegistry in Phase 3
    fn schema_files(
        &self,
    ) -> Arc<Vec<(FileId, graphql_db::FileContent, graphql_db::FileMetadata)>> {
        Arc::new(Vec::new())
    }

    /// Get all document files in the project
    /// Returns tuples of (FileId, FileContent, FileMetadata)
    /// TODO: Will be properly implemented with FileRegistry in Phase 3
    fn document_files(
        &self,
    ) -> Arc<Vec<(FileId, graphql_db::FileContent, graphql_db::FileMetadata)>> {
        Arc::new(Vec::new())
    }
}

/// Summary of a file's structure (stable across body edits)
/// This is the key to the golden invariant - we only extract names and signatures,
/// not selection sets or field selections.
#[salsa::tracked]
pub struct FileStructure {
    pub file_id: FileId,
    #[return_ref]
    pub type_defs: Vec<TypeDef>,
    #[return_ref]
    pub operations: Vec<OperationStructure>,
    #[return_ref]
    pub fragments: Vec<FragmentStructure>,
}

/// Get all types in the schema
/// This query depends on all schema file structures
#[salsa::tracked]
pub fn schema_types(db: &dyn GraphQLHirDatabase) -> Arc<HashMap<Arc<str>, TypeDef>> {
    let schema_files = db.schema_files();
    let mut types = HashMap::new();

    for (file_id, content, metadata) in schema_files.iter() {
        let structure = file_structure(db, *file_id, *content, *metadata);
        for type_def in structure.type_defs(db) {
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
        for fragment in structure.fragments(db) {
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
        operations.extend(structure.operations(db).clone());
    }

    Arc::new(operations)
}

/// Get fragments referenced by an operation (lazily computed)
/// This performs transitive resolution: if operation references fragment A,
/// and fragment A references fragment B, both A and B are returned.
///
/// Note: This is a simplified implementation for Phase 2. Full implementation
/// requires FileRegistry to look up fragment files.
#[salsa::tracked]
pub fn operation_fragment_deps(
    db: &dyn GraphQLHirDatabase,
    operation_id: OperationId,
) -> Arc<HashSet<Arc<str>>> {
    // Get the operation body
    let body = operation_body(db, operation_id);

    // For Phase 2, just return the direct fragment spreads
    // Full transitive resolution will be implemented with FileRegistry
    Arc::new(body.fragment_spreads(db).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests will be implemented alongside the structure and body modules
}
