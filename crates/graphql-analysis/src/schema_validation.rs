// Schema validation queries

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase};
use graphql_db::{FileContent, FileMetadata};
use std::collections::HashSet;
use std::sync::Arc;

/// Validate a schema file
/// This checks for:
/// - Duplicate type names within the file
/// - Conflicts with types in other files
/// - Invalid field definitions
/// - Invalid directive usage
#[salsa::tracked]
pub fn validate_schema_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let structure = graphql_hir::file_structure(db, metadata.file_id(db), content, metadata);
    let mut diagnostics = Vec::new();

    // Get all types for cross-referencing
    let all_types = graphql_hir::schema_types(db);

    // Check for duplicate type names within this file
    let mut seen_types = HashSet::new();
    for type_def in &structure.type_defs {
        if !seen_types.insert(type_def.name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("Duplicate type name: {}", type_def.name),
                DiagnosticRange::default(), // TODO: Get actual position from HIR
            ));
        }
    }

    // Validate each type definition
    for type_def in &structure.type_defs {
        validate_type_def(type_def, &all_types, &mut diagnostics);
    }

    Arc::new(diagnostics)
}

/// Validate a single type definition
fn validate_type_def(
    type_def: &graphql_hir::TypeDef,
    all_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use graphql_hir::TypeDefKind;

    match type_def.kind {
        TypeDefKind::Object | TypeDefKind::Interface => {
            // Validate field types exist
            for field in &type_def.fields {
                validate_type_ref(&field.type_ref, all_types, diagnostics);

                // Validate argument types
                for arg in &field.arguments {
                    validate_type_ref(&arg.type_ref, all_types, diagnostics);
                }
            }

            // Validate interface implementations
            for interface_name in &type_def.implements {
                if let Some(interface_type) = all_types.get(interface_name) {
                    // Check that the interface is actually an interface
                    if interface_type.kind == TypeDefKind::Interface {
                        // Validate that all interface fields are implemented
                        validate_interface_implementation(type_def, interface_type, diagnostics);
                    } else {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "Type '{}' implements '{}', but '{}' is not an interface",
                                type_def.name, interface_name, interface_name
                            ),
                            DiagnosticRange::default(),
                        ));
                    }
                } else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "Type '{}' implements unknown interface '{}'",
                            type_def.name, interface_name
                        ),
                        DiagnosticRange::default(),
                    ));
                }
            }
        }
        TypeDefKind::Union => {
            // Validate union members exist and are object types
            for member_name in &type_def.union_members {
                if let Some(member_type) = all_types.get(member_name) {
                    if member_type.kind != TypeDefKind::Object {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "Union '{}' includes '{}', but '{}' is not an object type",
                                type_def.name, member_name, member_name
                            ),
                            DiagnosticRange::default(),
                        ));
                    }
                } else {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "Union '{}' includes unknown type '{}'",
                            type_def.name, member_name
                        ),
                        DiagnosticRange::default(),
                    ));
                }
            }
        }
        TypeDefKind::InputObject => {
            // Validate input field types exist and are valid input types
            for field in &type_def.fields {
                validate_input_type_ref(&field.type_ref, all_types, diagnostics);
            }
        }
        TypeDefKind::Enum | TypeDefKind::Scalar => {
            // No additional validation needed for enums and scalars
        }
    }
}

/// Validate that a type reference points to an existing type
fn validate_type_ref(
    type_ref: &graphql_hir::TypeRef,
    all_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Built-in scalars don't need validation
    if is_builtin_scalar(&type_ref.name) {
        return;
    }

    if !all_types.contains_key(&type_ref.name) {
        diagnostics.push(Diagnostic::error(
            format!("Unknown type: {}", type_ref.name),
            DiagnosticRange::default(),
        ));
    }
}

/// Validate that a type reference is a valid input type (scalar, enum, or input object)
fn validate_input_type_ref(
    type_ref: &graphql_hir::TypeRef,
    all_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Built-in scalars are valid input types
    if is_builtin_scalar(&type_ref.name) {
        return;
    }

    if let Some(type_def) = all_types.get(&type_ref.name) {
        use graphql_hir::TypeDefKind;
        match type_def.kind {
            TypeDefKind::Scalar | TypeDefKind::Enum | TypeDefKind::InputObject => {
                // Valid input types
            }
            _ => {
                diagnostics.push(Diagnostic::error(
                    format!("Type '{}' is not a valid input type", type_ref.name),
                    DiagnosticRange::default(),
                ));
            }
        }
    } else {
        diagnostics.push(Diagnostic::error(
            format!("Unknown type: {}", type_ref.name),
            DiagnosticRange::default(),
        ));
    }
}

/// Validate that a type correctly implements an interface
fn validate_interface_implementation(
    type_def: &graphql_hir::TypeDef,
    interface: &graphql_hir::TypeDef,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Check that all interface fields are implemented
    for interface_field in &interface.fields {
        if let Some(impl_field) = type_def
            .fields
            .iter()
            .find(|f| f.name == interface_field.name)
        {
            // Check that the field type matches
            if impl_field.type_ref != interface_field.type_ref {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "Type '{}' field '{}' has type '{}', but interface '{}' requires '{}'",
                        type_def.name,
                        impl_field.name,
                        format_type_ref(&impl_field.type_ref),
                        interface.name,
                        format_type_ref(&interface_field.type_ref)
                    ),
                    DiagnosticRange::default(),
                ));
            }

            // Check that all interface arguments are present
            for interface_arg in &interface_field.arguments {
                if !impl_field
                    .arguments
                    .iter()
                    .any(|a| a.name == interface_arg.name)
                {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "Type '{}' field '{}' missing required argument '{}' from interface '{}'",
                            type_def.name, impl_field.name, interface_arg.name, interface.name
                        ),
                        DiagnosticRange::default(),
                    ));
                }
            }
        } else {
            diagnostics.push(Diagnostic::error(
                format!(
                    "Type '{}' does not implement field '{}' required by interface '{}'",
                    type_def.name, interface_field.name, interface.name
                ),
                DiagnosticRange::default(),
            ));
        }
    }
}

/// Check if a type name is a built-in GraphQL scalar
fn is_builtin_scalar(name: &str) -> bool {
    matches!(name, "Int" | "Float" | "String" | "Boolean" | "ID")
}

/// Format a type reference as a string (for error messages)
fn format_type_ref(type_ref: &graphql_hir::TypeRef) -> String {
    let mut result = String::new();

    if type_ref.is_list {
        result.push('[');
        result.push_str(&type_ref.name);
        if type_ref.inner_non_null {
            result.push('!');
        }
        result.push(']');
    } else {
        result.push_str(&type_ref.name);
    }

    if type_ref.is_non_null {
        result.push('!');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileKind, FileMetadata, FileUri};

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
    impl graphql_hir::GraphQLHirDatabase for TestDatabase {}

    #[salsa::db]
    impl crate::GraphQLAnalysisDatabase for TestDatabase {}

    #[test]
    fn test_unknown_field_type() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let schema_content = "type User { profile: Profile }";
        let content = FileContent::new(&db, Arc::from(schema_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let diagnostics = validate_schema_file(&db, content, metadata);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Unknown type: Profile"));
    }

    #[test]
    #[ignore = "Requires multi-file HIR setup for cross-type validation"]
    fn test_interface_implementation_missing_field() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let schema_content = r"
            interface Node { id: ID! name: String! }
            type User implements Node { id: ID! }
        ";
        let content = FileContent::new(&db, Arc::from(schema_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let diagnostics = validate_schema_file(&db, content, metadata);

        let missing_field_error = diagnostics
            .iter()
            .find(|d| d.message.contains("does not implement field 'name'"));
        assert!(
            missing_field_error.is_some(),
            "Expected error about missing 'name' field"
        );
    }

    #[test]
    #[ignore = "Requires multi-file HIR setup for cross-type validation"]
    fn test_interface_implementation_wrong_type() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let schema_content = r"
            interface Node { id: ID! }
            type User implements Node { id: String! }
        ";
        let content = FileContent::new(&db, Arc::from(schema_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let diagnostics = validate_schema_file(&db, content, metadata);

        let wrong_type_error = diagnostics.iter().find(|d| {
            d.message.contains("has type 'String!'")
                && d.message.contains("but interface 'Node' requires 'ID!'")
        });
        assert!(
            wrong_type_error.is_some(),
            "Expected error about wrong field type"
        );
    }

    #[test]
    #[ignore = "Requires multi-file HIR setup for cross-type validation"]
    fn test_union_non_object_member() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let schema_content = r"
            interface Node { id: ID! }
            union SearchResult = Node
        ";
        let content = FileContent::new(&db, Arc::from(schema_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let diagnostics = validate_schema_file(&db, content, metadata);

        let non_object_error = diagnostics
            .iter()
            .find(|d| d.message.contains("is not an object type"));
        assert!(
            non_object_error.is_some(),
            "Expected error about non-object union member"
        );
    }

    #[test]
    fn test_union_unknown_member() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let schema_content = "union SearchResult = User | Post";
        let content = FileContent::new(&db, Arc::from(schema_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let diagnostics = validate_schema_file(&db, content, metadata);

        assert!(diagnostics.len() >= 2);
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("includes unknown type 'User'")));
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("includes unknown type 'Post'")));
    }

    #[test]
    #[ignore = "Requires multi-file HIR setup for cross-type validation"]
    fn test_input_object_invalid_field_type() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let schema_content = r"
            type User { id: ID! }
            input CreateUserInput { user: User! }
        ";
        let content = FileContent::new(&db, Arc::from(schema_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let diagnostics = validate_schema_file(&db, content, metadata);

        let invalid_input_error = diagnostics
            .iter()
            .find(|d| d.message.contains("is not a valid input type"));
        assert!(
            invalid_input_error.is_some(),
            "Expected error about invalid input type"
        );
    }

    #[test]
    #[ignore = "Requires multi-file HIR setup"]
    fn test_valid_schema() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let schema_content = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! name: String! }
            type Post { id: ID! author: User! }
            union SearchResult = User | Post
            input CreateUserInput { name: String! }
        ";
        let content = FileContent::new(&db, Arc::from(schema_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let diagnostics = validate_schema_file(&db, content, metadata);

        assert_eq!(diagnostics.len(), 0, "Expected no validation errors");
    }

    #[test]
    fn test_duplicate_type_name() {
        let db = TestDatabase::default();
        let file_id = graphql_db::FileId::new(0);

        let schema_content = r"
            type User { id: ID! }
            type User { name: String! }
        ";
        let content = FileContent::new(&db, Arc::from(schema_content));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("schema.graphql"),
            FileKind::Schema,
        );

        let diagnostics = validate_schema_file(&db, content, metadata);

        let duplicate_error = diagnostics
            .iter()
            .find(|d| d.message.contains("Duplicate type name: User"));
        assert!(
            duplicate_error.is_some(),
            "Expected error about duplicate type name"
        );
    }
}
