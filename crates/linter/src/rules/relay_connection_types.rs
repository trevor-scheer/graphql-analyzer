use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::HashMap;

/// Lint rule that enforces Relay connection type conventions
///
/// Types whose names end in "Connection" must follow the Relay connection
/// specification:
/// - Must be an object type
/// - Must have an `edges` field that returns a list type
/// - Must have a `pageInfo` field that returns a non-null `PageInfo` type
pub struct RelayConnectionTypesRuleImpl;

const RULE_NAME: &str = "relayConnectionTypes";

impl LintRule for RelayConnectionTypesRuleImpl {
    fn name(&self) -> &'static str {
        RULE_NAME
    }

    fn description(&self) -> &'static str {
        "Enforces Relay connection type conventions on types ending in 'Connection'"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for RelayConnectionTypesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            if !type_def.name.ends_with("Connection") {
                continue;
            }

            let type_span = || {
                let start: usize = type_def.name_range.start().into();
                let end: usize = type_def.name_range.end().into();
                graphql_syntax::SourceSpan {
                    start,
                    end,
                    line_offset: 0,
                    byte_offset: 0,
                    source: None,
                }
            };

            // Connection types must be object types
            if type_def.kind != TypeDefKind::Object {
                diagnostics_by_file
                    .entry(type_def.file_id)
                    .or_default()
                    .push(
                        LintDiagnostic::new(
                            type_span(),
                            LintSeverity::Warning,
                            format!(
                                "Type '{}' ends in 'Connection' but is not an object type",
                                type_def.name
                            ),
                            RULE_NAME,
                        )
                        .with_help(
                            "Relay connection types must be object types with 'edges' and 'pageInfo' fields",
                        ),
                    );
                continue;
            }

            // Check for edges field
            let edges_field = type_def.fields.iter().find(|f| f.name.as_ref() == "edges");
            match edges_field {
                None => {
                    diagnostics_by_file
                        .entry(type_def.file_id)
                        .or_default()
                        .push(
                            LintDiagnostic::new(
                                type_span(),
                                LintSeverity::Warning,
                                format!(
                                    "Connection type '{}' is missing an 'edges' field",
                                    type_def.name
                                ),
                                RULE_NAME,
                            )
                            .with_help(
                                "Add an 'edges' field that returns a list type (e.g., 'edges: [UserEdge]')",
                            ),
                        );
                }
                Some(field) => {
                    if !field.type_ref.is_list {
                        let field_start: usize = field.name_range.start().into();
                        let field_end: usize = field.name_range.end().into();
                        let field_span = graphql_syntax::SourceSpan {
                            start: field_start,
                            end: field_end,
                            line_offset: 0,
                            byte_offset: 0,
                            source: None,
                        };
                        diagnostics_by_file
                            .entry(type_def.file_id)
                            .or_default()
                            .push(
                            LintDiagnostic::new(
                                field_span,
                                LintSeverity::Warning,
                                format!(
                                    "'edges' field on connection type '{}' must return a list type",
                                    type_def.name
                                ),
                                RULE_NAME,
                            )
                            .with_help("Change the type to a list (e.g., '[UserEdge]')"),
                        );
                    }
                }
            }

            // Check for pageInfo field
            let page_info_field = type_def
                .fields
                .iter()
                .find(|f| f.name.as_ref() == "pageInfo");
            match page_info_field {
                None => {
                    diagnostics_by_file
                        .entry(type_def.file_id)
                        .or_default()
                        .push(
                            LintDiagnostic::new(
                                type_span(),
                                LintSeverity::Warning,
                                format!(
                                    "Connection type '{}' is missing a 'pageInfo' field",
                                    type_def.name
                                ),
                                RULE_NAME,
                            )
                            .with_help(
                                "Add a 'pageInfo' field that returns a non-null PageInfo type (e.g., 'pageInfo: PageInfo!')",
                            ),
                        );
                }
                Some(field) => {
                    if field.type_ref.name.as_ref() != "PageInfo" {
                        let field_start: usize = field.name_range.start().into();
                        let field_end: usize = field.name_range.end().into();
                        let field_span = graphql_syntax::SourceSpan {
                            start: field_start,
                            end: field_end,
                            line_offset: 0,
                            byte_offset: 0,
                            source: None,
                        };
                        diagnostics_by_file
                            .entry(type_def.file_id)
                            .or_default()
                            .push(
                                LintDiagnostic::new(
                                    field_span,
                                    LintSeverity::Warning,
                                    format!(
                                        "'pageInfo' field on connection type '{}' must return 'PageInfo' type, found '{}'",
                                        type_def.name, field.type_ref.name
                                    ),
                                    RULE_NAME,
                                )
                                .with_help(
                                    "Change the type to 'PageInfo!'",
                                ),
                            );
                    } else if !field.type_ref.is_non_null {
                        let field_start: usize = field.name_range.start().into();
                        let field_end: usize = field.name_range.end().into();
                        let field_span = graphql_syntax::SourceSpan {
                            start: field_start,
                            end: field_end,
                            line_offset: 0,
                            byte_offset: 0,
                            source: None,
                        };
                        diagnostics_by_file
                            .entry(type_def.file_id)
                            .or_default()
                            .push(
                                LintDiagnostic::new(
                                    field_span,
                                    LintSeverity::Warning,
                                    format!(
                                        "'pageInfo' field on connection type '{}' must be non-null",
                                        type_def.name
                                    ),
                                    RULE_NAME,
                                )
                                .with_help("Change the type to 'PageInfo!' (non-null)"),
                            );
                    }
                }
            }
        }

        diagnostics_by_file
    }
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

    fn check_schema(schema: &str) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = RelayConnectionTypesRuleImpl;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        diagnostics.into_values().flatten().collect()
    }

    #[test]
    fn valid_connection_type() {
        let diagnostics =
            check_schema("type UserConnection { edges: [UserEdge] pageInfo: PageInfo! }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn valid_connection_with_extra_fields() {
        let diagnostics = check_schema(
            "type UserConnection { edges: [UserEdge] pageInfo: PageInfo! totalCount: Int! }",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn non_connection_type_ignored() {
        let diagnostics = check_schema("type User { id: ID! name: String! }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn missing_edges_field() {
        let diagnostics = check_schema("type UserConnection { pageInfo: PageInfo! }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("missing an 'edges' field"));
    }

    #[test]
    fn missing_page_info_field() {
        let diagnostics = check_schema("type UserConnection { edges: [UserEdge] }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("missing a 'pageInfo' field"));
    }

    #[test]
    fn missing_both_fields() {
        let diagnostics = check_schema("type UserConnection { totalCount: Int! }");
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn edges_not_a_list() {
        let diagnostics =
            check_schema("type UserConnection { edges: UserEdge pageInfo: PageInfo! }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("must return a list type"));
    }

    #[test]
    fn page_info_nullable() {
        let diagnostics =
            check_schema("type UserConnection { edges: [UserEdge] pageInfo: PageInfo }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("must be non-null"));
    }

    #[test]
    fn page_info_wrong_type() {
        let diagnostics =
            check_schema("type UserConnection { edges: [UserEdge] pageInfo: String! }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("must return 'PageInfo' type"));
    }

    #[test]
    fn non_object_connection_type() {
        let diagnostics = check_schema("scalar UserConnection");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("not an object type"));
    }

    #[test]
    fn enum_connection_type() {
        let diagnostics = check_schema("enum StatusConnection { ACTIVE INACTIVE }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("not an object type"));
    }

    #[test]
    fn interface_connection_type() {
        let diagnostics =
            check_schema("interface NodeConnection { edges: [NodeEdge] pageInfo: PageInfo! }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("not an object type"));
    }

    #[test]
    fn multiple_connection_types() {
        let diagnostics = check_schema(
            r#"
            type UserConnection { edges: [UserEdge] pageInfo: PageInfo! }
            type PostConnection { totalCount: Int! }
            "#,
        );
        // PostConnection is missing both edges and pageInfo
        assert_eq!(diagnostics.len(), 2);
    }
}
