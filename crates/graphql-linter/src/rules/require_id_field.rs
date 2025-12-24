use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{DocumentSchemaLintRule, LintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_db::{FileContent, FileId, FileMetadata, ProjectFiles};
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
        if !parse.errors.is_empty() {
            return diagnostics;
        }

        // Get schema types from HIR
        let schema_types = graphql_hir::schema_types_with_project(db, project_files);

        // Build a map of type names to whether they have an id field
        let mut types_with_id: HashMap<String, bool> = HashMap::new();
        for (type_name, type_def) in schema_types.iter() {
            let has_id = match type_def.kind {
                graphql_hir::TypeDefKind::Object | graphql_hir::TypeDefKind::Interface => {
                    type_def.fields.iter().any(|f| f.name.as_ref() == "id")
                }
                _ => false,
            };
            types_with_id.insert(type_name.to_string(), has_id);
        }

        // Get root operation types from schema
        let query_type = find_root_operation_type(&schema_types, "Query");
        let mutation_type = find_root_operation_type(&schema_types, "Mutation");
        let subscription_type = find_root_operation_type(&schema_types, "Subscription");

        // Walk the CST for position info
        let doc_cst = parse.tree.document();

        for definition in doc_cst.definitions() {
            match definition {
                cst::Definition::OperationDefinition(op) => {
                    let root_type = match op.operation_type() {
                        Some(op_type) if op_type.query_token().is_some() => query_type.as_deref(),
                        Some(op_type) if op_type.mutation_token().is_some() => {
                            mutation_type.as_deref()
                        }
                        Some(op_type) if op_type.subscription_token().is_some() => {
                            subscription_type.as_deref()
                        }
                        None => query_type.as_deref(), // Default to query for anonymous operations
                        _ => None,
                    };

                    if let (Some(root_type_name), Some(selection_set)) =
                        (root_type, op.selection_set())
                    {
                        let mut visited_fragments = HashSet::new();
                        check_selection_set(
                            &selection_set,
                            root_type_name,
                            &schema_types,
                            &types_with_id,
                            &mut visited_fragments,
                            &mut diagnostics,
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
                        let mut visited_fragments = HashSet::new();
                        check_selection_set(
                            &selection_set,
                            type_name,
                            &schema_types,
                            &types_with_id,
                            &mut visited_fragments,
                            &mut diagnostics,
                        );
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

#[allow(clippy::only_used_in_recursion)]
fn check_selection_set(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    types_with_id: &HashMap<String, bool>,
    visited_fragments: &mut HashSet<String>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // Check if this type has an id field
    let has_id_field = types_with_id
        .get(parent_type_name)
        .copied()
        .unwrap_or(false);
    if !has_id_field {
        return; // Type doesn't have id field, nothing to check
    }

    let mut has_id_in_selection = false;

    // Check selections for id field and recurse into nested selections
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    let field_name_str = field_name.text();

                    // Check if this is the id field
                    if field_name_str == "id" {
                        has_id_in_selection = true;
                    }

                    // Recurse into nested selection sets
                    if let Some(nested_selection_set) = field.selection_set() {
                        // Get the field's return type from schema
                        if let Some(field_type) =
                            get_field_type(parent_type_name, &field_name_str, schema_types)
                        {
                            check_selection_set(
                                &nested_selection_set,
                                &field_type,
                                schema_types,
                                types_with_id,
                                visited_fragments,
                                diagnostics,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(_fragment_spread) => {
                // TODO: Implement fragment resolution using HIR
                // For now, we conservatively assume fragments might contain id
                // This prevents false positives but may miss some cases
                has_id_in_selection = true;
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

                    // Check for id in inline fragment's direct fields
                    for nested_selection in nested_selection_set.selections() {
                        if let cst::Selection::Field(nested_field) = nested_selection {
                            if let Some(field_name) = nested_field.name() {
                                if field_name.text() == "id" {
                                    has_id_in_selection = true;
                                }

                                // Recurse into nested object selections
                                if let Some(field_selection_set) = nested_field.selection_set() {
                                    if let Some(field_type) = get_field_type(
                                        &inline_type,
                                        &field_name.text(),
                                        schema_types,
                                    ) {
                                        check_selection_set(
                                            &field_selection_set,
                                            &field_type,
                                            schema_types,
                                            types_with_id,
                                            visited_fragments,
                                            diagnostics,
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

    // If type has id field and it's not in the selection, emit diagnostic
    if !has_id_in_selection {
        let syntax_node = selection_set.syntax();
        let start_offset: usize = syntax_node.text_range().start().into();
        let end_offset: usize = start_offset + 1;

        diagnostics.push(LintDiagnostic::warning(
            start_offset,
            end_offset,
            format!("Selection set on type '{parent_type_name}' should include the 'id' field"),
            "require_id_field",
        ));
    }
}

/// Find root operation type (Query, Mutation, or Subscription)
/// Falls back to the default name if no custom schema definition exists
fn find_root_operation_type(
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
    default_name: &str,
) -> Option<String> {
    // TODO: Read from schema definition directive once HIR supports it
    // For now, use the default names
    if schema_types.contains_key(default_name) {
        Some(default_name.to_string())
    } else {
        None
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
