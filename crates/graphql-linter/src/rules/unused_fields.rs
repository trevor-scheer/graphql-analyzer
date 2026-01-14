use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::schema_utils::extract_root_type_names;
use crate::traits::{LintRule, ProjectLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TextRange;
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

impl ProjectLintRule for UnusedFieldsRuleImpl {
    #[allow(clippy::too_many_lines)]
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Step 1: Collect all schema fields
        let schema_types = graphql_hir::schema_types(db, project_files);
        let mut schema_fields: HashMap<String, HashSet<String>> = HashMap::new();
        let mut field_locations: HashMap<(String, String), (FileId, TextRange)> = HashMap::new();

        for (type_name, type_def) in schema_types {
            // Skip introspection types
            if is_introspection_type(type_name) {
                continue;
            }

            // Only track Object and Interface fields
            if matches!(
                type_def.kind,
                graphql_hir::TypeDefKind::Object | graphql_hir::TypeDefKind::Interface
            ) {
                let mut fields = HashSet::new();
                for field in &type_def.fields {
                    fields.insert(field.name.to_string());
                    field_locations.insert(
                        (type_name.to_string(), field.name.to_string()),
                        (type_def.file_id, field.name_range),
                    );
                }
                schema_fields.insert(type_name.to_string(), fields);
            }
        }

        // Step 2: Collect all used schema coordinates using per-file cached queries
        let mut used_coordinates: HashMap<String, HashSet<String>> = HashMap::new();
        let doc_ids = project_files.document_file_ids(db).ids(db);

        // Determine root types for skipping (supports custom schema definitions)
        let root_types = extract_root_type_names(db, project_files, schema_types);

        for file_id in doc_ids.iter() {
            // Use per-file lookup for granular caching
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            // Per-file cached query - only recomputes if THIS file changed
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

        // Step 3: Report unused fields
        for (type_name, fields) in &schema_fields {
            // Skip root operation types
            if root_types.is_root_type(type_name) {
                continue;
            }

            let used_in_type = used_coordinates.get(type_name);

            for field_name in fields {
                // Skip introspection fields
                if is_introspection_field(field_name) {
                    continue;
                }

                let is_used = used_in_type.is_some_and(|set| set.contains(field_name));

                if !is_used {
                    if let Some(&(file_id, name_range)) =
                        field_locations.get(&(type_name.clone(), field_name.clone()))
                    {
                        let message = format!(
                            "Field '{type_name}.{field_name}' is defined in the schema but never used in any operation or fragment"
                        );

                        let diag = LintDiagnostic::new(
                            crate::diagnostics::OffsetRange::new(
                                name_range.start().into(),
                                name_range.end().into(),
                            ),
                            self.default_severity(),
                            message,
                            self.name().to_string(),
                        );

                        diagnostics_by_file.entry(file_id).or_default().push(diag);
                    }
                }
            }
        }

        diagnostics_by_file
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
