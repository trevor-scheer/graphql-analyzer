use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::{FieldSignature, TypeDef, TypeDefKind};
use std::collections::HashMap;
use std::sync::Arc;

/// Lint rule that enforces Relay edge type conventions.
///
/// Edge types (identified by being returned from a connection type's `edges`
/// field) must have:
/// - A `node` field returning a named type (not a list)
/// - A `cursor` field returning `String`
///
/// Configurable options:
/// - `withEdgeSuffix` (default: true) - edge type names must end with "Edge"
/// - `shouldImplementNode` (default: true) - the `node` field type must
///   implement the `Node` interface
/// - `listTypeCanWrapOnlyEdgeType` (default: true) - list fields on
///   connection types may only wrap edge types
pub struct RelayEdgeTypesRuleImpl;

impl LintRule for RelayEdgeTypesRuleImpl {
    fn name(&self) -> &'static str {
        "relayEdgeTypes"
    }

    fn description(&self) -> &'static str {
        "Enforces Relay-compliant edge type definitions"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

#[derive(Debug)]
struct Options {
    with_edge_suffix: bool,
    should_implement_node: bool,
    list_type_can_wrap_only_edge_type: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            with_edge_suffix: true,
            should_implement_node: true,
            list_type_can_wrap_only_edge_type: true,
        }
    }
}

impl Options {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        let Some(obj) = value.and_then(|v| v.as_object()) else {
            return Self::default();
        };

        Self {
            with_edge_suffix: obj
                .get("withEdgeSuffix")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            should_implement_node: obj
                .get("shouldImplementNode")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            list_type_can_wrap_only_edge_type: obj
                .get("listTypeCanWrapOnlyEdgeType")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
        }
    }
}

/// Check if a type name looks like a connection type (ends with "Connection").
fn is_connection_type(name: &str) -> bool {
    name.ends_with("Connection")
}

/// Find the `edges` field on a connection type and return its inner type name.
fn find_edges_type(type_def: &TypeDef) -> Option<&FieldSignature> {
    type_def.fields.iter().find(|f| f.name.as_ref() == "edges")
}

impl StandaloneSchemaLintRule for RelayEdgeTypesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = Options::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Collect edge type names referenced by connection types, and validate
        // connection-level constraints at the same time.
        let mut edge_type_names: Vec<Arc<str>> = Vec::new();

        for type_def in schema_types.values() {
            if type_def.kind != TypeDefKind::Object || !is_connection_type(&type_def.name) {
                continue;
            }

            let Some(edges_field) = find_edges_type(type_def) else {
                continue;
            };

            // The edges field should be a list type
            if !edges_field.type_ref.is_list {
                continue;
            }

            let edge_type_name = &edges_field.type_ref.name;
            edge_type_names.push(Arc::clone(edge_type_name));

            // listTypeCanWrapOnlyEdgeType: check all list fields on the
            // connection type only wrap edge types
            if opts.list_type_can_wrap_only_edge_type {
                for field in &type_def.fields {
                    if field.type_ref.is_list && field.name.as_ref() != "edges" {
                        let wrapped_type = &field.type_ref.name;
                        // The wrapped type should be the edge type
                        if let Some(edge_def) = schema_types.get(wrapped_type.as_ref()) {
                            if edge_def.kind == TypeDefKind::Object
                                && !edge_def.name.ends_with("Edge")
                            {
                                let start: usize = field.name_range.start().into();
                                let end: usize = field.name_range.end().into();
                                diagnostics_by_file.entry(field.file_id).or_default().push(
                                    make_diagnostic(
                                        start,
                                        end,
                                        "A list type should only wrap an edge type.".to_string(),
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }

        // Now validate each edge type
        for edge_type_name in &edge_type_names {
            let Some(edge_def) = schema_types.get(edge_type_name.as_ref()) else {
                continue;
            };

            if edge_def.kind != TypeDefKind::Object {
                continue;
            }

            // withEdgeSuffix: edge type name must end with "Edge"
            if opts.with_edge_suffix && !edge_def.name.ends_with("Edge") {
                let start: usize = edge_def.name_range.start().into();
                let end: usize = edge_def.name_range.end().into();
                diagnostics_by_file
                    .entry(edge_def.file_id)
                    .or_default()
                    .push(make_diagnostic(
                        start,
                        end,
                        "Edge type must have \"Edge\" suffix.".to_string(),
                    ));
            }

            // Check for required `node` field
            let node_field = edge_def.fields.iter().find(|f| f.name.as_ref() == "node");
            match node_field {
                None => {
                    let start: usize = edge_def.name_range.start().into();
                    let end: usize = edge_def.name_range.end().into();
                    diagnostics_by_file
                        .entry(edge_def.file_id)
                        .or_default()
                        .push(
                            make_diagnostic(
                                start,
                                end,
                                "Edge type must contain a field `node` that return either a \
                                 Scalar, Enum, Object, Interface, Union, or a non-null wrapper \
                                 around one of those types."
                                    .to_string(),
                            )
                            .with_help("Add a 'node' field that returns the connected type"),
                        );
                }
                Some(node) => {
                    // node field must not be a list
                    if node.type_ref.is_list {
                        let start: usize = node.name_range.start().into();
                        let end: usize = node.name_range.end().into();
                        diagnostics_by_file
                            .entry(node.file_id)
                            .or_default()
                            .push(make_diagnostic(
                                start,
                                end,
                                "Field `node` must return either a Scalar, Enum, Object, \
                                 Interface, Union, or a non-null wrapper around one of those \
                                 types."
                                    .to_string(),
                            ));
                    }

                    // shouldImplementNode: the type returned by `node` must
                    // implement the Node interface
                    if opts.should_implement_node && !node.type_ref.is_list {
                        let node_type_name = &node.type_ref.name;
                        if let Some(node_type_def) = schema_types.get(node_type_name.as_ref()) {
                            let implements_node = node_type_def
                                .implements
                                .iter()
                                .any(|i| i.as_ref() == "Node");
                            if !implements_node
                                && matches!(
                                    node_type_def.kind,
                                    TypeDefKind::Object | TypeDefKind::Interface
                                )
                            {
                                let start: usize = node.name_range.start().into();
                                let end: usize = node.name_range.end().into();
                                diagnostics_by_file.entry(node.file_id).or_default().push(
                                    make_diagnostic(
                                        start,
                                        end,
                                        "Edge type's field `node` must implement `Node` \
                                         interface."
                                            .to_string(),
                                    )
                                    .with_help(format!(
                                        "Add 'implements Node' to the '{node_type_name}' type definition",
                                    )),
                                );
                            }
                        }
                    }
                }
            }

            // Check for required `cursor` field
            let cursor_field = edge_def.fields.iter().find(|f| f.name.as_ref() == "cursor");
            match cursor_field {
                None => {
                    let start: usize = edge_def.name_range.start().into();
                    let end: usize = edge_def.name_range.end().into();
                    diagnostics_by_file
                        .entry(edge_def.file_id)
                        .or_default()
                        .push(
                            make_diagnostic(
                                start,
                                end,
                                "Edge type must contain a field `cursor` that return either a \
                                 String, Scalar, or a non-null wrapper wrapper around one of \
                                 those types."
                                    .to_string(),
                            )
                            .with_help("Add a 'cursor: String!' field to the edge type"),
                        );
                }
                Some(cursor) => {
                    // cursor must return String (or a scalar)
                    let cursor_type = cursor.type_ref.name.as_ref();
                    if cursor_type != "String" {
                        // Check if it's at least a scalar
                        let is_scalar = schema_types
                            .get(cursor_type)
                            .is_some_and(|t| t.kind == TypeDefKind::Scalar)
                            || matches!(cursor_type, "Int" | "Float" | "Boolean" | "ID");
                        if !is_scalar {
                            let start: usize = cursor.name_range.start().into();
                            let end: usize = cursor.name_range.end().into();
                            diagnostics_by_file.entry(cursor.file_id).or_default().push(
                                make_diagnostic(
                                    start,
                                    end,
                                    "Field `cursor` must return either a String, Scalar, or a \
                                     non-null wrapper wrapper around one of those types."
                                        .to_string(),
                                ),
                            );
                        }
                    }
                }
            }
        }

        diagnostics_by_file
    }
}

fn make_diagnostic(start: usize, end: usize, message: String) -> LintDiagnostic {
    let span = graphql_syntax::SourceSpan {
        start,
        end,
        line_offset: 0,
        byte_offset: 0,
        source: None,
    };
    LintDiagnostic::new(span, LintSeverity::Warning, message, "relayEdgeTypes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneSchemaLintRule;
    use graphql_base_db::{
        DocumentFileIds, DocumentKind, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata,
        FileUri, Language, ProjectFiles, SchemaFileIds,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_schema_project(db: &RootDatabase, schema: &str) -> ProjectFiles {
        let file_id = FileId::new(0);
        let content = FileContent::new(db, Arc::from(schema));
        let metadata = FileMetadata::new(
            db,
            file_id,
            FileUri::new("file:///schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let entry = FileEntry::new(db, content, metadata);
        let mut entries = std::collections::HashMap::new();
        entries.insert(file_id, entry);
        let schema_file_ids = SchemaFileIds::new(db, Arc::new(vec![file_id]));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map = FileEntryMap::new(db, Arc::new(entries));
        ProjectFiles::new(
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
        )
    }

    fn check_with_options(
        schema: &str,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = RelayEdgeTypesRuleImpl;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, options);
        diagnostics.into_values().flatten().collect()
    }

    fn check(schema: &str) -> Vec<LintDiagnostic> {
        check_with_options(schema, None)
    }

    #[test]
    fn valid_relay_edge_type() {
        let schema = r"
            interface Node {
                id: ID!
            }
            type User implements Node {
                id: ID!
                name: String!
            }
            type UserEdge {
                node: User
                cursor: String!
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert!(
            diagnostics.is_empty(),
            "Expected no diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn missing_node_field() {
        let schema = r"
            type UserEdge {
                cursor: String!
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Edge type must contain a field `node`"));
    }

    #[test]
    fn missing_cursor_field() {
        let schema = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! }
            type UserEdge {
                node: User
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Edge type must contain a field `cursor`"));
    }

    #[test]
    fn missing_both_fields() {
        let schema = r"
            type UserEdge {
                someField: String
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert_eq!(diagnostics.len(), 2);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("`node`")));
        assert!(messages.iter().any(|m| m.contains("`cursor`")));
    }

    #[test]
    fn node_field_must_not_be_list() {
        let schema = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! }
            type UserEdge {
                node: [User]
                cursor: String!
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Field `node` must return either"));
    }

    #[test]
    fn cursor_field_wrong_type() {
        let schema = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! }
            type UserEdge {
                node: User
                cursor: User
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Field `cursor` must return either"));
    }

    #[test]
    fn cursor_field_allows_scalar() {
        let schema = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! }
            scalar Cursor
            type UserEdge {
                node: User
                cursor: Cursor
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert!(
            diagnostics.is_empty(),
            "Scalar cursor should be allowed: {diagnostics:?}"
        );
    }

    #[test]
    fn cursor_field_allows_id() {
        let schema = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! }
            type UserEdge {
                node: User
                cursor: ID!
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert!(
            diagnostics.is_empty(),
            "ID cursor should be allowed: {diagnostics:?}"
        );
    }

    #[test]
    fn edge_type_missing_suffix() {
        let schema = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! name: String! }
            type UserItem {
                node: User
                cursor: String!
            }
            type UserConnection {
                edges: [UserItem]
            }
        ";
        let diagnostics = check(schema);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Edge type must have \"Edge\" suffix."));
    }

    #[test]
    fn edge_suffix_disabled() {
        let schema = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! }
            type UserItem {
                node: User
                cursor: String!
            }
            type UserConnection {
                edges: [UserItem]
            }
        ";
        let opts = serde_json::json!({ "withEdgeSuffix": false });
        let diagnostics = check_with_options(schema, Some(&opts));
        assert!(
            diagnostics.is_empty(),
            "Should pass with suffix check disabled: {diagnostics:?}"
        );
    }

    #[test]
    fn node_does_not_implement_node_interface() {
        let schema = r"
            interface Node { id: ID! }
            type User {
                id: ID!
                name: String!
            }
            type UserEdge {
                node: User
                cursor: String!
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let diagnostics = check(schema);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Edge type's field `node` must implement `Node` interface."));
    }

    #[test]
    fn should_implement_node_disabled() {
        let schema = r"
            type User {
                id: ID!
            }
            type UserEdge {
                node: User
                cursor: String!
            }
            type UserConnection {
                edges: [UserEdge]
            }
        ";
        let opts = serde_json::json!({ "shouldImplementNode": false });
        let diagnostics = check_with_options(schema, Some(&opts));
        assert!(
            diagnostics.is_empty(),
            "Should pass with Node interface check disabled: {diagnostics:?}"
        );
    }

    #[test]
    fn non_connection_types_ignored() {
        let schema = r"
            type User {
                name: String!
            }
        ";
        let diagnostics = check(schema);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn connection_without_edges_field_ignored() {
        let schema = r"
            type UserConnection {
                nodes: [User]
            }
            type User { id: ID! }
        ";
        let diagnostics = check(schema);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn edge_type_not_referenced_by_connection_ignored() {
        // An edge type that isn't used by any connection should not be checked
        let schema = r"
            type UserEdge {
                someField: String
            }
        ";
        let diagnostics = check(schema);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn multiple_connections() {
        let schema = r"
            interface Node { id: ID! }
            type User implements Node { id: ID! }
            type Post implements Node { id: ID! }
            type UserEdge {
                node: User
                cursor: String!
            }
            type PostEdge {
                node: Post
                cursor: String!
            }
            type UserConnection {
                edges: [UserEdge]
            }
            type PostConnection {
                edges: [PostEdge]
            }
        ";
        let diagnostics = check(schema);
        assert!(
            diagnostics.is_empty(),
            "Expected no diagnostics: {diagnostics:?}"
        );
    }
}
