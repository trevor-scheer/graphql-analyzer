//! Integration tests for graphql-hir.
//!
//! These tests verify HIR construction, fragment resolution, schema types,
//! and incremental computation behavior.

use graphql_base_db::{ExtractionOffset, FileContent, FileId, FileKind, FileMetadata, FileUri};
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
        FileKind::Schema,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let file2_id = FileId::new(1);
    let file2_content = FileContent::new(&db, Arc::from("fragment F2 on User { name }"));
    let file2_metadata = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("f2.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let file2_id = FileId::new(1);
    let file2_content =
        FileContent::new(&db, Arc::from("fragment PostFields on Post { title body }"));
    let file2_metadata = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("post-fragment.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
    );

    let file2_id = FileId::new(1);
    let file2_content = FileContent::new(&db, Arc::from("fragment PostFields on Post { title }"));
    let file2_metadata = FileMetadata::new(
        &db,
        file2_id,
        FileUri::new("post-fragment.graphql"),
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::TypeScript,
        ExtractionOffset::default(),
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
        FileKind::ExecutableGraphQL,
        ExtractionOffset::default(),
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
        FileKind::TypeScript,
        ExtractionOffset::default(),
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
            FileKind::Schema,
            ExtractionOffset::default(),
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
            FileKind::Schema,
            ExtractionOffset::default(),
        );

        let second_id = FileId::new(1);
        let second_content = FileContent::new(&db, Arc::from("type TypeB { id: ID! }"));
        let second_metadata = FileMetadata::new(
            &db,
            second_id,
            FileUri::new("b.graphql"),
            FileKind::Schema,
            ExtractionOffset::default(),
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
                FileKind::Schema,
                ExtractionOffset::default(),
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
            FileKind::Schema,
            ExtractionOffset::default(),
        );

        let op1_id = FileId::new(1);
        let op1_content = FileContent::new(&db, Arc::from("query GetUsers { users { id } }"));
        let op1_metadata = FileMetadata::new(
            &db,
            op1_id,
            FileUri::new("op1.graphql"),
            FileKind::ExecutableGraphQL,
            ExtractionOffset::default(),
        );

        let op2_id = FileId::new(2);
        let op2_content = FileContent::new(&db, Arc::from("query GetUserNames { users { name } }"));
        let op2_metadata = FileMetadata::new(
            &db,
            op2_id,
            FileUri::new("op2.graphql"),
            FileKind::ExecutableGraphQL,
            ExtractionOffset::default(),
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
                FileKind::ExecutableGraphQL,
                ExtractionOffset::default(),
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
            FileKind::Schema,
            ExtractionOffset::default(),
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
}
