use crate::context::DocumentSchemaContext;
use apollo_parser::cst::{self, CstNode};
use graphql_project::{Diagnostic, DocumentIndex, Position, Range, SchemaIndex};
use std::collections::HashSet;

use super::DocumentSchemaRule;

/// Lint rule that checks if an `id` field is requested when available on a type
pub struct RequireIdFieldRule;

impl DocumentSchemaRule for RequireIdFieldRule {
    fn name(&self) -> &'static str {
        "require_id_field"
    }

    fn description(&self) -> &'static str {
        "Requires that the 'id' field be requested in selection sets when available on a type"
    }

    fn check(&self, ctx: &DocumentSchemaContext) -> Vec<Diagnostic> {
        let document = ctx.document;
        let schema_index = ctx.schema;
        let fragments = ctx.fragments;
        let mut diagnostics = Vec::new();

        let doc_cst = ctx.parsed.document();

        for definition in doc_cst.definitions() {
            match definition {
                cst::Definition::OperationDefinition(operation) => {
                    let root_type_name = match operation.operation_type() {
                        Some(op_type) if op_type.query_token().is_some() => {
                            schema_index.schema().schema_definition.query.as_ref()
                        }
                        Some(op_type) if op_type.mutation_token().is_some() => {
                            schema_index.schema().schema_definition.mutation.as_ref()
                        }
                        Some(op_type) if op_type.subscription_token().is_some() => schema_index
                            .schema()
                            .schema_definition
                            .subscription
                            .as_ref(),
                        None => schema_index.schema().schema_definition.query.as_ref(),
                        _ => None,
                    };

                    if let Some(root_type_name) = root_type_name {
                        if let Some(selection_set) = operation.selection_set() {
                            let mut visited_fragments = HashSet::new();
                            check_selection_set_for_id(
                                &selection_set,
                                root_type_name.as_str(),
                                schema_index,
                                fragments,
                                &mut visited_fragments,
                                &mut diagnostics,
                                document,
                            );
                        }
                    }
                }
                cst::Definition::FragmentDefinition(fragment) => {
                    if let Some(type_condition) = fragment.type_condition() {
                        if let Some(named_type) = type_condition.named_type() {
                            if let Some(type_name) = named_type.name() {
                                let type_name_str = type_name.text();
                                if let Some(selection_set) = fragment.selection_set() {
                                    let mut visited_fragments = HashSet::new();
                                    check_selection_set_for_id(
                                        &selection_set,
                                        type_name_str.as_ref(),
                                        schema_index,
                                        fragments,
                                        &mut visited_fragments,
                                        &mut diagnostics,
                                        document,
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

#[allow(clippy::too_many_lines)]
fn check_selection_set_for_id(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    schema_index: &SchemaIndex,
    fragments: Option<&DocumentIndex>,
    visited_fragments: &mut HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
    document: &str,
) {
    let Some(fields) = schema_index.get_fields(parent_type_name) else {
        return;
    };

    let has_id_field = fields.iter().any(|f| f.name == "id");
    let mut has_id_in_selection = false;

    // First, recurse into all nested selections
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    let field_name_str = field_name.text();

                    if field_name_str == "id" {
                        has_id_in_selection = true;
                    }

                    if let Some(field_info) = fields.iter().find(|f| f.name == field_name_str) {
                        if let Some(nested_selection_set) = field.selection_set() {
                            let nested_type = field_info
                                .type_name
                                .trim_matches(|c| c == '[' || c == ']' || c == '!');

                            check_selection_set_for_id(
                                &nested_selection_set,
                                nested_type,
                                schema_index,
                                fragments,
                                visited_fragments,
                                diagnostics,
                                document,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                // Check if this fragment spread or its nested fragments contain the id field
                if let Some(fragment_name) = fragment_spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        let name_str = name.text().to_string();
                        if fragment_contains_id_field(
                            &name_str,
                            parent_type_name,
                            schema_index,
                            fragments,
                            visited_fragments,
                        ) {
                            has_id_in_selection = true;
                        }
                    }
                }
            }
            cst::Selection::InlineFragment(inline_fragment) => {
                // For inline fragments, we recursively check nested fields but don't enforce
                // the id requirement on the inline fragment itself since it's part of the parent's
                // selection set and the parent may have already selected id
                if let Some(nested_selection_set) = inline_fragment.selection_set() {
                    // Still need to check nested object selections within the inline fragment
                    for nested_selection in nested_selection_set.selections() {
                        if let cst::Selection::Field(nested_field) = nested_selection {
                            if let Some(field_name) = nested_field.name() {
                                let field_name_str = field_name.text();

                                // Determine the type context for this inline fragment
                                let type_name_owned =
                                    inline_fragment.type_condition().and_then(|type_condition| {
                                        type_condition.named_type().and_then(|named_type| {
                                            named_type.name().map(|name| name.text().to_string())
                                        })
                                    });
                                let type_name_ref =
                                    type_name_owned.as_deref().unwrap_or(parent_type_name);

                                // Get fields for the inline fragment's type
                                if let Some(inline_fields) = schema_index.get_fields(type_name_ref)
                                {
                                    if let Some(field_info) =
                                        inline_fields.iter().find(|f| f.name == field_name_str)
                                    {
                                        if let Some(field_selection_set) =
                                            nested_field.selection_set()
                                        {
                                            let nested_type = field_info
                                                .type_name
                                                .trim_matches(|c| c == '[' || c == ']' || c == '!');

                                            check_selection_set_for_id(
                                                &field_selection_set,
                                                nested_type,
                                                schema_index,
                                                fragments,
                                                visited_fragments,
                                                diagnostics,
                                                document,
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
    }

    // After recursing, check if this type has an id field and if it's missing from the selection
    if !has_id_field {
        return;
    }

    if !has_id_in_selection {
        let syntax_node = selection_set.syntax();
        let offset: usize = syntax_node.text_range().start().into();
        let line_col = offset_to_line_col(document, offset);

        let range = Range {
            start: Position {
                line: line_col.0,
                character: line_col.1,
            },
            end: Position {
                line: line_col.0,
                character: line_col.1 + 1,
            },
        };

        let message =
            format!("Selection set on type '{parent_type_name}' should include the 'id' field");

        diagnostics.push(
            Diagnostic::warning(range, message)
                .with_code("require_id_field")
                .with_source("graphql-linter"),
        );
    }
}

/// Check if a fragment (or its nested fragments) contains the id field
fn fragment_contains_id_field(
    fragment_name: &str,
    parent_type_name: &str,
    schema_index: &SchemaIndex,
    fragments: Option<&DocumentIndex>,
    visited_fragments: &mut HashSet<String>,
) -> bool {
    // Prevent infinite recursion with circular fragment references
    if visited_fragments.contains(fragment_name) {
        return false;
    }
    visited_fragments.insert(fragment_name.to_string());

    let Some(fragments_index) = fragments else {
        return false;
    };

    // Look up the fragment in the document index
    let Some(fragment_infos) = fragments_index.fragments.get(fragment_name) else {
        return false;
    };

    // Check all definitions of this fragment (there should typically be only one)
    for fragment_info in fragment_infos {
        // Parse the fragment's AST if we have it
        let parsed_ast = fragments_index
            .parsed_asts
            .get(&fragment_info.file_path)
            .map_or_else(
                || {
                    // For extracted blocks, find the one containing this fragment
                    fragments_index
                        .extracted_blocks
                        .get(&fragment_info.file_path)
                        .and_then(|blocks| {
                            blocks
                                .iter()
                                .find(|block| {
                                    // Parse and check if this block contains the fragment
                                    let doc = block.parsed.document();
                                    doc.definitions().any(|def| {
                                        if let cst::Definition::FragmentDefinition(frag) = def {
                                            frag.fragment_name()
                                                .and_then(|name| name.name())
                                                .is_some_and(|name| name.text() == fragment_name)
                                        } else {
                                            false
                                        }
                                    })
                                })
                                .map(|block| &block.parsed)
                        })
                },
                Some,
            );

        let Some(ast) = parsed_ast else {
            continue;
        };

        // Find the fragment definition in the AST
        let doc = ast.document();
        for definition in doc.definitions() {
            if let cst::Definition::FragmentDefinition(fragment) = definition {
                // Check if this is the fragment we're looking for
                let is_target_fragment = fragment
                    .fragment_name()
                    .and_then(|name| name.name())
                    .is_some_and(|name| name.text() == fragment_name);

                if !is_target_fragment {
                    continue;
                }

                // Get the fragment's type condition
                let fragment_type = fragment
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map_or_else(|| parent_type_name.to_string(), |n| n.text().to_string());

                // Check if the fragment's selection set contains the id field
                if let Some(selection_set) = fragment.selection_set() {
                    if selection_set_contains_id_field(
                        &selection_set,
                        &fragment_type,
                        schema_index,
                        fragments,
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

/// Check if a selection set directly contains the id field or references fragments that do
fn selection_set_contains_id_field(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    schema_index: &SchemaIndex,
    fragments: Option<&DocumentIndex>,
    visited_fragments: &mut HashSet<String>,
) -> bool {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                // Check if this field is the id field
                if let Some(field_name) = field.name() {
                    if field_name.text() == "id" {
                        return true;
                    }

                    // Recursively check nested selection sets
                    if let Some(nested_selection_set) = field.selection_set() {
                        let Some(fields) = schema_index.get_fields(parent_type_name) else {
                            continue;
                        };

                        let field_name_str = field_name.text();
                        if let Some(field_info) = fields.iter().find(|f| f.name == field_name_str) {
                            let nested_type = field_info
                                .type_name
                                .trim_matches(|c| c == '[' || c == ']' || c == '!');

                            if selection_set_contains_id_field(
                                &nested_selection_set,
                                nested_type,
                                schema_index,
                                fragments,
                                visited_fragments,
                            ) {
                                return true;
                            }
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                // Recursively check if the fragment contains id
                if let Some(fragment_name) = fragment_spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        let name_str = name.text().to_string();
                        if fragment_contains_id_field(
                            &name_str,
                            parent_type_name,
                            schema_index,
                            fragments,
                            visited_fragments,
                        ) {
                            return true;
                        }
                    }
                }
            }
            cst::Selection::InlineFragment(inline_fragment) => {
                // Check inline fragment's selection set
                if let Some(selection_set) = inline_fragment.selection_set() {
                    let type_name_owned =
                        inline_fragment.type_condition().and_then(|type_condition| {
                            type_condition.named_type().and_then(|named_type| {
                                named_type.name().map(|name| name.text().to_string())
                            })
                        });
                    let type_name_ref = type_name_owned.as_deref().unwrap_or(parent_type_name);

                    if selection_set_contains_id_field(
                        &selection_set,
                        type_name_ref,
                        schema_index,
                        fragments,
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

fn offset_to_line_col(document: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    let mut current_offset = 0;

    for ch in document.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }

        current_offset += ch.len_utf8();
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::DocumentSchemaContext;
    use graphql_project::Severity;

    #[test]
    fn test_missing_id_field() {
        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                email: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        let document = r"
            query GetUser($userId: ID!) {
                user(id: $userId) {
                    name
                    email
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 1, "Should have exactly one diagnostic");
        assert!(diagnostics[0].message.contains("id"));
        assert!(diagnostics[0].message.contains("User"));
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn test_with_id_field_present() {
        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                email: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        let document = r"
            query GetUser($userId: ID!) {
                user(id: $userId) {
                    id
                    name
                    email
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 0, "Should have no diagnostics");
    }

    #[test]
    fn test_type_without_id_field() {
        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                settings: Settings
            }

            type Settings {
                theme: String!
                language: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        let document = r"
            query GetSettings {
                settings {
                    theme
                    language
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(
            diagnostics.len(),
            0,
            "Should have no diagnostics when type has no id field"
        );
    }

    #[test]
    fn test_nested_selection_sets() {
        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                posts: [Post!]!
            }

            type Post {
                id: ID!
                title: String!
                content: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        let document = r"
            query GetUser($userId: ID!) {
                user(id: $userId) {
                    id
                    name
                    posts {
                        title
                        content
                    }
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(
            diagnostics.len(),
            1,
            "Should have one diagnostic for missing id in Post"
        );
        assert!(diagnostics[0].message.contains("Post"));
    }

    #[test]
    fn test_fragment_with_missing_id() {
        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                email: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        let document = r"
            fragment UserInfo on User {
                name
                email
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(
            diagnostics.len(),
            1,
            "Should have one diagnostic for missing id in fragment"
        );
        assert!(diagnostics[0].message.contains("User"));
    }

    #[test]
    fn test_inline_fragment() {
        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                node(id: ID!): Node
            }

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
        );

        let rule = RequireIdFieldRule;

        let document = r"
            query GetNode($nodeId: ID!) {
                node(id: $nodeId) {
                    id
                    ... on User {
                        name
                    }
                    ... on Post {
                        title
                    }
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document,
            file_name: "test.graphql",
            schema: &schema,
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 0, "Should have no diagnostics");
    }

    #[test]
    fn test_fragment_spread_with_id_field() {
        use graphql_project::{DocumentIndex, FragmentInfo};
        use std::collections::HashMap;
        use std::sync::Arc;

        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                email: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        // Fragment that includes the id field
        let fragment_document = r"
            fragment UserInfo on User {
                id
                name
                email
            }
        ";

        // Operation that uses the fragment
        let operation_document = r"
            query GetUser($userId: ID!) {
                user(id: $userId) {
                    ...UserInfo
                }
            }
        ";

        // Create a document index with the fragment
        let mut parsed_asts = HashMap::new();
        let fragment_parsed = apollo_parser::Parser::new(fragment_document).parse();
        parsed_asts.insert("fragment.graphql".to_string(), Arc::new(fragment_parsed));

        let mut fragments = HashMap::new();
        fragments.insert(
            "UserInfo".to_string(),
            vec![FragmentInfo {
                name: "UserInfo".to_string(),
                type_condition: "User".to_string(),
                file_path: "fragment.graphql".to_string(),
                line: 1,
                column: 22,
            }],
        );

        let document_index = DocumentIndex {
            parsed_asts,
            fragments,
            ..Default::default()
        };

        // Check the operation
        let operation_parsed = apollo_parser::Parser::new(operation_document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document: operation_document,
            file_name: "operation.graphql",
            schema: &schema,
            fragments: Some(&document_index),
            parsed: &operation_parsed,
        });

        assert_eq!(
            diagnostics.len(),
            0,
            "Should have no diagnostics when fragment includes id field"
        );
    }

    #[test]
    fn test_fragment_spread_without_id_field() {
        use graphql_project::{DocumentIndex, FragmentInfo};
        use std::collections::HashMap;
        use std::sync::Arc;

        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                email: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        // Fragment that does NOT include the id field
        let fragment_document = r"
            fragment UserInfo on User {
                name
                email
            }
        ";

        // Operation that uses the fragment
        let operation_document = r"
            query GetUser($userId: ID!) {
                user(id: $userId) {
                    ...UserInfo
                }
            }
        ";

        // Create a document index with the fragment
        let mut parsed_asts = HashMap::new();
        let fragment_parsed = apollo_parser::Parser::new(fragment_document).parse();
        parsed_asts.insert("fragment.graphql".to_string(), Arc::new(fragment_parsed));

        let mut fragments = HashMap::new();
        fragments.insert(
            "UserInfo".to_string(),
            vec![FragmentInfo {
                name: "UserInfo".to_string(),
                type_condition: "User".to_string(),
                file_path: "fragment.graphql".to_string(),
                line: 1,
                column: 22,
            }],
        );

        let document_index = DocumentIndex {
            parsed_asts,
            fragments,
            ..Default::default()
        };

        // Check the operation
        let operation_parsed = apollo_parser::Parser::new(operation_document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document: operation_document,
            file_name: "operation.graphql",
            schema: &schema,
            fragments: Some(&document_index),
            parsed: &operation_parsed,
        });

        assert_eq!(
            diagnostics.len(),
            1,
            "Should have one diagnostic when fragment doesn't include id field"
        );
        assert!(diagnostics[0].message.contains("User"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_nested_fragment_spreads() {
        use graphql_project::{DocumentIndex, FragmentInfo};
        use std::collections::HashMap;
        use std::sync::Arc;

        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                profile: Profile!
            }

            type Profile {
                id: ID!
                bio: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        // Base fragment with id
        let base_fragment = r"
            fragment UserBase on User {
                id
                name
            }
        ";

        // Profile fragment with id
        let profile_fragment = r"
            fragment ProfileInfo on Profile {
                id
                bio
            }
        ";

        // Composite fragment that uses other fragments
        let composite_fragment = r"
            fragment UserWithProfile on User {
                ...UserBase
                profile {
                    ...ProfileInfo
                }
            }
        ";

        // Operation that uses the composite fragment
        let operation_document = r"
            query GetUser($userId: ID!) {
                user(id: $userId) {
                    ...UserWithProfile
                }
            }
        ";

        // Create a document index with all fragments
        let mut parsed_asts = HashMap::new();
        parsed_asts.insert(
            "base.graphql".to_string(),
            Arc::new(apollo_parser::Parser::new(base_fragment).parse()),
        );
        parsed_asts.insert(
            "profile.graphql".to_string(),
            Arc::new(apollo_parser::Parser::new(profile_fragment).parse()),
        );
        parsed_asts.insert(
            "composite.graphql".to_string(),
            Arc::new(apollo_parser::Parser::new(composite_fragment).parse()),
        );

        let mut fragments = HashMap::new();
        fragments.insert(
            "UserBase".to_string(),
            vec![FragmentInfo {
                name: "UserBase".to_string(),
                type_condition: "User".to_string(),
                file_path: "base.graphql".to_string(),
                line: 1,
                column: 22,
            }],
        );
        fragments.insert(
            "ProfileInfo".to_string(),
            vec![FragmentInfo {
                name: "ProfileInfo".to_string(),
                type_condition: "Profile".to_string(),
                file_path: "profile.graphql".to_string(),
                line: 1,
                column: 22,
            }],
        );
        fragments.insert(
            "UserWithProfile".to_string(),
            vec![FragmentInfo {
                name: "UserWithProfile".to_string(),
                type_condition: "User".to_string(),
                file_path: "composite.graphql".to_string(),
                line: 1,
                column: 22,
            }],
        );

        let document_index = DocumentIndex {
            parsed_asts,
            fragments,
            ..Default::default()
        };

        // Check the operation
        let operation_parsed = apollo_parser::Parser::new(operation_document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document: operation_document,
            file_name: "operation.graphql",
            schema: &schema,
            fragments: Some(&document_index),
            parsed: &operation_parsed,
        });

        assert_eq!(
            diagnostics.len(),
            0,
            "Should have no diagnostics when nested fragments include id fields"
        );
    }

    #[test]
    fn test_fragment_spread_with_additional_fields() {
        use graphql_project::{DocumentIndex, FragmentInfo};
        use std::collections::HashMap;
        use std::sync::Arc;

        let schema = SchemaIndex::from_schema(
            r"
            type Query {
                user(id: ID!): User
            }

            type User {
                id: ID!
                name: String!
                email: String!
            }
            ",
        );

        let rule = RequireIdFieldRule;

        // Fragment without id
        let fragment_document = r"
            fragment UserInfo on User {
                name
                email
            }
        ";

        // Operation that adds id alongside the fragment
        let operation_document = r"
            query GetUser($userId: ID!) {
                user(id: $userId) {
                    id
                    ...UserInfo
                }
            }
        ";

        // Create a document index with the fragment
        let mut parsed_asts = HashMap::new();
        let fragment_parsed = apollo_parser::Parser::new(fragment_document).parse();
        parsed_asts.insert("fragment.graphql".to_string(), Arc::new(fragment_parsed));

        let mut fragments = HashMap::new();
        fragments.insert(
            "UserInfo".to_string(),
            vec![FragmentInfo {
                name: "UserInfo".to_string(),
                type_condition: "User".to_string(),
                file_path: "fragment.graphql".to_string(),
                line: 1,
                column: 22,
            }],
        );

        let document_index = DocumentIndex {
            parsed_asts,
            fragments,
            ..Default::default()
        };

        // Check the operation
        let operation_parsed = apollo_parser::Parser::new(operation_document).parse();
        let diagnostics = rule.check(&DocumentSchemaContext {
            document: operation_document,
            file_name: "operation.graphql",
            schema: &schema,
            fragments: Some(&document_index),
            parsed: &operation_parsed,
        });

        assert_eq!(
            diagnostics.len(),
            0,
            "Should have no diagnostics when id is included alongside fragment"
        );
    }
}
