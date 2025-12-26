use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_db::{FileContent, FileId, FileMetadata};
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
        let project_files = db
            .project_files()
            .expect("project files must be set for linting");
        let all_fragments = graphql_hir::all_fragments_with_project(db, project_files);

        // Add cross-file fragments to the registry
        for (fragment_name, fragment_info) in all_fragments.iter() {
            // Skip if we already have this fragment from the current document
            if fragments.get(fragment_name.as_ref()).is_some() {
                continue;
            }

            // Get the file content and metadata for this fragment
            let fragment_file_id = fragment_info.file_id;

            // Get the file from document_files
            let document_files = db.document_files();
            if let Some((_, file_content, file_metadata)) = document_files
                .iter()
                .find(|(fid, _, _)| *fid == fragment_file_id)
            {
                // Parse the file (cached by Salsa)
                let fragment_parse = graphql_syntax::parse(db, *file_content, *file_metadata);
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

        // Now check all selection sets for redundant fields
        for definition in doc_cst.definitions() {
            match definition {
                cst::Definition::OperationDefinition(operation) => {
                    if let Some(selection_set) = operation.selection_set() {
                        check_selection_set_for_redundancy(
                            &selection_set,
                            &fragments,
                            &mut diagnostics,
                        );
                    }
                }
                cst::Definition::FragmentDefinition(fragment) => {
                    if let Some(selection_set) = fragment.selection_set() {
                        check_selection_set_for_redundancy(
                            &selection_set,
                            &fragments,
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

    // Now check each field to see if it's redundant
    for selection in &selections {
        if let cst::Selection::Field(field) = selection {
            if let Some(field_key) = FieldKey::from_field(field) {
                if fields_from_fragments.contains(&field_key) {
                    let field_name = field.name().unwrap();
                    let syntax_node = field_name.syntax();
                    let start_offset: usize = syntax_node.text_range().start().into();
                    let end_offset: usize = syntax_node.text_range().end().into();

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

                    diagnostics.push(LintDiagnostic::warning(
                        start_offset,
                        end_offset,
                        message,
                        "redundant_fields",
                    ));
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
