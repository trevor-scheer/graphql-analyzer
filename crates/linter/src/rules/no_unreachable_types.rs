use crate::diagnostics::{CodeSuggestion, LintDiagnostic, LintSeverity};
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
        "noUnreachableTypes"
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

        // Build a reverse-implementation map: interface name → all types that implement it.
        // When an interface becomes reachable, all its implementors become reachable too
        // (mirrors graphql-js `schema.getImplementations(type)`).
        let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
        for type_def in schema_types.values() {
            for iface in &type_def.implements {
                implementors
                    .entry(iface.to_string())
                    .or_default()
                    .push(type_def.name.to_string());
            }
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

                // Add implemented interfaces (outgoing: this type → its interfaces)
                for iface in &type_def.implements {
                    let iface_name = iface.to_string();
                    if !reachable.contains(&iface_name) {
                        queue.push_back(iface_name);
                    }
                }

                // Add implementing types (incoming: interface → all types that implement it).
                // This mirrors graphql-js `schema.getImplementations(type)` which upstream
                // uses to mark concrete types reachable whenever their interface is reachable.
                if let Some(impls) = implementors.get(&type_name) {
                    for impl_name in impls {
                        if !reachable.contains(impl_name) {
                            queue.push_back(impl_name.clone());
                        }
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

        // Report unreachable types. Scalars are included per upstream behavior.
        for type_def in schema_types.values() {
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
                    TypeDefKind::Object => "Object type",
                    TypeDefKind::Interface => "Interface type",
                    TypeDefKind::Union => "Union type",
                    TypeDefKind::Enum => "Enum type",
                    TypeDefKind::InputObject => "Input object type",
                    TypeDefKind::Scalar => "Scalar type",
                    _ => "Type",
                };

                // Suggestion: remove the entire type def (matches upstream's
                // `fixer.remove(node.parent)`). Range comes from
                // `TypeDef.definition_range`, which the HIR already
                // populates with the full declaration's byte span.
                let def_start: usize = type_def.definition_range.start().into();
                let def_end: usize = type_def.definition_range.end().into();
                let suggestion = CodeSuggestion::delete(
                    format!("Remove `{}`", type_def.name),
                    def_start,
                    def_end,
                );

                diagnostics_by_file
                    .entry(type_def.file_id)
                    .or_default()
                    .push(
                        LintDiagnostic::new(
                            span,
                            LintSeverity::Warning,
                            format!("{kind_name} `{}` is unreachable.", type_def.name),
                            "noUnreachableTypes",
                        )
                        .with_message_id("no-unreachable-types")
                        .with_suggestion(suggestion)
                        .with_help(
                            "Remove the unreachable type, or reference it from a reachable type",
                        )
                        .with_tag(crate::diagnostics::DiagnosticTag::Unnecessary),
                    );
            }
        }

        // Sort diagnostics within each file by span start so callers see a
        // deterministic, source-order output regardless of HashMap iteration order.
        for diags in diagnostics_by_file.values_mut() {
            diags.sort_by_key(|d| d.span.start);
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
            .filter(|d| d.message.contains("`User`"))
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
            .filter(|d| d.message.contains("`OrphanType`"))
            .collect();
        assert_eq!(orphan_warnings.len(), 1);
    }
}
