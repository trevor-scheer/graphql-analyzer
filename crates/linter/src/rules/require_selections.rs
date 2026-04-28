use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::schema_utils::extract_root_type_names;
use crate::traits::{DocumentSchemaLintRule, LintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Options for the `requireSelections` rule
///
/// Example configuration:
/// ```yaml
/// lint:
///   rules:
///     # Default: requires 'id' field
///     requireSelections: error
///
///     # Custom fields to require (if they exist on the type)
///     requireSelections: [error, { fields: ["id", "__typename"] }]
///
///     # Object style
///     requireSelections:
///       severity: error
///       options:
///         fields: ["id", "__typename"]
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RequireSelectionsOptions {
    /// Field names to require if they exist on the type.
    /// Defaults to `["id"]`.
    pub fields: Vec<String>,
}

impl Default for RequireSelectionsOptions {
    fn default() -> Self {
        Self {
            fields: vec!["id".to_string()],
        }
    }
}

impl RequireSelectionsOptions {
    /// Parse options from a JSON value, falling back to defaults on error
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Trait implementation for `requireSelections` rule
pub struct RequireSelectionsRuleImpl;

impl LintRule for RequireSelectionsRuleImpl {
    fn name(&self) -> &'static str {
        "requireSelections"
    }

    fn description(&self) -> &'static str {
        "Enforces that specific fields (e.g. id, __typename) are selected on object types where they exist, supporting cache normalization"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Error
    }
}

impl DocumentSchemaLintRule for RequireSelectionsRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let opts = RequireSelectionsOptions::from_json(options);
        let mut diagnostics = Vec::new();
        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        // Get schema types from HIR
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Build a map of type names to their required fields (from options) that exist
        let mut types_with_required_fields: HashMap<String, Vec<String>> = HashMap::new();
        for (type_name, type_def) in schema_types {
            let required_fields: Vec<String> = match type_def.kind {
                graphql_hir::TypeDefKind::Object | graphql_hir::TypeDefKind::Interface => opts
                    .fields
                    .iter()
                    .filter(|field| {
                        // __typename is implicitly available on all object/interface types
                        *field == "__typename"
                            || type_def.fields.iter().any(|f| f.name.as_ref() == *field)
                    })
                    .cloned()
                    .collect(),
                _ => Vec::new(),
            };
            types_with_required_fields.insert(type_name.to_string(), required_fields);
        }

        // Get all fragments from the project (for cross-file resolution)
        let all_fragments = graphql_hir::all_fragments(db, project_files);

        // Get root operation types from schema definition or fall back to defaults
        let root_types = extract_root_type_names(db, project_files, schema_types);

        // Create context for fragment resolution
        let check_context = CheckContext {
            db,
            project_files,
            schema_types,
            types_with_required_fields: &types_with_required_fields,
            all_fragments,
            root_types: &root_types,
        };

        for doc in parse.documents() {
            let doc_cst = doc.tree.document();
            check_document(
                &doc_cst,
                root_types.query.as_deref(),
                root_types.mutation.as_deref(),
                root_types.subscription.as_deref(),
                &check_context,
                &mut diagnostics,
                &doc,
            );
        }

        diagnostics
    }
}

/// Check a GraphQL document for `requireSelections` violations
fn check_document(
    doc_cst: &cst::Document,
    query_type: Option<&str>,
    mutation_type: Option<&str>,
    subscription_type: Option<&str>,
    check_context: &CheckContext,
    diagnostics: &mut Vec<LintDiagnostic>,
    doc: &graphql_syntax::DocumentRef<'_>,
) {
    for definition in doc_cst.definitions() {
        match definition {
            cst::Definition::OperationDefinition(op) => {
                use super::{get_operation_kind, OperationKind};
                let root_type = op.operation_type().map_or(query_type, |op_type| {
                    match get_operation_kind(&op_type) {
                        OperationKind::Query => query_type,
                        OperationKind::Mutation => mutation_type,
                        OperationKind::Subscription => subscription_type,
                    }
                });

                if let (Some(root_type_name), Some(selection_set)) = (root_type, op.selection_set())
                {
                    // Operation root selection sets are skipped via `is_root_type`,
                    // so the display name here is only used by recursive children
                    // (which override it with their own field name/alias).
                    let display_name = op
                        .name()
                        .map_or_else(|| root_type_name.to_string(), |n| n.text().to_string());
                    let mut visited_fragments = HashSet::new();
                    let mut checked_fragments = HashSet::new();
                    check_selection_set(
                        &selection_set,
                        root_type_name,
                        &display_name,
                        check_context,
                        &mut visited_fragments,
                        &mut checked_fragments,
                        diagnostics,
                        doc,
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
                    let frag_name = frag
                        .fragment_name()
                        .and_then(|fn_| fn_.name())
                        .map(|n| n.text().to_string());
                    let display_name = frag_name.unwrap_or_else(|| type_name.to_string());
                    let mut visited_fragments = HashSet::new();
                    let mut checked_fragments = HashSet::new();
                    check_selection_set(
                        &selection_set,
                        type_name,
                        &display_name,
                        check_context,
                        &mut visited_fragments,
                        &mut checked_fragments,
                        diagnostics,
                        doc,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Context for checking selection sets with fragment resolution
struct CheckContext<'a> {
    db: &'a dyn graphql_hir::GraphQLHirDatabase,
    project_files: graphql_base_db::ProjectFiles,
    schema_types: &'a HashMap<Arc<str>, graphql_hir::TypeDef>,
    types_with_required_fields: &'a HashMap<String, Vec<String>>,
    all_fragments: &'a HashMap<Arc<str>, graphql_hir::FragmentStructure>,
    root_types: &'a crate::schema_utils::RootTypeNames,
}

#[allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]
fn check_selection_set(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    parent_display_name: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
    checked_fragments: &mut HashSet<String>,
    diagnostics: &mut Vec<LintDiagnostic>,
    doc: &graphql_syntax::DocumentRef<'_>,
) {
    // Skip root operation types (Query/Mutation/Subscription) since they are
    // singletons that don't benefit from cache normalization
    let required_fields = if context.root_types.is_root_type(parent_type_name) {
        Vec::new()
    } else {
        context
            .types_with_required_fields
            .get(parent_type_name)
            .cloned()
            .unwrap_or_default()
    };

    // Track which required fields are present in the selection
    let mut found_fields: HashSet<String> = HashSet::new();

    // Named fragment spreads visited while resolving the required fields, in
    // the order they were first walked. Mirrors graphql-eslint's
    // `checkedFragmentSpreads` set, which feeds the
    // ` or add to used fragment(s) X` diagnostic suffix. Inline fragments are
    // intentionally excluded — they don't have a name to list.
    let mut walked_fragments: Vec<String> = Vec::new();
    let mut walked_fragments_seen: HashSet<String> = HashSet::new();

    // Always iterate through selections to recurse into nested selection sets
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    let field_name_str = field_name.text();

                    // Check both the actual field name and any alias against required
                    // fields. Upstream explicitly handles `id: name` (alias `id` over
                    // field `name`) as satisfying the `id` requirement.
                    let satisfied_name = if required_fields.contains(&field_name_str.to_string()) {
                        Some(field_name_str.to_string())
                    } else {
                        field
                            .alias()
                            .and_then(|a| a.name())
                            .map(|a| a.text().to_string())
                            .filter(|alias| required_fields.contains(alias))
                    };
                    if let Some(name) = satisfied_name {
                        found_fields.insert(name);
                    }

                    // Always recurse into nested selection sets
                    if let Some(nested_selection_set) = field.selection_set() {
                        if let Some(field_type) =
                            get_field_type(parent_type_name, &field_name_str, context.schema_types)
                        {
                            let nested_display_name =
                                field.alias().and_then(|a| a.name()).map_or_else(
                                    || field_name_str.to_string(),
                                    |n| n.text().to_string(),
                                );
                            check_selection_set(
                                &nested_selection_set,
                                &field_type,
                                &nested_display_name,
                                context,
                                visited_fragments,
                                checked_fragments,
                                diagnostics,
                                doc,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                if let Some(fragment_name) = fragment_spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        let name_str = name.text().to_string();

                        // Check whether the fragment provides required fields for
                        // this level (to avoid a false "missing" diagnostic here).
                        if !required_fields.is_empty() {
                            for required_field in &required_fields {
                                let mut visited_clone = visited_fragments.clone();
                                if fragment_contains_field(
                                    &name_str,
                                    parent_type_name,
                                    required_field,
                                    context,
                                    &mut visited_clone,
                                    &mut walked_fragments,
                                    &mut walked_fragments_seen,
                                ) {
                                    found_fields.insert(required_field.clone());
                                }
                            }
                        }

                        // Also recurse into the fragment body to lint its own
                        // nested selection sets. Upstream graphql-eslint walks
                        // every node it encounters, including those inside
                        // spread fragments. We replicate that here.
                        check_fragment_body_violations(
                            &name_str,
                            context,
                            checked_fragments,
                            diagnostics,
                        );
                    }
                }
            }
            cst::Selection::InlineFragment(inline_fragment) => {
                if let Some(nested_selection_set) = inline_fragment.selection_set() {
                    let inline_type = inline_fragment
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| parent_type_name.to_string(), |n| n.text().to_string());

                    // Scan the inline fragment's selections to:
                    // 1. Collect fields that satisfy the *parent* type's required fields
                    // 2. Recurse into nested sub-selection sets
                    for nested_selection in nested_selection_set.selections() {
                        match nested_selection {
                            cst::Selection::Field(nested_field) => {
                                if let Some(field_name) = nested_field.name() {
                                    let field_name_str = field_name.text();
                                    let satisfied_name =
                                        if required_fields.contains(&field_name_str.to_string()) {
                                            Some(field_name_str.to_string())
                                        } else {
                                            nested_field
                                                .alias()
                                                .and_then(|a| a.name())
                                                .map(|a| a.text().to_string())
                                                .filter(|alias| required_fields.contains(alias))
                                        };
                                    if let Some(name) = satisfied_name {
                                        found_fields.insert(name);
                                    }

                                    if let Some(field_selection_set) = nested_field.selection_set()
                                    {
                                        if let Some(field_type) = get_field_type(
                                            &inline_type,
                                            &field_name.text(),
                                            context.schema_types,
                                        ) {
                                            let nested_display_name = nested_field
                                                .alias()
                                                .and_then(|a| a.name())
                                                .map_or_else(
                                                    || field_name.text().to_string(),
                                                    |n| n.text().to_string(),
                                                );
                                            check_selection_set(
                                                &field_selection_set,
                                                &field_type,
                                                &nested_display_name,
                                                context,
                                                visited_fragments,
                                                checked_fragments,
                                                diagnostics,
                                                doc,
                                            );
                                        }
                                    }
                                }
                            }
                            cst::Selection::FragmentSpread(fragment_spread) => {
                                if let Some(fragment_name) = fragment_spread.fragment_name() {
                                    if let Some(name) = fragment_name.name() {
                                        let name_str = name.text().to_string();

                                        if !required_fields.is_empty() {
                                            for required_field in &required_fields {
                                                let mut visited_clone = visited_fragments.clone();
                                                if fragment_contains_field(
                                                    &name_str,
                                                    parent_type_name,
                                                    required_field,
                                                    context,
                                                    &mut visited_clone,
                                                    &mut walked_fragments,
                                                    &mut walked_fragments_seen,
                                                ) {
                                                    found_fields.insert(required_field.clone());
                                                }
                                            }
                                        }

                                        check_fragment_body_violations(
                                            &name_str,
                                            context,
                                            checked_fragments,
                                            diagnostics,
                                        );
                                    }
                                }
                            }
                            cst::Selection::InlineFragment(_) => {
                                // Nested inline fragments handled by recursion
                            }
                        }
                    }

                    // When the inline fragment narrows to a concrete type (e.g.
                    // `... on User { title }` inside a union/interface field),
                    // check that the concrete type's required fields are satisfied.
                    // The combined view is: parent-level fields already in
                    // `found_fields` (e.g. `id` selected directly on the interface)
                    // PLUS the inline fragment's own fields.
                    //
                    // We only do this when the type actually narrows — if the
                    // inline fragment has the same type as the parent
                    // (a bare `... { ... }`) the parent's own `required_fields`
                    // check already covers it.
                    if inline_type != parent_type_name {
                        check_inline_fragment_type(
                            &nested_selection_set,
                            &inline_type,
                            &found_fields,
                            context,
                            visited_fragments,
                            checked_fragments,
                            diagnostics,
                            doc,
                        );
                    }
                }
            }
        }
    }

    // Collect missing fields and emit a single grouped diagnostic per
    // selection set, matching graphql-eslint's `require-selections` output.
    let missing_fields: Vec<&String> = required_fields
        .iter()
        .filter(|f| !found_fields.contains(*f))
        .collect();

    if !missing_fields.is_empty() {
        // Calculate insertion position and indentation for the fix
        let selection_set_start: usize = selection_set.syntax().text_range().start().into();
        let selection_set_source = selection_set.syntax().to_string();

        let (insert_pos, indent) = selection_set.selections().next().map_or_else(
            || {
                // Empty selection set - insert after the opening brace with default indent
                (selection_set_start + 1, "  ".to_string())
            },
            |first| {
                let pos: usize = first.syntax().text_range().start().into();
                let relative_pos = pos - selection_set_start;
                let indent = extract_indentation(&selection_set_source, relative_pos);
                (pos, indent)
            },
        );

        let fix_label = if missing_fields.len() == 1 {
            format!("Add `{}` selection", missing_fields[0])
        } else {
            // TODO(parity): graphql-eslint emits one suggestion per `idName`
            // in a multi-suggestion code action. We only have a single-fix API
            // today, so we concatenate the missing fields into one fix.
            let joined = missing_fields
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("Add {joined} selections")
        };
        let mut fix_text = String::new();
        for f in &missing_fields {
            fix_text.push_str(f);
            fix_text.push('\n');
            fix_text.push_str(&indent);
        }

        let fix = CodeFix::new(fix_label, vec![TextEdit::insert(insert_pos, fix_text)]);

        let plural_suffix = if missing_fields.len() > 1 { "s" } else { "" };
        let joined_field_refs = english_join_words(
            &missing_fields
                .iter()
                .map(|f| format!("`{parent_display_name}.{f}`"))
                .collect::<Vec<_>>(),
        );

        // graphql-eslint appends ` or add to used fragment(s) X` when the
        // missing field is reachable through fragment(s) walked above that
        // didn't ultimately satisfy it. We only get here when no walked
        // fragment contained the field, so listing all walked fragments
        // mirrors upstream's `checkedFragmentSpreads` set behavior exactly.
        let addition = if walked_fragments.is_empty() {
            String::new()
        } else {
            let frag_plural = if walked_fragments.len() > 1 { "s" } else { "" };
            let joined = english_join_words(
                &walked_fragments
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>(),
            );
            format!(" or add to used fragment{frag_plural} {joined}")
        };

        // Mirror graphql-eslint: diagnostic points at the SelectionSet's
        // opening `{` with a start-only `loc` (no end position). Emit a
        // degenerate range (start == end); the eslint adapter strips
        // `endLine`/`endColumn` for rules listed in `START_ONLY_RULES`.
        diagnostics.push(
            LintDiagnostic::error(
                doc.span(selection_set_start, selection_set_start),
                format!(
                    "Field{plural_suffix} {joined_field_refs} must be selected when it's available on a type.\nInclude it in your selection set{addition}."
                ),
                "requireSelections",
            )
            .with_message_id("require-selections")
            .with_fix(fix),
        );
    }
}

/// Check required fields for a type-narrowing inline fragment (`... on ConcreteType { ... }`).
///
/// When an inline fragment narrows to a concrete type inside a union/interface
/// field, we must verify that `ConcreteType`'s required fields are satisfied by the
/// COMBINED view of: fields already selected at the parent level (`parent_found`)
/// plus fields inside the inline fragment itself.
///
/// This mirrors graphql-eslint's behaviour where `vehicles { id ...on Car { mileage } }`
/// is valid because `id` — already selected on the `Vehicle` interface — also satisfies
/// the `Car.id` requirement.
#[allow(clippy::too_many_arguments)]
fn check_inline_fragment_type(
    inline_selection_set: &cst::SelectionSet,
    inline_type_name: &str,
    parent_found_fields: &HashSet<String>,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
    checked_fragments: &mut HashSet<String>,
    diagnostics: &mut Vec<LintDiagnostic>,
    doc: &graphql_syntax::DocumentRef<'_>,
) {
    let required_fields = if context.root_types.is_root_type(inline_type_name) {
        return; // root types don't need cache keys
    } else {
        context
            .types_with_required_fields
            .get(inline_type_name)
            .cloned()
            .unwrap_or_default()
    };

    if required_fields.is_empty() {
        return;
    }

    // Start with fields already satisfied at the parent selection-set level.
    let mut found_fields: HashSet<String> = parent_found_fields
        .iter()
        .filter(|f| required_fields.contains(*f))
        .cloned()
        .collect();

    let mut walked_fragments: Vec<String> = Vec::new();
    let mut walked_fragments_seen: HashSet<String> = HashSet::new();

    for selection in inline_selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    let field_name_str = field_name.text();
                    let satisfied_name = if required_fields.contains(&field_name_str.to_string()) {
                        Some(field_name_str.to_string())
                    } else {
                        field
                            .alias()
                            .and_then(|a| a.name())
                            .map(|a| a.text().to_string())
                            .filter(|alias| required_fields.contains(alias))
                    };
                    if let Some(name) = satisfied_name {
                        found_fields.insert(name);
                    }

                    // Recurse into nested sub-selections
                    if let Some(nested_selection_set) = field.selection_set() {
                        if let Some(field_type) =
                            get_field_type(inline_type_name, &field_name_str, context.schema_types)
                        {
                            let nested_display_name =
                                field.alias().and_then(|a| a.name()).map_or_else(
                                    || field_name_str.to_string(),
                                    |n| n.text().to_string(),
                                );
                            check_selection_set(
                                &nested_selection_set,
                                &field_type,
                                &nested_display_name,
                                context,
                                visited_fragments,
                                checked_fragments,
                                diagnostics,
                                doc,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                if let Some(fragment_name) = fragment_spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        let name_str = name.text().to_string();
                        for required_field in &required_fields {
                            let mut visited_clone = visited_fragments.clone();
                            if fragment_contains_field(
                                &name_str,
                                inline_type_name,
                                required_field,
                                context,
                                &mut visited_clone,
                                &mut walked_fragments,
                                &mut walked_fragments_seen,
                            ) {
                                found_fields.insert(required_field.clone());
                            }
                        }
                    }
                }
            }
            cst::Selection::InlineFragment(nested_inline) => {
                if let Some(nested_ss) = nested_inline.selection_set() {
                    let nested_type = nested_inline
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| inline_type_name.to_string(), |n| n.text().to_string());
                    if nested_type == inline_type_name {
                        // Same type: collect fields for the same required-field check
                        for nested_sel in nested_ss.selections() {
                            if let cst::Selection::Field(f) = nested_sel {
                                if let Some(fn_) = f.name() {
                                    let fn_str = fn_.text();
                                    let satisfied = if required_fields.contains(&fn_str.to_string())
                                    {
                                        Some(fn_str.to_string())
                                    } else {
                                        f.alias()
                                            .and_then(|a| a.name())
                                            .map(|a| a.text().to_string())
                                            .filter(|alias| required_fields.contains(alias))
                                    };
                                    if let Some(name) = satisfied {
                                        found_fields.insert(name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let missing_fields: Vec<&String> = required_fields
        .iter()
        .filter(|f| !found_fields.contains(*f))
        .collect();

    if !missing_fields.is_empty() {
        let selection_set_start: usize = inline_selection_set.syntax().text_range().start().into();
        let selection_set_source = inline_selection_set.syntax().to_string();

        let (insert_pos, indent) = inline_selection_set.selections().next().map_or_else(
            || (selection_set_start + 1, "  ".to_string()),
            |first| {
                let pos: usize = first.syntax().text_range().start().into();
                let relative_pos = pos - selection_set_start;
                let indent = extract_indentation(&selection_set_source, relative_pos);
                (pos, indent)
            },
        );

        let fix_label = if missing_fields.len() == 1 {
            format!("Add `{}` selection", missing_fields[0])
        } else {
            let joined = missing_fields
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("Add {joined} selections")
        };
        let mut fix_text = String::new();
        for f in &missing_fields {
            fix_text.push_str(f);
            fix_text.push('\n');
            fix_text.push_str(&indent);
        }
        let fix = CodeFix::new(fix_label, vec![TextEdit::insert(insert_pos, fix_text)]);

        let plural_suffix = if missing_fields.len() > 1 { "s" } else { "" };
        let joined_field_refs = english_join_words(
            &missing_fields
                .iter()
                .map(|f| format!("`{inline_type_name}.{f}`"))
                .collect::<Vec<_>>(),
        );

        let addition = if walked_fragments.is_empty() {
            String::new()
        } else {
            let frag_plural = if walked_fragments.len() > 1 { "s" } else { "" };
            let joined = english_join_words(
                &walked_fragments
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>(),
            );
            format!(" or add to used fragment{frag_plural} {joined}")
        };

        diagnostics.push(
            LintDiagnostic::error(
                doc.span(selection_set_start, selection_set_start),
                format!(
                    "Field{plural_suffix} {joined_field_refs} must be selected when it's available on a type.\nInclude it in your selection set{addition}."
                ),
                "requireSelections",
            )
            .with_message_id("require-selections")
            .with_fix(fix),
        );
    }
}

/// Format a list of items using English-style disjunction (matching
/// `Intl.ListFormat("en-US", { type: "disjunction" })` used by graphql-eslint):
/// `a`, `a or b`, `a, b, or c`.
fn english_join_words(words: &[String]) -> String {
    match words.len() {
        0 => String::new(),
        1 => words[0].clone(),
        2 => format!("{} or {}", words[0], words[1]),
        _ => {
            let (last, rest) = words.split_last().expect("match arm guarantees len >= 3");
            format!("{}, or {last}", rest.join(", "))
        }
    }
}

/// Get the return type name for a field, unwrapping `List` and `NonNull` wrappers
fn get_field_type(
    parent_type_name: &str,
    field_name: &str,
    schema_types: &HashMap<Arc<str>, graphql_hir::TypeDef>,
) -> Option<String> {
    let type_def = schema_types.get(parent_type_name)?;

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

    Some(field.type_ref.name.to_string())
}

/// Extract the indentation (whitespace) before a given position in source
fn extract_indentation(source: &str, pos: usize) -> String {
    let before = &source[..pos];
    if let Some(newline_pos) = before.rfind('\n') {
        let indent_slice = &before[newline_pos + 1..];
        indent_slice
            .chars()
            .take_while(|c| c.is_whitespace() && *c != '\n')
            .collect()
    } else {
        "  ".to_string()
    }
}

/// Walk a fragment body and emit diagnostics for nested selection-set violations.
///
/// Upstream graphql-eslint processes every `SelectionSet` node it encounters,
/// including those found by following fragment spreads. The selector fires on
/// Field-parented selection sets but NOT on FragmentDefinition-parented ones
/// (the fragment's own selection set is never directly checked for its required
/// fields — only the sub-selections inside field accesses are).
///
/// This function mirrors that: it walks the fragment body, finds Field-parented
/// sub-selection sets, and calls `check_selection_set` on them. Fragment spreads
/// within the body are recursed into via another call to this function. Union
/// fragment spreads are handled by calling `check_selection_set` directly on
/// the fragment body with the concrete union member type.
///
/// `checked_fragments` prevents revisiting the same fragment (cycle guard).
fn check_fragment_body_violations(
    fragment_name: &str,
    context: &CheckContext,
    checked_fragments: &mut HashSet<String>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if !checked_fragments.insert(fragment_name.to_string()) {
        return; // already processed
    }

    let Some(fragment_info) = context.all_fragments.get(fragment_name) else {
        return;
    };

    let file_id = fragment_info.file_id;

    let Some((file_content, file_metadata)) =
        graphql_base_db::file_lookup(context.db, context.project_files, file_id)
    else {
        return;
    };

    let parse = graphql_syntax::parse(context.db, file_content, file_metadata);
    if parse.has_errors() {
        return;
    }

    for doc_ref in parse.documents() {
        let doc_cst = doc_ref.tree.document();
        for definition in doc_cst.definitions() {
            if let cst::Definition::FragmentDefinition(frag) = definition {
                let is_target = frag
                    .fragment_name()
                    .and_then(|n| n.name())
                    .is_some_and(|n| n.text() == fragment_name);
                if !is_target {
                    continue;
                }

                let type_condition = frag
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|n| n.text().to_string());
                let Some(frag_type) = type_condition else {
                    continue;
                };

                if let Some(selection_set) = frag.selection_set() {
                    lint_fragment_sub_selections(
                        &selection_set,
                        &frag_type,
                        context,
                        checked_fragments,
                        diagnostics,
                        &doc_ref,
                    );
                }
            }
        }
    }
}

/// Walk a selection set and emit diagnostics for any Field-parented sub-selection
/// sets that are missing required fields.
///
/// This intentionally does NOT check the passed `selection_set` itself for
/// missing required fields at the parent type level — that mirrors the upstream
/// graphql-eslint selector which skips `FragmentDefinition`-parented selection
/// sets. Only Field-parented (and transitively, fragment-spread-followed) sub-
/// selections are checked.
///
/// For union types, fragment spreads are handled by calling `check_selection_set`
/// directly on the spread's body with the resolved concrete member type — matching
/// upstream's `checkSelections` recursive union handling.
#[allow(clippy::too_many_arguments)]
fn lint_fragment_sub_selections(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    context: &CheckContext,
    checked_fragments: &mut HashSet<String>,
    diagnostics: &mut Vec<LintDiagnostic>,
    doc: &graphql_syntax::DocumentRef<'_>,
) {
    let parent_type_is_union = context
        .schema_types
        .get(parent_type_name)
        .is_some_and(|t| t.kind == graphql_hir::TypeDefKind::Union);

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    let field_name_str = field_name.text();
                    if let Some(nested_ss) = field.selection_set() {
                        if let Some(field_type) =
                            get_field_type(parent_type_name, &field_name_str, context.schema_types)
                        {
                            let display_name = field.alias().and_then(|a| a.name()).map_or_else(
                                || field_name_str.to_string(),
                                |n| n.text().to_string(),
                            );
                            let mut visited_frags: HashSet<String> = HashSet::new();
                            // Field-parented selection set: check it fully.
                            check_selection_set(
                                &nested_ss,
                                &field_type,
                                &display_name,
                                context,
                                &mut visited_frags,
                                checked_fragments,
                                diagnostics,
                                doc,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                if let Some(frag_name_node) = spread.fragment_name().and_then(|fn_| fn_.name()) {
                    let spread_name = frag_name_node.text().to_string();

                    if parent_type_is_union {
                        // Union context: look up the spread's fragment type and
                        // call check_selection_set on the fragment body directly
                        // with the resolved union member type (upstream's union
                        // handling in checkSelections).
                        if let Some(frag_info) = context.all_fragments.get(spread_name.as_str()) {
                            let frag_type = frag_info.type_condition.to_string();
                            // Only use the concrete union member type if it's in the union.
                            // If the fragment is on the union type itself, use the union type.
                            let resolved_type = if frag_type == parent_type_name {
                                frag_type.clone()
                            } else {
                                // Verify it's actually a member of the union.
                                let in_union =
                                    context.schema_types.get(parent_type_name).is_some_and(|t| {
                                        t.kind == graphql_hir::TypeDefKind::Union
                                            && t.union_members
                                                .iter()
                                                .any(|m| m.as_ref() == frag_type)
                                    });
                                if in_union {
                                    frag_type.clone()
                                } else {
                                    continue;
                                }
                            };

                            // For a union fragment, call check_selection_set on the
                            // fragment's body with the resolved type. This is how upstream
                            // handles union + fragment spread: it calls checkSelections
                            // recursively on the fragment's selectionSet.
                            let file_id = frag_info.file_id;
                            if let Some((fc, fm)) = graphql_base_db::file_lookup(
                                context.db,
                                context.project_files,
                                file_id,
                            ) {
                                let parse = graphql_syntax::parse(context.db, fc, fm);
                                if !parse.has_errors() {
                                    for frag_doc_ref in parse.documents() {
                                        for frag_def in frag_doc_ref.tree.document().definitions() {
                                            if let cst::Definition::FragmentDefinition(fd) =
                                                frag_def
                                            {
                                                let matches = fd
                                                    .fragment_name()
                                                    .and_then(|n| n.name())
                                                    .is_some_and(|n| n.text() == spread_name);
                                                if matches {
                                                    if let Some(frag_ss) = fd.selection_set() {
                                                        let frag_display = fd
                                                            .fragment_name()
                                                            .and_then(|n| n.name())
                                                            .map_or_else(
                                                                || resolved_type.clone(),
                                                                |n| n.text().to_string(),
                                                            );
                                                        let mut visited_frags: HashSet<String> =
                                                            HashSet::new();
                                                        check_selection_set(
                                                            &frag_ss,
                                                            &resolved_type,
                                                            &frag_display,
                                                            context,
                                                            &mut visited_frags,
                                                            checked_fragments,
                                                            diagnostics,
                                                            &frag_doc_ref,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Non-union: recurse into the fragment body for sub-selection violations.
                        check_fragment_body_violations(
                            &spread_name,
                            context,
                            checked_fragments,
                            diagnostics,
                        );
                    }
                }
            }
            cst::Selection::InlineFragment(inline_frag) => {
                if let Some(nested_ss) = inline_frag.selection_set() {
                    let inline_type = inline_frag
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| parent_type_name.to_string(), |n| n.text().to_string());

                    if inline_type == parent_type_name {
                        // Same-type bare inline fragment: recurse without a new type check.
                        lint_fragment_sub_selections(
                            &nested_ss,
                            inline_type.as_str(),
                            context,
                            checked_fragments,
                            diagnostics,
                            doc,
                        );
                    } else {
                        // Type-narrowing inline fragment: check required fields on the
                        // concrete type via check_selection_set directly.
                        let display_name = inline_type.clone();
                        let mut visited_frags: HashSet<String> = HashSet::new();
                        check_selection_set(
                            &nested_ss,
                            &inline_type,
                            &display_name,
                            context,
                            &mut visited_frags,
                            checked_fragments,
                            diagnostics,
                            doc,
                        );
                    }
                }
            }
        }
    }
}

/// Check if a fragment (or its nested fragments) contains the specified field.
///
/// Also records every named fragment spread visited during the walk into
/// `walked_fragments` (insertion-ordered, deduplicated via
/// `walked_fragments_seen`). The caller uses that list to render the
/// ` or add to used fragment(s) X` suffix on the diagnostic, matching
/// graphql-eslint's `checkedFragmentSpreads` set.
fn fragment_contains_field(
    fragment_name: &str,
    parent_type_name: &str,
    target_field: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
    walked_fragments: &mut Vec<String>,
    walked_fragments_seen: &mut HashSet<String>,
) -> bool {
    if walked_fragments_seen.insert(fragment_name.to_string()) {
        walked_fragments.push(fragment_name.to_string());
    }

    if visited_fragments.contains(fragment_name) {
        return false;
    }
    visited_fragments.insert(fragment_name.to_string());

    let Some(fragment_info) = context.all_fragments.get(fragment_name) else {
        return false;
    };

    let file_id = fragment_info.file_id;

    let Some((file_content, file_metadata)) =
        graphql_base_db::file_lookup(context.db, context.project_files, file_id)
    else {
        return false;
    };

    let parse = graphql_syntax::parse(context.db, file_content, file_metadata);
    if parse.has_errors() {
        return false;
    }

    for doc_ref in parse.documents() {
        let doc_cst = doc_ref.tree.document();
        for definition in doc_cst.definitions() {
            if let cst::Definition::FragmentDefinition(frag) = definition {
                let is_target_fragment = frag
                    .fragment_name()
                    .and_then(|name| name.name())
                    .is_some_and(|name| name.text() == fragment_name);

                if !is_target_fragment {
                    continue;
                }

                if let Some(selection_set) = frag.selection_set() {
                    return check_fragment_selection_for_field(
                        &selection_set,
                        parent_type_name,
                        target_field,
                        context,
                        visited_fragments,
                        walked_fragments,
                        walked_fragments_seen,
                    );
                }
            }
        }
    }

    false
}

/// Check if a selection set within a fragment contains the specified field
fn check_fragment_selection_for_field(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    target_field: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
    walked_fragments: &mut Vec<String>,
    walked_fragments_seen: &mut HashSet<String>,
) -> bool {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    if field_name.text() == target_field {
                        return true;
                    }
                    // An alias like `id: name` satisfies the `id` requirement.
                    if let Some(alias) = field.alias().and_then(|a| a.name()) {
                        if alias.text() == target_field {
                            return true;
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                if let Some(fragment_name) = fragment_spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        let name_str = name.text().to_string();
                        if fragment_contains_field(
                            &name_str,
                            parent_type_name,
                            target_field,
                            context,
                            visited_fragments,
                            walked_fragments,
                            walked_fragments_seen,
                        ) {
                            return true;
                        }
                    }
                }
            }
            cst::Selection::InlineFragment(inline_fragment) => {
                if let Some(nested_selection_set) = inline_fragment.selection_set() {
                    let inline_type = inline_fragment
                        .type_condition()
                        .and_then(|tc| tc.named_type())
                        .and_then(|nt| nt.name())
                        .map_or_else(|| parent_type_name.to_string(), |n| n.text().to_string());

                    if check_fragment_selection_for_field(
                        &nested_selection_set,
                        &inline_type,
                        target_field,
                        context,
                        visited_fragments,
                        walked_fragments,
                        walked_fragments_seen,
                    ) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_base_db::{
        DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language, ProjectFiles,
    };
    use graphql_hir::GraphQLHirDatabase;
    use graphql_ide_db::RootDatabase;

    /// Helper to create test project files with schema and document
    fn create_test_project(
        db: &dyn GraphQLHirDatabase,
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

    /// Helper to create test project with multiple document files (for cross-file fragment resolution)
    fn create_multi_file_project(
        db: &dyn GraphQLHirDatabase,
        schema_source: &str,
        documents: &[(&str, &str)], // Vec of (uri, source)
    ) -> Vec<(FileId, FileContent, FileMetadata, ProjectFiles)> {
        let schema_file_id = FileId::new(0);
        let schema_content = FileContent::new(db, Arc::from(schema_source));
        let schema_metadata = FileMetadata::new(
            db,
            schema_file_id,
            FileUri::new("file:///schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );

        let mut file_entries = std::collections::HashMap::new();
        let schema_entry = graphql_base_db::FileEntry::new(db, schema_content, schema_metadata);
        file_entries.insert(schema_file_id, schema_entry);

        let mut doc_file_ids_vec = Vec::new();
        let mut doc_infos = Vec::new();

        for (i, (uri, source)) in documents.iter().enumerate() {
            let doc_file_id = FileId::new((i + 1) as u32);
            let doc_content = FileContent::new(db, Arc::from(*source));
            let doc_metadata = FileMetadata::new(
                db,
                doc_file_id,
                FileUri::new(*uri),
                Language::GraphQL,
                DocumentKind::Executable,
            );

            let doc_entry = graphql_base_db::FileEntry::new(db, doc_content, doc_metadata);
            file_entries.insert(doc_file_id, doc_entry);
            doc_file_ids_vec.push(doc_file_id);
            doc_infos.push((doc_file_id, doc_content, doc_metadata));
        }

        let schema_file_ids =
            graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![schema_file_id]));
        let document_file_ids =
            graphql_base_db::DocumentFileIds::new(db, Arc::new(doc_file_ids_vec));
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

        doc_infos
            .into_iter()
            .map(|(file_id, content, metadata)| (file_id, content, metadata, project_files))
            .collect()
    }

    const TEST_SCHEMA: &str = "
type Query {
    user(id: ID!): User
    node(id: ID!): Node
    search(term: String!): SearchResult
}

type User implements Node {
    id: ID!
    name: String!
    email: String!
    posts: [Post!]!
    profile: Profile
}

type Post implements Node {
    id: ID!
    title: String!
    body: String!
    author: User!
}

type Profile {
    bio: String
    avatar: String
}

interface Node {
    id: ID!
}

union SearchResult = User | Post
";

    #[test]
    fn test_missing_id_field_warns() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetUser {
    user(id: \"1\") {
        name
        email
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        // graphql-eslint references the parent field's alias/name (here `user`)
        // rather than the type name.
        assert!(diagnostics[0].message.contains("`user.id`"));
        assert!(diagnostics[0].message.starts_with("Field `user.id`"));
        assert_eq!(diagnostics[0].severity, LintSeverity::Error);
    }

    #[test]
    fn test_id_field_present_no_warning() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetUser {
    user(id: \"1\") {
        id
        name
        email
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_type_without_id_no_warning() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        // Profile type has no `id` field
        let source = "
query GetUser {
    user(id: \"1\") {
        id
        profile {
            bio
            avatar
        }
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_fragment_provides_field_no_warning() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
fragment UserFields on User {
    id
    name
}

query GetUser {
    user(id: \"1\") {
        ...UserFields
        email
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // The fragment definition itself has `id`, so no warning for the query.
        // The fragment definition also selects on User which has id -> no warning.
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_custom_fields_via_options() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetUser {
    user(id: \"1\") {
        id
        name
    }
}
";
        let options = serde_json::json!({ "fields": ["id", "__typename"] });

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(
            &db,
            file_id,
            content,
            metadata,
            project_files,
            Some(&options),
        );

        // Has `id` but missing `__typename`
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("__typename"));
    }

    #[test]
    fn test_multiple_required_fields() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetUser {
    user(id: \"1\") {
        name
    }
}
";
        let options = serde_json::json!({ "fields": ["id", "__typename"] });

        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(
            &db,
            file_id,
            content,
            metadata,
            project_files,
            Some(&options),
        );

        // graphql-eslint groups missing fields into a single diagnostic per
        // selection set with a plural suffix and an `or`-joined list.
        assert_eq!(diagnostics.len(), 1);
        let msg = &diagnostics[0].message;
        assert!(msg.starts_with("Fields "), "got: {msg}");
        assert!(msg.contains("`user.id`"), "got: {msg}");
        assert!(msg.contains("`user.__typename`"), "got: {msg}");
        assert!(msg.contains(" or "), "got: {msg}");
    }

    #[test]
    fn test_inline_fragment_provides_field() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetUser {
    user(id: \"1\") {
        ... on User {
            id
            name
        }
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_nested_selection_set_checked() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        // user has id selected, but posts (which also has id) does not
        let source = "
query GetUser {
    user(id: \"1\") {
        id
        posts {
            title
        }
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        // Parent field is `posts`; graphql-eslint uses the alias/name here.
        assert!(diagnostics[0].message.contains("`posts.id`"));
    }

    #[test]
    fn test_mutation_operation() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let schema = "
type Query {
    user: User
}

type Mutation {
    updateUser(id: ID!): User
}

type User {
    id: ID!
    name: String!
}
";

        let source = "
mutation UpdateUser {
    updateUser(id: \"1\") {
        name
    }
}
";
        let (file_id, content, metadata, project_files) = create_test_project(&db, schema, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        // Parent field is `updateUser`; graphql-eslint uses the alias/name.
        assert!(diagnostics[0].message.contains("`updateUser.id`"));
    }

    #[test]
    fn test_cross_file_fragment_resolution() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let fragment_source = "
fragment UserFields on User {
    id
    name
}
";
        let query_source = "
query GetUser {
    user(id: \"1\") {
        ...UserFields
        email
    }
}
";

        let results = create_multi_file_project(
            &db,
            TEST_SCHEMA,
            &[
                ("file:///fragments.graphql", fragment_source),
                ("file:///query.graphql", query_source),
            ],
        );

        // Check the query file (second file)
        let (file_id, content, metadata, project_files) = &results[1];
        let diagnostics = rule.check(&db, *file_id, *content, *metadata, *project_files, None);

        // Fragment provides `id`, so no warning
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_diagnostic_points_at_selection_set_open_brace() {
        // Mirrors graphql-eslint: diagnostic points at the SelectionSet's
        // opening `{` with a degenerate (start == end) range so the eslint
        // adapter emits a start-only `loc`.
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let schema = "type Query { user: User } type User { id: ID! name: String! }";
        // Offsets:     0         1         2
        //              0123456789012345678901234567
        let source = "query Q { user { name } }";
        let (file_id, content, metadata, project_files) = create_test_project(&db, schema, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        // The `{` of the inner selection set `{ name }` is at offset 15.
        assert_eq!(diagnostics[0].span.start, 15);
        // Degenerate range so the JS adapter strips end positions.
        assert_eq!(diagnostics[0].span.end, diagnostics[0].span.start);
    }

    #[test]
    fn test_diagnostic_has_fix() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetUser {
    user(id: \"1\") {
        name
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].has_fix());
    }

    #[test]
    fn test_interface_type_checked() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetNode {
    node(id: \"1\") {
        ... on User {
            name
        }
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // The node field returns Node interface which has `id`.
        // The selection set on Node is missing `id`.
        // The inline fragment on User is also missing `id`.
        assert!(!diagnostics.is_empty());
        // Parent field is `node`; ensure at least one diagnostic surfaces
        // `node.id` (the alias/name form, not the type name).
        assert!(diagnostics.iter().any(|d| d.message.contains("`node.id`")));
    }

    #[test]
    fn test_fragment_routing_suffix_single_fragment_without_field() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        // The fragment is reachable from `user` but does NOT contain `id`.
        // graphql-eslint appends ` or add to used fragment `UserName`` to the
        // selection-set message.
        let source = "
fragment UserName on User {
    name
}

query GetUser {
    user(id: \"1\") {
        ...UserName
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        let user_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("`user.id`"))
            .collect();
        assert_eq!(user_diags.len(), 1, "got: {diagnostics:#?}");
        let msg = &user_diags[0].message;
        assert!(
            msg.ends_with("Include it in your selection set or add to used fragment `UserName`."),
            "got: {msg}"
        );
    }

    #[test]
    fn test_fragment_routing_suffix_two_fragments_without_field() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        // Two fragments are reachable from `user`; neither has `id`.
        let source = "
fragment UserName on User {
    name
}

fragment UserEmail on User {
    email
}

query GetUser {
    user(id: \"1\") {
        ...UserName
        ...UserEmail
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        let user_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("`user.id`"))
            .collect();
        assert_eq!(user_diags.len(), 1, "got: {diagnostics:#?}");
        let msg = &user_diags[0].message;
        // Plural: "fragments". englishJoinWords disjunction joins two with " or ".
        assert!(
            msg.ends_with(
                "Include it in your selection set or add to used fragments `UserName` or `UserEmail`."
            ),
            "got: {msg}"
        );
    }

    #[test]
    fn test_fragment_routing_suffix_omits_fragment_with_field() {
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        // One fragment HAS `id`; it satisfies the rule and no diagnostic is
        // emitted. Match graphql-eslint: when any walked fragment provides the
        // field, `hasIdField` returns true and `report` short-circuits.
        let source = "
fragment UserId on User {
    id
}

fragment UserEmail on User {
    email
}

query GetUser {
    user(id: \"1\") {
        ...UserId
        ...UserEmail
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // Fragment provides `id` -> no diagnostic on the `user` selection set.
        assert!(
            !diagnostics.iter().any(|d| d.message.contains("`user.id`")),
            "got: {diagnostics:#?}"
        );
    }

    #[test]
    fn test_fragment_routing_suffix_no_fragments_no_suffix() {
        // Regression: existing behavior with no fragments at all -> no suffix.
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetUser {
    user(id: \"1\") {
        name
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        let msg = &diagnostics[0].message;
        assert!(
            msg.ends_with("Include it in your selection set."),
            "got: {msg}"
        );
        assert!(!msg.contains("fragment"), "got: {msg}");
    }

    #[test]
    fn test_fragment_routing_suffix_inline_fragments_not_listed() {
        // Inline fragments don't have a name to list. graphql-eslint's
        // `checkedFragmentSpreads` set is only populated by named fragment
        // spreads, never by inline fragments. So an inline fragment alone
        // must not produce a fragment-routing suffix.
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
query GetUser {
    user(id: \"1\") {
        ... on User {
            name
        }
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // Inline fragment doesn't have `id`, so the `user` selection set is
        // still missing it. We expect a diagnostic with NO fragment suffix.
        let user_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("`user.id`"))
            .collect();
        assert_eq!(user_diags.len(), 1, "got: {diagnostics:#?}");
        let msg = &user_diags[0].message;
        assert!(
            msg.ends_with("Include it in your selection set."),
            "got: {msg}"
        );
    }

    #[test]
    fn test_fragment_routing_suffix_lists_transitively_walked_fragments() {
        // graphql-eslint's `checkedFragmentSpreads` is populated by the
        // recursive `hasIdField` walk: every named fragment spread it visits
        // (including transitively through nested spreads) gets added.
        let db = RootDatabase::default();
        let rule = RequireSelectionsRuleImpl;

        let source = "
fragment Inner on User {
    name
}

fragment Outer on User {
    ...Inner
}

query GetUser {
    user(id: \"1\") {
        ...Outer
    }
}
";
        let (file_id, content, metadata, project_files) =
            create_test_project(&db, TEST_SCHEMA, source);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        let user_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.message.contains("`user.id`"))
            .collect();
        assert_eq!(user_diags.len(), 1, "got: {diagnostics:#?}");
        let msg = &user_diags[0].message;
        // Both `Outer` and the transitively-walked `Inner` appear, in
        // visit order. Plural "fragments".
        assert!(
            msg.ends_with(
                "Include it in your selection set or add to used fragments `Outer` or `Inner`."
            ),
            "got: {msg}"
        );
    }
}
