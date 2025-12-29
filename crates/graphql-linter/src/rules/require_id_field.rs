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

        // Get all fragments from the project (for cross-file resolution)
        let all_fragments = graphql_hir::all_fragments_with_project(db, project_files);

        // Get root operation types from schema
        let query_type = find_root_operation_type(&schema_types, "Query");
        let mutation_type = find_root_operation_type(&schema_types, "Mutation");
        let subscription_type = find_root_operation_type(&schema_types, "Subscription");

        // Create context for fragment resolution
        let check_context = CheckContext {
            db,
            schema_types: &schema_types,
            types_with_id: &types_with_id,
            all_fragments: &all_fragments,
        };

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
                            &check_context,
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
                            &check_context,
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

/// Context for checking selection sets with fragment resolution
struct CheckContext<'a> {
    db: &'a dyn graphql_hir::GraphQLHirDatabase,
    schema_types: &'a HashMap<Arc<str>, graphql_hir::TypeDef>,
    types_with_id: &'a HashMap<String, bool>,
    all_fragments: &'a HashMap<Arc<str>, graphql_hir::FragmentStructure>,
}

#[allow(clippy::only_used_in_recursion)]
fn check_selection_set(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // Check if this type has an id field
    let has_id_field = context
        .types_with_id
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
                            get_field_type(parent_type_name, &field_name_str, context.schema_types)
                        {
                            check_selection_set(
                                &nested_selection_set,
                                &field_type,
                                context,
                                visited_fragments,
                                diagnostics,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                // Check if the fragment contains the id field
                if let Some(fragment_name) = fragment_spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        let name_str = name.text().to_string();
                        if fragment_contains_id(
                            &name_str,
                            parent_type_name,
                            context,
                            visited_fragments,
                        ) {
                            has_id_in_selection = true;
                        }
                    }
                }
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
                                        context.schema_types,
                                    ) {
                                        check_selection_set(
                                            &field_selection_set,
                                            &field_type,
                                            context,
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

/// Check if a fragment (or its nested fragments) contains the `id` field
fn fragment_contains_id(
    fragment_name: &str,
    parent_type_name: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
) -> bool {
    // Prevent infinite recursion with circular fragment references
    if visited_fragments.contains(fragment_name) {
        return false;
    }
    visited_fragments.insert(fragment_name.to_string());

    // Look up the fragment in HIR
    let Some(fragment_info) = context.all_fragments.get(fragment_name) else {
        // Fragment not found - might be undefined
        return false;
    };

    // Get the fragment's file and parse it (cached by Salsa)
    let file_id = fragment_info.file_id;

    // We need to get the file content and metadata to parse it
    // Use the document_files from project_files to find this file
    let document_files = context.db.document_files();

    let Some((file_content, file_metadata)) = document_files
        .iter()
        .find(|(fid, _, _)| *fid == file_id)
        .map(|(_, c, m)| (*c, *m))
    else {
        return false;
    };

    // Parse the file (cached by Salsa)
    let parse = graphql_syntax::parse(context.db, file_content, file_metadata);
    if !parse.errors.is_empty() {
        return false;
    }

    // Find the fragment definition in the CST
    let doc_cst = parse.tree.document();
    for definition in doc_cst.definitions() {
        if let cst::Definition::FragmentDefinition(frag) = definition {
            // Check if this is the fragment we're looking for
            let is_target_fragment = frag
                .fragment_name()
                .and_then(|name| name.name())
                .is_some_and(|name| name.text() == fragment_name);

            if !is_target_fragment {
                continue;
            }

            // Found the fragment, check its selection set for id
            if let Some(selection_set) = frag.selection_set() {
                return check_fragment_selection_for_id(
                    &selection_set,
                    parent_type_name,
                    context,
                    visited_fragments,
                );
            }
        }
    }

    false
}

/// Check if a selection set within a fragment contains the `id` field
/// This is similar to `check_selection_set` but only checks for presence of id,
/// doesn't emit diagnostics
fn check_fragment_selection_for_id(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
) -> bool {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    // Check if this is the id field
                    if field_name.text() == "id" {
                        return true;
                    }

                    // Recurse into nested selection sets
                    if let Some(nested_selection_set) = field.selection_set() {
                        if let Some(field_type) = get_field_type(
                            parent_type_name,
                            &field_name.text(),
                            context.schema_types,
                        ) {
                            if check_fragment_selection_for_id(
                                &nested_selection_set,
                                &field_type,
                                context,
                                visited_fragments,
                            ) {
                                return true;
                            }
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                // Recursively check nested fragment spreads
                if let Some(fragment_name) = fragment_spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        let name_str = name.text().to_string();
                        if fragment_contains_id(
                            &name_str,
                            parent_type_name,
                            context,
                            visited_fragments,
                        ) {
                            return true;
                        }
                    }
                }
            }
            cst::Selection::InlineFragment(inline_fragment) => {
                // Check inline fragments
                if let Some(nested_selection_set) = inline_fragment.selection_set() {
                    let inline_type = inline_fragment
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| parent_type_name.to_string(), |n| n.text().to_string());

                    if check_fragment_selection_for_id(
                        &nested_selection_set,
                        &inline_type,
                        context,
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
