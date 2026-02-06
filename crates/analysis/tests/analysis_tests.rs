//! Integration tests for graphql-analysis.
//!
//! These tests verify validation, schema merging, and document validation.

use graphql_analysis::{
    analyze_field_usage, file_diagnostics, file_validation_diagnostics,
    merged_schema::merged_schema_with_diagnostics, validate_document_file, validate_file,
    FieldCoverageReport, TypeCoverage,
};
use graphql_base_db::{ExtractionOffset, FileContent, FileId, FileKind, FileMetadata, FileUri};
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("fragment.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(&db, Arc::from("query { world }"));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("query.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
    );

    let doc_id = FileId::new(1);
    let doc_content = FileContent::new(&db, Arc::from("query { hello }"));
    let doc_metadata = FileMetadata::new(
        &db,
        doc_id,
        FileUri::new("query.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
    );

    let frag_id = FileId::new(1);
    let frag_content = FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
    let frag_metadata = FileMetadata::new(
        &db,
        frag_id,
        FileUri::new("fragments.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let query_id = FileId::new(2);
    let query_content = FileContent::new(&db, Arc::from("query { user { ...UserFields } }"));
    let query_metadata = FileMetadata::new(
        &db,
        query_id,
        FileUri::new("query.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
    );

    let file2_id = FileId::new(1);
    let content2 = FileContent::new(&db, Arc::from("type User { id: ID! name: String! }"));
    let metadata2 = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("schema2.graphql"),
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
    );

    let file2_id = FileId::new(1);
    let content2 = FileContent::new(&db, Arc::from("extend type Query { world: String }"));
    let metadata2 = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("schema2.graphql"),
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
    );

    let file_id2 = FileId::new(1);
    let content2 = FileContent::new(&db, Arc::from("type Query { world: String }"));
    let metadata2 = FileMetadata::new(
        &db,
        file_id2,
        FileUri::new("duplicate.graphql"),
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
