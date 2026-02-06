//! Integration tests for HIR body extraction.
//!
//! These tests verify operation and fragment body extraction functionality.

use graphql_base_db::{ExtractionOffset, FileContent, FileId, FileKind, FileMetadata, FileUri};
use graphql_hir::{fragment_body, operation_body, operation_transitive_fragments};
use graphql_test_utils::{create_project_files, TestDatabase};
use std::sync::Arc;

#[test]
fn test_operation_body_extraction() {
    let db = TestDatabase::default();
    let file_id = FileId::new(0);
    let content = FileContent::new(&db, Arc::from("query GetUser { user { id name } }"));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("test.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let body = operation_body(&db, content, metadata, 0);
    assert_eq!(body.selections.len(), 1);
    assert!(body.fragment_spreads.is_empty());
}

#[test]
fn test_operation_body_with_fragment_spread() {
    let db = TestDatabase::default();
    let file_id = FileId::new(0);
    let content = FileContent::new(&db, Arc::from("query GetUser { user { ...UserFields } }"));
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("test.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let body = operation_body(&db, content, metadata, 0);
    assert_eq!(body.selections.len(), 1);
    assert!(body.fragment_spreads.contains(&Arc::from("UserFields")));
}

#[test]
fn test_fragment_body_extraction() {
    let db = TestDatabase::default();
    let file_id = FileId::new(0);
    let content = FileContent::new(
        &db,
        Arc::from("fragment UserFields on User { id name email }"),
    );
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("test.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let body = fragment_body(&db, content, metadata, Arc::from("UserFields"));
    assert_eq!(body.selections.len(), 3);
    assert!(body.fragment_spreads.is_empty());
}

#[test]
fn test_fragment_body_with_nested_spread() {
    let db = TestDatabase::default();
    let file_id = FileId::new(0);
    let content = FileContent::new(
        &db,
        Arc::from("fragment UserFields on User { id ...NameFields }"),
    );
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("test.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let body = fragment_body(&db, content, metadata, Arc::from("UserFields"));
    assert_eq!(body.selections.len(), 2);
    assert!(body.fragment_spreads.contains(&Arc::from("NameFields")));
}

#[test]
fn test_variable_usage_extraction() {
    let db = TestDatabase::default();
    let file_id = FileId::new(0);
    let content = FileContent::new(
        &db,
        Arc::from("query GetUser($id: ID!) { user(id: $id) { name } }"),
    );
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("test.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let body = operation_body(&db, content, metadata, 0);
    assert!(body.variable_usages.contains(&Arc::from("id")));
}

#[test]
fn test_transitive_fragments() {
    let mut db = TestDatabase::default();
    let file_id = FileId::new(0);

    let content = FileContent::new(
        &db,
        Arc::from(
            "
            query GetUser { user { ...FragA } }
            fragment FragA on User { id ...FragB }
            fragment FragB on User { name }
            ",
        ),
    );
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("test.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let project_files = create_project_files(&mut db, &[], &[(file_id, content, metadata)]);

    let transitive = operation_transitive_fragments(&db, content, metadata, 0, project_files);

    assert!(transitive.contains(&Arc::from("FragA")));
    assert!(transitive.contains(&Arc::from("FragB")));
}
