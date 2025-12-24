use crate::context::ProjectContext;
use graphql_project::{Diagnostic, Position, Range};
use std::collections::{HashMap, HashSet};

use super::ProjectRule;

/// Lint rule that detects fragment definitions that are never used in any operation
///
/// This is a project-wide rule because fragments are effectively global - a fragment
/// defined in one file can be used by operations in other files. Therefore, we need
/// to scan all operations across the entire project to determine if a fragment is truly unused.
pub struct UnusedFragmentsRule;

impl ProjectRule for UnusedFragmentsRule {
    fn name(&self) -> &'static str {
        "unused_fragments"
    }

    fn description(&self) -> &'static str {
        "Detects fragment definitions that are never used in any operation across the project"
    }

    fn check(&self, ctx: &ProjectContext) -> HashMap<String, Vec<Diagnostic>> {
        let document_index = ctx.documents;
        let mut diagnostics_by_file: HashMap<String, Vec<Diagnostic>> = HashMap::new();

        // Step 1: Collect all fragment names that are actually used in operations
        let used_fragments = collect_used_fragment_names(document_index);

        // Step 2: Check all fragment definitions and report unused ones
        for (fragment_name, fragment_infos) in &document_index.fragments {
            if !used_fragments.contains(fragment_name) {
                for frag_info in fragment_infos {
                    let range = Range {
                        start: Position {
                            line: frag_info.line,
                            character: frag_info.column,
                        },
                        end: Position {
                            line: frag_info.line,
                            character: frag_info.column + fragment_name.len(),
                        },
                    };

                    let message = format!(
                        "Fragment '{fragment_name}' is defined but never used in any operation"
                    );

                    let diag = Diagnostic::warning(range, message)
                        .with_code("unused_fragment")
                        .with_source("graphql-linter");

                    diagnostics_by_file
                        .entry(frag_info.file_path.clone())
                        .or_default()
                        .push(diag);
                }
            }
        }

        diagnostics_by_file
    }
}

/// Collect all fragment names that are used in operations across the entire project
///
/// This scans all operations (in both pure GraphQL files and extracted blocks from
/// TypeScript/JavaScript) and collects the names of fragments they reference.
fn collect_used_fragment_names(document_index: &graphql_project::DocumentIndex) -> HashSet<String> {
    use apollo_parser::cst;

    let mut used_fragments = HashSet::new();

    // Scan operations in pure GraphQL files (from parsed_asts)
    for ast in document_index.parsed_asts.values() {
        for definition in ast.document().definitions() {
            if let cst::Definition::OperationDefinition(operation) = definition {
                if let Some(selection_set) = operation.selection_set() {
                    collect_fragment_spreads_from_selection_set(
                        &selection_set,
                        &mut used_fragments,
                    );
                }
            }
        }
    }

    // Also scan operations in extracted blocks (TypeScript/JavaScript files)
    for blocks in document_index.extracted_blocks.values() {
        for block in blocks {
            for definition in block.parsed.document().definitions() {
                if let cst::Definition::OperationDefinition(operation) = definition {
                    if let Some(selection_set) = operation.selection_set() {
                        collect_fragment_spreads_from_selection_set(
                            &selection_set,
                            &mut used_fragments,
                        );
                    }
                }
            }
        }
    }

    used_fragments
}

/// Recursively collect fragment spread names from a selection set
///
/// This traverses the selection set tree, collecting fragment spread names from:
/// - Direct fragment spreads
/// - Nested fields with selection sets
/// - Inline fragments with selection sets
fn collect_fragment_spreads_from_selection_set(
    selection_set: &apollo_parser::cst::SelectionSet,
    fragments: &mut HashSet<String>,
) {
    use apollo_parser::cst;

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::FragmentSpread(fragment_spread) => {
                if let Some(name) = fragment_spread.fragment_name() {
                    if let Some(name_token) = name.name() {
                        fragments.insert(name_token.text().to_string());
                    }
                }
            }
            cst::Selection::Field(field) => {
                if let Some(nested_set) = field.selection_set() {
                    collect_fragment_spreads_from_selection_set(&nested_set, fragments);
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested_set) = inline.selection_set() {
                    collect_fragment_spreads_from_selection_set(&nested_set, fragments);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_project::{DocumentIndex, FragmentInfo};
    use std::sync::Arc;

    fn create_test_schema() -> graphql_project::SchemaIndex {
        graphql_project::SchemaIndex::from_schema(
            r"
            type Query {
                user: User
            }
            type User {
                id: ID!
                name: String!
            }
            ",
        )
    }

    fn create_test_context() -> (DocumentIndex, graphql_project::SchemaIndex) {
        let mut document_index = DocumentIndex::new();

        // Add a fragment
        let frag_info = FragmentInfo {
            name: "UserFields".to_string(),
            type_condition: "User".to_string(),
            file_path: "fragments.graphql".to_string(),
            line: 0,
            column: 9,
        };
        document_index
            .fragments
            .entry("UserFields".to_string())
            .or_default()
            .push(frag_info);

        // Parse and add an operation that uses the fragment
        let query = r"
            query GetUser {
                user {
                    ...UserFields
                }
            }
        ";
        let parsed = apollo_parser::Parser::new(query).parse();
        document_index
            .parsed_asts
            .insert("query.graphql".to_string(), Arc::new(parsed));

        (document_index, create_test_schema())
    }

    #[test]
    fn test_does_not_report_used_fragments() {
        let rule = UnusedFragmentsRule;
        let (document_index, schema_index) = create_test_context();
        let ctx = ProjectContext {
            documents: &document_index,
            schema: &schema_index,
        };
        let diagnostics = rule.check(&ctx);

        // UserFields is used, so no diagnostics
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_reports_unused_fragments() {
        let rule = UnusedFragmentsRule;
        let mut document_index = DocumentIndex::new();
        let schema_index = create_test_schema();

        // Add an unused fragment
        let frag_info = FragmentInfo {
            name: "UnusedFragment".to_string(),
            type_condition: "User".to_string(),
            file_path: "fragments.graphql".to_string(),
            line: 5,
            column: 9,
        };
        document_index
            .fragments
            .entry("UnusedFragment".to_string())
            .or_default()
            .push(frag_info);

        // Add an operation that doesn't use it
        let query = r"
            query GetUser {
                user {
                    id
                    name
                }
            }
        ";
        let parsed = apollo_parser::Parser::new(query).parse();
        document_index
            .parsed_asts
            .insert("query.graphql".to_string(), Arc::new(parsed));

        let ctx = ProjectContext {
            documents: &document_index,
            schema: &schema_index,
        };

        let diagnostics = rule.check(&ctx);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics.contains_key("fragments.graphql"));
        let file_diags = &diagnostics["fragments.graphql"];
        assert_eq!(file_diags.len(), 1);
        assert!(file_diags[0].message.contains("UnusedFragment"));
        assert!(file_diags[0].message.contains("never used"));
    }
}
