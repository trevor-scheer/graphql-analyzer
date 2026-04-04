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
                    let op_loc = if let Some(name) = op.name() {
                        DiagnosticLocation {
                            start: name.syntax().text_range().start().into(),
                            end: name.syntax().text_range().end().into(),
                        }
                    } else if let Some(op_type) = op.operation_type() {
                        DiagnosticLocation {
                            start: op_type.syntax().text_range().start().into(),
                            end: op_type.syntax().text_range().end().into(),
                        }
                    } else {
                        let start: usize = selection_set.syntax().text_range().start().into();
                        DiagnosticLocation {
                            start,
                            end: start + 1,
                        }
                    };
                    let mut visited_fragments = HashSet::new();
                    check_selection_set(
                        &selection_set,
                        root_type_name,
                        op_loc,
                        check_context,
                        &mut visited_fragments,
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
                    let frag_loc = frag.fragment_name().and_then(|fn_| fn_.name()).map_or_else(
                        || {
                            let start: usize = selection_set.syntax().text_range().start().into();
                            DiagnosticLocation {
                                start,
                                end: start + 1,
                            }
                        },
                        |name| DiagnosticLocation {
                            start: name.syntax().text_range().start().into(),
                            end: name.syntax().text_range().end().into(),
                        },
                    );
                    let mut visited_fragments = HashSet::new();
                    check_selection_set(
                        &selection_set,
                        type_name,
                        frag_loc,
                        check_context,
                        &mut visited_fragments,
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

/// Location for diagnostic placement (start and end offsets)
#[derive(Clone, Copy)]
struct DiagnosticLocation {
    start: usize,
    end: usize,
}

#[allow(clippy::only_used_in_recursion)]
fn check_selection_set(
    selection_set: &cst::SelectionSet,
    parent_type_name: &str,
    parent_location: DiagnosticLocation,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
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

    // Always iterate through selections to recurse into nested selection sets
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    let field_name_str = field_name.text();

                    if required_fields.contains(&field_name_str.to_string()) {
                        found_fields.insert(field_name_str.to_string());
                    }

                    // Always recurse into nested selection sets
                    if let Some(nested_selection_set) = field.selection_set() {
                        if let Some(field_type) =
                            get_field_type(parent_type_name, &field_name_str, context.schema_types)
                        {
                            let field_loc = DiagnosticLocation {
                                start: field_name.syntax().text_range().start().into(),
                                end: field_name.syntax().text_range().end().into(),
                            };
                            check_selection_set(
                                &nested_selection_set,
                                &field_type,
                                field_loc,
                                context,
                                visited_fragments,
                                diagnostics,
                                doc,
                            );
                        }
                    }
                }
            }
            cst::Selection::FragmentSpread(fragment_spread) => {
                if !required_fields.is_empty() {
                    if let Some(fragment_name) = fragment_spread.fragment_name() {
                        if let Some(name) = fragment_name.name() {
                            let name_str = name.text().to_string();
                            for required_field in &required_fields {
                                let mut visited_clone = visited_fragments.clone();
                                if fragment_contains_field(
                                    &name_str,
                                    parent_type_name,
                                    required_field,
                                    context,
                                    &mut visited_clone,
                                ) {
                                    found_fields.insert(required_field.clone());
                                }
                            }
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

                    // Check for required fields in inline fragment's selections
                    for nested_selection in nested_selection_set.selections() {
                        match nested_selection {
                            cst::Selection::Field(nested_field) => {
                                if let Some(field_name) = nested_field.name() {
                                    let field_name_str = field_name.text();
                                    if required_fields.contains(&field_name_str.to_string()) {
                                        found_fields.insert(field_name_str.to_string());
                                    }

                                    // Recurse into nested object selections
                                    if let Some(field_selection_set) = nested_field.selection_set()
                                    {
                                        if let Some(field_type) = get_field_type(
                                            &inline_type,
                                            &field_name.text(),
                                            context.schema_types,
                                        ) {
                                            let field_loc = DiagnosticLocation {
                                                start: field_name
                                                    .syntax()
                                                    .text_range()
                                                    .start()
                                                    .into(),
                                                end: field_name.syntax().text_range().end().into(),
                                            };
                                            check_selection_set(
                                                &field_selection_set,
                                                &field_type,
                                                field_loc,
                                                context,
                                                visited_fragments,
                                                diagnostics,
                                                doc,
                                            );
                                        }
                                    }
                                }
                            }
                            cst::Selection::FragmentSpread(fragment_spread) => {
                                if !required_fields.is_empty() {
                                    if let Some(fragment_name) = fragment_spread.fragment_name() {
                                        if let Some(name) = fragment_name.name() {
                                            let name_str = name.text().to_string();
                                            for required_field in &required_fields {
                                                let mut visited_clone = visited_fragments.clone();
                                                if fragment_contains_field(
                                                    &name_str,
                                                    parent_type_name,
                                                    required_field,
                                                    context,
                                                    &mut visited_clone,
                                                ) {
                                                    found_fields.insert(required_field.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            cst::Selection::InlineFragment(_) => {
                                // Nested inline fragments handled by recursion
                            }
                        }
                    }
                }
            }
        }
    }

    // Emit diagnostics for each missing required field
    for required_field in &required_fields {
        if !found_fields.contains(required_field) {
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

            let fix = CodeFix::new(
                format!("Add '{required_field}' field to {parent_type_name}"),
                vec![TextEdit::insert(
                    insert_pos,
                    format!("{required_field}\n{indent}"),
                )],
            );

            diagnostics.push(
                LintDiagnostic::error(
                    doc.span(parent_location.start, parent_location.end),
                    format!(
                        "Selection set on type '{parent_type_name}' is missing required field '{required_field}'"
                    ),
                    "requireSelections",
                )
                .with_fix(fix),
            );
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

/// Check if a fragment (or its nested fragments) contains the specified field
fn fragment_contains_field(
    fragment_name: &str,
    parent_type_name: &str,
    target_field: &str,
    context: &CheckContext,
    visited_fragments: &mut HashSet<String>,
) -> bool {
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
) -> bool {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name) = field.name() {
                    if field_name.text() == target_field {
                        return true;
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
        let project_files =
            ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map);

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
        let project_files =
            ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map);

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
        assert!(diagnostics[0].message.contains("User"));
        assert!(diagnostics[0].message.contains("'id'"));
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

        // Missing both `id` and `__typename`
        assert_eq!(diagnostics.len(), 2);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("'id'")));
        assert!(messages.iter().any(|m| m.contains("'__typename'")));
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
        assert!(diagnostics[0].message.contains("Post"));
        assert!(diagnostics[0].message.contains("'id'"));
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
        assert!(diagnostics[0].message.contains("User"));
        assert!(diagnostics[0].message.contains("'id'"));
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
        assert!(diagnostics.iter().any(|d| d.message.contains("'id'")));
    }
}
