use crate::diagnostics::{CodeSuggestion, LintDiagnostic, LintSeverity};
use crate::traits::{DocumentSchemaLintRule, LintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use std::collections::HashMap;
use std::sync::Arc;

/// Comprehensive rule that detects usage of deprecated schema elements
///
/// This rule checks for:
/// - Deprecated fields in object/interface types
/// - Deprecated arguments in field/directive calls
/// - Deprecated enum values
pub struct NoDeprecatedRuleImpl;

impl LintRule for NoDeprecatedRuleImpl {
    fn name(&self) -> &'static str {
        "noDeprecated"
    }

    fn description(&self) -> &'static str {
        "Warns when using deprecated fields, arguments, or enum values"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl DocumentSchemaLintRule for NoDeprecatedRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        // Parse the file (cached by Salsa)
        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        // Get schema types from HIR
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Unified: process all documents (works for both pure GraphQL and TS/JS)
        for doc in parse.documents() {
            let doc_cst = doc.tree.document();
            check_document_for_deprecated(&doc_cst, schema_types, &mut diagnostics, &doc);
        }

        diagnostics
    }
}

/// Check a document for deprecated field, argument, and enum usage
fn check_document_for_deprecated(
    doc_cst: &cst::Document,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<LintDiagnostic>,
    doc: &graphql_syntax::DocumentRef<'_>,
) {
    for definition in doc_cst.definitions() {
        match definition {
            cst::Definition::OperationDefinition(operation) => {
                use super::{get_operation_kind, OperationKind};
                // Determine root type based on operation type
                let root_type_name =
                    operation
                        .operation_type()
                        .map_or("Query", |op_type| match get_operation_kind(&op_type) {
                            OperationKind::Query => "Query",
                            OperationKind::Mutation => "Mutation",
                            OperationKind::Subscription => "Subscription",
                        });

                if let Some(selection_set) = operation.selection_set() {
                    check_selection_set(
                        &selection_set,
                        Some(root_type_name),
                        schema_types,
                        diagnostics,
                        doc,
                    );
                }
            }
            cst::Definition::FragmentDefinition(fragment) => {
                // Get the type condition for the fragment
                let type_name = fragment
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|name| name.text().to_string());

                if let Some(selection_set) = fragment.selection_set() {
                    check_selection_set(
                        &selection_set,
                        type_name.as_deref(),
                        schema_types,
                        diagnostics,
                        doc,
                    );
                }
            }
            _ => {
                // Schema definitions don't need deprecation checks here
            }
        }
    }
}

/// Check a selection set for deprecated field usage
fn check_selection_set(
    selection_set: &cst::SelectionSet,
    parent_type_name: Option<&str>,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<LintDiagnostic>,
    doc: &graphql_syntax::DocumentRef<'_>,
) {
    let Some(parent_type_name) = parent_type_name else {
        // Skip if we don't know the parent type
        return;
    };

    let Some(parent_type) = schema_types.get(parent_type_name) else {
        // Parent type not found in schema
        return;
    };

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name_node) = field.name() {
                    let field_name = field_name_node.text();

                    // Check if this field is deprecated
                    if let Some(field_def) = parent_type
                        .fields
                        .iter()
                        .find(|f| f.name.as_ref() == field_name.as_ref())
                    {
                        if field_def.is_deprecated {
                            let reason = field_def
                                .deprecation_reason
                                .as_deref()
                                .unwrap_or("No longer supported");
                            let syntax_node = field_name_node.syntax();
                            let offset: usize = syntax_node.text_range().start().into();

                            // graphql-eslint format:
                            //   `Field "name" is marked as deprecated in your GraphQL schema (reason: ...)`
                            // displayNodeName uses the kind label ("field")
                            // capitalized in the rule's reporter.
                            let message = format!(
                                "Field \"{}\" is marked as deprecated in your GraphQL schema (reason: {})",
                                field_name.as_ref(),
                                reason
                            );

                            // Mirror upstream's `fixer.remove(node)`: remove the
                            // entire Field selection (including args/sub-selections).
                            // Use block-local offsets — the convert_fix layer
                            // handles block→file translation for embedded blocks.
                            let field_range = field.syntax().text_range();
                            let field_start: usize = field_range.start().into();
                            let field_end: usize = field_range.end().into();
                            let suggestion = CodeSuggestion::delete(
                                format!("Remove field \"{}\"", field_name.as_ref()),
                                field_start,
                                field_end,
                            );

                            diagnostics.push(
                                LintDiagnostic::new(
                                    doc.span(offset, offset + field_name.as_ref().len()),
                                    LintSeverity::Warning,
                                    message,
                                    "noDeprecated",
                                )
                                .with_message_id("no-deprecated")
                                .with_help(
                                    "Use the replacement field if one is specified in the deprecation reason",
                                )
                                .with_suggestion(suggestion)
                                .with_tag(crate::diagnostics::DiagnosticTag::Deprecated),
                            );
                        }

                        // Check arguments for deprecation
                        if let Some(arguments) = field.arguments() {
                            for arg in arguments.arguments() {
                                if let Some(arg_name_node) = arg.name() {
                                    let arg_name = arg_name_node.text();

                                    if let Some(arg_def) = field_def
                                        .arguments
                                        .iter()
                                        .find(|a| a.name.as_ref() == arg_name.as_ref())
                                    {
                                        if arg_def.is_deprecated {
                                            let reason = arg_def
                                                .deprecation_reason
                                                .as_deref()
                                                .unwrap_or("No longer supported");
                                            let syntax_node = arg_name_node.syntax();
                                            let offset: usize =
                                                syntax_node.text_range().start().into();

                                            let message = format!(
                                                "Argument \"{}\" is marked as deprecated in your GraphQL schema (reason: {})",
                                                arg_name.as_ref(),
                                                reason
                                            );

                                            // Mirror upstream's `fixer.remove(node)`:
                                            // remove the entire Argument node.
                                            let arg_range = arg.syntax().text_range();
                                            let arg_start: usize = arg_range.start().into();
                                            let arg_end: usize = arg_range.end().into();
                                            let suggestion = CodeSuggestion::delete(
                                                format!(
                                                    "Remove argument \"{}\"",
                                                    arg_name.as_ref()
                                                ),
                                                arg_start,
                                                arg_end,
                                            );

                                            diagnostics.push(
                                                LintDiagnostic::new(
                                                    doc.span(offset, offset + arg_name.as_ref().len()),
                                                    LintSeverity::Warning,
                                                    message,
                                                    "noDeprecated",
                                                )
                                                .with_message_id("no-deprecated")
                                                .with_help(
                                                    "Use the replacement field if one is specified in the deprecation reason",
                                                )
                                                .with_suggestion(suggestion)
                                                .with_tag(crate::diagnostics::DiagnosticTag::Deprecated),
                                            );
                                        }

                                        // Check value: enum values (best-effort) and
                                        // input object fields (type-aware via arg type).
                                        if let Some(value) = arg.value() {
                                            check_value_for_deprecated(
                                                &value,
                                                Some(arg_def.type_ref.name.as_ref()),
                                                schema_types,
                                                diagnostics,
                                                doc,
                                            );
                                        }
                                    } else {
                                        // Arg not in schema — still check enum/input values
                                        if let Some(value) = arg.value() {
                                            check_value_for_deprecated(
                                                &value,
                                                None,
                                                schema_types,
                                                diagnostics,
                                                doc,
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        // Recurse into nested selection set with field's return type
                        if let Some(nested_selection_set) = field.selection_set() {
                            // Get the named type (unwrap list/non-null wrappers)
                            let field_type_name = field_def.type_ref.name.as_ref();
                            check_selection_set(
                                &nested_selection_set,
                                Some(field_type_name),
                                schema_types,
                                diagnostics,
                                doc,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(_spread) => {
                // Fragment spreads don't directly use schema elements
                // The fragment definition itself will be checked separately
            }
            cst::Selection::InlineFragment(inline) => {
                // Get type condition or fallback to parent type
                let type_name = inline
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|name| name.text().to_string());

                let type_name_ref = type_name.as_deref().or(Some(parent_type_name));

                if let Some(selection_set) = inline.selection_set() {
                    check_selection_set(
                        &selection_set,
                        type_name_ref,
                        schema_types,
                        diagnostics,
                        doc,
                    );
                }
            }
        }
    }
}

/// Check a value for deprecated enum values and deprecated input object fields.
///
/// `expected_type_name` is the named type of this value position (e.g. the
/// input type for an object literal). When present and the type is an input
/// object, we use it for precise field-level deprecation checks rather than
/// falling back to schema-wide enum scanning.
fn check_value_for_deprecated(
    value: &cst::Value,
    expected_type_name: Option<&str>,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    diagnostics: &mut Vec<LintDiagnostic>,
    doc: &graphql_syntax::DocumentRef<'_>,
) {
    match value {
        cst::Value::EnumValue(enum_value) => {
            if let Some(enum_name_node) = enum_value.name() {
                let enum_name = enum_name_node.text();

                // Try to find which enum type this value belongs to.
                // Best-effort across all enum types since we may not have
                // full type context at this call site.
                for type_def in schema_types.values() {
                    if type_def.kind == graphql_hir::TypeDefKind::Enum {
                        if let Some(enum_val) = type_def
                            .enum_values
                            .iter()
                            .find(|v| v.name.as_ref() == enum_name.as_ref())
                        {
                            if enum_val.is_deprecated {
                                let reason = enum_val
                                    .deprecation_reason
                                    .as_deref()
                                    .unwrap_or("No longer supported");
                                let syntax_node = enum_name_node.syntax();
                                let offset: usize = syntax_node.text_range().start().into();

                                let message = format!(
                                    "Enum \"{}\" is marked as deprecated in your GraphQL schema (reason: {})",
                                    enum_name.as_ref(),
                                    reason
                                );

                                // Mirror upstream's `fixer.remove(node)`: remove
                                // the EnumValue node (the bare identifier).
                                let ev_range = enum_value.syntax().text_range();
                                let ev_start: usize = ev_range.start().into();
                                let ev_end: usize = ev_range.end().into();
                                let suggestion = CodeSuggestion::delete(
                                    format!("Remove enum \"{}\"", enum_name.as_ref()),
                                    ev_start,
                                    ev_end,
                                );

                                diagnostics.push(
                                    LintDiagnostic::new(
                                        doc.span(offset, offset + enum_name.as_ref().len()),
                                        LintSeverity::Warning,
                                        message,
                                        "noDeprecated",
                                    )
                                    .with_message_id("no-deprecated")
                                    .with_help(
                                        "Use the replacement field if one is specified in the deprecation reason",
                                    )
                                    .with_suggestion(suggestion)
                                    .with_tag(crate::diagnostics::DiagnosticTag::Deprecated),
                                );
                                // Found the enum; no need to scan remaining types.
                                break;
                            }
                        }
                    }
                }
            }
        }
        cst::Value::ListValue(list) => {
            for item in list.values() {
                check_value_for_deprecated(
                    &item,
                    expected_type_name,
                    schema_types,
                    diagnostics,
                    doc,
                );
            }
        }
        cst::Value::ObjectValue(obj) => {
            // When we know the input type, check each field for deprecation and
            // recurse with its nested type. Without a type, recurse with None.
            let input_type_def = expected_type_name.and_then(|name| schema_types.get(name));

            for obj_field in obj.object_fields() {
                let Some(field_name_node) = obj_field.name() else {
                    continue;
                };
                let field_name = field_name_node.text();

                // If we have the input type, check this field for deprecation.
                let nested_type_name: Option<&str> = if let Some(type_def) = input_type_def {
                    if let Some(field_sig) = type_def
                        .fields
                        .iter()
                        .find(|f| f.name.as_ref() == field_name.as_ref())
                    {
                        if field_sig.is_deprecated {
                            let reason = field_sig
                                .deprecation_reason
                                .as_deref()
                                .unwrap_or("No longer supported");
                            let syntax_node = field_name_node.syntax();
                            let offset: usize = syntax_node.text_range().start().into();

                            let message = format!(
                                "Object field \"{}\" is marked as deprecated in your GraphQL schema (reason: {})",
                                field_name.as_ref(),
                                reason
                            );

                            let field_range = obj_field.syntax().text_range();
                            let field_start: usize = field_range.start().into();
                            let field_end: usize = field_range.end().into();
                            let suggestion = CodeSuggestion::delete(
                                format!("Remove field \"{}\"", field_name.as_ref()),
                                field_start,
                                field_end,
                            );

                            diagnostics.push(
                                LintDiagnostic::new(
                                    doc.span(offset, offset + field_name.as_ref().len()),
                                    LintSeverity::Warning,
                                    message,
                                    "noDeprecated",
                                )
                                .with_message_id("no-deprecated")
                                .with_help(
                                    "Use the replacement field if one is specified in the deprecation reason",
                                )
                                .with_suggestion(suggestion)
                                .with_tag(crate::diagnostics::DiagnosticTag::Deprecated),
                            );
                        }
                        // Pass the nested field's type down for further recursion.
                        Some(field_sig.type_ref.name.as_ref())
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(field_value) = obj_field.value() {
                    check_value_for_deprecated(
                        &field_value,
                        nested_type_name,
                        schema_types,
                        diagnostics,
                        doc,
                    );
                }
            }
        }
        _ => {
            // String, Int, Float, BooleanValue, Variable, NullValue — no deprecation to check.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::DocumentSchemaLintRule;
    use graphql_base_db::{DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language};
    use graphql_ide_db::RootDatabase;

    fn create_test_project(
        db: &dyn graphql_hir::GraphQLHirDatabase,
        schema_source: &str,
        document_source: &str,
    ) -> (FileId, FileContent, FileMetadata, ProjectFiles) {
        let schema_file_id = FileId::new(0);
        let schema_content = FileContent::new(db, Arc::from(schema_source));
        let schema_metadata = FileMetadata::new(
            db,
            schema_file_id,
            FileUri::new("file:///schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let doc_file_id = FileId::new(1);
        let doc_content = FileContent::new(db, Arc::from(document_source));
        let doc_metadata = FileMetadata::new(
            db,
            doc_file_id,
            FileUri::new("file:///query.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
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
        let project_files = ProjectFiles::new(
            db,
            schema_file_ids,
            document_file_ids,
            graphql_base_db::ResolvedSchemaFileIds::new(db, std::sync::Arc::new(vec![])),
            file_entry_map,
            graphql_base_db::FilePathMap::new(
                db,
                Arc::new(std::collections::HashMap::new()),
                Arc::new(std::collections::HashMap::new()),
            ),
        );

        (doc_file_id, doc_content, doc_metadata, project_files)
    }

    const SCHEMA_WITH_DEPRECATIONS: &str = r#"
type Query {
    user(id: ID!): User
    oldUser(id: ID!): User @deprecated(reason: "Use user instead")
}

type User {
    id: ID!
    name: String!
    email: String!
    username: String @deprecated(reason: "Use name instead")
    posts(status: PostStatus): [Post!]!
}

type Post {
    id: ID!
    title: String!
}

enum PostStatus {
    PUBLISHED
    DRAFT
    ARCHIVED @deprecated(reason: "Use DRAFT instead")
}
"#;

    #[test]
    fn test_deprecated_field_warning() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        let source = r#"
query GetUser {
    user(id: "1") {
        id
        username
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, SCHEMA_WITH_DEPRECATIONS, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        // graphql-eslint parity: messages quote names with double quotes.
        assert_eq!(
            diagnostics[0].message,
            "Field \"username\" is marked as deprecated in your GraphQL schema (reason: Use name instead)"
        );

        // Enrichment: deprecated-class rules should carry help text and the
        // Deprecated tag so editors can render them appropriately.
        assert!(
            diagnostics[0].help.is_some(),
            "noDeprecated should include help text"
        );
        assert!(
            diagnostics[0]
                .tags
                .contains(&crate::diagnostics::DiagnosticTag::Deprecated),
            "noDeprecated should tag diagnostics as Deprecated"
        );
    }

    #[test]
    fn test_no_warning_for_non_deprecated_fields() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

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
            create_test_project(&db, SCHEMA_WITH_DEPRECATIONS, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_deprecated_root_field_warning() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        let source = r#"
query GetUser {
    oldUser(id: "1") {
        id
        name
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, SCHEMA_WITH_DEPRECATIONS, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("oldUser"));
        assert!(diagnostics[0].message.contains("deprecated"));
    }

    #[test]
    fn test_deprecated_enum_value_warning() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        let source = r#"
query GetUserPosts {
    user(id: "1") {
        posts(status: ARCHIVED) {
            id
            title
        }
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, SCHEMA_WITH_DEPRECATIONS, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("ARCHIVED"));
        assert!(diagnostics[0].message.contains("deprecated"));
    }

    #[test]
    fn test_non_deprecated_enum_value_no_warning() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        let source = r#"
query GetUserPosts {
    user(id: "1") {
        posts(status: PUBLISHED) {
            id
            title
        }
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, SCHEMA_WITH_DEPRECATIONS, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_multiple_deprecated_usages() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        let source = r#"
query GetUser {
    oldUser(id: "1") {
        id
        username
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, SCHEMA_WITH_DEPRECATIONS, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 2);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("oldUser")));
        assert!(messages.iter().any(|m| m.contains("username")));
    }

    #[test]
    fn test_deprecated_field_in_fragment() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        let source = r#"
fragment UserFields on User {
    id
    username
}

query GetUser {
    user(id: "1") {
        ...UserFields
    }
}
"#;

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, SCHEMA_WITH_DEPRECATIONS, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("username"));
    }

    #[test]
    fn test_mutation_with_deprecated_field() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        // A field with an explicit deprecation reason should produce a diagnostic.
        let schema = r#"
type Query {
    user: User
}

type Mutation {
    updateUser(id: ID!): User
}

type User {
    id: ID!
    name: String!
    oldField: String @deprecated(reason: "Use name instead")
}
"#;

        let source = r#"
mutation UpdateUser {
    updateUser(id: "1") {
        oldField
    }
}
"#;

        let (file_id, content, metadata, project_files) = create_test_project(&db, schema, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("oldField"));
    }

    #[test]
    fn test_nested_selection_deprecated_field() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        let schema = r#"
type Query {
    user: User
}

type User {
    id: ID!
    profile: Profile
}

type Profile {
    bio: String
    oldAvatar: String @deprecated(reason: "Use avatar instead")
}
"#;

        let source = r"
query GetUser {
    user {
        id
        profile {
            bio
            oldAvatar
        }
    }
}
";

        let (file_id, content, metadata, project_files) = create_test_project(&db, schema, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("oldAvatar"));
    }

    #[test]
    fn test_inline_fragment_deprecated_field() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        let schema = r#"
type Query {
    node(id: ID!): Node
}

interface Node {
    id: ID!
}

type User implements Node {
    id: ID!
    name: String!
    oldField: String @deprecated(reason: "Use name instead")
}
"#;

        let source = r#"
query GetNode {
    node(id: "1") {
        id
        ... on User {
            name
            oldField
        }
    }
}
"#;

        let (file_id, content, metadata, project_files) = create_test_project(&db, schema, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("oldField"));
    }

    #[test]
    fn test_deprecated_without_reason() {
        let db = RootDatabase::default();
        let rule = NoDeprecatedRuleImpl;

        // Bare @deprecated (no explicit reason) uses the GraphQL spec default
        // reason "No longer supported", matching graphql-eslint upstream behavior.
        let schema = r"
type Query {
    user: User
}

type User {
    id: ID!
    legacyField: String @deprecated
}
";

        let source = r"
query GetUser {
    user {
        legacyField
    }
}
";

        let (file_id, content, metadata, project_files) = create_test_project(&db, schema, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Field \"legacyField\" is marked as deprecated in your GraphQL schema (reason: No longer supported)"
        );
    }
}
