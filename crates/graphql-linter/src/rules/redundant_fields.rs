use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use std::collections::{HashMap, HashSet};

/// Lint rule that detects fields that are redundant because they are already
/// included in a sibling fragment spread within the same selection set.
///
/// This rule only considers fields redundant if they have the same alias
/// (or no alias). Aliased fields are treated as distinct from non-aliased
/// or differently-aliased versions of the same field.
///
/// Example:
/// ```graphql
/// fragment UserFields on User {
///   id
///   name
/// }
///
/// query GetUser {
///   user {
///     ...UserFields
///     id    # Redundant - already in UserFields
///     name  # Redundant - already in UserFields
///     userId: id  # NOT redundant - different alias
///   }
/// }
/// ```
pub struct RedundantFieldsRuleImpl;

impl LintRule for RedundantFieldsRuleImpl {
    fn name(&self) -> &'static str {
        "redundant_fields"
    }

    fn description(&self) -> &'static str {
        "Detects fields that are redundant because they are already included in a sibling fragment spread"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for RedundantFieldsRuleImpl {
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

        let doc_cst = parse.tree.document();

        // Collect fragment definitions from the current document
        let mut fragments = FragmentRegistry::new();
        for definition in doc_cst.definitions() {
            if let cst::Definition::FragmentDefinition(fragment) = definition {
                if let Some(name) = fragment.fragment_name().and_then(|n| n.name()) {
                    let fragment_name = name.text().to_string();
                    fragments.register(fragment_name, fragment.clone());
                }
            }
        }

        // Get all fragments from the project (for cross-file resolution)
        let all_fragments = graphql_hir::all_fragments(db, project_files);

        // Add cross-file fragments to the registry
        for (fragment_name, fragment_info) in all_fragments {
            // Skip if we already have this fragment from the current document
            if fragments.get(fragment_name.as_ref()).is_some() {
                continue;
            }

            // Get the file content and metadata for this fragment
            let fragment_file_id = fragment_info.file_id;

            // Use per-file lookup for granular caching
            if let Some((file_content, file_metadata)) =
                graphql_db::file_lookup(db, project_files, fragment_file_id)
            {
                // Parse the file (cached by Salsa)
                let fragment_parse = graphql_syntax::parse(db, file_content, file_metadata);
                if fragment_parse.errors.is_empty() {
                    let fragment_doc_cst = fragment_parse.tree.document();

                    // Find the fragment definition
                    for definition in fragment_doc_cst.definitions() {
                        if let cst::Definition::FragmentDefinition(fragment) = definition {
                            if let Some(name) = fragment.fragment_name().and_then(|n| n.name()) {
                                if name.text() == fragment_name.as_ref() {
                                    fragments.register(fragment_name.to_string(), fragment.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check main document for redundant fields (for .graphql files only)
        // For TS/JS files, parse.tree is the first block and we check all blocks below
        let file_kind = metadata.kind(db);
        if file_kind == graphql_db::FileKind::ExecutableGraphQL
            || file_kind == graphql_db::FileKind::Schema
        {
            check_document_for_redundancy(&doc_cst, &fragments, &mut diagnostics);
        }

        // Check selection sets in extracted blocks (TypeScript/JavaScript)
        for block in &parse.blocks {
            let block_doc = block.tree.document();
            let mut block_diagnostics = Vec::new();
            check_document_for_redundancy(&block_doc, &fragments, &mut block_diagnostics);
            // Add block context to each diagnostic for proper position calculation
            for diag in block_diagnostics {
                diagnostics.push(diag.with_block_context(block.line, block.source.clone()));
            }
        }

        diagnostics
    }
}

/// Check a GraphQL document for redundant fields
fn check_document_for_redundancy(
    doc_cst: &cst::Document,
    fragments: &FragmentRegistry,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for definition in doc_cst.definitions() {
        match definition {
            cst::Definition::OperationDefinition(operation) => {
                if let Some(selection_set) = operation.selection_set() {
                    check_selection_set_for_redundancy(&selection_set, fragments, diagnostics);
                }
            }
            cst::Definition::FragmentDefinition(fragment) => {
                if let Some(selection_set) = fragment.selection_set() {
                    check_selection_set_for_redundancy(&selection_set, fragments, diagnostics);
                }
            }
            _ => {}
        }
    }
}

/// A key that uniquely identifies a field selection by its field name and alias
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FieldKey {
    field_name: String,
    alias: Option<String>,
}

impl FieldKey {
    fn from_field(field: &cst::Field) -> Option<Self> {
        let field_name = field.name()?.text().to_string();
        let alias = field
            .alias()
            .and_then(|a| a.name())
            .map(|n| n.text().to_string());
        Some(Self { field_name, alias })
    }
}

/// Registry to store and look up fragment definitions
struct FragmentRegistry {
    fragments: HashMap<String, cst::FragmentDefinition>,
}

impl FragmentRegistry {
    fn new() -> Self {
        Self {
            fragments: HashMap::new(),
        }
    }

    fn register(&mut self, name: String, fragment: cst::FragmentDefinition) {
        self.fragments.insert(name, fragment);
    }

    fn get(&self, name: &str) -> Option<&cst::FragmentDefinition> {
        self.fragments.get(name)
    }

    /// Recursively collect all field keys from a fragment and its transitive dependencies
    fn collect_fields_from_fragment(
        &self,
        fragment_name: &str,
        visited: &mut HashSet<String>,
    ) -> HashSet<FieldKey> {
        let mut fields = HashSet::new();

        if !visited.insert(fragment_name.to_string()) {
            return fields;
        }

        if let Some(fragment) = self.get(fragment_name) {
            if let Some(selection_set) = fragment.selection_set() {
                self.collect_fields_from_selection_set(&selection_set, &mut fields, visited);
            }
        }

        fields
    }

    fn collect_fields_from_selection_set(
        &self,
        selection_set: &cst::SelectionSet,
        fields: &mut HashSet<FieldKey>,
        visited: &mut HashSet<String>,
    ) {
        for selection in selection_set.selections() {
            match selection {
                cst::Selection::Field(field) => {
                    if let Some(field_key) = FieldKey::from_field(&field) {
                        fields.insert(field_key);
                    }
                }
                cst::Selection::FragmentSpread(fragment_spread) => {
                    if let Some(fragment_name) = fragment_spread.fragment_name() {
                        if let Some(name_token) = fragment_name.name() {
                            let name = name_token.text();
                            let fragment_fields = self.collect_fields_from_fragment(&name, visited);
                            fields.extend(fragment_fields);
                        }
                    }
                }
                cst::Selection::InlineFragment(inline_fragment) => {
                    if let Some(nested_set) = inline_fragment.selection_set() {
                        self.collect_fields_from_selection_set(&nested_set, fields, visited);
                    }
                }
            }
        }
    }
}

/// Check a selection set for redundant fields
#[allow(clippy::too_many_lines)]
fn check_selection_set_for_redundancy(
    selection_set: &cst::SelectionSet,
    fragments: &FragmentRegistry,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let selections: Vec<_> = selection_set.selections().collect();

    // Collect all fields provided by fragment spreads in this selection set
    let mut fields_from_fragments = HashSet::new();
    let mut fragment_spreads = Vec::new();

    for selection in &selections {
        if let cst::Selection::FragmentSpread(fragment_spread) = selection {
            if let Some(fragment_name) = fragment_spread.fragment_name() {
                if let Some(name_token) = fragment_name.name() {
                    let name = name_token.text();
                    let mut visited = HashSet::new();
                    let fragment_fields =
                        fragments.collect_fields_from_fragment(&name, &mut visited);
                    fields_from_fragments.extend(fragment_fields);
                    fragment_spreads.push(name.to_string());
                }
            }
        }
    }

    // Track direct field selections and their counts
    let mut direct_field_counts: HashMap<FieldKey, Vec<&cst::Field>> = HashMap::new();

    for selection in &selections {
        if let cst::Selection::Field(field) = selection {
            if let Some(field_key) = FieldKey::from_field(field) {
                direct_field_counts
                    .entry(field_key)
                    .or_default()
                    .push(field);
            }
        }
    }

    // Report duplicate direct field selections
    for (field_key, fields) in &direct_field_counts {
        if fields.len() > 1 {
            for field in fields.iter().skip(1) {
                if let Some(field_name) = field.name() {
                    let name_syntax = field_name.syntax();
                    let start_offset: usize = name_syntax.text_range().start().into();
                    let end_offset: usize = name_syntax.text_range().end().into();

                    // Get full field range for the fix
                    let field_syntax = field.syntax();
                    let field_start: usize = field_syntax.text_range().start().into();
                    let field_end: usize = field_syntax.text_range().end().into();

                    let field_desc = field_key.alias.as_ref().map_or_else(
                        || format!("'{}'", field_key.field_name),
                        |alias| format!("'{}: {}'", alias, field_key.field_name),
                    );
                    let message = format!(
                        "Field {field_desc} is selected multiple times in the same selection set"
                    );

                    let fix = CodeFix::new(
                        format!("Remove duplicate field {field_desc}"),
                        vec![TextEdit::delete(field_start, field_end)],
                    );

                    diagnostics.push(
                        LintDiagnostic::warning(
                            start_offset,
                            end_offset,
                            message,
                            "redundant_fields",
                        )
                        .with_fix(fix),
                    );
                }
            }
        }
    }

    // Now check each field to see if it's redundant via fragment
    for selection in &selections {
        if let cst::Selection::Field(field) = selection {
            if let Some(field_key) = FieldKey::from_field(field) {
                if fields_from_fragments.contains(&field_key) {
                    let field_name = field.name().unwrap();
                    let name_syntax = field_name.syntax();
                    let start_offset: usize = name_syntax.text_range().start().into();
                    let end_offset: usize = name_syntax.text_range().end().into();

                    // Get full field range for the fix
                    let field_syntax = field.syntax();
                    let field_start: usize = field_syntax.text_range().start().into();
                    let field_end: usize = field_syntax.text_range().end().into();

                    let fragment_list = if fragment_spreads.len() == 1 {
                        format!("fragment '{}'", fragment_spreads[0])
                    } else {
                        format!(
                            "fragments {}",
                            fragment_spreads
                                .iter()
                                .map(|f| format!("'{f}'"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };

                    let field_desc = if let Some(alias) = &field_key.alias {
                        format!("'{}: {}'", alias, field_key.field_name)
                    } else {
                        format!("'{}'", field_key.field_name)
                    };

                    let message = format!(
                        "Field {field_desc} is redundant - already included in {fragment_list}"
                    );

                    let fix = CodeFix::new(
                        format!("Remove redundant field {field_desc}"),
                        vec![TextEdit::delete(field_start, field_end)],
                    );

                    diagnostics.push(
                        LintDiagnostic::warning(
                            start_offset,
                            end_offset,
                            message,
                            "redundant_fields",
                        )
                        .with_fix(fix),
                    );
                }
            }

            // Recursively check nested selection sets
            if let Some(nested_set) = field.selection_set() {
                check_selection_set_for_redundancy(&nested_set, fragments, diagnostics);
            }
        } else if let cst::Selection::InlineFragment(inline_fragment) = selection {
            if let Some(nested_set) = inline_fragment.selection_set() {
                check_selection_set_for_redundancy(&nested_set, fragments, diagnostics);
            }
        }
    }
}
