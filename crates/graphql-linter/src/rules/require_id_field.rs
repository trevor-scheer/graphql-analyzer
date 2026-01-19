use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::schema_utils::extract_root_type_names;
use crate::traits::{DocumentSchemaLintRule, LintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
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
        if parse.has_errors() {
            return diagnostics;
        }

        // Get schema types from HIR
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Build a map of type names to whether they have an id field
        let mut types_with_id: HashMap<String, bool> = HashMap::new();
        for (type_name, type_def) in schema_types {
            let has_id = match type_def.kind {
                graphql_hir::TypeDefKind::Object | graphql_hir::TypeDefKind::Interface => {
                    type_def.fields.iter().any(|f| f.name.as_ref() == "id")
                }
                _ => false,
            };
            types_with_id.insert(type_name.to_string(), has_id);
        }

        // Get all fragments from the project (for cross-file resolution)
        let all_fragments = graphql_hir::all_fragments(db, project_files);

        // Get root operation types from schema definition or fall back to defaults
        let root_types = extract_root_type_names(db, project_files, schema_types);
        let query_type = root_types.query;
        let mutation_type = root_types.mutation;
        let subscription_type = root_types.subscription;

        // Create context for fragment resolution
        let check_context = CheckContext {
            db,
            project_files,
            schema_types,
            types_with_id: &types_with_id,
            all_fragments,
        };

        // Unified: check all documents (works for both pure GraphQL and TS/JS)
        for doc in parse.documents() {
            let doc_cst = doc.tree.document();
            let mut doc_diagnostics = Vec::new();
            check_document(
                &doc_cst,
                query_type.as_deref(),
                mutation_type.as_deref(),
                subscription_type.as_deref(),
                &check_context,
                &mut doc_diagnostics,
            );

            // Add block context for embedded GraphQL (line_offset > 0)
            if doc.line_offset > 0 {
                for diag in doc_diagnostics {
                    diagnostics.push(
                        diag.with_block_context(doc.line_offset, std::sync::Arc::from(doc.source)),
                    );
                }
            } else {
                diagnostics.extend(doc_diagnostics);
            }
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
                use super::{get_operation_kind, OperationKind};
                let root_type = op.operation_type().map_or(query_type, |op_type| {
                    match get_operation_kind(&op_type) {
                        OperationKind::Query => query_type,
                        OperationKind::Mutation => mutation_type,
                        OperationKind::Subscription => subscription_type,
                    }
                });

                if let (Some(root_type_name), Some(selection_set)) = (root_type, op.selection_set())
                {
                    // For root operations, we use the operation name or operation type as location
                    // since there's no parent field. Root types (Query/Mutation/Subscription)
                    // typically don't have an id field anyway.
                    let op_loc = if let Some(name) = op.name() {
                        DiagnosticLocation {
                            start: name.syntax().text_range().start().into(),
                            end: name.syntax().text_range().end().into(),
                        }
                    } else if let Some(op_type) = op.operation_type() {
                        DiagnosticLocation {
                            start: op_type.syntax().text_range().start().into(),
                            end: op_type.syntax().text_range().end().into(),
                        }
                    } else {
                        // Anonymous query shorthand - use selection set start
                        let start: usize = selection_set.syntax().text_range().start().into();
                        DiagnosticLocation {
                            start,
                            end: start + 1,
                        }
                    };
                    let mut visited_fragments = HashSet::new();
                    check_selection_set(
                        &selection_set,
                        root_type_name,
                        op_loc,
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
                    // Position diagnostic on fragment name
                    let frag_loc = frag.fragment_name().and_then(|fn_| fn_.name()).map_or_else(
                        || {
                            let start: usize = selection_set.syntax().text_range().start().into();
                            DiagnosticLocation {
                                start,
                                end: start + 1,
                            }
                        },
                        |name| DiagnosticLocation {
                            start: name.syntax().text_range().start().into(),
                            end: name.syntax().text_range().end().into(),
                        },
                    );
                    let mut visited_fragments = HashSet::new();
                    check_selection_set(
                        &selection_set,
                        type_name,
                        frag_loc,
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
    project_files: graphql_base_db::ProjectFiles,
    schema_types: &'a HashMap<Arc<str>, graphql_hir::TypeDef>,
    types_with_id: &'a HashMap<String, bool>,
    all_fragments: &'a HashMap<Arc<str>, graphql_hir::FragmentStructure>,
}

/// Location for diagnostic placement (start and end offsets)
#[derive(Clone, Copy)]
struct DiagnosticLocation {
    start: usize,
    end: usize,
}

#[allow(clippy::only_used_in_recursion, clippy::too_many_lines)]
fn check_selection_set(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    parent_location: DiagnosticLocation,
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
                            let field_loc = DiagnosticLocation {
                                start: field_name.syntax().text_range().start().into(),
                                end: field_name.syntax().text_range().end().into(),
                            };
                            check_selection_set(
                                &nested_selection_set,
                                &field_type,
                                field_loc,
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
                            // Clone visited_fragments so sibling checks don't interfere
                            // (each fragment_contains_id call chain has its own cycle detection)
                            let mut visited_clone = visited_fragments.clone();
                            if fragment_contains_id(
                                &name_str,
                                parent_type_name,
                                context,
                                &mut visited_clone,
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

                    // Check for id in inline fragment's selections (only if type has id)
                    for nested_selection in nested_selection_set.selections() {
                        match nested_selection {
                            cst::Selection::Field(nested_field) => {
                                if let Some(field_name) = nested_field.name() {
                                    if has_id_field && field_name.text() == "id" {
                                        has_id_in_selection = true;
                                    }

                                    // ALWAYS recurse into nested object selections
                                    if let Some(field_selection_set) = nested_field.selection_set()
                                    {
                                        if let Some(field_type) = get_field_type(
                                            &inline_type,
                                            &field_name.text(),
                                            context.schema_types,
                                        ) {
                                            let field_loc = DiagnosticLocation {
                                                start: field_name
                                                    .syntax()
                                                    .text_range()
                                                    .start()
                                                    .into(),
                                                end: field_name.syntax().text_range().end().into(),
                                            };
                                            check_selection_set(
                                                &field_selection_set,
                                                &field_type,
                                                field_loc,
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
                                            // Clone visited_fragments so sibling checks don't interfere
                                            let mut visited_clone = visited_fragments.clone();
                                            if fragment_contains_id(
                                                &name_str,
                                                parent_type_name,
                                                context,
                                                &mut visited_clone,
                                            ) {
                                                has_id_in_selection = true;
                                            }
                                        }
                                    }
                                }
                            }
                            cst::Selection::InlineFragment(_) => {
                                // Nested inline fragments are handled by recursion in check_selection_set
                                // if we were to call it here. For now, we rely on the parent logic.
                            }
                        }
                    }
                }
            }
        }
    }

    // Only emit diagnostic if type has id field and it's not in the selection
    if has_id_field && !has_id_in_selection {
        // Calculate insertion position and indentation for the fix
        let selection_set_start: usize = selection_set.syntax().text_range().start().into();
        let selection_set_source = selection_set.syntax().to_string();

        let (insert_pos, indent) = selection_set.selections().next().map_or_else(
            || {
                // Empty selection set - insert after the opening brace with default indent
                (selection_set_start + 1, "  ".to_string())
            },
            |first| {
                let pos: usize = first.syntax().text_range().start().into();
                // Calculate position relative to selection set start
                let relative_pos = pos - selection_set_start;
                let indent = extract_indentation(&selection_set_source, relative_pos);
                (pos, indent)
            },
        );

        let fix = CodeFix::new(
            format!("Add 'id' field to {parent_type_name}"),
            vec![TextEdit::insert(insert_pos, format!("id\n{indent}"))],
        );

        diagnostics.push(
            LintDiagnostic::warning(
                parent_location.start,
                parent_location.end,
                format!("Selection set on type '{parent_type_name}' should include the 'id' field"),
                "require_id_field",
            )
            .with_fix(fix),
        );
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

/// Extract the indentation (whitespace) before a given position in source
/// by looking backwards to find the most recent newline
fn extract_indentation(source: &str, pos: usize) -> String {
    let before = &source[..pos];
    // Find the last newline before this position
    if let Some(newline_pos) = before.rfind('\n') {
        // Extract everything between the newline and the position
        let indent_slice = &before[newline_pos + 1..];
        // Only keep whitespace characters
        indent_slice
            .chars()
            .take_while(|c| c.is_whitespace() && *c != '\n')
            .collect()
    } else {
        // No newline found, use default indentation
        "  ".to_string()
    }
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

    // Get the file content and metadata via file_lookup (granular per-file caching)
    let Some((file_content, file_metadata)) =
        graphql_base_db::file_lookup(context.db, context.project_files, file_id)
    else {
        return false;
    };

    // Parse the file (cached by Salsa)
    let parse = graphql_syntax::parse(context.db, file_content, file_metadata);
    if parse.has_errors() {
        return false;
    }

    // Find the fragment definition in all CST documents
    // For TypeScript/JavaScript files, we need to check all blocks, not just parse.tree
    for doc_ref in parse.documents() {
        let doc_cst = doc_ref.tree.document();
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
    use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};
    use graphql_hir::GraphQLHirDatabase;
    use graphql_ide_db::RootDatabase;

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

        let schema_file_ids =
            graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![schema_file_id]));
        let document_file_ids =
            graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![doc_file_id]));
        let mut file_entries = std::collections::HashMap::new();
        let schema_entry = graphql_base_db::FileEntry::new(db, schema_content, schema_metadata);
        let doc_entry = graphql_base_db::FileEntry::new(db, doc_content, doc_metadata);
        file_entries.insert(schema_file_id, schema_entry);
        file_entries.insert(doc_file_id, doc_entry);
        let file_entry_map = graphql_base_db::FileEntryMap::new(db, Arc::new(file_entries));
        let project_files =
            ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map);

        (doc_file_id, doc_content, doc_metadata, project_files)
    }

    const TEST_SCHEMA: &str = r"
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
";

    #[test]
    fn test_missing_id_on_type_with_id() {
        let db = RootDatabase::default();
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
        let db = RootDatabase::default();
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
        let db = RootDatabase::default();
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
        let db = RootDatabase::default();
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
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r"
type Query {
    stats: Stats!
}

type Stats {
    viewCount: Int!
    likeCount: Int!
}
";

        let source = r"
query GetStats {
    stats {
        viewCount
        likeCount
    }
}
";

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, schema, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_typescript_file_with_gql_tag() {
        // This tests that TypeScript files with gql`` template literals are processed
        let db = RootDatabase::default();
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
        let db = RootDatabase::default();
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
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r"
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
";

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
        let db = RootDatabase::default();
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
        let db = RootDatabase::default();
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
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().any(|d| d.message.contains("'User'")));
    }

    /// Helper to create test project with multiple document files
    fn create_multi_file_project(
        db: &dyn GraphQLHirDatabase,
        schema_source: &str,
        documents: &[(&str, &str, FileKind)], // (uri, source, kind)
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

        let mut file_entries = std::collections::HashMap::new();
        let schema_entry = graphql_base_db::FileEntry::new(db, schema_content, schema_metadata);
        file_entries.insert(schema_file_id, schema_entry);

        let mut doc_file_ids = Vec::new();
        let mut first_doc: Option<(FileId, FileContent, FileMetadata)> = None;

        #[allow(clippy::cast_possible_truncation)]
        for (i, (uri, source, kind)) in documents.iter().enumerate() {
            let file_id = FileId::new((i + 1) as u32);
            let content = FileContent::new(db, Arc::from(*source));
            let metadata = FileMetadata::new(db, file_id, FileUri::new(*uri), *kind);

            let entry = graphql_base_db::FileEntry::new(db, content, metadata);
            file_entries.insert(file_id, entry);
            doc_file_ids.push(file_id);

            if first_doc.is_none() {
                first_doc = Some((file_id, content, metadata));
            }
        }

        let schema_file_ids =
            graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![schema_file_id]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(doc_file_ids));
        let file_entry_map = graphql_base_db::FileEntryMap::new(db, Arc::new(file_entries));
        let project_files =
            ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map);

        let (file_id, content, metadata) = first_doc.expect("At least one document required");
        (file_id, content, metadata, project_files)
    }

    #[test]
    fn test_cross_file_fragment_with_id_no_warning() {
        // Test case for issue #195: Fragment defined in separate file should be checked for id
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let fragment_source = r"
fragment UserBasic on User {
    id
    name
}
";

        let query_source = r#"
query GetUser {
    user(id: "1") {
        ...UserBasic
    }
}
"#;

        let documents = [
            (
                "file:///fragments.graphql",
                fragment_source,
                FileKind::ExecutableGraphQL,
            ),
            (
                "file:///query.graphql",
                query_source,
                FileKind::ExecutableGraphQL,
            ),
        ];

        let (_, _, _, project_files) = create_multi_file_project(&db, TEST_SCHEMA, &documents);

        // Check the query file (second file, so we need to get it from project_files)
        let query_file_id = FileId::new(2);
        let (query_content, query_metadata) =
            graphql_base_db::file_lookup(&db, project_files, query_file_id)
                .expect("Query file should exist");

        let diagnostics = rule.check(
            &db,
            query_file_id,
            query_content,
            query_metadata,
            project_files,
        );

        // No warning because fragment includes id
        assert_eq!(
            diagnostics.len(),
            0,
            "Should not warn when fragment contains id: {diagnostics:?}"
        );
    }

    #[test]
    fn test_cross_file_fragment_without_id_warning() {
        // Test case for issue #195: Fragment defined in separate file should be checked for id
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let fragment_source = r"
fragment UserBasic on User {
    name
    email
}
";

        let query_source = r#"
query GetUser {
    user(id: "1") {
        ...UserBasic
    }
}
"#;

        let documents = [
            (
                "file:///fragments.graphql",
                fragment_source,
                FileKind::ExecutableGraphQL,
            ),
            (
                "file:///query.graphql",
                query_source,
                FileKind::ExecutableGraphQL,
            ),
        ];

        let (_, _, _, project_files) = create_multi_file_project(&db, TEST_SCHEMA, &documents);

        // Check the query file (second file, so we need to get it from project_files)
        let query_file_id = FileId::new(2);
        let (query_content, query_metadata) =
            graphql_base_db::file_lookup(&db, project_files, query_file_id)
                .expect("Query file should exist");

        let diagnostics = rule.check(
            &db,
            query_file_id,
            query_content,
            query_metadata,
            project_files,
        );

        // Should warn because fragment does not include id
        assert!(
            !diagnostics.is_empty(),
            "Should warn when fragment does not contain id"
        );
        assert!(diagnostics.iter().any(|d| d.message.contains("'User'")));
    }

    #[test]
    fn test_typescript_cross_file_fragment_with_id() {
        // Test case for issue #195: TypeScript file using fragment from another file
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let fragment_source = r"
fragment UserBasic on User {
    id
    name
}
";

        let ts_source = r#"
import { gql } from '@apollo/client';

export const GET_USER = gql`
    query GetUser {
        user(id: "1") {
            ...UserBasic
        }
    }
`;
"#;

        let documents = [
            (
                "file:///fragments.graphql",
                fragment_source,
                FileKind::ExecutableGraphQL,
            ),
            ("file:///query.ts", ts_source, FileKind::TypeScript),
        ];

        let (_, _, _, project_files) = create_multi_file_project(&db, TEST_SCHEMA, &documents);

        // Check the TypeScript file (second file)
        let ts_file_id = FileId::new(2);
        let (ts_content, ts_metadata) =
            graphql_base_db::file_lookup(&db, project_files, ts_file_id)
                .expect("TypeScript file should exist");

        let diagnostics = rule.check(&db, ts_file_id, ts_content, ts_metadata, project_files);

        // No warning because fragment includes id
        assert_eq!(
            diagnostics.len(),
            0,
            "Should not warn when fragment contains id: {diagnostics:?}"
        );
    }

    #[test]
    fn test_inline_fragment_with_fragment_spread_containing_id() {
        // Test that fragment spreads inside inline fragments are checked for id
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let source = r#"
fragment UserFields on User {
    id
    name
}

query GetPost {
    post(id: "1") {
        id
        author {
            ... on User {
                ...UserFields
            }
        }
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // No warning because fragment includes id and is used inside inline fragment
        assert_eq!(
            diagnostics.len(),
            0,
            "Should not warn when fragment in inline fragment contains id: {diagnostics:?}"
        );
    }

    #[test]
    fn test_fragment_spread_inside_field_in_fragment_definition() {
        // Issue #376: Fragment spread inside a field in a fragment definition should
        // recognize that id is included via the spread
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r"
type Query {
    battle(id: ID!): Battle
}

type Battle {
    id: ID!
    trainer1: Trainer
}

type Trainer {
    id: ID!
    name: String
}
";

        let source = r"
fragment TrainerBasic on Trainer {
    id
    name
}

fragment BattleDetailed on Battle {
    trainer1 {
        ...TrainerBasic
    }
}
";

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, schema, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should NOT warn on trainer1 because TrainerBasic includes id
        // Should only warn on BattleDetailed since Battle.id is not selected
        let trainer_warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("'Trainer'"))
            .collect();
        assert_eq!(
            trainer_warnings.len(),
            0,
            "Should not warn on Trainer when fragment spread contains id: {trainer_warnings:?}"
        );
    }

    #[test]
    fn test_cross_file_fragment_spread_inside_field_in_fragment_definition() {
        // Issue #376: Cross-file variant - fragment spread inside a field in a fragment
        // definition should recognize that id is included via the spread
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r"
type Query {
    battle(id: ID!): Battle
}

type Battle {
    id: ID!
    trainer1: Trainer
}

type Trainer {
    id: ID!
    name: String
}
";

        // Fragment with id in separate file
        let trainer_fragment_source = r"
fragment TrainerBasic on Trainer {
    id
    name
}
";

        // Fragment using the trainer fragment via nested field
        let battle_fragment_source = r"
fragment BattleDetailed on Battle {
    trainer1 {
        ...TrainerBasic
    }
}
";

        let documents = [
            (
                "file:///trainer-fragments.graphql",
                trainer_fragment_source,
                FileKind::ExecutableGraphQL,
            ),
            (
                "file:///battle-fragments.graphql",
                battle_fragment_source,
                FileKind::ExecutableGraphQL,
            ),
        ];

        let (_, _, _, project_files) = create_multi_file_project(&db, schema, &documents);

        // Check the battle fragments file (second file)
        let battle_file_id = FileId::new(2);
        let (battle_content, battle_metadata) =
            graphql_base_db::file_lookup(&db, project_files, battle_file_id)
                .expect("Battle fragments file should exist");

        let diagnostics = rule.check(
            &db,
            battle_file_id,
            battle_content,
            battle_metadata,
            project_files,
        );

        // Should NOT warn on trainer1 because TrainerBasic includes id
        // Should only warn on BattleDetailed since Battle.id is not selected
        let trainer_warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("'Trainer'"))
            .collect();
        assert_eq!(
            trainer_warnings.len(),
            0,
            "Should not warn on Trainer when cross-file fragment spread contains id: {trainer_warnings:?}"
        );
    }

    #[test]
    fn test_fragment_reused_in_sibling_spread_and_field() {
        // Issue #376: When a fragment is used both in a sibling spread (BattleBasic) and
        // directly in a field (trainer1), the visited_fragments set gets polluted
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r"
type Query {
    battle(id: ID!): Battle
}

type Battle {
    id: ID!
    trainer1: Trainer
}

type Trainer {
    id: ID!
    name: String
}
";

        // This is the exact issue scenario from #376
        let source = r"
fragment TrainerBasic on Trainer {
    id
    name
}

fragment BattleBasic on Battle {
    id
    trainer1 {
        ...TrainerBasic
    }
}

fragment BattleDetailed on Battle {
    ...BattleBasic
    trainer1 {
        ...TrainerBasic
    }
}
";

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, schema, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should NOT warn on trainer1 in BattleDetailed because TrainerBasic includes id
        // The bug was that visited_fragments accumulates "TrainerBasic" when checking
        // ...BattleBasic (which uses TrainerBasic), then when we check trainer1 { ...TrainerBasic }
        // the TrainerBasic check returns false because it's already in visited_fragments
        let trainer_warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("'Trainer'"))
            .collect();
        assert_eq!(
            trainer_warnings.len(),
            0,
            "Should not warn on Trainer when fragment spread contains id (visited_fragments pollution bug): {trainer_warnings:?}"
        );
    }

    #[test]
    fn test_issue_376_exact_scenario() {
        // Issue #376: EXACT scenario from the issue - BattleBasic is referenced but not defined
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r"
type Query {
    battle(id: ID!): Battle
}

type Battle {
    id: ID!
    trainer1: Trainer
}

type Trainer {
    id: ID!
    name: String
}
";

        // Exact example from issue #376
        // Note: BattleBasic is referenced but NOT defined - this might be part of the issue
        let source = r"
fragment TrainerBasic on Trainer {
    id
    name
}

fragment BattleDetailed on Battle {
    ...BattleBasic
    trainer1 {
        ...TrainerBasic
    }
}
";

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, schema, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should NOT warn on trainer1 because TrainerBasic includes id
        // Even though BattleBasic is undefined, the trainer1 field's TrainerBasic spread
        // should still be recognized as including id
        let trainer_warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("'Trainer'"))
            .collect();
        assert_eq!(
            trainer_warnings.len(),
            0,
            "Issue #376: Should not warn on Trainer when fragment spread contains id: {trainer_warnings:?}"
        );
    }

    #[test]
    fn test_fragment_defined_after_usage() {
        // Test case: Fragment is defined AFTER it's used (reverse order)
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r"
type Query {
    battle(id: ID!): Battle
}

type Battle {
    id: ID!
    trainer1: Trainer
}

type Trainer {
    id: ID!
    name: String
}
";

        // TrainerBasic is used before it's defined in the file
        let source = r"
fragment BattleDetailed on Battle {
    trainer1 {
        ...TrainerBasic
    }
}

fragment TrainerBasic on Trainer {
    id
    name
}
";

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, schema, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should NOT warn on trainer1 because TrainerBasic includes id
        let trainer_warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("'Trainer'"))
            .collect();
        assert_eq!(
            trainer_warnings.len(),
            0,
            "Should not warn on Trainer even when fragment is defined after usage: {trainer_warnings:?}"
        );
    }

    #[test]
    fn test_typescript_fragments_across_gql_blocks() {
        // Issue #376: Fragments used across different gql`` blocks in TypeScript
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r"
type Query {
    battle(id: ID!): Battle
}

type Battle {
    id: ID!
    trainer1: Trainer
}

type Trainer {
    id: ID!
    name: String
}
";

        // Two separate gql blocks - fragment in one, usage in another
        let ts_source = r"
import { gql } from '@apollo/client';

export const TRAINER_FRAGMENT = gql`
    fragment TrainerBasic on Trainer {
        id
        name
    }
`;

export const BATTLE_FRAGMENT = gql`
    fragment BattleDetailed on Battle {
        trainer1 {
            ...TrainerBasic
        }
    }
`;
";

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, schema, ts_source, FileKind::TypeScript);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should NOT warn on trainer1 because TrainerBasic includes id
        let trainer_warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("'Trainer'"))
            .collect();
        assert_eq!(
            trainer_warnings.len(),
            0,
            "Issue #376: Should not warn on Trainer in TypeScript when fragment in other block contains id: {trainer_warnings:?}"
        );
    }

    #[test]
    fn test_same_fragment_used_in_multiple_fields() {
        // Issue #376: When the same fragment is used in multiple sibling fields,
        // the visited_fragments set prevents re-checking
        let db = RootDatabase::default();
        let rule = RequireIdFieldRuleImpl;

        let schema = r"
type Query {
    battle(id: ID!): Battle
}

type Battle {
    id: ID!
    trainer1: Trainer
    trainer2: Trainer
}

type Trainer {
    id: ID!
    name: String
}
";

        let source = r"
fragment TrainerBasic on Trainer {
    id
    name
}

fragment BattleDetailed on Battle {
    trainer1 {
        ...TrainerBasic
    }
    trainer2 {
        ...TrainerBasic
    }
}
";

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, schema, source, FileKind::ExecutableGraphQL);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files);

        // Should NOT warn on trainer1 OR trainer2 because TrainerBasic includes id
        // Bug: visited_fragments might prevent second check of TrainerBasic
        let trainer_warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("'Trainer'"))
            .collect();
        assert_eq!(
            trainer_warnings.len(),
            0,
            "Issue #376: Should not warn on any Trainer field when fragment spread contains id: {trainer_warnings:?}"
        );
    }
}
