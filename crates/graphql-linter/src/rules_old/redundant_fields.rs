use crate::context::StandaloneDocumentContext;
use apollo_parser::cst::{self, CstNode};
use graphql_project::{Diagnostic, Position, Range};
use std::collections::{HashMap, HashSet};

use super::StandaloneDocumentRule;

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
pub struct RedundantFieldsRule;

impl StandaloneDocumentRule for RedundantFieldsRule {
    fn name(&self) -> &'static str {
        "redundant_fields"
    }

    fn description(&self) -> &'static str {
        "Detects fields that are redundant because they are already included in a sibling fragment spread"
    }

    fn check(&self, ctx: &StandaloneDocumentContext) -> Vec<Diagnostic> {
        let document = ctx.document;
        let mut diagnostics = Vec::new();

        let doc_cst = ctx.parsed.document();

        // Collect fragment definitions - first from the document, then from the global index
        let mut fragments = FragmentRegistry::new();

        // Add fragments defined in this document
        for definition in doc_cst.definitions() {
            if let cst::Definition::FragmentDefinition(fragment) = definition {
                if let Some(name) = fragment.fragment_name().and_then(|n| n.name()) {
                    let fragment_name = name.text().to_string();
                    fragments.register(fragment_name, fragment.clone());
                }
            }
        }

        // Add fragments from the global index (all other files in the project)
        if let Some(doc_index) = ctx.fragments {
            // Load fragments from all parsed ASTs
            for ast in doc_index.parsed_asts.values() {
                for definition in ast.document().definitions() {
                    if let apollo_parser::cst::Definition::FragmentDefinition(fragment) = definition
                    {
                        if let Some(name) = fragment.fragment_name().and_then(|n| n.name()) {
                            let fragment_name = name.text().to_string();
                            // Don't overwrite local fragments
                            if fragments.get(&fragment_name).is_none() {
                                fragments.register(fragment_name, fragment.clone());
                            }
                        }
                    }
                }
            }

            // Also load from extracted blocks (TypeScript/JavaScript files)
            for blocks in doc_index.extracted_blocks.values() {
                for block in blocks {
                    for definition in block.parsed.document().definitions() {
                        if let apollo_parser::cst::Definition::FragmentDefinition(fragment) =
                            definition
                        {
                            if let Some(name) = fragment.fragment_name().and_then(|n| n.name()) {
                                let fragment_name = name.text().to_string();
                                if fragments.get(&fragment_name).is_none() {
                                    fragments.register(fragment_name, fragment.clone());
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
                            document,
                        );
                    }
                }
                cst::Definition::FragmentDefinition(fragment) => {
                    if let Some(selection_set) = fragment.selection_set() {
                        check_selection_set_for_redundancy(
                            &selection_set,
                            &fragments,
                            &mut diagnostics,
                            document,
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
    /// The actual field name being queried
    field_name: String,
    /// The alias for the field, if any (None means no alias)
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
    diagnostics: &mut Vec<Diagnostic>,
    document: &str,
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

    // Track fields we've seen directly in this selection set to detect duplicates
    let mut seen_fields: HashMap<FieldKey, &cst::Field> = HashMap::new();

    // Now check each field to see if it's redundant
    for selection in &selections {
        if let cst::Selection::Field(field) = selection {
            if let Some(field_key) = FieldKey::from_field(field) {
                // Check if this field is a duplicate of a field we've already seen
                if let Some(_first_field) = seen_fields.get(&field_key) {
                    // This is a duplicate field in the same selection set
                    let field_name_node = field.name().unwrap();
                    let syntax_node = field_name_node.syntax();
                    let offset: usize = syntax_node.text_range().start().into();
                    let line_col = offset_to_line_col(document, offset);

                    let range = Range {
                        start: Position {
                            line: line_col.0,
                            character: line_col.1,
                        },
                        end: Position {
                            line: line_col.0,
                            character: line_col.1 + field_name_node.text().len(),
                        },
                    };

                    let field_desc = if let Some(alias) = &field_key.alias {
                        format!("'{}: {}'", alias, field_key.field_name)
                    } else {
                        format!("'{}'", field_key.field_name)
                    };

                    let message = format!(
                        "Field {field_desc} is redundant - already selected in this selection set"
                    );

                    diagnostics.push(
                        Diagnostic::warning(range, message)
                            .with_code("redundant_field")
                            .with_source("graphql-linter"),
                    );
                } else if fields_from_fragments.contains(&field_key) {
                    // This field is redundant because it's already in a fragment spread
                    let field_name = field.name().unwrap();
                    let syntax_node = field_name.syntax();
                    let offset: usize = syntax_node.text_range().start().into();
                    let line_col = offset_to_line_col(document, offset);

                    let range = Range {
                        start: Position {
                            line: line_col.0,
                            character: line_col.1,
                        },
                        end: Position {
                            line: line_col.0,
                            character: line_col.1 + field_name.text().len(),
                        },
                    };

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

                    diagnostics.push(
                        Diagnostic::warning(range, message)
                            .with_code("redundant_field")
                            .with_source("graphql-linter"),
                    );
                } else {
                    // Not redundant - track it for duplicate detection
                    seen_fields.insert(field_key, field);
                }
            }

            // Recursively check nested selection sets
            if let Some(nested_set) = field.selection_set() {
                check_selection_set_for_redundancy(&nested_set, fragments, diagnostics, document);
            }
        } else if let cst::Selection::InlineFragment(inline_fragment) = selection {
            if let Some(nested_set) = inline_fragment.selection_set() {
                check_selection_set_for_redundancy(&nested_set, fragments, diagnostics, document);
            }
        }
    }
}

fn offset_to_line_col(document: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    let mut current_offset = 0;

    for ch in document.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }

        current_offset += ch.len_utf8();
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::StandaloneDocumentContext;
    use graphql_project::Severity;

    #[test]
    fn test_redundant_field_in_operation() {
        let rule = RedundantFieldsRule;

        let document = r"
            fragment UserFields on User {
                id
                name
            }

            query GetUser {
                user {
                    ...UserFields
                    id
                    name
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().any(|d| d.message.contains("'id'")));
        assert!(diagnostics.iter().any(|d| d.message.contains("'name'")));
        assert!(diagnostics.iter().all(|d| d.severity == Severity::Warning));
    }

    #[test]
    fn test_no_redundancy_when_no_fragments() {
        let rule = RedundantFieldsRule;

        let document = r"
            query GetUser {
                user {
                    id
                    name
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_no_redundancy_with_different_fields() {
        let rule = RedundantFieldsRule;

        let document = r"
            fragment UserFields on User {
                id
                name
            }

            query GetUser {
                user {
                    ...UserFields
                    email
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_transitive_fragment_dependencies() {
        let rule = RedundantFieldsRule;

        let document = r"
            fragment BaseFields on User {
                id
            }

            fragment UserFields on User {
                ...BaseFields
                name
            }

            query GetUser {
                user {
                    ...UserFields
                    id
                    name
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().any(|d| d.message.contains("'id'")));
        assert!(diagnostics.iter().any(|d| d.message.contains("'name'")));
    }

    #[test]
    fn test_nested_selection_sets() {
        let rule = RedundantFieldsRule;

        let document = r"
            fragment ProfileFields on Profile {
                bio
            }

            query GetUser {
                user {
                    id
                    profile {
                        ...ProfileFields
                        bio
                    }
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("'bio'"));
    }

    #[test]
    fn test_multiple_fragments() {
        let rule = RedundantFieldsRule;

        let document = r"
            fragment IdField on User {
                id
            }

            fragment NameField on User {
                name
            }

            query GetUser {
                user {
                    ...IdField
                    ...NameField
                    id
                    name
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().any(|d| d.message.contains("'id'")));
        assert!(diagnostics.iter().any(|d| d.message.contains("'name'")));
        assert!(diagnostics.iter().all(|d| d.message.contains("fragments")));
    }

    #[test]
    fn test_circular_fragment_reference() {
        let rule = RedundantFieldsRule;

        let document = r"
            fragment A on User {
                id
                ...B
            }

            fragment B on User {
                name
                ...A
            }

            query GetUser {
                user {
                    ...A
                    id
                    name
                    email
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        // Fragment A contains: id, ...B (which includes name and ...A recursively)
        // Fragment B contains: name, ...A (which includes id and ...B recursively)
        // So both fragments contain both id and name
        // Diagnostics:
        // 1. Fragment A: id is redundant (already in B via ...A)
        // 2. Fragment B: name is redundant (already in A via ...B)
        // 3. Query: id is redundant (already in ...A)
        // 4. Query: name is redundant (already in ...A)
        assert_eq!(
            diagnostics.len(),
            4,
            "Should detect redundancies in both fragments and the query"
        );
        assert!(
            diagnostics
                .iter()
                .filter(|d| d.message.contains("'id'"))
                .count()
                == 2
        );
        assert!(
            diagnostics
                .iter()
                .filter(|d| d.message.contains("'name'"))
                .count()
                == 2
        );
    }

    #[test]
    fn test_aliased_fields_not_redundant() {
        let rule = RedundantFieldsRule;

        let document = r"
            fragment UserFields on User {
                id
                name
            }

            query GetUser {
                user {
                    ...UserFields
                    userId: id
                    userName: name
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(
            diagnostics.len(),
            0,
            "Aliased fields should not be considered redundant"
        );
    }

    #[test]
    fn test_same_alias_is_redundant() {
        let rule = RedundantFieldsRule;

        let document = r"
            fragment UserFields on User {
                userId: id
            }

            query GetUser {
                user {
                    ...UserFields
                    userId: id
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("'userId: id'"));
    }

    #[test]
    fn test_duplicate_fields_in_same_selection_set() {
        let rule = RedundantFieldsRule;

        let document = r"
            mutation PerformAttack($battleId: ID!) {
                performBattleAction(battleId: $battleId) {
                    id
                    id
                }
            }
        ";

        let parsed = apollo_parser::Parser::new(document).parse();
        let diagnostics = rule.check(&StandaloneDocumentContext {
            document,
            file_name: "test.graphql",
            fragments: None,
            parsed: &parsed,
        });

        assert_eq!(
            diagnostics.len(),
            1,
            "Should detect one duplicate 'id' field"
        );
        assert!(
            diagnostics[0].message.contains("'id'"),
            "Message should mention the 'id' field"
        );
        assert!(
            diagnostics[0]
                .message
                .contains("already selected in this selection set"),
            "Message should indicate it's a duplicate in the same selection set"
        );
    }
}
