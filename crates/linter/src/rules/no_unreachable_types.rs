use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::{HashMap, HashSet, VecDeque};

/// Lint rule that detects unreachable types in the schema
///
/// Types that are not reachable from any root type (Query, Mutation, Subscription)
/// are dead code in the schema. They add complexity without being usable.
pub struct NoUnreachableTypesRuleImpl;

impl LintRule for NoUnreachableTypesRuleImpl {
    fn name(&self) -> &'static str {
        "no_unreachable_types"
    }

    fn description(&self) -> &'static str {
        "Detects types that are not reachable from any root operation type"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for NoUnreachableTypesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        // Build a reachability set starting from root types using BFS
        let mut reachable: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();

        // Seed with root type names
        if let Some(ref name) = root_type_names.query {
            queue.push_back(name.clone());
        }
        if let Some(ref name) = root_type_names.mutation {
            queue.push_back(name.clone());
        }
        if let Some(ref name) = root_type_names.subscription {
            queue.push_back(name.clone());
        }

        // BFS to find all reachable types
        while let Some(type_name) = queue.pop_front() {
            if !reachable.insert(type_name.clone()) {
                continue;
            }

            if let Some(type_def) = schema_types.get(type_name.as_str()) {
                // Add types referenced by fields
                for field in &type_def.fields {
                    let referenced = field.type_ref.name.to_string();
                    if !reachable.contains(&referenced) {
                        queue.push_back(referenced);
                    }

                    // Add types referenced by arguments
                    for arg in &field.arguments {
                        let arg_type = arg.type_ref.name.to_string();
                        if !reachable.contains(&arg_type) {
                            queue.push_back(arg_type);
                        }
                    }
                }

                // Add implemented interfaces
                for iface in &type_def.implements {
                    let iface_name = iface.to_string();
                    if !reachable.contains(&iface_name) {
                        queue.push_back(iface_name);
                    }
                }

                // Add union members
                for member in &type_def.union_members {
                    let member_name = member.to_string();
                    if !reachable.contains(&member_name) {
                        queue.push_back(member_name);
                    }
                }
            }
        }

        // Report unreachable types (skip scalars, they're often used via directives or custom logic)
        for type_def in schema_types.values() {
            if type_def.kind == TypeDefKind::Scalar {
                continue;
            }

            if !reachable.contains(type_def.name.as_ref()) {
                let start: usize = type_def.name_range.start().into();
                let end: usize = type_def.name_range.end().into();
                let span = graphql_syntax::SourceSpan {
                    start,
                    end,
                    line_offset: 0,
                    byte_offset: 0,
                    source: None,
                };

                let kind_name = match type_def.kind {
                    TypeDefKind::Interface => "Interface",
                    TypeDefKind::Union => "Union",
                    TypeDefKind::Enum => "Enum",
                    TypeDefKind::InputObject => "Input",
                    _ => "Type",
                };

                diagnostics_by_file
                    .entry(type_def.file_id)
                    .or_default()
                    .push(LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        format!(
                            "{kind_name} '{}' is not reachable from any root type",
                            type_def.name
                        ),
                        "no_unreachable_types",
                    ));
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
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_reachable_type() {
        let db = RootDatabase::default();
        let rule = NoUnreachableTypesRuleImpl;
        let schema = "type Query { user: User } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let user_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("'User'"))
            .collect();
        assert!(user_warnings.is_empty());
    }

    #[test]
    fn test_unreachable_type() {
        let db = RootDatabase::default();
        let rule = NoUnreachableTypesRuleImpl;
        let schema =
            "type Query { user: User } type User { id: ID! } type OrphanType { name: String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let orphan_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("'OrphanType'"))
            .collect();
        assert_eq!(orphan_warnings.len(), 1);
    }
}
