use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{DocumentSchemaLintRule, LintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Trait implementation for `require_id_field` rule
pub struct RequireIdFieldRuleImpl;

impl LintRule for RequireIdFieldRuleImpl {
    fn name(&self) -> &'static str {
        "require_id_field"
    }

    fn description(&self) -> &'static str {
        "Warns when the `id` field is not requested on types that have it"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl DocumentSchemaLintRule for RequireIdFieldRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        project_files: ProjectFiles,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();
        let parse = graphql_syntax::parse(db, content, metadata);
        if !parse.errors.is_empty() {
            return diagnostics;
        }

        // Get schema types from HIR
        let schema_types = graphql_hir::schema_types_with_project(db, project_files);

        // Build a map of type names to whether they have an id field
        let mut types_with_id: HashMap<String, bool> = HashMap::new();
        for (type_name, type_def) in schema_types.iter() {
            let has_id = match type_def.kind {
                graphql_hir::TypeDefKind::Object | graphql_hir::TypeDefKind::Interface => {
                    type_def.fields.iter().any(|f| f.name.as_ref() == "id")
                }
                _ => false,
            };
            types_with_id.insert(type_name.to_string(), has_id);
        }

        // Get all fragments from the project (for cross-file resolution)
        let all_fragments = graphql_hir::all_fragments_with_project(db, project_files);

        // Get root operation types from schema
        let query_type = find_root_operation_type(&schema_types, "Query");
        let mutation_type = find_root_operation_type(&schema_types, "Mutation");
        let subscription_type = find_root_operation_type(&schema_types, "Subscription");

        // Create context for fragment resolution
        let check_context = CheckContext {
            db,
            project_files,
            schema_types: &schema_types,
            types_with_id: &types_with_id,
            all_fragments: &all_fragments,
        };

        // Check the main document (for .graphql files)
        let doc_cst = parse.tree.document();
        check_document(
            &doc_cst,
            query_type.as_deref(),
            mutation_type.as_deref(),
            subscription_type.as_deref(),
            &check_context,
            &mut diagnostics,
        );

        // Also check operations in extracted blocks (TypeScript/JavaScript)
        for block in &parse.blocks {
            let block_doc = block.tree.document();
            check_document(
                &block_doc,
                query_type.as_deref(),
                mutation_type.as_deref(),
                subscription_type.as_deref(),
                &check_context,
                &mut diagnostics,
            );
        }

        diagnostics
    }
}

/// Check a GraphQL document for `require_id_field` violations
fn check_document(
    doc_cst: &cst::Document,
    query_type: Option<&str>,
    mutation_type: Option<&str>,
    subscription_type: Option<&str>,
    check_context: &CheckContext,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for definition in doc_cst.definitions() {
        match definition {
            cst::Definition::OperationDefinition(op) => {
                let root_type = match op.operation_type() {
                    Some(op_type) if op_type.query_token().is_some() => query_type,
                    Some(op_type) if op_type.mutation_token().is_some() => mutation_type,
                    Some(op_type) if op_type.subscription_token().is_some() => subscription_type,
                    None => query_type, // Default to query for anonymous operations
                    _ => None,
                };

                if let (Some(root_type_name), Some(selection_set)) = (root_type, op.selection_set())
                {
                    let mut visited_fragments = HashSet::new();
                    check_selection_set(
                        &selection_set,
                        root_type_name,
                        check_context,
                        &mut visited_fragments,
                        diagnostics,
                    );
                }
            }
            cst::Definition::FragmentDefinition(frag) => {
                let type_condition = frag
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|name| name.text().to_string());

                if let (Some(type_name), Some(selection_set)) =
                    (type_condition.as_deref(), frag.selection_set())
                {
                    let mut visited_fragments = HashSet::new();
                    check_selection_set(
                        &selection_set,
                        type_name,
                        check_context,
                        &mut visited_fragments,
                        diagnostics,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Context for checking selection sets with fragment resolution
struct CheckContext<'a> {
    db: &'a dyn graphql_hir::GraphQLHirDatabase,
    project_files: graphql_db::ProjectFiles,
    schema_types: &'a HashMap<Arc<str>, graphql_hir::TypeDef>,
    types_with_id: &'a HashMap<String, bool>,
    all_fragments: &'a HashMap<Arc<str>, graphql_hir::FragmentStructure>,
}

#[allow(clippy::only_used_in_recursion, clippy::too_many_lines)]
fn check_selection_set(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // Check if this type has an id field
    let has_id_field = context
        .types_with_id
        .get(parent_type_name)
        .copied()
        .unwrap_or(false);

    let mut has_id_in_selection = false;

    // ALWAYS iterate through selections to recurse into nested selection sets,
    // even if the current type doesn't have an id field. This ensures we check
    // nested types like Query.allPokemon.nodes which returns Pokemon (has id).
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    let field_name_str = field_name.text();

                    // Check if this is the id field (only relevant if type has id)
                    if has_id_field && field_name_str == "id" {
                        has_id_in_selection = true;
                    }

                    // ALWAYS recurse into nested selection sets
                    if let Some(nested_selection_set) = field.selection_set() {
                        // Get the field's return type from schema
                        if let Some(field_type) =
                            get_field_type(parent_type_name, &field_name_str, context.schema_types)
                        {
                            check_selection_set(
                                &nested_selection_set,
                                &field_type,
                                context,
                                visited_fragments,
                                diagnostics,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                // Check if the fragment contains the id field (only relevant if type has id)
                if has_id_field {
                    if let Some(fragment_name) = fragment_spread.fragment_name() {
                        if let Some(name) = fragment_name.name() {
                            let name_str = name.text().to_string();
                            if fragment_contains_id(
                                &name_str,
                                parent_type_name,
                                context,
                                visited_fragments,
                            ) {
                                has_id_in_selection = true;
                            }
                        }
                    }
                }
            }
            cst::Selection::InlineFragment(inline_fragment) => {
                // For inline fragments, check nested selections
                if let Some(nested_selection_set) = inline_fragment.selection_set() {
                    // Determine the type for the inline fragment
                    let inline_type = inline_fragment
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| parent_type_name.to_string(), |n| n.text().to_string());

                    // Check for id in inline fragment's direct fields (only if type has id)
                    for nested_selection in nested_selection_set.selections() {
                        if let cst::Selection::Field(nested_field) = nested_selection {
                            if let Some(field_name) = nested_field.name() {
                                if has_id_field && field_name.text() == "id" {
                                    has_id_in_selection = true;
                                }

                                // ALWAYS recurse into nested object selections
                                if let Some(field_selection_set) = nested_field.selection_set() {
                                    if let Some(field_type) = get_field_type(
                                        &inline_type,
                                        &field_name.text(),
                                        context.schema_types,
                                    ) {
                                        check_selection_set(
                                            &field_selection_set,
                                            &field_type,
                                            context,
                                            visited_fragments,
                                            diagnostics,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Only emit diagnostic if type has id field and it's not in the selection
    if has_id_field && !has_id_in_selection {
        let syntax_node = selection_set.syntax();
        let start_offset: usize = syntax_node.text_range().start().into();
        let end_offset: usize = start_offset + 1;

        diagnostics.push(LintDiagnostic::warning(
            start_offset,
            end_offset,
            format!("Selection set on type '{parent_type_name}' should include the 'id' field"),
            "require_id_field",
        ));
    }
}

/// Find root operation type (Query, Mutation, or Subscription)
/// Falls back to the default name if no custom schema definition exists
fn find_root_operation_type(
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    default_name: &str,
) -> Option<String> {
    // TODO: Read from schema definition directive once HIR supports it
    // For now, use the default names
    if schema_types.contains_key(default_name) {
        Some(default_name.to_string())
    } else {
        None
    }
}

/// Get the return type name for a field, unwrapping `List` and `NonNull` wrappers
fn get_field_type(
    parent_type_name: &str,
    field_name: &str,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
) -> Option<String> {
    let type_def = schema_types.get(parent_type_name)?;

    // Only Object and Interface types have fields
    if !matches!(
        type_def.kind,
        graphql_hir::TypeDefKind::Object | graphql_hir::TypeDefKind::Interface
    ) {
        return None;
    }

    let field = type_def
        .fields
        .iter()
        .find(|f| f.name.as_ref() == field_name)?;

    // The TypeRef name is already unwrapped from List/NonNull wrappers
    Some(field.type_ref.name.to_string())
}

/// Check if a fragment (or its nested fragments) contains the `id` field
fn fragment_contains_id(
    fragment_name: &str,
    parent_type_name: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
) -> bool {
    // Prevent infinite recursion with circular fragment references
    if visited_fragments.contains(fragment_name) {
        return false;
    }
    visited_fragments.insert(fragment_name.to_string());

    // Look up the fragment in HIR
    let Some(fragment_info) = context.all_fragments.get(fragment_name) else {
        return false;
    };

    // Get the fragment's file and parse it (cached by Salsa)
    let file_id = fragment_info.file_id;

    // We need to get the file content and metadata to parse it
    // Use project_files directly (not db.document_files() which uses db.project_files())
    let document_files_input = context.project_files.document_files(context.db);
    let document_files = document_files_input.files(context.db);

    let Some((file_content, file_metadata)) = document_files
        .iter()
        .find(|(fid, _, _)| *fid == file_id)
        .map(|(_, c, m)| (*c, *m))
    else {
        return false;
    };

    // Parse the file (cached by Salsa)
    let parse = graphql_syntax::parse(context.db, file_content, file_metadata);
    if !parse.errors.is_empty() {
        return false;
    }

    // Find the fragment definition in the CST
    let doc_cst = parse.tree.document();
    for definition in doc_cst.definitions() {
        if let cst::Definition::FragmentDefinition(frag) = definition {
            // Check if this is the fragment we're looking for
            let is_target_fragment = frag
                .fragment_name()
                .and_then(|name| name.name())
                .is_some_and(|name| name.text() == fragment_name);

            if !is_target_fragment {
                continue;
            }

            // Found the fragment, check its selection set for id
            if let Some(selection_set) = frag.selection_set() {
                return check_fragment_selection_for_id(
                    &selection_set,
                    parent_type_name,
                    context,
                    visited_fragments,
                );
            }
        }
    }

    false
}

/// Check if a selection set within a fragment contains the `id` field
/// This only checks for `id` at the top level of the selection set.
/// We do NOT recurse into nested field selections because:
/// - `abilities { ...AbilityInfo }` selects `id` on Ability, not on the current type
/// - We only care if `id` is selected directly on the current type
fn check_fragment_selection_for_id(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
) -> bool {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    // Check if this is the id field at the top level
                    if field_name.text() == "id" {
                        return true;
                    }
                    // NOTE: We intentionally do NOT recurse into nested field selections.
                    // A field like `abilities { id }` selects `id` on a nested type (Ability),
                    // not on the current type (Pokemon). The require_id_field rule checks
                    // each selection set independently, so nested selections will be validated
                    // when their own selection sets are processed.
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                // Recursively check nested fragment spreads
                if let Some(fragment_name) = fragment_spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        let name_str = name.text().to_string();
                        if fragment_contains_id(
                            &name_str,
                            parent_type_name,
                            context,
                            visited_fragments,
                        ) {
                            return true;
                        }
                    }
                }
            }
            cst::Selection::InlineFragment(inline_fragment) => {
                // Check inline fragments
                if let Some(nested_selection_set) = inline_fragment.selection_set() {
                    let inline_type = inline_fragment
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| parent_type_name.to_string(), |n| n.text().to_string());

                    if check_fragment_selection_for_id(
                        &nested_selection_set,
                        &inline_type,
                        context,
                        visited_fragments,
                    ) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};
    use graphql_hir::GraphQLHirDatabase;

    /// Helper to create test project files with schema and document
    fn create_test_project(
        db: &dyn GraphQLHirDatabase,
        schema_source: &str,
        document_source: &str,
        document_kind: FileKind,
    ) -> (FileId, FileContent, FileMetadata, ProjectFiles) {
        // Create schema file
        let schema_file_id = FileId::new(0);
        let schema_content = FileContent::new(db, Arc::from(schema_source));
        let schema_metadata = FileMetadata::new(
            db,
            schema_file_id,
            FileUri::new("file:///schema.graphql"),
            FileKind::Schema,
        );

        // Create document file
        let doc_file_id = FileId::new(1);
        let doc_content = FileContent::new(db, Arc::from(document_source));
        let doc_metadata = FileMetadata::new(
            db,
            doc_file_id,
            FileUri::new("file:///query.graphql"),
            document_kind,
        );

        let schema_files = graphql_db::SchemaFiles::new(
            db,
            Arc::new(vec![(schema_file_id, schema_content, schema_metadata)]),
        );
        let document_files = graphql_db::DocumentFiles::new(
            db,
            Arc::new(vec![(doc_file_id, doc_content, doc_metadata)]),
        );
        let project_files = ProjectFiles::new(db, schema_files, document_files);

        (doc_file_id, doc_content, doc_metadata, project_files)
    }

    const TEST_SCHEMA: &str = r#"
type Query {
    user(id: ID!): User
    users: [User!]!
    post(id: ID!): Post
}

type User {
    id: ID!
    name: String!
    email: String!
    posts: [Post!]!
}

type Post {
    id: ID!
    title: String!
    author: User!
    comments: [Comment!]!
}

type Comment {
    id: ID!
    text: String!
    author: User!
}

type Stats {
    viewCount: Int!
    likeCount: Int!
}
"#;

    #[test]
    fn test_missing_id_on_type_with_id() {
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
query GetUser {
    user(id: "1") {
        name
        email
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Selection set on type 'User' should include the 'id' field"));
    }

    #[test]
    fn test_id_present_no_warning() {
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
query GetUser {
    user(id: "1") {
        id
        name
        email
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_nested_selection_requires_id() {
        // This tests the fix for nested selection set recursion:
        // Query.user doesn't have id (Query type has no id field),
        // but we need to recurse into User's selection set to check for id there
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
query GetUserPosts {
    user(id: "1") {
        id
        name
        posts {
            title
        }
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should warn about Post missing id
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Selection set on type 'Post' should include the 'id' field"));
    }

    #[test]
    fn test_deeply_nested_selection_requires_id() {
        // Test that we recurse multiple levels deep
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
query GetUserPostComments {
    user(id: "1") {
        id
        posts {
            id
            comments {
                text
                author {
                    name
                }
            }
        }
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should warn about Comment and nested User missing id
        assert_eq!(diagnostics.len(), 2);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("'Comment'")));
        assert!(messages.iter().any(|m| m.contains("'User'")));
    }

    #[test]
    fn test_type_without_id_field_no_warning() {
        // Stats type doesn't have an id field, so no warning should be emitted
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r#"
type Query {
    stats: Stats!
}

type Stats {
    viewCount: Int!
    likeCount: Int!
}
"#;

        let source = r#"
query GetStats {
    stats {
        viewCount
        likeCount
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, schema, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_typescript_file_with_gql_tag() {
        // This tests that TypeScript files with gql`` template literals are processed
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
import { gql } from '@apollo/client';

const GET_USER = gql`
    query GetUser {
        user(id: "1") {
            name
            email
        }
    }
`;
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::TypeScript);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should warn about User missing id in the TypeScript file
        // Note: May produce duplicates due to issue #194
        assert!(!diagnostics.is_empty(), "Expected at least one warning");
        assert!(diagnostics
            .iter()
            .all(|d| d.message.contains("Selection set on type 'User'")));
    }

    #[test]
    fn test_typescript_file_multiple_queries() {
        // Test multiple gql blocks in a single TypeScript file
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
import { gql } from '@apollo/client';

const GET_USER = gql`
    query GetUser {
        user(id: "1") {
            id
            name
        }
    }
`;

const GET_POSTS = gql`
    query GetPosts {
        users {
            name
            posts {
                title
            }
        }
    }
`;
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::TypeScript);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should warn about User and Post missing id in the second query
        assert_eq!(diagnostics.len(), 2);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("'User'")));
        assert!(messages.iter().any(|m| m.contains("'Post'")));
    }

    #[test]
    fn test_typescript_nested_selection_recursion() {
        // Test that nested selections work in TypeScript files
        // This combines the TypeScript block processing and nested recursion fixes
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
import { gql } from '@apollo/client';

const QUERY = gql`
    query DeepNested {
        users {
            id
            posts {
                id
                author {
                    name
                }
            }
        }
    }
`;
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::TypeScript);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should warn about nested author (User type) missing id
        // Note: May produce duplicates due to issue #194
        assert!(!diagnostics.is_empty(), "Expected at least one warning");
        assert!(diagnostics
            .iter()
            .all(|d| d.message.contains("Selection set on type 'User'")));
    }

    #[test]
    fn test_fragment_with_id_no_warning() {
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
fragment UserFields on User {
    id
    name
    email
}

query GetUser {
    user(id: "1") {
        ...UserFields
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // No warning because fragment includes id
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_fragment_without_id_warning() {
        let db = graphql_db::RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
fragment UserFields on User {
    name
    email
}

query GetUser {
    user(id: "1") {
        ...UserFields
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should warn - both the fragment definition and the operation usage
        // The fragment itself is checked, and the operation using it is checked
        assert!(diagnostics.len() >= 1);
        assert!(diagnostics.iter().any(|d| d.message.contains("'User'")));
    }
}
