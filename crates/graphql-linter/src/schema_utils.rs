//! Utilities for extracting schema information
//!
//! This module provides helper functions for extracting schema metadata
//! like root operation type names from GraphQL schema definitions.

use graphql_db::ProjectFiles;
use std::collections::HashMap;
use std::sync::Arc;

/// Root operation type names extracted from schema definition
#[derive(Debug, Default, Clone)]
pub struct RootTypeNames {
    pub query: Option<String>,
    pub mutation: Option<String>,
    pub subscription: Option<String>,
}

impl RootTypeNames {
    /// Check if a type name is one of the root operation types
    pub fn is_root_type(&self, type_name: &str) -> bool {
        self.query.as_deref() == Some(type_name)
            || self.mutation.as_deref() == Some(type_name)
            || self.subscription.as_deref() == Some(type_name)
    }
}

/// Extract root operation type names from schema files
///
/// This function parses all schema files in the project to find schema definitions
/// (e.g., `schema { query: RootQuery }`). If no explicit schema definition exists,
/// it falls back to the default names (Query, Mutation, Subscription) if those
/// types exist in the schema.
///
/// # Arguments
///
/// * `db` - The HIR database for parsing
/// * `project_files` - The project files to search
/// * `schema_types` - Map of type names to type definitions (to check if default types exist)
///
/// # Returns
///
/// A `RootTypeNames` struct with the resolved root type names
pub fn extract_root_type_names(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    project_files: ProjectFiles,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
) -> RootTypeNames {
    // First, try to find explicit schema definition in schema files
    let schema_ids = project_files.schema_file_ids(db).ids(db);

    for file_id in schema_ids.iter() {
        let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id) else {
            continue;
        };

        let parse = graphql_syntax::parse(db, content, metadata);

        // Look for schema definition in all documents
        for doc in parse.documents() {
            for definition in &doc.ast.definitions {
                if let apollo_compiler::ast::Definition::SchemaDefinition(schema_def) = definition {
                    return extract_from_schema_definition(schema_def);
                }
            }
        }
    }

    // No explicit schema definition found, use defaults if types exist
    RootTypeNames {
        query: schema_types
            .contains_key("Query")
            .then(|| "Query".to_string()),
        mutation: schema_types
            .contains_key("Mutation")
            .then(|| "Mutation".to_string()),
        subscription: schema_types
            .contains_key("Subscription")
            .then(|| "Subscription".to_string()),
    }
}

/// Extract root type names from a schema definition AST node
fn extract_from_schema_definition(
    schema_def: &apollo_compiler::ast::SchemaDefinition,
) -> RootTypeNames {
    let mut result = RootTypeNames::default();

    for root_op in &schema_def.root_operations {
        let (op_type, named_type) = root_op.as_ref();
        let type_name = named_type.as_str().to_string();
        match op_type {
            apollo_compiler::ast::OperationType::Query => {
                result.query = Some(type_name);
            }
            apollo_compiler::ast::OperationType::Mutation => {
                result.mutation = Some(type_name);
            }
            apollo_compiler::ast::OperationType::Subscription => {
                result.subscription = Some(type_name);
            }
        }
    }

    result
}

// Tests for schema_utils are in the integration tests since they require
// the full database setup which is complex to replicate in a unit test.
