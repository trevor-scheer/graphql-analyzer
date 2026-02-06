use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::schema_utils::extract_root_type_names;
use crate::traits::{LintRule, ProjectLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::{HashMap, HashSet};

/// Trait implementation for `unused_fields` rule
pub struct UnusedFieldsRuleImpl;

impl LintRule for UnusedFieldsRuleImpl {
    fn name(&self) -> &'static str {
        "unused_fields"
    }

    fn description(&self) -> &'static str {
        "Detects schema fields that are never used in any operation or fragment"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

/// Information about a schema field for diagnostic reporting
struct FieldInfo {
    /// Type name containing the field
    type_name: String,
    /// Field name
    field_name: String,
    /// File where the field is defined
    file_id: FileId,
    /// Byte offset of the field name (for diagnostic range)
    name_start: usize,
    /// Byte offset of the end of the field name
    name_end: usize,
}

impl ProjectLintRule for UnusedFieldsRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Step 1: Collect all schema fields with their CST positions
        let schema_ids = project_files.schema_file_ids(db).ids(db);
        let mut all_fields: Vec<FieldInfo> = Vec::new();

        for file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };

            // Parse the schema file to get CST positions
            let parse = graphql_syntax::parse(db, content, metadata);
            if parse.has_errors() {
                continue;
            }

            // Iterate over all GraphQL documents (unified API)
            for doc in parse.documents() {
                collect_schema_fields(&doc.tree.document(), *file_id, &mut all_fields);
            }
        }

        // Step 2: Collect all used schema coordinates using per-file cached queries
        let mut used_coordinates: HashMap<String, HashSet<String>> = HashMap::new();
        let doc_ids = project_files.document_file_ids(db).ids(db);

        // Determine root types for skipping (supports custom schema definitions)
        let schema_types = graphql_hir::schema_types(db, project_files);
        let root_types = extract_root_type_names(db, project_files, schema_types);

        for file_id in doc_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            let file_coords = graphql_hir::file_schema_coordinates(
                db,
                *file_id,
                content,
                metadata,
                project_files,
            );
            for coord in file_coords.iter() {
                used_coordinates
                    .entry(coord.type_name.to_string())
                    .or_default()
                    .insert(coord.field_name.to_string());
            }
        }

        // Step 3: Report unused fields (no auto-fix - removing schema fields is a breaking change)
        for field_info in &all_fields {
            // Skip introspection types
            if is_introspection_type(&field_info.type_name) {
                continue;
            }

            // Skip root operation types
            if root_types.is_root_type(&field_info.type_name) {
                continue;
            }

            // Skip introspection fields
            if is_introspection_field(&field_info.field_name) {
                continue;
            }

            let is_used = used_coordinates
                .get(&field_info.type_name)
                .is_some_and(|set| set.contains(&field_info.field_name));

            if !is_used {
                let message = format!(
                    "Field '{}.{}' is defined in the schema but never used in any operation or fragment. \
                    This field may be safe to remove if no external clients are using it.",
                    field_info.type_name, field_info.field_name
                );

                let diag = LintDiagnostic::warning(
                    field_info.name_start,
                    field_info.name_end,
                    message,
                    "unused_fields",
                );

                diagnostics_by_file
                    .entry(field_info.file_id)
                    .or_default()
                    .push(diag);
            }
        }

        diagnostics_by_file
    }
}

/// Collect schema field definitions from a CST document with their positions
fn collect_schema_fields(doc: &cst::Document, file_id: FileId, fields: &mut Vec<FieldInfo>) {
    for definition in doc.definitions() {
        match definition {
            cst::Definition::ObjectTypeDefinition(obj) => {
                let Some(type_name) = obj.name() else {
                    continue;
                };
                let type_name_str = type_name.text().to_string();

                if let Some(fields_def) = obj.fields_definition() {
                    collect_field_definitions(&type_name_str, file_id, &fields_def, fields);
                }
            }
            cst::Definition::InterfaceTypeDefinition(iface) => {
                let Some(type_name) = iface.name() else {
                    continue;
                };
                let type_name_str = type_name.text().to_string();

                if let Some(fields_def) = iface.fields_definition() {
                    collect_field_definitions(&type_name_str, file_id, &fields_def, fields);
                }
            }
            cst::Definition::ObjectTypeExtension(ext) => {
                let Some(type_name) = ext.name() else {
                    continue;
                };
                let type_name_str = type_name.text().to_string();

                if let Some(fields_def) = ext.fields_definition() {
                    collect_field_definitions(&type_name_str, file_id, &fields_def, fields);
                }
            }
            cst::Definition::InterfaceTypeExtension(ext) => {
                let Some(type_name) = ext.name() else {
                    continue;
                };
                let type_name_str = type_name.text().to_string();

                if let Some(fields_def) = ext.fields_definition() {
                    collect_field_definitions(&type_name_str, file_id, &fields_def, fields);
                }
            }
            _ => {}
        }
    }
}

/// Collect field definitions from a `FieldsDefinition` CST node
fn collect_field_definitions(
    type_name: &str,
    file_id: FileId,
    fields_def: &cst::FieldsDefinition,
    fields: &mut Vec<FieldInfo>,
) {
    for field in fields_def.field_definitions() {
        let Some(name) = field.name() else {
            continue;
        };

        let name_syntax = name.syntax();
        let name_start: usize = name_syntax.text_range().start().into();
        let name_end: usize = name_syntax.text_range().end().into();

        fields.push(FieldInfo {
            type_name: type_name.to_string(),
            field_name: name.text().to_string(),
            file_id,
            name_start,
            name_end,
        });
    }
}

/// Check if a type is a built-in introspection type
fn is_introspection_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "__Schema"
            | "__Type"
            | "__Field"
            | "__InputValue"
            | "__EnumValue"
            | "__TypeKind"
            | "__Directive"
            | "__DirectiveLocation"
    )
}

/// Check if a field name is an introspection field
fn is_introspection_field(field_name: &str) -> bool {
    matches!(field_name, "__typename" | "__schema" | "__type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::ProjectLintRule;
    use graphql_base_db::{ExtractionOffset, FileContent, FileId, FileKind, FileMetadata, FileUri};
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project(
        db: &dyn graphql_hir::GraphQLHirDatabase,
        schema_source: &str,
        document_source: &str,
    ) -> ProjectFiles {
        let schema_file_id = FileId::new(0);
        let schema_content = FileContent::new(db, Arc::from(schema_source));
        let schema_metadata = FileMetadata::new(
            db,
            schema_file_id,
            FileUri::new("file:///schema.graphql"),
            FileKind::Schema,
            ExtractionOffset::default(),
        );

        let doc_file_id = FileId::new(1);
        let doc_content = FileContent::new(db, Arc::from(document_source));
        let doc_metadata = FileMetadata::new(
            db,
            doc_file_id,
            FileUri::new("file:///query.graphql"),
            FileKind::ExecutableGraphQL,
            ExtractionOffset::default(),
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

        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_all_fields_used_no_warning() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    user: User
}

type User {
    id: ID!
    name: String!
}
";

        let document = r"
query GetUser {
    user {
        id
        name
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_unused_field_warning() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    unusedField: String
}
";

        let document = r"
query GetUser {
    user {
        id
        name
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        let file_diags = diagnostics.values().next().unwrap();
        assert_eq!(file_diags.len(), 1);
        assert!(file_diags[0].message.contains("User.unusedField"));
        assert!(file_diags[0].message.contains("never used"));
    }

    #[test]
    fn test_field_used_in_fragment_not_reported() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    email: String
}
";

        let document = r"
fragment UserFields on User {
    id
    name
    email
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_root_type_fields_not_reported() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    user: User
    posts: [Post!]!
}

type User {
    id: ID!
}

type Post {
    id: ID!
}
";

        let document = r"
query GetUser {
    user {
        id
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        let root_type_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("Query.posts"))
            .collect();
        assert!(
            root_type_warnings.is_empty(),
            "Root type fields should not be reported as unused"
        );
    }

    #[test]
    fn test_introspection_types_not_reported() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    user: User
}

type User {
    id: ID!
}
";

        let document = r"
query GetUser {
    user {
        id
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        let introspection_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("__"))
            .collect();
        assert!(
            introspection_warnings.is_empty(),
            "Introspection types should not be reported as unused"
        );
    }

    #[test]
    fn test_multiple_unused_fields() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    user: User
}

type User {
    id: ID!
    name: String!
    email: String
    phone: String
}
";

        let document = r"
query GetUser {
    user {
        id
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        let total_diags: usize = diagnostics.values().map(Vec::len).sum();
        assert_eq!(total_diags, 3);
    }

    #[test]
    fn test_interface_field_used_through_interface() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    node: Node
}

interface Node {
    id: ID!
}

type User implements Node {
    id: ID!
    name: String!
}
";

        let document = r"
query GetNode {
    node {
        id
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        let interface_id_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("Node.id"))
            .collect();
        assert!(
            interface_id_warnings.is_empty(),
            "Interface Node.id field should not be reported when used"
        );
    }

    #[test]
    fn test_implementing_type_field_tracked_separately() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    user: User
}

type User {
    id: ID!
    name: String!
}
";

        let document = r"
query GetUser {
    user {
        id
        name
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        assert!(
            diagnostics.is_empty(),
            "All fields are used, no warnings expected"
        );
    }

    #[test]
    fn test_nested_field_used() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
type Query {
    user: User
}

type User {
    id: ID!
    profile: Profile
}

type Profile {
    bio: String
    avatar: String
}
";

        let document = r"
query GetUser {
    user {
        id
        profile {
            bio
        }
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        let avatar_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("Profile.avatar"))
            .collect();
        assert_eq!(
            avatar_warnings.len(),
            1,
            "Unused avatar field should be reported"
        );
    }

    #[test]
    fn test_custom_schema_definition_root_types() {
        let db = RootDatabase::default();
        let rule = UnusedFieldsRuleImpl;

        let schema = r"
schema {
    query: RootQuery
    mutation: RootMutation
}

type RootQuery {
    user: User
    posts: [Post!]!
}

type RootMutation {
    createUser: User
}

type User {
    id: ID!
}

type Post {
    id: ID!
}
";

        let document = r"
query GetUser {
    user {
        id
    }
}
";

        let project_files = create_test_project(&db, schema, document);
        let diagnostics = rule.check(&db, project_files, None);

        let root_query_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("RootQuery."))
            .collect();
        assert!(
            root_query_warnings.is_empty(),
            "Custom root type fields should not be reported as unused"
        );
    }
}
