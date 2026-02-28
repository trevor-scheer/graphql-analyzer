//! Integration tests for graphql-analysis.
//!
//! These tests verify validation, schema merging, and document validation.

use graphql_analysis::{
    analyze_field_usage, file_diagnostics, file_validation_diagnostics,
    merged_schema::merged_schema_with_diagnostics, validate_document_file, validate_file,
    FieldCoverageReport, TypeCoverage,
};
use graphql_base_db::{DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language};
use graphql_test_utils::{create_project_files, TestDatabase, TestDatabaseWithProject};
use std::sync::Arc;

// ============================================================================
// file_diagnostics tests (from lib.rs)
// ============================================================================

#[test]
fn test_file_diagnostics_empty() {
    let db = TestDatabase::default();
    let file_id = FileId::new(0);

    let content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("file:///test.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let diagnostics = file_diagnostics(&db, content, metadata, None);

    assert!(
        diagnostics.is_empty(),
        "Valid schema should have no diagnostics, got: {diagnostics:?}"
    );
}

// ============================================================================
// validation tests (from validation.rs)
// ============================================================================

#[test]
fn test_validate_file_no_schema() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let content = FileContent::new(&db, Arc::from("query { hello }"));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(&mut db, &[], &[]);

    let diagnostics = validate_file(&db, content, metadata, project_files);
    assert_eq!(
        diagnostics.len(),
        0,
        "Expected no diagnostics when schema is missing"
    );
}

#[test]
fn test_validate_file_with_valid_fragment() {
    let mut db = TestDatabase::default();

    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("fragment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
    assert_eq!(
        diagnostics.len(),
        0,
        "Expected no diagnostics for valid fragment. Fragments don't need operations in the same file."
    );
}

#[test]
fn test_validate_file_with_invalid_fragment() {
    let mut db = TestDatabase::default();

    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(
        &db,
        Arc::from("fragment UserFields on User { invalidField }"),
    );
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("fragment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics for fragment with invalid field"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("invalidField") || d.message.contains("field")),
        "Expected error message about invalid field 'invalidField'"
    );
}

#[test]
fn test_validate_file_invalid_field() {
    let mut db = TestDatabase::default();

    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(&db, Arc::from("query { world }"));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics for invalid field selection"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("world") || d.message.contains("field")),
        "Expected error message about invalid field 'world'"
    );
}

#[test]
fn test_validate_file_valid_query() {
    let mut db = TestDatabase::default();

    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(&db, Arc::from("query { hello }"));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);
    assert_eq!(
        diagnostics.len(),
        0,
        "Expected no diagnostics for valid query"
    );
}

#[test]
fn test_cross_file_fragment_resolution() {
    let mut db = TestDatabase::default();

    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(
        &db,
        Arc::from("type Query { user: User } type User { id: ID! name: String! }"),
    );
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let frag_id = FileId::new(1);
    let frag_content = FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
    let frag_metadata = FileMetadata::new(
        &db,
        frag_id,
        FileUri::new("fragments.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let query_id = FileId::new(2);
    let query_content = FileContent::new(&db, Arc::from("query { user { ...UserFields } }"));
    let query_metadata = FileMetadata::new(
        &db,
        query_id,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[
            (frag_id, frag_content, frag_metadata),
            (query_id, query_content, query_metadata),
        ],
    );

    let diagnostics = validate_file(&db, query_content, query_metadata, project_files);
    assert_eq!(
        diagnostics.len(),
        0,
        "Expected no diagnostics when fragment is defined in another file. Got: {diagnostics:?}"
    );
}

// ============================================================================
// schema_validation tests
// ============================================================================

#[test]
fn test_valid_schema() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let schema_content = r"
        type Query { search: [SearchResult!]! }
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
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let project_files = create_project_files(&mut db, &[(file_id, content, metadata)], &[]);
    let diagnostics = file_validation_diagnostics(&db, content, metadata, Some(project_files));

    assert_eq!(
        diagnostics.len(),
        0,
        "Expected no validation errors. Got: {diagnostics:?}"
    );
}

#[test]
fn test_duplicate_type_name() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let schema_content = r"
        type User { id: ID! }
        type User { name: String! }
    ";
    let content = FileContent::new(&db, Arc::from(schema_content));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let project_files = create_project_files(&mut db, &[(file_id, content, metadata)], &[]);
    let diagnostics = file_validation_diagnostics(&db, content, metadata, Some(project_files));

    assert!(!diagnostics.is_empty(), "Expected validation errors");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("User") || d.message.contains("duplicate")),
        "Expected error about duplicate type name. Got: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_syntax() {
    let db = TestDatabase::default();
    let file_id = FileId::new(0);

    let schema_content = "type User { invalid syntax here";
    let content = FileContent::new(&db, Arc::from(schema_content));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let diagnostics = file_validation_diagnostics(&db, content, metadata, None);

    assert!(!diagnostics.is_empty(), "Expected parse/validation errors");
}

#[test]
fn test_duplicate_field_in_extension() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let schema_content = r"
        type Query { user: User }
        type User { id: ID!, email: String! }
        extend type User { email: String! }
    ";
    let content = FileContent::new(&db, Arc::from(schema_content));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let project_files = create_project_files(&mut db, &[(file_id, content, metadata)], &[]);
    let diagnostics = file_validation_diagnostics(&db, content, metadata, Some(project_files));

    assert!(
        !diagnostics.is_empty(),
        "Expected error for duplicate field in extension"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("email") || d.message.contains("duplicate")),
        "Expected error about duplicate field. Got: {diagnostics:?}"
    );
}

// ============================================================================
// document_validation tests (from document_validation.rs)
// ============================================================================

#[test]
fn test_unknown_variable_type() {
    let mut db = TestDatabaseWithProject::default();
    let file_id = FileId::new(0);

    let doc_content = "query GetUser($input: UserInput!) { user }";
    let content = FileContent::new(&db, Arc::from(doc_content));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(&mut db, &[], &[(file_id, content, metadata)]);
    db.set_project_files(Some(project_files));

    let diagnostics = validate_document_file(&db, content, metadata, project_files);

    let unknown_type_error = diagnostics
        .iter()
        .find(|d| d.message.contains("Unknown variable type: UserInput"));
    assert!(
        unknown_type_error.is_some(),
        "Expected error about unknown variable type"
    );
}

#[test]
fn test_fragment_unknown_type_condition() {
    let mut db = TestDatabaseWithProject::default();
    let file_id = FileId::new(0);

    let doc_content = "fragment UserFields on User { id }";
    let content = FileContent::new(&db, Arc::from(doc_content));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("fragment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(&mut db, &[], &[(file_id, content, metadata)]);
    db.set_project_files(Some(project_files));

    let diagnostics = validate_document_file(&db, content, metadata, project_files);

    let unknown_type_error = diagnostics
        .iter()
        .find(|d| d.message.contains("has unknown type condition 'User'"));
    assert!(
        unknown_type_error.is_some(),
        "Expected error about unknown type condition"
    );
}

#[test]
fn test_missing_root_type() {
    let mut db = TestDatabaseWithProject::default();
    let file_id = FileId::new(0);

    let doc_content = "query { hello }";
    let content = FileContent::new(&db, Arc::from(doc_content));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(&mut db, &[], &[(file_id, content, metadata)]);
    db.set_project_files(Some(project_files));

    let diagnostics = validate_document_file(&db, content, metadata, project_files);

    let missing_root_error = diagnostics
        .iter()
        .find(|d| d.message.contains("does not define a 'Query' type"));
    assert!(
        missing_root_error.is_some(),
        "Expected error about missing Query root type"
    );
}

#[test]
fn test_valid_document() {
    let mut db = TestDatabaseWithProject::default();

    let schema_file_id = FileId::new(0);
    let schema_content = r"
        type Query { user(id: ID!): User }
        type User { id: ID! name: String! }
        input UserFilter { name: String }
    ";
    let schema_fc = FileContent::new(&db, Arc::from(schema_content));
    let schema_metadata = FileMetadata::new(
        &db,
        schema_file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let doc_file_id = FileId::new(1);
    let doc_content = r"
        query GetUser($id: ID!, $filter: UserFilter) {
            user(id: $id) { id name }
        }
        fragment UserFields on User { id name }
    ";
    let doc_fc = FileContent::new(&db, Arc::from(doc_content));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_file_id,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_file_id, schema_fc, schema_metadata)],
        &[(doc_file_id, doc_fc, doc_metadata)],
    );
    db.set_project_files(Some(project_files));

    let diagnostics = validate_document_file(&db, doc_fc, doc_metadata, project_files);

    assert_eq!(diagnostics.len(), 0, "Expected no validation errors");
}

// ============================================================================
// merged_schema tests (from merged_schema.rs)
// ============================================================================

#[test]
fn test_merged_schema_single_file() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );
    let schema_files = [(file_id, content, metadata)];
    let project_files = create_project_files(&mut db, &schema_files, &[]);
    let schema = merged_schema_with_diagnostics(&db, project_files).schema;
    assert!(
        schema.is_some(),
        "Expected schema to be merged successfully"
    );

    let schema = schema.unwrap();
    assert!(
        schema.types.contains_key("Query"),
        "Expected Query type to exist in merged schema"
    );
}

#[test]
fn test_merged_schema_multiple_files() {
    let mut db = TestDatabase::default();

    let file1_id = FileId::new(0);
    let content1 = FileContent::new(&db, Arc::from("type Query { hello: String }"));
    let metadata1 = FileMetadata::new(
        &db,
        file1_id,
        FileUri::new("schema1.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let file2_id = FileId::new(1);
    let content2 = FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
    let metadata2 = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("schema2.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let schema_files = [
        (file1_id, content1, metadata1),
        (file2_id, content2, metadata2),
    ];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let schema = merged_schema_with_diagnostics(&db, project_files).schema;
    assert!(
        schema.is_some(),
        "Expected schema to be merged successfully"
    );

    let schema = schema.unwrap();
    assert!(
        schema.types.contains_key("Query"),
        "Expected Query type to exist in merged schema"
    );
    assert!(
        schema.types.contains_key("User"),
        "Expected User type to exist in merged schema"
    );
}

#[test]
fn test_merged_schema_with_extensions() {
    let mut db = TestDatabase::default();

    let file1_id = FileId::new(0);
    let content1 = FileContent::new(&db, Arc::from("type Query { hello: String }"));
    let metadata1 = FileMetadata::new(
        &db,
        file1_id,
        FileUri::new("schema1.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let file2_id = FileId::new(1);
    let content2 = FileContent::new(&db, Arc::from("extend type Query { world: String }"));
    let metadata2 = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("schema2.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let schema_files = [
        (file1_id, content1, metadata1),
        (file2_id, content2, metadata2),
    ];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let schema = merged_schema_with_diagnostics(&db, project_files).schema;
    assert!(
        schema.is_some(),
        "Expected schema to be merged successfully"
    );

    let schema = schema.unwrap();
    let query_type = schema.types.get("Query");
    assert!(query_type.is_some(), "Expected Query type to exist");

    if let Some(apollo_compiler::schema::ExtendedType::Object(obj)) = query_type {
        let field_names: Vec<&str> = obj
            .fields
            .keys()
            .map(apollo_compiler::Name::as_str)
            .collect();
        assert!(
            field_names.contains(&"hello"),
            "Expected hello field in Query type"
        );
        assert!(
            field_names.contains(&"world"),
            "Expected world field in Query type (from extension)"
        );
    } else {
        panic!("Expected Query to be an object type");
    }
}

#[test]
fn test_merged_schema_no_files() {
    let mut db = TestDatabase::default();

    let project_files = create_project_files(&mut db, &[], &[]);

    let schema = merged_schema_with_diagnostics(&db, project_files).schema;
    assert!(schema.is_none(), "Expected None when no schema files exist");
}

#[test]
fn test_merged_schema_invalid_syntax() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let content = FileContent::new(&db, Arc::from("type Query { invalid syntax here"));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let schema_files = [(file_id, content, metadata)];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let schema = merged_schema_with_diagnostics(&db, project_files).schema;
    assert!(
        schema.is_none(),
        "Expected None when schema has parse errors"
    );
}

#[test]
fn test_merged_schema_validation_error() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let content = FileContent::new(
        &db,
        Arc::from("type Query { hello: String }\ntype Query { world: String }"),
    );
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let schema_files = [(file_id, content, metadata)];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let schema = merged_schema_with_diagnostics(&db, project_files).schema;
    assert!(
        schema.is_none(),
        "Expected None when schema has validation errors"
    );
}

#[test]
fn test_interface_implementation_missing_field() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let content = FileContent::new(
        &db,
        Arc::from(
            r"
            interface Node { id: ID! name: String! }
            type User implements Node { id: ID! }
            type Query { user: User }
        ",
        ),
    );
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let schema_files = [(file_id, content, metadata)];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let result = merged_schema_with_diagnostics(&db, project_files);

    assert!(
        result.schema.is_some(),
        "Expected schema to be present (with errors) for document validation"
    );
    assert!(
        !result.diagnostics_by_file.is_empty(),
        "Expected diagnostics for missing interface field"
    );
    let all_diagnostics: Vec<_> = result.diagnostics_by_file.values().flatten().collect();
    assert!(
        all_diagnostics
            .iter()
            .any(|d| d.message.to_lowercase().contains("name")
                || d.message.to_lowercase().contains("interface")),
        "Expected error about missing 'name' field. Got: {all_diagnostics:?}"
    );
}

#[test]
fn test_valid_interface_implementation() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let content = FileContent::new(
        &db,
        Arc::from(
            r"
            interface Node { id: ID! }
            type User implements Node { id: ID! name: String! }
            type Query { user: User }
        ",
        ),
    );
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let schema_files = [(file_id, content, metadata)];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let result = merged_schema_with_diagnostics(&db, project_files);

    assert!(
        result.schema.is_some(),
        "Expected valid schema for correct interface implementation"
    );
    assert!(
        result.diagnostics_by_file.is_empty(),
        "Expected no diagnostics. Got: {:?}",
        result.diagnostics_by_file
    );
}

#[test]
fn test_schema_diagnostics_attributed_to_correct_file() {
    let mut db = TestDatabase::default();

    let file_id1 = FileId::new(0);
    let content1 = FileContent::new(
        &db,
        Arc::from(
            r"
            type Query { user: User }
            interface Node { id: ID! name: String! }
        ",
        ),
    );
    let metadata1 = FileMetadata::new(
        &db,
        file_id1,
        FileUri::new("types.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let file_id2 = FileId::new(1);
    let content2 = FileContent::new(
        &db,
        Arc::from(
            r"
            type User implements Node { id: ID! }
        ",
        ),
    );
    let metadata2 = FileMetadata::new(
        &db,
        file_id2,
        FileUri::new("user.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let schema_files = [
        (file_id1, content1, metadata1),
        (file_id2, content2, metadata2),
    ];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let result = merged_schema_with_diagnostics(&db, project_files);

    assert!(
        result.schema.is_some(),
        "Expected schema to be present for document validation"
    );
    assert!(
        !result.diagnostics_by_file.is_empty(),
        "Expected diagnostics for missing interface field"
    );

    assert!(
        !result.diagnostics_by_file.contains_key("types.graphql"),
        "types.graphql should have no diagnostics. Got: {:?}",
        result.diagnostics_by_file
    );
    assert!(
        result.diagnostics_by_file.contains_key("user.graphql"),
        "user.graphql should have the missing field error. Got: {:?}",
        result.diagnostics_by_file
    );

    let diags_for_types = file_diagnostics(&db, content1, metadata1, Some(project_files));
    let diags_for_user = file_diagnostics(&db, content2, metadata2, Some(project_files));

    assert!(
        diags_for_types.is_empty(),
        "file_diagnostics for types.graphql should be empty. Got: {diags_for_types:?}"
    );
    assert!(
        !diags_for_user.is_empty(),
        "file_diagnostics for user.graphql should have errors. Got: {diags_for_user:?}"
    );
}

#[test]
fn test_schema_build_error_attributed_to_correct_file() {
    let mut db = TestDatabase::default();

    let file_id1 = FileId::new(0);
    let content1 = FileContent::new(&db, Arc::from("type Query { hello: String }"));
    let metadata1 = FileMetadata::new(
        &db,
        file_id1,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let file_id2 = FileId::new(1);
    let content2 = FileContent::new(&db, Arc::from("type Query { world: String }"));
    let metadata2 = FileMetadata::new(
        &db,
        file_id2,
        FileUri::new("duplicate.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let schema_files = [
        (file_id1, content1, metadata1),
        (file_id2, content2, metadata2),
    ];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let result = merged_schema_with_diagnostics(&db, project_files);

    assert!(
        result.schema.is_none(),
        "Expected schema build to fail with duplicate type"
    );
    assert!(
        !result.diagnostics_by_file.is_empty(),
        "Expected diagnostics for duplicate type"
    );

    let all_files_with_errors: Vec<_> = result.diagnostics_by_file.keys().collect();
    assert!(
        all_files_with_errors
            .iter()
            .all(|f| f.contains("query.graphql") || f.contains("duplicate.graphql")),
        "Diagnostics should only be for files involved in the error. Got: {all_files_with_errors:?}"
    );
}

// ============================================================================
// project_lints tests (from project_lints.rs) - public API only
// ============================================================================

#[test]
fn test_analyze_field_usage_basic() {
    let mut db = TestDatabaseWithProject::default();

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
        Language::GraphQL,
        DocumentKind::Schema,
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
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    db.set_project_files(Some(project_files));

    let coverage = analyze_field_usage(&db, project_files);

    assert_eq!(coverage.total_fields, 4); // Query.user, User.id, User.name, User.email
    assert_eq!(coverage.used_fields, 3); // Query.user, User.id, User.name

    let user_id = coverage
        .field_usages
        .get(&(Arc::from("User"), Arc::from("id")));
    assert!(user_id.is_some());
    assert_eq!(user_id.unwrap().usage_count, 1);
    assert!(user_id.unwrap().operations.contains(&Arc::from("GetUser")));

    let user_email = coverage
        .field_usages
        .get(&(Arc::from("User"), Arc::from("email")));
    assert!(user_email.is_some());
    assert_eq!(user_email.unwrap().usage_count, 0);
}

#[test]
fn test_analyze_field_usage_multiple_operations() {
    let mut db = TestDatabaseWithProject::default();

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
            }
            ",
        ),
    );
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
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

            query GetUserName {
                user {
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
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    db.set_project_files(Some(project_files));

    let coverage = analyze_field_usage(&db, project_files);

    let user_name = coverage
        .field_usages
        .get(&(Arc::from("User"), Arc::from("name")));
    assert!(user_name.is_some());
    assert_eq!(user_name.unwrap().usage_count, 2);
    assert_eq!(user_name.unwrap().operations.len(), 2);

    let user_id = coverage
        .field_usages
        .get(&(Arc::from("User"), Arc::from("id")));
    assert!(user_id.is_some());
    assert_eq!(user_id.unwrap().usage_count, 1);
}

#[test]
fn test_analyze_field_usage_with_fragments() {
    let mut db = TestDatabaseWithProject::default();

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
        Language::GraphQL,
        DocumentKind::Schema,
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
                email
            }
            ",
        ),
    );
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    db.set_project_files(Some(project_files));

    let coverage = analyze_field_usage(&db, project_files);

    let user_email = coverage
        .field_usages
        .get(&(Arc::from("User"), Arc::from("email")));
    assert!(user_email.is_some());
    assert_eq!(user_email.unwrap().usage_count, 1);

    let user_name = coverage
        .field_usages
        .get(&(Arc::from("User"), Arc::from("name")));
    assert!(user_name.is_some());
    assert_eq!(user_name.unwrap().usage_count, 0);
}

#[test]
fn test_field_coverage_report_percentage() {
    let report = FieldCoverageReport {
        total_fields: 10,
        used_fields: 7,
        ..FieldCoverageReport::default()
    };

    assert!((report.coverage_percentage() - 70.0).abs() < 0.01);
}

#[test]
fn test_type_coverage_percentage() {
    let coverage = TypeCoverage {
        total_fields: 5,
        used_fields: 4,
    };

    assert!((coverage.coverage_percentage() - 80.0).abs() < 0.01);
}

// ============================================================================
// TypeScript schema file tests
// ============================================================================

/// Test that diagnostics from TypeScript schema files have correct line offsets.
///
/// When GraphQL is embedded in TypeScript template literals, diagnostics should
/// report positions relative to the original file, not the extracted GraphQL content.
#[test]
fn test_typescript_schema_diagnostics_have_correct_line_offset() {
    let mut db = TestDatabase::default();

    // TypeScript file with GraphQL starting on line 3 (0-indexed: line 2)
    // The GraphQL content itself starts on line 4 (0-indexed: line 3)
    let ts_content = r#"import { gql } from "graphql-tag";

export const schema = gql`
  type Query {
    user: User
  }

  type User implements Node {
    id: ID!
  }
`;
"#;

    let file_id = FileId::new(0);
    let content = FileContent::new(&db, Arc::from(ts_content));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("file:///schema.ts"),
        Language::TypeScript,
        DocumentKind::Schema,
    );

    let schema_files = [(file_id, content, metadata)];
    let project_files = create_project_files(&mut db, &schema_files, &[]);

    let result = merged_schema_with_diagnostics(&db, project_files);

    // Should have diagnostics for "User implements Node" - Node is not defined
    assert!(
        !result.diagnostics_by_file.is_empty(),
        "Expected diagnostics for missing Node interface"
    );

    // Get diagnostics for this file
    let file_diagnostics = result
        .diagnostics_by_file
        .get("file:///schema.ts")
        .expect("Expected diagnostics for schema.ts");

    assert!(
        !file_diagnostics.is_empty(),
        "Expected at least one diagnostic"
    );

    // The error should be on line 7 (0-indexed) where "type User implements Node" is
    // In the original TS file:
    // Line 0: import { gql } from "graphql-tag";
    // Line 1: (empty)
    // Line 2: export const schema = gql`
    // Line 3:   type Query {
    // Line 4:     user: User
    // Line 5:   }
    // Line 6:   (empty)
    // Line 7:   type User implements Node {
    // Line 8:     id: ID!
    // Line 9:   }
    // Line 10: `;

    let diag = &file_diagnostics[0];
    // The diagnostic should point to line 7 (0-indexed) in the original TS file,
    // NOT line 4 (which would be the position in the extracted GraphQL content)
    assert!(
        diag.range.start.line >= 7,
        "Diagnostic line should be >= 7 (in original TS file), got line {}. \
         If this is ~4-5, the line offset from extraction is not being applied. \
         Diagnostic: {:?}",
        diag.range.start.line,
        diag
    );
}

/// Test that TypeScript files with multiple gql blocks where fragments are
/// shared between blocks don't produce false "fragment defined multiple times" errors.
///
/// This is a regression test for a bug where:
/// 1. Block A defines fragments F1, F2, F3
/// 2. Block B defines fragment F4 which spreads ...F1, ...F2, ...F3
/// 3. When validating Block B, we add F1's AST, F2's AST, F3's AST to the builder
/// 4. But F1, F2, F3 are all in the SAME AST (Block A), so we'd add it 3 times
/// 5. Apollo-compiler then reports "fragment defined multiple times"
#[test]
fn test_typescript_multi_block_no_duplicate_fragment_errors() {
    let mut db = TestDatabase::default();

    // Schema with types needed for the fragments
    let schema_content = r"
        type Query { repository(owner: String!, name: String!): Repository }
        type Repository {
            id: ID!
            name: String!
            stargazerCount: Int!
            forkCount: Int!
        }
    ";
    let schema_id = FileId::new(0);
    let schema_fc = FileContent::new(&db, Arc::from(schema_content));
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    // TypeScript file with multiple gql blocks
    // Block 1: Defines RepoBasic, RepoStats fragments
    // Block 2: Defines RepoFull fragment that spreads ...RepoBasic, ...RepoStats
    // Block 3: Query that uses ...RepoFull
    let ts_content = r#"import { gql } from "@apollo/client";

export const REPO_FRAGMENTS = gql`
  fragment RepoBasic on Repository {
    id
    name
  }

  fragment RepoStats on Repository {
    stargazerCount
    forkCount
  }
`;

export const REPO_FULL = gql`
  fragment RepoFull on Repository {
    ...RepoBasic
    ...RepoStats
  }
`;

export const GET_REPO = gql`
  query GetRepo($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) {
      ...RepoFull
    }
  }
`;
"#;

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(&db, Arc::from(ts_content));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("file:///components/Repo.tsx"),
        Language::TypeScript,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_fc, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);

    // Filter for "defined multiple times" errors
    let duplicate_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.message.contains("defined multiple times"))
        .collect();

    assert!(
        duplicate_errors.is_empty(),
        "Should not have 'fragment defined multiple times' errors when fragments are \
         defined once but referenced from multiple blocks in the same file. \
         Got {} duplicate errors: {:?}",
        duplicate_errors.len(),
        duplicate_errors
    );
}

/// Regression test: CLI validates document files correctly.
///
/// This tests that `file_validation_diagnostics` (used by CLI's validate command)
/// correctly reports validation errors from document files. Previously, there was
/// a bug where document files were registered with raw paths but looked up with
/// file:// URIs, causing all document validation to silently fail.
///
/// Uses a nullable variable for a non-nullable argument as an example validation error.
#[test]
fn test_cli_validates_document_files() {
    let mut db = TestDatabase::default();

    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(
        &db,
        Arc::from("type Query { user(id: ID!): User }\ntype User { id: ID! name: String! }"),
    );
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let doc_id = FileId::new(1);
    // TypeScript file with embedded GraphQL containing a validation error
    // (nullable variable $id: ID used where non-nullable ID! is required)
    let ts_content = r#"import { gql } from "@apollo/client";

export const GET_USER = gql`
  query GetUser($id: ID) {
    user(id: $id) {
      id
      name
    }
  }
`;
"#;
    let doc_content = FileContent::new(&db, Arc::from(ts_content));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("file:///components/UserPanel.tsx"),
        Language::TypeScript,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    // file_validation_diagnostics is what CLI's validate command uses
    let diagnostics =
        file_validation_diagnostics(&db, doc_content, doc_metadata, Some(project_files));

    assert!(
        !diagnostics.is_empty(),
        "CLI validate should report validation errors from document files. Got none."
    );
}

#[test]
fn test_relay_arguments_directive_errors_suppressed() {
    // Relay's @arguments and @argumentDefinitions accept dynamic arguments
    // that mirror the target fragment's definitions. These can't be statically
    // declared, so we suppress "argument X is not supported by @arguments" errors.
    let mut db = TestDatabase::default();

    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(
        &db,
        Arc::from("type Query { viewer: Viewer }\ntype Viewer { user: User }\ntype User { id: ID! name: String }"),
    );
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("file:///schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    // Add Relay's @arguments directive with no arguments (dynamic)
    let builtins_id = FileId::new(1);
    let builtins_content = FileContent::new(
        &db,
        Arc::from("directive @arguments on FRAGMENT_SPREAD\ndirective @argumentDefinitions on FRAGMENT_DEFINITION"),
    );
    let builtins_metadata = FileMetadata::new(
        &db,
        builtins_id,
        FileUri::new("file:///builtins.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    // Fragment definition (needed for the spread)
    let frag_id = FileId::new(2);
    let frag_content = FileContent::new(
        &db,
        Arc::from("fragment UserFields on User @argumentDefinitions(showName: {type: \"Boolean\", defaultValue: true}) { id }"),
    );
    let frag_metadata = FileMetadata::new(
        &db,
        frag_id,
        FileUri::new("file:///fragment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    // Query using @arguments with dynamic args
    let doc_id = FileId::new(3);
    let doc_content = FileContent::new(
        &db,
        Arc::from("query Test { viewer { user { ...UserFields @arguments(showName: false) } } }"),
    );
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("file:///query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[
            (schema_id, schema_content, schema_metadata),
            (builtins_id, builtins_content, builtins_metadata),
        ],
        &[
            (frag_id, frag_content, frag_metadata),
            (doc_id, doc_content, doc_metadata),
        ],
    );

    let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);

    // Should NOT contain "is not supported by @arguments" errors
    let arg_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.message.contains("is not supported by `@arguments`"))
        .collect();

    assert!(
        arg_errors.is_empty(),
        "Relay @arguments dynamic args should be suppressed, but got: {:?}",
        arg_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    // Also verify @argumentDefinitions suppression on the fragment file
    let frag_diagnostics = validate_file(&db, frag_content, frag_metadata, project_files);

    let arg_def_errors: Vec<_> = frag_diagnostics
        .iter()
        .filter(|d| {
            d.message
                .contains("is not supported by `@argumentDefinitions`")
        })
        .collect();

    assert!(
        arg_def_errors.is_empty(),
        "Relay @argumentDefinitions dynamic args should be suppressed, but got: {:?}",
        arg_def_errors
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_non_relay_unknown_directive_args_still_reported() {
    // Without Relay builtins, unknown directive args should still be reported
    let mut db = TestDatabase::default();

    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(
        &db,
        Arc::from("type Query { user: User }\ntype User { id: ID! name: String }"),
    );
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("file:///schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(
        &db,
        Arc::from("query Test { user { name @skip(badArg: true) } }"),
    );
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("file:///query.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[(schema_id, schema_content, schema_metadata)],
        &[(doc_id, doc_content, doc_metadata)],
    );

    let diagnostics = validate_file(&db, doc_content, doc_metadata, project_files);

    let arg_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.message.contains("is not supported by `@skip`"))
        .collect();

    assert!(
        !arg_errors.is_empty(),
        "Unknown args on non-Relay directives should still be reported"
    );
}

// ============================================================================
// Issue #650: HashMap-based lookups for field usage analysis
// ============================================================================

/// Regression test for issue #650: verify that analyze_field_usage produces
/// correct results across multiple document files and remains correct after
/// editing a file. The HashMap-based lookup ensures O(1) per-operation file
/// access instead of the previous O(D) linear scan.
#[test]
fn test_issue_650_linear_lookup_in_field_usage() {
    use graphql_base_db::{DocumentFileIds, FileEntry, FileEntryMap, ProjectFiles, SchemaFileIds};
    use graphql_test_utils::{queries, TrackedDatabase};
    use salsa::Setter;

    // Helper to create ProjectFiles for TrackedDatabase (same pattern as hir tests)
    fn create_tracked_project_files(
        db: &TrackedDatabase,
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

    let mut db = TrackedDatabase::new();

    // Schema with multiple types
    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(
        &db,
        Arc::from(
            r"
            type Query {
                user: User
                post: Post
                comment: Comment
            }

            type User {
                id: ID!
                name: String!
                email: String!
            }

            type Post {
                id: ID!
                title: String!
                body: String!
            }

            type Comment {
                id: ID!
                text: String!
            }
            ",
        ),
    );
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    // Document file 1: queries User
    let doc1_id = FileId::new(1);
    let doc1_content = FileContent::new(
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
    let doc1_metadata = FileMetadata::new(
        &db,
        doc1_id,
        FileUri::new("user.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    // Document file 2: queries Post
    let doc2_id = FileId::new(2);
    let doc2_content = FileContent::new(
        &db,
        Arc::from(
            r"
            query GetPost {
                post {
                    id
                    title
                }
            }
            ",
        ),
    );
    let doc2_metadata = FileMetadata::new(
        &db,
        doc2_id,
        FileUri::new("post.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    // Document file 3: queries Comment
    let doc3_id = FileId::new(3);
    let doc3_content = FileContent::new(
        &db,
        Arc::from(
            r"
            query GetComment {
                comment {
                    id
                    text
                }
            }
            ",
        ),
    );
    let doc3_metadata = FileMetadata::new(
        &db,
        doc3_id,
        FileUri::new("comment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_tracked_project_files(
        &db,
        &[(schema_id, schema_content, schema_metadata)],
        &[
            (doc1_id, doc1_content, doc1_metadata),
            (doc2_id, doc2_content, doc2_metadata),
            (doc3_id, doc3_content, doc3_metadata),
        ],
    );

    // Step 1: Run analyze_field_usage and verify correctness
    let coverage = analyze_field_usage(&db, project_files);

    // Total schema fields: Query(user, post, comment) + User(id, name, email) +
    // Post(id, title, body) + Comment(id, text) = 3 + 3 + 3 + 2 = 11
    assert_eq!(coverage.total_fields, 11);

    // Used fields: Query(user, post, comment) + User(id, name) + Post(id, title) +
    // Comment(id, text) = 3 + 2 + 2 + 2 = 9
    assert_eq!(coverage.used_fields, 9);

    // Verify specific field usages
    let user_name = coverage
        .field_usages
        .get(&(Arc::from("User"), Arc::from("name")));
    assert!(user_name.is_some(), "User.name should be tracked");
    assert_eq!(user_name.unwrap().usage_count, 1);
    assert!(user_name
        .unwrap()
        .operations
        .contains(&Arc::from("GetUser")));

    let post_title = coverage
        .field_usages
        .get(&(Arc::from("Post"), Arc::from("title")));
    assert!(post_title.is_some(), "Post.title should be tracked");
    assert_eq!(post_title.unwrap().usage_count, 1);
    assert!(post_title
        .unwrap()
        .operations
        .contains(&Arc::from("GetPost")));

    let comment_text = coverage
        .field_usages
        .get(&(Arc::from("Comment"), Arc::from("text")));
    assert!(comment_text.is_some(), "Comment.text should be tracked");
    assert_eq!(comment_text.unwrap().usage_count, 1);

    // Unused fields should have 0 usage count
    let user_email = coverage
        .field_usages
        .get(&(Arc::from("User"), Arc::from("email")));
    assert!(user_email.is_some(), "User.email should be tracked");
    assert_eq!(user_email.unwrap().usage_count, 0);

    let post_body = coverage
        .field_usages
        .get(&(Arc::from("Post"), Arc::from("body")));
    assert!(post_body.is_some(), "Post.body should be tracked");
    assert_eq!(post_body.unwrap().usage_count, 0);

    // Step 2: Edit one document file (add email to GetUser query)
    let cp = db.checkpoint();

    doc1_content.set_text(&mut db).to(Arc::from(
        r"
        query GetUser {
            user {
                id
                name
                email
            }
        }
        ",
    ));

    // Step 3: Re-run and verify correctness after edit
    let coverage_after = analyze_field_usage(&db, project_files);

    // Total fields unchanged (schema did not change)
    assert_eq!(coverage_after.total_fields, 11);

    // Now User.email is used too: 9 + 1 = 10
    assert_eq!(coverage_after.used_fields, 10);

    // User.email should now have usage_count=1
    let user_email_after = coverage_after
        .field_usages
        .get(&(Arc::from("User"), Arc::from("email")));
    assert!(user_email_after.is_some());
    assert_eq!(user_email_after.unwrap().usage_count, 1);
    assert!(user_email_after
        .unwrap()
        .operations
        .contains(&Arc::from("GetUser")));

    // Post and Comment results should still be correct
    let post_title_after = coverage_after
        .field_usages
        .get(&(Arc::from("Post"), Arc::from("title")));
    assert!(post_title_after.is_some());
    assert_eq!(post_title_after.unwrap().usage_count, 1);

    let comment_text_after = coverage_after
        .field_usages
        .get(&(Arc::from("Comment"), Arc::from("text")));
    assert!(comment_text_after.is_some());
    assert_eq!(comment_text_after.unwrap().usage_count, 1);

    // Verify that file_lookup calls are bounded: with the HashMap optimization,
    // the initial build of the document_files map calls file_lookup once per doc
    // file (3 calls), and no additional linear scans are needed per operation.
    let file_lookup_count = db.count_since(queries::FILE_LOOKUP, cp);
    // With 3 document files, we expect at most 3 file_lookup calls for building
    // the HashMap (one per document file). Without the HashMap fix, the old code
    // would do additional file_lookup calls via linear .iter().find() per operation.
    assert!(
        file_lookup_count <= 3,
        "Expected at most 3 file_lookup calls (one per document file for HashMap construction), \
         got {file_lookup_count}. This suggests linear scanning is still happening."
    );
}

// ============================================================================
// Issue #644: Body edits should not cascade-invalidate validation of other files
// ============================================================================

/// Test that a body-only edit to file A (changing fields in a fragment but
/// NOT its name or type condition) does NOT cause file B's validation to
/// re-execute.
///
/// This tests the full validation path:
/// validate_document_file -> project_fragment_name_index -> file_defined_fragment_names
/// validate_document_file -> project_operation_name_index -> file_operation_names
///
/// When file A has a body-only edit, its file_defined_fragment_names output
/// stays the same (same names), so project_fragment_name_index is backdated,
/// and file B's validate_document_file should NOT re-execute.
#[test]
fn test_issue_644_body_edit_does_not_revalidate_other_files() {
    use graphql_base_db::{DocumentFileIds, FileEntry, FileEntryMap, ProjectFiles, SchemaFileIds};
    use graphql_test_utils::TrackedDatabase;
    use salsa::Setter;

    fn create_tracked_project_files(
        db: &TrackedDatabase,
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

    let mut db = TrackedDatabase::new();

    // Create a schema
    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(
        &db,
        Arc::from("type Query { user: User }\ntype User { id: ID! name: String! }"),
    );
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    // File A: contains a fragment
    let file_a_id = FileId::new(1);
    let file_a_content =
        FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
    let file_a_metadata = FileMetadata::new(
        &db,
        file_a_id,
        FileUri::new("a.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    // File B: contains an operation using the fragment
    let file_b_id = FileId::new(2);
    let file_b_content =
        FileContent::new(&db, Arc::from("query GetUser { user { ...UserFields } }"));
    let file_b_metadata = FileMetadata::new(
        &db,
        file_b_id,
        FileUri::new("b.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_tracked_project_files(
        &db,
        &[(schema_id, schema_content, schema_metadata)],
        &[
            (file_a_id, file_a_content, file_a_metadata),
            (file_b_id, file_b_content, file_b_metadata),
        ],
    );

    // Cold: validate both files
    let _ = validate_document_file(&db, file_a_content, file_a_metadata, project_files);
    let _ = validate_document_file(&db, file_b_content, file_b_metadata, project_files);

    // Now edit file A's body (change field selection, but fragment name unchanged)
    file_a_content
        .set_text(&mut db)
        .to(Arc::from("fragment UserFields on User { id }"));

    let checkpoint = db.checkpoint();

    // Re-validate file B
    let _ = validate_document_file(&db, file_b_content, file_b_metadata, project_files);

    // validate_document_file for file B should NOT re-execute because:
    // - file B's content didn't change
    // - file B depends on project_fragment_name_index and project_operation_name_index
    // - Those indexes depend on per-file queries whose outputs didn't change
    //   (same fragment names, same operation names)
    // - So they get backdated, and file B's validation is served from cache
    let validate_count = db.count_since("validate_document_file", checkpoint);
    assert!(
        validate_count <= 1,
        "Expected validate_document_file to run at most 1 time (for changed file A only), \
         but ran {validate_count} times. \
         This indicates body-only edits are causing unnecessary re-validation of other files."
    );
}

/// Test that a STRUCTURAL edit (renaming a fragment) does NOT cause validation
/// of UNRELATED files to re-execute.
///
/// Setup:
/// - File A: fragment UserFields on User { id }
/// - File B: query GetUser { user { ...UserFields } }  (uses file A's fragment)
/// - File C: query GetStatus { status }                 (unrelated, no fragments)
///
/// When file A renames its fragment (UserFields -> UserInfo), file B's validation
/// should re-execute (it depends on UserFields). But file C should NOT re-validate
/// because it has no dependency on any fragment.
#[test]
fn test_issue_644_structural_edit_only_affects_dependent_files() {
    use graphql_base_db::{DocumentFileIds, FileEntry, FileEntryMap, ProjectFiles, SchemaFileIds};
    use graphql_test_utils::TrackedDatabase;
    use salsa::Setter;

    fn create_tracked_project_files(
        db: &TrackedDatabase,
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

    let mut db = TrackedDatabase::new();

    // Schema
    let schema_id = FileId::new(0);
    let schema_content = FileContent::new(
        &db,
        Arc::from("type Query { user: User status: String }\ntype User { id: ID! name: String! }"),
    );
    let schema_metadata = FileMetadata::new(
        &db,
        schema_id,
        FileUri::new("schema.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    // File A: fragment
    let file_a_id = FileId::new(1);
    let file_a_content = FileContent::new(&db, Arc::from("fragment UserFields on User { id }"));
    let file_a_metadata = FileMetadata::new(
        &db,
        file_a_id,
        FileUri::new("a.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    // File B: operation using fragment
    let file_b_id = FileId::new(2);
    let file_b_content =
        FileContent::new(&db, Arc::from("query GetUser { user { ...UserFields } }"));
    let file_b_metadata = FileMetadata::new(
        &db,
        file_b_id,
        FileUri::new("b.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    // File C: unrelated operation (no fragments)
    let file_c_id = FileId::new(3);
    let file_c_content = FileContent::new(&db, Arc::from("query GetStatus { status }"));
    let file_c_metadata = FileMetadata::new(
        &db,
        file_c_id,
        FileUri::new("c.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let project_files = create_tracked_project_files(
        &db,
        &[(schema_id, schema_content, schema_metadata)],
        &[
            (file_a_id, file_a_content, file_a_metadata),
            (file_b_id, file_b_content, file_b_metadata),
            (file_c_id, file_c_content, file_c_metadata),
        ],
    );

    // Cold: validate all files
    let _ = validate_document_file(&db, file_a_content, file_a_metadata, project_files);
    let _ = validate_document_file(&db, file_b_content, file_b_metadata, project_files);
    let _ = validate_document_file(&db, file_c_content, file_c_metadata, project_files);

    // STRUCTURAL edit: rename fragment in file A
    file_a_content
        .set_text(&mut db)
        .to(Arc::from("fragment UserInfo on User { id }"));

    let checkpoint = db.checkpoint();

    // Re-validate all files
    let _ = validate_document_file(&db, file_a_content, file_a_metadata, project_files);
    let _ = validate_document_file(&db, file_b_content, file_b_metadata, project_files);
    let _ = validate_document_file(&db, file_c_content, file_c_metadata, project_files);

    let validate_count = db.count_since("validate_document_file", checkpoint);

    // File A changed, so it must re-validate (1).
    // File B depends on the fragment name index which changed, so it re-validates (2).
    // File C has NO dependency on fragments, so it should NOT re-validate.
    //
    // With a monolithic fragment index, all 3 files re-validate (validate_count == 3).
    // With proper granular invalidation, only files A and B should re-validate (<= 2).
    assert!(
        validate_count <= 2,
        "Expected validate_document_file to run at most 2 times \
         (for file A which changed and file B which depends on renamed fragment), \
         but ran {validate_count} times. \
         File C (unrelated) should NOT be re-validated after a structural edit to file A."
    );
}
