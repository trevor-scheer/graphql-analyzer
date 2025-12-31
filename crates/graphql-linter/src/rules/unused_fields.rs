use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_db::{FileId, ProjectFiles};
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
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Step 1: Collect all schema fields
        let schema_types = graphql_hir::schema_types_with_project(db, project_files);
        let mut schema_fields: HashMap<String, HashSet<String>> = HashMap::new();
        let mut field_locations: HashMap<(String, String), FileId> = HashMap::new();

        for (type_name, type_def) in schema_types.iter() {
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
                        type_def.file_id,
                    );
                }
                schema_fields.insert(type_name.to_string(), fields);
            }
        }

        // Step 2: Collect all used fields from operations and fragments
        let mut used_fields: HashMap<String, HashSet<String>> = HashMap::new();
        let doc_ids = project_files.document_file_ids(db).ids(db);

        // Determine root types for skipping
        let root_types = get_root_type_names(db, &schema_types);

        for file_id in doc_ids.iter() {
            // Use per-file lookup to avoid depending on entire file_map
            let Some((content, metadata)) = graphql_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            let parse = graphql_syntax::parse(db, content, metadata);

            // Scan operations and fragments in main AST
            for definition in &parse.ast.definitions {
                match definition {
                    apollo_compiler::ast::Definition::OperationDefinition(operation) => {
                        let root_type = match operation.operation_type {
                            apollo_compiler::ast::OperationType::Query => {
                                root_types.query.as_deref()
                            }
                            apollo_compiler::ast::OperationType::Mutation => {
                                root_types.mutation.as_deref()
                            }
                            apollo_compiler::ast::OperationType::Subscription => {
                                root_types.subscription.as_deref()
                            }
                        };

                        if let Some(root_type) = root_type {
                            collect_used_fields_from_selection_set(
                                &operation.selection_set,
                                root_type,
                                &schema_types,
                                &mut used_fields,
                            );
                        }
                    }
                    apollo_compiler::ast::Definition::FragmentDefinition(fragment) => {
                        let type_condition = fragment.type_condition.as_str();
                        collect_used_fields_from_selection_set(
                            &fragment.selection_set,
                            type_condition,
                            &schema_types,
                            &mut used_fields,
                        );
                    }
                    _ => {}
                }
            }

            // Also scan operations and fragments in extracted blocks
            for block in &parse.blocks {
                for definition in &block.ast.definitions {
                    match definition {
                        apollo_compiler::ast::Definition::OperationDefinition(operation) => {
                            let root_type = match operation.operation_type {
                                apollo_compiler::ast::OperationType::Query => {
                                    root_types.query.as_deref()
                                }
                                apollo_compiler::ast::OperationType::Mutation => {
                                    root_types.mutation.as_deref()
                                }
                                apollo_compiler::ast::OperationType::Subscription => {
                                    root_types.subscription.as_deref()
                                }
                            };

                            if let Some(root_type) = root_type {
                                collect_used_fields_from_selection_set(
                                    &operation.selection_set,
                                    root_type,
                                    &schema_types,
                                    &mut used_fields,
                                );
                            }
                        }
                        apollo_compiler::ast::Definition::FragmentDefinition(fragment) => {
                            let type_condition = fragment.type_condition.as_str();
                            collect_used_fields_from_selection_set(
                                &fragment.selection_set,
                                type_condition,
                                &schema_types,
                                &mut used_fields,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }

        // Step 3: Report unused fields
        for (type_name, fields) in &schema_fields {
            // Skip root operation types
            if root_types.is_root_type(type_name) {
                continue;
            }

            let used_in_type = used_fields.get(type_name);

            for field_name in fields {
                // Skip introspection fields
                if is_introspection_field(field_name) {
                    continue;
                }

                let is_used = used_in_type.is_some_and(|set| set.contains(field_name));

                if !is_used {
                    if let Some(&file_id) =
                        field_locations.get(&(type_name.clone(), field_name.clone()))
                    {
                        let message = format!(
                            "Field '{type_name}.{field_name}' is defined in the schema but never used in any operation or fragment"
                        );

                        let diag = LintDiagnostic::new(
                            crate::diagnostics::OffsetRange::new(0, field_name.len()),
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

/// Helper struct to track root type names
struct RootTypes {
    query: Option<String>,
    mutation: Option<String>,
    subscription: Option<String>,
}

impl RootTypes {
    fn is_root_type(&self, type_name: &str) -> bool {
        self.query.as_deref() == Some(type_name)
            || self.mutation.as_deref() == Some(type_name)
            || self.subscription.as_deref() == Some(type_name)
    }
}

/// Get root type names from schema
fn get_root_type_names(
    _db: &dyn graphql_hir::GraphQLHirDatabase,
    schema_types: &HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
) -> RootTypes {
    // Default to "Query", "Mutation", "Subscription" if they exist
    let query = schema_types
        .contains_key("Query")
        .then(|| "Query".to_string());
    let mutation = schema_types
        .contains_key("Mutation")
        .then(|| "Mutation".to_string());
    let subscription = schema_types
        .contains_key("Subscription")
        .then(|| "Subscription".to_string());

    RootTypes {
        query,
        mutation,
        subscription,
    }
}

/// Recursively collect used fields from a selection set
fn collect_used_fields_from_selection_set(
    selections: &[apollo_compiler::ast::Selection],
    parent_type: &str,
    schema_types: &HashMap<std::sync::Arc<str>, graphql_hir::TypeDef>,
    used_fields: &mut HashMap<String, HashSet<String>>,
) {
    for selection in selections {
        match selection {
            apollo_compiler::ast::Selection::Field(field) => {
                let field_name = field.name.as_str();

                // Record this field as used
                used_fields
                    .entry(parent_type.to_string())
                    .or_default()
                    .insert(field_name.to_string());

                // Recursively process nested selections if present
                if !field.selection_set.is_empty() {
                    // Find the field's return type from schema
                    if let Some(type_def) = schema_types.get(parent_type) {
                        if let Some(field_sig) = type_def
                            .fields
                            .iter()
                            .find(|f| f.name.as_ref() == field_name)
                        {
                            // Extract base type name (remove wrappers)
                            let nested_type = field_sig.type_ref.name.as_ref();
                            collect_used_fields_from_selection_set(
                                &field.selection_set,
                                nested_type,
                                schema_types,
                                used_fields,
                            );
                        }
                    }
                }
            }
            apollo_compiler::ast::Selection::FragmentSpread(_) => {
                // Fragment spreads are processed separately when we scan fragments
            }
            apollo_compiler::ast::Selection::InlineFragment(inline) => {
                // Use type condition if present, otherwise use parent type
                let type_name = inline
                    .type_condition
                    .as_ref()
                    .map_or(parent_type, apollo_compiler::Name::as_str);

                collect_used_fields_from_selection_set(
                    &inline.selection_set,
                    type_name,
                    schema_types,
                    used_fields,
                );
            }
        }
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
