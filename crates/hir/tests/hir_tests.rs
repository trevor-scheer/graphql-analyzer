//! Integration tests for graphql-hir.
//!
//! These tests verify HIR construction, fragment resolution, schema types,
//! and incremental computation behavior.

use graphql_base_db::{DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language};
use graphql_hir::{
    all_fragments, file_defined_fragment_names, file_operation_names, file_structure,
    file_used_fragment_names, fragment_source, schema_types,
};
use graphql_test_utils::{create_project_files, TestDatabase};
use salsa::Setter;
use std::sync::Arc;

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
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("test.graphql"),
        Language::GraphQL,
        DocumentKind::Schema,
    );

    let structure = file_structure(&db, file_id, content, metadata);
    assert_eq!(structure.type_defs.len(), 1);
    assert_eq!(structure.type_defs[0].name.as_ref(), "User");
}

#[test]
fn test_all_fragments_granular_invalidation() {
    let mut db = TestDatabase::default();

    let file1_id = FileId::new(0);
    let file1_content = FileContent::new(&db, Arc::from("fragment F1 on User { id }"));
    let file1_metadata = FileMetadata::new(
        &db,
        file1_id,
        FileUri::new("f1.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let file2_id = FileId::new(1);
    let file2_content = FileContent::new(&db, Arc::from("fragment F2 on User { name }"));
    let file2_metadata = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("f2.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let doc_files = [
        (file1_id, file1_content, file1_metadata),
        (file2_id, file2_content, file2_metadata),
    ];
    let project_files = create_project_files(&mut db, &[], &doc_files);

    let frags1 = all_fragments(&db, project_files);
    assert_eq!(frags1.len(), 2);
    assert!(frags1.contains_key("F1"));
    assert!(frags1.contains_key("F2"));

    file2_content
        .set_text(&mut db)
        .to(Arc::from("fragment F2 on User { name email }"));

    let frags2 = all_fragments(&db, project_files);
    assert_eq!(frags2.len(), 2);
    assert!(frags2.contains_key("F1"), "F1 should still exist");
    assert!(frags2.contains_key("F2"), "F2 should still exist");
}

#[test]
fn test_fragment_source_per_fragment_lookup() {
    let mut db = TestDatabase::default();

    let file1_id = FileId::new(0);
    let file1_content = FileContent::new(&db, Arc::from("fragment UserFields on User { id name }"));
    let file1_metadata = FileMetadata::new(
        &db,
        file1_id,
        FileUri::new("user-fragment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let file2_id = FileId::new(1);
    let file2_content =
        FileContent::new(&db, Arc::from("fragment PostFields on Post { title body }"));
    let file2_metadata = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("post-fragment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let doc_files = [
        (file1_id, file1_content, file1_metadata),
        (file2_id, file2_content, file2_metadata),
    ];
    let project_files = create_project_files(&mut db, &[], &doc_files);

    let user_source = fragment_source(&db, project_files, Arc::from("UserFields"));
    let post_source = fragment_source(&db, project_files, Arc::from("PostFields"));
    let nonexistent = fragment_source(&db, project_files, Arc::from("NonExistent"));

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

    let file1_id = FileId::new(0);
    let file1_content = FileContent::new(&db, Arc::from("fragment UserFields on User { id }"));
    let file1_metadata = FileMetadata::new(
        &db,
        file1_id,
        FileUri::new("user-fragment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let file2_id = FileId::new(1);
    let file2_content = FileContent::new(&db, Arc::from("fragment PostFields on Post { title }"));
    let file2_metadata = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("post-fragment.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

    let doc_files = [
        (file1_id, file1_content, file1_metadata),
        (file2_id, file2_content, file2_metadata),
    ];
    let project_files = create_project_files(&mut db, &[], &doc_files);

    let user_source_1 = fragment_source(&db, project_files, Arc::from("UserFields"));
    let post_source_1 = fragment_source(&db, project_files, Arc::from("PostFields"));

    assert!(user_source_1.is_some());
    assert!(post_source_1.is_some());

    file2_content
        .set_text(&mut db)
        .to(Arc::from("fragment PostFields on Post { title body }"));

    let user_source_2 = fragment_source(&db, project_files, Arc::from("UserFields"));
    let post_source_2 = fragment_source(&db, project_files, Arc::from("PostFields"));

    assert_eq!(
        user_source_1.as_ref().map(AsRef::as_ref),
        user_source_2.as_ref().map(AsRef::as_ref),
        "UserFields source should be unchanged"
    );

    assert!(
        post_source_2.as_ref().unwrap().contains("body"),
        "PostFields should be updated with 'body' field"
    );
}

#[test]
fn test_file_structure_finds_fragments_in_typescript() {
    let db = TestDatabase::default();
    let file_id = FileId::new(100);

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
    let metadata = FileMetadata::new(
        &db,
        file_id,
        FileUri::new("test.ts"),
        Language::TypeScript,
        DocumentKind::Executable,
    );

    let structure = file_structure(&db, file_id, content, metadata);

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

    let graphql_file_id = FileId::new(1);
    let graphql_content =
        FileContent::new(&db, Arc::from("fragment GraphQLFragment on User { id }"));
    let graphql_metadata = FileMetadata::new(
        &db,
        graphql_file_id,
        FileUri::new("test.graphql"),
        Language::GraphQL,
        DocumentKind::Executable,
    );

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
        Language::TypeScript,
        DocumentKind::Executable,
    );

    let project_files = create_project_files(
        &mut db,
        &[],
        &[
            (graphql_file_id, graphql_content, graphql_metadata),
            (ts_file_id, ts_content, ts_metadata),
        ],
    );

    let fragments = all_fragments(&db, project_files);

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

// ============================================================================
// Caching verification tests using TrackedDatabase
// ============================================================================

/// Tests for `file_schema_coordinates` query which tracks field usage across files
mod schema_coordinates_tests {
    use graphql_hir::file_schema_coordinates;

    #[test]
    fn test_file_schema_coordinates_includes_fragment_spread_fields() {
        // This test verifies that fields used through fragment spreads are correctly
        // tracked by file_schema_coordinates. The bug was that fragment spreads
        // were being skipped, causing false "unused field" lint warnings.
        //
        // Scenario: An operation uses a fragment spread, and the fragment selects
        // fields from a type. Those fields should be tracked as "used" even though
        // they're not directly in the operation's selection set.
        let project = graphql_test_utils::TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r"
                    type Query { rateLimit: RateLimit }
                    type RateLimit {
                        cost: Int!
                        limit: Int!
                        remaining: Int!
                        nodeCount: Int!
                    }
                ",
            )
            // Fragment defines fields to select
            .with_document(
                "fragment.graphql",
                r"
                    fragment RateLimitFields on RateLimit {
                        cost
                        limit
                        remaining
                        nodeCount
                    }
                ",
            )
            // Operation uses the fragment spread
            .with_document(
                "operation.graphql",
                r"
                    query GetRateLimit {
                        rateLimit {
                            ...RateLimitFields
                        }
                    }
                ",
            )
            .build_detailed();

        // Collect coordinates from the operation file
        let op_file = &project.documents[1]; // operation.graphql
        let coords = file_schema_coordinates(
            &project.db,
            op_file.id,
            op_file.content,
            op_file.metadata,
            project.project_files,
        );

        // The operation file directly uses Query.rateLimit
        assert!(
            coords
                .iter()
                .any(|c| c.type_name.as_ref() == "Query" && c.field_name.as_ref() == "rateLimit"),
            "Should track Query.rateLimit. Got: {coords:?}"
        );

        // The operation file uses RateLimit.nodeCount via fragment spread.
        // This is the key assertion that should FAIL before the fix.
        // The fragment spread `...RateLimitFields` references fields that
        // should be tracked as used by the operation.
        assert!(
            coords.iter().any(
                |c| c.type_name.as_ref() == "RateLimit" && c.field_name.as_ref() == "nodeCount"
            ),
            "Should track RateLimit.nodeCount used via fragment spread. Got: {coords:?}"
        );
    }

    #[test]
    fn test_file_schema_coordinates_includes_nested_fragment_spread_fields() {
        // Test transitive fragment dependencies: A -> B -> C
        // Fields in C should be tracked when A uses B which uses C.
        let project = graphql_test_utils::TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r"
                    type Query { user: User }
                    type User {
                        id: ID!
                        name: String!
                        profile: Profile
                    }
                    type Profile {
                        bio: String
                        avatar: String
                    }
                ",
            )
            .with_document(
                "profile-fragment.graphql",
                r"
                    fragment ProfileFields on Profile {
                        bio
                        avatar
                    }
                ",
            )
            .with_document(
                "user-fragment.graphql",
                r"
                    fragment UserWithProfile on User {
                        id
                        name
                        profile {
                            ...ProfileFields
                        }
                    }
                ",
            )
            .with_document(
                "query.graphql",
                r"
                    query GetUser {
                        user {
                            ...UserWithProfile
                        }
                    }
                ",
            )
            .build_detailed();

        // The query file uses fragments transitively
        let query_file = &project.documents[2]; // query.graphql
        let coords = file_schema_coordinates(
            &project.db,
            query_file.id,
            query_file.content,
            query_file.metadata,
            project.project_files,
        );

        // Query.user should be tracked (directly in operation)
        assert!(
            coords
                .iter()
                .any(|c| c.type_name.as_ref() == "Query" && c.field_name.as_ref() == "user"),
            "Should track Query.user"
        );

        // These should be tracked via transitive fragment spreads
        // First level: UserWithProfile fragment
        assert!(
            coords
                .iter()
                .any(|c| c.type_name.as_ref() == "User" && c.field_name.as_ref() == "id"),
            "Should track User.id via fragment spread. Got: {coords:?}"
        );

        // Second level (nested): ProfileFields fragment within UserWithProfile
        assert!(
            coords
                .iter()
                .any(|c| c.type_name.as_ref() == "Profile" && c.field_name.as_ref() == "bio"),
            "Should track Profile.bio via nested fragment spread. Got: {coords:?}"
        );
    }
}

// ============================================================================
// Schema type extension tests
// ============================================================================

mod type_extension_tests {
    use graphql_hir::{schema_types, TypeDefKind};
    use graphql_test_utils::TestProjectBuilder;

    #[test]
    fn test_object_type_extension_merges_fields() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                "type Query {\n  user: User\n}\ntype User {\n  id: ID!\n}",
            )
            .with_schema(
                "client-schema.graphql",
                "extend type Query {\n  isLoggedIn: Boolean!\n  cartItems: Int!\n}",
            )
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query type should exist");

        assert_eq!(query_type.kind, TypeDefKind::Object);
        let field_names: Vec<&str> = query_type.fields.iter().map(|f| f.name.as_ref()).collect();
        assert!(
            field_names.contains(&"user"),
            "Should have base field 'user'"
        );
        assert!(
            field_names.contains(&"isLoggedIn"),
            "Should have extension field 'isLoggedIn'"
        );
        assert!(
            field_names.contains(&"cartItems"),
            "Should have extension field 'cartItems'"
        );
        assert_eq!(field_names.len(), 3, "Should have exactly 3 fields");
    }

    #[test]
    fn test_interface_type_extension_merges_fields() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema("schema.graphql", "interface Node {\n  id: ID!\n}")
            .with_schema(
                "ext.graphql",
                "extend interface Node {\n  createdAt: String!\n}",
            )
            .build();

        let types = schema_types(&db, project);
        let node_type = types.get("Node").expect("Node type should exist");

        assert_eq!(node_type.kind, TypeDefKind::Interface);
        let field_names: Vec<&str> = node_type.fields.iter().map(|f| f.name.as_ref()).collect();
        assert!(field_names.contains(&"id"), "Should have base field 'id'");
        assert!(
            field_names.contains(&"createdAt"),
            "Should have extension field 'createdAt'"
        );
    }

    #[test]
    fn test_enum_type_extension_merges_values() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema("schema.graphql", "enum Status {\n  ACTIVE\n  INACTIVE\n}")
            .with_schema(
                "ext.graphql",
                "extend enum Status {\n  PENDING\n  ARCHIVED\n}",
            )
            .build();

        let types = schema_types(&db, project);
        let status_type = types.get("Status").expect("Status type should exist");

        assert_eq!(status_type.kind, TypeDefKind::Enum);
        let value_names: Vec<&str> = status_type
            .enum_values
            .iter()
            .map(|v| v.name.as_ref())
            .collect();
        assert!(value_names.contains(&"ACTIVE"), "Should have base value");
        assert!(value_names.contains(&"INACTIVE"), "Should have base value");
        assert!(
            value_names.contains(&"PENDING"),
            "Should have extension value 'PENDING'"
        );
        assert!(
            value_names.contains(&"ARCHIVED"),
            "Should have extension value 'ARCHIVED'"
        );
        assert_eq!(value_names.len(), 4, "Should have exactly 4 values");
    }

    #[test]
    fn test_union_type_extension_merges_members() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                "type Cat { name: String! }\ntype Dog { name: String! }\ntype Bird { name: String! }\nunion Animal = Cat | Dog",
            )
            .with_schema("ext.graphql", "extend union Animal = Bird")
            .build();

        let types = schema_types(&db, project);
        let animal_type = types.get("Animal").expect("Animal type should exist");

        assert_eq!(animal_type.kind, TypeDefKind::Union);
        let member_names: Vec<&str> = animal_type
            .union_members
            .iter()
            .map(AsRef::as_ref)
            .collect();
        assert!(member_names.contains(&"Cat"), "Should have base member");
        assert!(member_names.contains(&"Dog"), "Should have base member");
        assert!(
            member_names.contains(&"Bird"),
            "Should have extension member 'Bird'"
        );
        assert_eq!(member_names.len(), 3, "Should have exactly 3 members");
    }

    #[test]
    fn test_input_object_type_extension_merges_fields() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                "input CreateUserInput {\n  name: String!\n  email: String!\n}",
            )
            .with_schema(
                "ext.graphql",
                "extend input CreateUserInput {\n  avatar: String\n}",
            )
            .build();

        let types = schema_types(&db, project);
        let input_type = types
            .get("CreateUserInput")
            .expect("CreateUserInput type should exist");

        assert_eq!(input_type.kind, TypeDefKind::InputObject);
        let field_names: Vec<&str> = input_type.fields.iter().map(|f| f.name.as_ref()).collect();
        assert!(field_names.contains(&"name"), "Should have base field");
        assert!(field_names.contains(&"email"), "Should have base field");
        assert!(
            field_names.contains(&"avatar"),
            "Should have extension field 'avatar'"
        );
        assert_eq!(field_names.len(), 3, "Should have exactly 3 fields");
    }

    #[test]
    fn test_extension_implements_interface() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                "interface Node { id: ID! }\ntype User { id: ID! name: String! }",
            )
            .with_schema("ext.graphql", "extend type User implements Node")
            .build();

        let types = schema_types(&db, project);
        let user_type = types.get("User").expect("User type should exist");

        assert!(
            user_type.implements.iter().any(|i| i.as_ref() == "Node"),
            "User should implement Node via extension"
        );
    }

    #[test]
    fn test_extension_without_base_type_creates_standalone() {
        // Extension without a base type definition - should still appear in schema_types
        let (db, project) = TestProjectBuilder::new()
            .with_schema("ext.graphql", "extend type Query {\n  hello: String!\n}")
            .build();

        let types = schema_types(&db, project);
        let query_type = types
            .get("Query")
            .expect("Query should exist from extension");

        assert_eq!(query_type.fields.len(), 1);
        assert_eq!(query_type.fields[0].name.as_ref(), "hello");
    }

    #[test]
    fn test_multiple_extensions_for_same_type() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema("schema.graphql", "type Query {\n  user: String\n}")
            .with_schema(
                "ext1.graphql",
                "extend type Query {\n  isLoggedIn: Boolean!\n}",
            )
            .with_schema("ext2.graphql", "extend type Query {\n  cartItems: Int!\n}")
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query should exist");

        let field_names: Vec<&str> = query_type.fields.iter().map(|f| f.name.as_ref()).collect();
        assert!(field_names.contains(&"user"));
        assert!(field_names.contains(&"isLoggedIn"));
        assert!(field_names.contains(&"cartItems"));
        assert_eq!(field_names.len(), 3);
    }

    #[test]
    fn test_duplicate_extension_field_not_doubled() {
        // If an extension duplicates a base field name, it should not be added twice
        let (db, project) = TestProjectBuilder::new()
            .with_schema("schema.graphql", "type Query {\n  user: String\n}")
            .with_schema("ext.graphql", "extend type Query {\n  user: Int\n}")
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query should exist");

        let user_fields: Vec<_> = query_type
            .fields
            .iter()
            .filter(|f| f.name.as_ref() == "user")
            .collect();
        assert_eq!(user_fields.len(), 1, "Should not duplicate field 'user'");
    }
}

mod caching_tests {
    use super::*;
    use graphql_test_utils::tracking::{queries, TrackedDatabase};
    use salsa::Setter;

    fn create_tracked_project_files(
        db: &TrackedDatabase,
        schema_files: &[(FileId, FileContent, FileMetadata)],
        document_files: &[(FileId, FileContent, FileMetadata)],
    ) -> graphql_base_db::ProjectFiles {
        use graphql_base_db::{DocumentFileIds, FileEntry, FileEntryMap, SchemaFileIds};
        use std::collections::HashMap;

        let schema_ids: Vec<FileId> = schema_files.iter().map(|(id, _, _)| *id).collect();
        let doc_ids: Vec<FileId> = document_files.iter().map(|(id, _, _)| *id).collect();

        let mut entries = HashMap::new();
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

        graphql_base_db::ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_cache_hit_on_repeated_query() {
        let db = TrackedDatabase::new();

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("test.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let schema_files = [(file_id, content, metadata)];
        let project_files = create_tracked_project_files(&db, &schema_files, &[]);

        let checkpoint = db.checkpoint();
        let types1 = schema_types(&db, project_files);
        assert_eq!(types1.len(), 1);

        let cold_count = db.count_since(queries::SCHEMA_TYPES, checkpoint);
        assert!(
            cold_count >= 1,
            "First query should execute schema_types at least once, got {cold_count}"
        );

        let checkpoint2 = db.checkpoint();
        let types2 = schema_types(&db, project_files);
        assert_eq!(types2.len(), 1);

        let warm_count = db.count_since(queries::SCHEMA_TYPES, checkpoint2);
        assert_eq!(
            warm_count, 0,
            "Second query should NOT re-execute schema_types (cached)"
        );
    }

    #[test]
    fn test_granular_caching_editing_one_file() {
        let mut db = TrackedDatabase::new();

        let first_id = FileId::new(0);
        let first_content = FileContent::new(&db, Arc::from("type TypeA { id: ID! }"));
        let first_metadata = FileMetadata::new(
            &db,
            first_id,
            FileUri::new("a.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let second_id = FileId::new(1);
        let second_content = FileContent::new(&db, Arc::from("type TypeB { id: ID! }"));
        let second_metadata = FileMetadata::new(
            &db,
            second_id,
            FileUri::new("b.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let schema_files = [
            (first_id, first_content, first_metadata),
            (second_id, second_content, second_metadata),
        ];
        let project_files = create_tracked_project_files(&db, &schema_files, &[]);

        let types = schema_types(&db, project_files);
        assert_eq!(types.len(), 2);
        assert!(types.contains_key("TypeA"));
        assert!(types.contains_key("TypeB"));

        let checkpoint = db.checkpoint();

        first_content
            .set_text(&mut db)
            .to(Arc::from("type TypeA { id: ID! name: String }"));

        let types_after = schema_types(&db, project_files);
        assert_eq!(types_after.len(), 2);

        let parse_count = db.count_since(queries::PARSE, checkpoint);
        let file_structure_count = db.count_since(queries::FILE_STRUCTURE, checkpoint);

        assert!(
            parse_count <= 2,
            "Expected ~1 parse call (for edited file), got {parse_count}"
        );
        assert!(
            file_structure_count <= 2,
            "Expected ~1 file_structure call (for edited file), got {file_structure_count}"
        );
    }

    #[test]
    fn test_unrelated_file_edit_doesnt_invalidate_schema() {
        let mut db = TrackedDatabase::new();

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

        let project_files = create_tracked_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[(doc_id, doc_content, doc_metadata)],
        );

        let types = schema_types(&db, project_files);
        assert_eq!(types.len(), 1);

        let checkpoint = db.checkpoint();

        doc_content
            .set_text(&mut db)
            .to(Arc::from("query { hello world }"));

        let types_after = schema_types(&db, project_files);
        assert_eq!(types_after.len(), 1);

        let schema_types_count = db.count_since(queries::SCHEMA_TYPES, checkpoint);
        assert_eq!(
            schema_types_count, 0,
            "Editing a document file should NOT invalidate schema_types query"
        );
    }

    #[test]
    fn test_editing_one_of_many_files_is_o1_not_on() {
        const NUM_FILES: usize = 10;
        let mut db = TrackedDatabase::new();

        let mut schema_files = Vec::with_capacity(NUM_FILES);
        let mut file_contents = Vec::with_capacity(NUM_FILES);

        for i in 0..NUM_FILES {
            let file_id = FileId::new(u32::try_from(i).expect("NUM_FILES fits in u32"));
            let type_name = format!("Type{i}");
            let content_str = format!("type {type_name} {{ id: ID! }}");
            let content = FileContent::new(&db, Arc::from(content_str.as_str()));
            let uri = format!("file{i}.graphql");
            let metadata = FileMetadata::new(
                &db,
                file_id,
                FileUri::new(uri),
                Language::GraphQL,
                DocumentKind::Schema,
            );

            file_contents.push(content);
            schema_files.push((file_id, content, metadata));
        }

        let project_files = create_tracked_project_files(&db, &schema_files, &[]);

        let types = schema_types(&db, project_files);
        assert_eq!(types.len(), NUM_FILES);

        let checkpoint = db.checkpoint();

        file_contents[0]
            .set_text(&mut db)
            .to(Arc::from("type Type0 { id: ID! name: String }"));

        let types_after = schema_types(&db, project_files);
        assert_eq!(types_after.len(), NUM_FILES);

        let parse_delta = db.count_since(queries::PARSE, checkpoint);
        let file_structure_delta = db.count_since(queries::FILE_STRUCTURE, checkpoint);

        let max_allowed = NUM_FILES / 2;
        assert!(
            parse_delta <= max_allowed,
            "Expected O(1) parse calls, got {parse_delta} (O(N) would be ~{NUM_FILES})"
        );
        assert!(
            file_structure_delta <= max_allowed,
            "Expected O(1) file_structure calls, got {file_structure_delta} (O(N) would be ~{NUM_FILES})"
        );
    }

    #[test]
    fn test_golden_invariant_schema_stable_across_operation_edits() {
        let mut db = TrackedDatabase::new();

        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from(
                "type Query { users: [User!]! } type User { id: ID! name: String! email: String }",
            ),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let op1_id = FileId::new(1);
        let op1_content = FileContent::new(&db, Arc::from("query GetUsers { users { id } }"));
        let op1_metadata = FileMetadata::new(
            &db,
            op1_id,
            FileUri::new("op1.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let op2_id = FileId::new(2);
        let op2_content = FileContent::new(&db, Arc::from("query GetUserNames { users { name } }"));
        let op2_metadata = FileMetadata::new(
            &db,
            op2_id,
            FileUri::new("op2.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );

        let project_files = create_tracked_project_files(
            &db,
            &[(schema_id, schema_content, schema_metadata)],
            &[
                (op1_id, op1_content, op1_metadata),
                (op2_id, op2_content, op2_metadata),
            ],
        );

        let types = schema_types(&db, project_files);
        assert_eq!(types.len(), 2);

        let checkpoint = db.checkpoint();

        op1_content
            .set_text(&mut db)
            .to(Arc::from("query GetUsers { users { id name } }"));
        op2_content
            .set_text(&mut db)
            .to(Arc::from("query GetUserNames { users { name email } }"));

        let types_after = schema_types(&db, project_files);
        assert_eq!(types_after.len(), 2);

        let schema_types_delta = db.count_since(queries::SCHEMA_TYPES, checkpoint);
        let file_type_defs_delta = db.count_since(queries::FILE_TYPE_DEFS, checkpoint);

        assert_eq!(
            schema_types_delta, 0,
            "GOLDEN INVARIANT VIOLATED: schema_types ran {schema_types_delta} times after operation edit"
        );

        assert_eq!(
            file_type_defs_delta, 0,
            "file_type_defs should be cached when operations are edited, got {file_type_defs_delta}"
        );
    }

    #[test]
    fn test_per_file_contribution_queries_incremental() {
        const NUM_FILES: usize = 5;
        let mut db = TrackedDatabase::new();

        let mut doc_files = Vec::with_capacity(NUM_FILES);
        let mut file_contents = Vec::with_capacity(NUM_FILES);

        for i in 0..NUM_FILES {
            let file_id = FileId::new(u32::try_from(i).expect("NUM_FILES fits in u32"));
            let fragment_name = format!("Fragment{i}");
            let content_str = format!(
                "fragment {fragment_name} on User {{ id }} query Q{i} {{ user {{ ...{fragment_name} }} }}"
            );
            let content = FileContent::new(&db, Arc::from(content_str.as_str()));
            let uri = format!("file{i}.graphql");
            let metadata = FileMetadata::new(
                &db,
                file_id,
                FileUri::new(uri),
                Language::GraphQL,
                DocumentKind::Executable,
            );

            file_contents.push(content);
            doc_files.push((file_id, content, metadata));
        }

        let _project_files = create_tracked_project_files(&db, &[], &doc_files);

        for (file_id, content, metadata) in &doc_files {
            let _ = file_defined_fragment_names(&db, *file_id, *content, *metadata);
            let _ = file_used_fragment_names(&db, *file_id, *content, *metadata);
            let _ = file_operation_names(&db, *file_id, *content, *metadata);
        }

        let checkpoint = db.checkpoint();

        file_contents[0].set_text(&mut db).to(Arc::from(
            "fragment Fragment0 on User { id name } query Q0 { user { ...Fragment0 } }",
        ));

        for (file_id, content, metadata) in &doc_files {
            let _ = file_defined_fragment_names(&db, *file_id, *content, *metadata);
            let _ = file_used_fragment_names(&db, *file_id, *content, *metadata);
            let _ = file_operation_names(&db, *file_id, *content, *metadata);
        }

        let defined_delta = db.count_since(queries::FILE_DEFINED_FRAGMENT_NAMES, checkpoint);
        let used_delta = db.count_since(queries::FILE_USED_FRAGMENT_NAMES, checkpoint);
        let op_names_delta = db.count_since(queries::FILE_OPERATION_NAMES, checkpoint);

        let max_allowed = NUM_FILES / 2;
        assert!(
            defined_delta <= max_allowed,
            "Expected O(1) file_defined_fragment_names calls, got {defined_delta} (O(N) would be ~{NUM_FILES})"
        );
        assert!(
            used_delta <= max_allowed,
            "Expected O(1) file_used_fragment_names calls, got {used_delta} (O(N) would be ~{NUM_FILES})"
        );
        assert!(
            op_names_delta <= max_allowed,
            "Expected O(1) file_operation_names calls, got {op_names_delta} (O(N) would be ~{NUM_FILES})"
        );
    }

    #[test]
    fn test_executions_since_for_debugging() {
        let db = TrackedDatabase::new();

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("test.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let schema_files = [(file_id, content, metadata)];
        let project_files = create_tracked_project_files(&db, &schema_files, &[]);

        let checkpoint = db.checkpoint();
        let _ = schema_types(&db, project_files);

        let executions = db.executions_since(checkpoint);

        assert!(
            !executions.is_empty(),
            "Should have recorded query executions"
        );

        let has_schema_types = executions.iter().any(|q| q == queries::SCHEMA_TYPES);
        assert!(
            has_schema_types,
            "Executions should include schema_types: {executions:?}"
        );
    }

    #[test]
    fn test_issue_649_interface_implementors_cached() {
        let db = TrackedDatabase::new();

        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from(
                r"
            interface Node {
                id: ID!
            }

            type User implements Node {
                id: ID!
                name: String!
            }

            type Post implements Node {
                id: ID!
                title: String!
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

        let project_files =
            create_tracked_project_files(&db, &[(schema_id, schema_content, schema_metadata)], &[]);

        // First call should execute
        let implementors = graphql_hir::interface_implementors(&db, project_files);
        assert!(implementors.contains_key(&Arc::from("Node") as &Arc<str>));
        let node_impls = implementors.get(&Arc::from("Node") as &Arc<str>).unwrap();
        assert_eq!(node_impls.len(), 2);

        let checkpoint = db.checkpoint();

        // Second call should be cached (no re-execution)
        let _implementors2 = graphql_hir::interface_implementors(&db, project_files);
        let exec_count = db.count_since(queries::INTERFACE_IMPLEMENTORS, checkpoint);
        assert_eq!(
            exec_count, 0,
            "interface_implementors should be cached on second call"
        );
    }
}

// ============================================================================
// Directive extraction tests
// ============================================================================

mod directive_tests {
    use graphql_hir::{file_structure, schema_types, TypeDefKind};
    use graphql_test_utils::TestProjectBuilder;

    #[test]
    fn test_type_directives_extracted() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r"type Query @auth(requires: ADMIN) { hello: String }",
            )
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query type should exist");

        assert_eq!(query_type.directives.len(), 1);
        assert_eq!(query_type.directives[0].name.as_ref(), "auth");
        assert_eq!(query_type.directives[0].arguments.len(), 1);
        assert_eq!(
            query_type.directives[0].arguments[0].name.as_ref(),
            "requires"
        );
        assert_eq!(
            query_type.directives[0].arguments[0].value.as_ref(),
            "ADMIN"
        );
    }

    #[test]
    fn test_field_directives_extracted() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r#"type Query { secret: String @deprecated(reason: "use newSecret") }"#,
            )
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query type should exist");
        let field = &query_type.fields[0];

        assert!(field.is_deprecated);
        assert_eq!(field.directives.len(), 1);
        assert_eq!(field.directives[0].name.as_ref(), "deprecated");
    }

    #[test]
    fn test_enum_value_directives_extracted() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r#"enum Status { ACTIVE INACTIVE @deprecated(reason: "no longer used") }"#,
            )
            .build();

        let types = schema_types(&db, project);
        let status = types.get("Status").expect("Status type should exist");

        let inactive = status
            .enum_values
            .iter()
            .find(|v| v.name.as_ref() == "INACTIVE")
            .expect("INACTIVE should exist");

        assert!(inactive.is_deprecated);
        assert_eq!(inactive.directives.len(), 1);
        assert_eq!(inactive.directives[0].name.as_ref(), "deprecated");
    }

    #[test]
    fn test_argument_directives_extracted() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r"type Query { users(limit: Int @deprecated): [String] }",
            )
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query type should exist");
        let arg = &query_type.fields[0].arguments[0];

        assert!(arg.is_deprecated);
        assert_eq!(arg.directives.len(), 1);
        assert_eq!(arg.directives[0].name.as_ref(), "deprecated");
    }

    #[test]
    fn test_scalar_type_directives_extracted() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r#"scalar DateTime @specifiedBy(url: "https://scalars.graphql.org/andimarek/date-time")"#,
            )
            .build();

        let types = schema_types(&db, project);
        let datetime = types.get("DateTime").expect("DateTime type should exist");

        assert_eq!(datetime.kind, TypeDefKind::Scalar);
        assert_eq!(datetime.directives.len(), 1);
        assert_eq!(datetime.directives[0].name.as_ref(), "specifiedBy");
    }

    #[test]
    fn test_extension_directives_merged() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema("schema.graphql", r"type Query @auth { hello: String }")
            .with_schema(
                "ext.graphql",
                r"extend type Query @rateLimit { world: String }",
            )
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query type should exist");

        let directive_names: Vec<&str> = query_type
            .directives
            .iter()
            .map(|d| d.name.as_ref())
            .collect();
        assert!(
            directive_names.contains(&"auth"),
            "Should have base directive"
        );
        assert!(
            directive_names.contains(&"rateLimit"),
            "Should have extension directive"
        );
    }

    #[test]
    fn test_scalar_extension_extracted() {
        use graphql_base_db::{DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language};

        let db = graphql_test_utils::TestDatabase::default();
        let file_id = FileId::new(0);
        let content = FileContent::new(
            &db,
            std::sync::Arc::from("extend scalar JSON @specifiedBy(url: \"https://json.org\")"),
        );
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("ext.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let structure = file_structure(&db, file_id, content, metadata);
        assert_eq!(structure.type_defs.len(), 1);
        assert_eq!(structure.type_defs[0].name.as_ref(), "JSON");
        assert_eq!(structure.type_defs[0].kind, TypeDefKind::Scalar);
        assert!(structure.type_defs[0].is_extension);
        assert_eq!(structure.type_defs[0].directives.len(), 1);
        assert_eq!(
            structure.type_defs[0].directives[0].name.as_ref(),
            "specifiedBy"
        );
    }

    #[test]
    fn test_scalar_extension_merges_directives() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema("schema.graphql", r"scalar JSON")
            .with_schema(
                "ext.graphql",
                r#"extend scalar JSON @specifiedBy(url: "https://json.org")"#,
            )
            .build();

        let types = schema_types(&db, project);
        let json_type = types.get("JSON").expect("JSON type should exist");

        assert_eq!(json_type.kind, TypeDefKind::Scalar);
        assert!(
            !json_type.is_extension,
            "Merged type should be the base type"
        );
        assert_eq!(json_type.directives.len(), 1);
        assert_eq!(json_type.directives[0].name.as_ref(), "specifiedBy");
    }

    #[test]
    fn test_multiple_directives_on_type() {
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r"type Query @auth @rateLimit(max: 100) { hello: String }",
            )
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query type should exist");

        assert_eq!(query_type.directives.len(), 2);
        assert_eq!(query_type.directives[0].name.as_ref(), "auth");
        assert_eq!(query_type.directives[0].arguments.len(), 0);
        assert_eq!(query_type.directives[1].name.as_ref(), "rateLimit");
        assert_eq!(query_type.directives[1].arguments.len(), 1);
        assert_eq!(query_type.directives[1].arguments[0].name.as_ref(), "max");
        assert_eq!(query_type.directives[1].arguments[0].value.as_ref(), "100");
    }

    #[test]
    fn test_repeatable_directives_preserved() {
        // Repeatable directives (e.g. @tag) can appear multiple times with
        // different arguments. Merging must not deduplicate by name.
        let (db, project) = TestProjectBuilder::new()
            .with_schema(
                "schema.graphql",
                r#"type Query @tag(name: "public") { hello: String }"#,
            )
            .with_schema("ext.graphql", r#"extend type Query @tag(name: "internal")"#)
            .build();

        let types = schema_types(&db, project);
        let query_type = types.get("Query").expect("Query type should exist");

        let tag_directives: Vec<_> = query_type
            .directives
            .iter()
            .filter(|d| d.name.as_ref() == "tag")
            .collect();
        assert_eq!(
            tag_directives.len(),
            2,
            "Both @tag usages should be preserved for repeatable directives"
        );

        let tag_values: Vec<&str> = tag_directives
            .iter()
            .map(|d| d.arguments[0].value.as_ref())
            .collect();
        assert!(tag_values.contains(&r#""public""#));
        assert!(tag_values.contains(&r#""internal""#));
    }
}

// ============================================================================
// Source location tests
// ============================================================================

mod source_location_tests {
    use super::*;

    #[test]
    fn test_schema_types_have_correct_file_ids_across_files() {
        let mut db = TestDatabase::default();

        let schema1_id = FileId::new(0);
        let schema1_content = FileContent::new(
            &db,
            Arc::from(
                r#"
                type Query {
                    user: User
                }
                "#,
            ),
        );
        let schema1_metadata = FileMetadata::new(
            &db,
            schema1_id,
            FileUri::new("schema1.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let schema2_id = FileId::new(1);
        let schema2_content = FileContent::new(
            &db,
            Arc::from(
                r#"
                type User {
                    id: ID!
                    name: String!
                }
                "#,
            ),
        );
        let schema2_metadata = FileMetadata::new(
            &db,
            schema2_id,
            FileUri::new("schema2.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let project_files = create_project_files(
            &mut db,
            &[
                (schema1_id, schema1_content, schema1_metadata),
                (schema2_id, schema2_content, schema2_metadata),
            ],
            &[],
        );

        let types = graphql_hir::schema_types(&db, project_files);

        let query_type = types.get("Query").expect("Query type should exist");
        assert_eq!(query_type.file_id, schema1_id, "Query should be in schema1");

        let user_type = types.get("User").expect("User type should exist");
        assert_eq!(user_type.file_id, schema2_id, "User should be in schema2");
    }

    #[test]
    fn test_type_def_name_range_is_nonzero() {
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

        let project_files = create_project_files(
            &mut db,
            &[(schema_id, schema_content, schema_metadata)],
            &[],
        );

        let types = graphql_hir::schema_types(&db, project_files);
        let query_type = types.get("Query").expect("Query type should exist");

        assert!(
            !query_type.name_range.is_empty(),
            "Type name range should be non-empty"
        );

        let source: &str = &schema_content.text(&db);
        let start = u32::from(query_type.name_range.start()) as usize;
        let end = u32::from(query_type.name_range.end()) as usize;
        assert_eq!(
            &source[start..end],
            "Query",
            "Name range should point to 'Query'"
        );
    }

    #[test]
    fn test_field_name_range_is_nonzero() {
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

        let project_files = create_project_files(
            &mut db,
            &[(schema_id, schema_content, schema_metadata)],
            &[],
        );

        let types = graphql_hir::schema_types(&db, project_files);
        let query_type = types.get("Query").expect("Query type should exist");
        let hello_field = query_type
            .fields
            .iter()
            .find(|f| f.name.as_ref() == "hello")
            .expect("hello field should exist");

        assert!(
            !hello_field.name_range.is_empty(),
            "Field name range should be non-empty"
        );

        let source: &str = &schema_content.text(&db);
        let start = u32::from(hello_field.name_range.start()) as usize;
        let end = u32::from(hello_field.name_range.end()) as usize;
        assert_eq!(
            &source[start..end],
            "hello",
            "Name range should point to 'hello'"
        );
    }

    #[test]
    fn test_field_has_correct_file_id_from_extension() {
        let mut db = TestDatabase::default();

        let schema1_id = FileId::new(0);
        let schema1_content = FileContent::new(&db, Arc::from("type Query { hello: String }"));
        let schema1_metadata = FileMetadata::new(
            &db,
            schema1_id,
            FileUri::new("schema1.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let schema2_id = FileId::new(1);
        let schema2_content =
            FileContent::new(&db, Arc::from("extend type Query { world: String }"));
        let schema2_metadata = FileMetadata::new(
            &db,
            schema2_id,
            FileUri::new("schema2.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let project_files = create_project_files(
            &mut db,
            &[
                (schema1_id, schema1_content, schema1_metadata),
                (schema2_id, schema2_content, schema2_metadata),
            ],
            &[],
        );

        let types = graphql_hir::schema_types(&db, project_files);
        let query_type = types.get("Query").expect("Query type should exist");

        let hello_field = query_type
            .fields
            .iter()
            .find(|f| f.name.as_ref() == "hello")
            .expect("hello field should exist");
        assert_eq!(
            hello_field.file_id, schema1_id,
            "hello should be from schema1"
        );

        let world_field = query_type
            .fields
            .iter()
            .find(|f| f.name.as_ref() == "world")
            .expect("world field should exist");
        assert_eq!(
            world_field.file_id, schema2_id,
            "world should be from schema2"
        );
    }

    #[test]
    fn test_argument_def_name_range_is_nonzero() {
        let mut db = TestDatabase::default();

        let schema_id = FileId::new(0);
        let schema_content = FileContent::new(
            &db,
            Arc::from("type Query { hello(name: String!): String }"),
        );
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let project_files = create_project_files(
            &mut db,
            &[(schema_id, schema_content, schema_metadata)],
            &[],
        );

        let types = graphql_hir::schema_types(&db, project_files);
        let query_type = types.get("Query").expect("Query type should exist");
        let hello_field = query_type
            .fields
            .iter()
            .find(|f| f.name.as_ref() == "hello")
            .expect("hello field should exist");
        let name_arg = hello_field
            .arguments
            .iter()
            .find(|a| a.name.as_ref() == "name")
            .expect("name argument should exist");

        assert!(
            !name_arg.name_range.is_empty(),
            "Argument name range should be non-empty"
        );

        let source: &str = &schema_content.text(&db);
        let start = u32::from(name_arg.name_range.start()) as usize;
        let end = u32::from(name_arg.name_range.end()) as usize;
        assert_eq!(
            &source[start..end],
            "name",
            "Name range should point to 'name'"
        );
    }

    #[test]
    fn test_source_locations_enable_direct_lookup() {
        let mut db = TestDatabase::default();

        let schema_id = FileId::new(0);
        let source_text =
            "type Query { users(limit: Int): [User] }\n\ntype User { id: ID! name: String! }";
        let schema_content = FileContent::new(&db, Arc::from(source_text));
        let schema_metadata = FileMetadata::new(
            &db,
            schema_id,
            FileUri::new("schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let project_files = create_project_files(
            &mut db,
            &[(schema_id, schema_content, schema_metadata)],
            &[],
        );

        let types = graphql_hir::schema_types(&db, project_files);

        // Simulate goto-definition for User.name
        let user_type = types.get("User").expect("User type should exist");
        let name_field = user_type
            .fields
            .iter()
            .find(|f| f.name.as_ref() == "name")
            .expect("name field should exist");

        // Direct O(1) lookup via HIR source locations
        assert_eq!(name_field.file_id, schema_id);
        let start = u32::from(name_field.name_range.start()) as usize;
        let end = u32::from(name_field.name_range.end()) as usize;
        assert_eq!(&source_text[start..end], "name");

        // Simulate goto-definition for Query.users(limit:)
        let query_type = types.get("Query").expect("Query type should exist");
        let users_field = query_type
            .fields
            .iter()
            .find(|f| f.name.as_ref() == "users")
            .expect("users field should exist");
        let limit_arg = users_field
            .arguments
            .iter()
            .find(|a| a.name.as_ref() == "limit")
            .expect("limit argument should exist");

        assert_eq!(limit_arg.file_id, schema_id);
        let arg_start = u32::from(limit_arg.name_range.start()) as usize;
        let arg_end = u32::from(limit_arg.name_range.end()) as usize;
        assert_eq!(&source_text[arg_start..arg_end], "limit");
    }
}
