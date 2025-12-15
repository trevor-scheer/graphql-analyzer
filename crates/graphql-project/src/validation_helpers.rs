//! Shared validation and fragment resolution helpers
//!
//! This module contains pure helper functions used by both `StaticGraphQLProject`
//! and `DynamicGraphQLProject` for common GraphQL validation tasks.

use std::collections::HashSet;

/// Check if a GraphQL source contains only fragment definitions (no operations)
///
/// Uses the CST parser to accurately determine document structure.
pub fn is_fragment_only(source: &str) -> bool {
    use apollo_parser::cst;
    use apollo_parser::Parser;

    let parsed = Parser::new(source).parse();
    let mut has_fragment = false;
    let mut has_operation = false;

    for def in parsed.document().definitions() {
        match def {
            cst::Definition::FragmentDefinition(_) => has_fragment = true,
            cst::Definition::OperationDefinition(_) => has_operation = true,
            _ => {}
        }
    }

    has_fragment && !has_operation
}

/// Simple heuristic check for fragment-only content
///
/// This is faster but less accurate than `is_fragment_only()`.
/// Use when performance matters and false positives are acceptable.
pub fn is_fragment_only_simple(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.starts_with("fragment")
        && !trimmed.contains("query")
        && !trimmed.contains("mutation")
        && !trimmed.contains("subscription")
}

/// Collect all fragment definition names in a GraphQL source
///
/// Returns a set of fragment names defined in the source.
/// This does not include fragments that are merely referenced.
pub fn collect_fragment_definitions(source: &str) -> HashSet<String> {
    use apollo_parser::cst;
    use apollo_parser::Parser;

    let parsed = Parser::new(source).parse();
    let mut fragment_names = HashSet::new();

    for def in parsed.document().definitions() {
        if let cst::Definition::FragmentDefinition(frag) = def {
            if let Some(name) = frag.fragment_name() {
                if let Some(name_token) = name.name() {
                    fragment_names.insert(name_token.text().to_string());
                }
            }
        }
    }

    fragment_names
}

/// Collect fragment names directly referenced in a GraphQL source
///
/// This finds all fragment spreads in operations and fragments, but does not
/// recursively follow fragment dependencies. Use this for immediate references only.
///
/// Returns a vector (may contain duplicates) of fragment names found.
pub fn collect_referenced_fragments(source: &str) -> Vec<String> {
    use apollo_parser::cst;
    use apollo_parser::Parser;

    let parsed = Parser::new(source).parse();
    let mut fragments = Vec::new();

    for def in parsed.document().definitions() {
        match def {
            cst::Definition::OperationDefinition(op) => {
                if let Some(selection_set) = op.selection_set() {
                    collect_fragment_spreads(&selection_set, &mut fragments);
                }
            }
            cst::Definition::FragmentDefinition(frag) => {
                if let Some(selection_set) = frag.selection_set() {
                    collect_fragment_spreads(&selection_set, &mut fragments);
                }
            }
            _ => {}
        }
    }

    fragments
}

/// Recursively collect fragment spread names from a selection set
///
/// This walks through fields, inline fragments, and fragment spreads,
/// accumulating all fragment spread names encountered.
pub fn collect_fragment_spreads(
    selection_set: &apollo_parser::cst::SelectionSet,
    fragments: &mut Vec<String>,
) {
    use apollo_parser::cst;

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::FragmentSpread(fragment_spread) => {
                if let Some(name) = fragment_spread.fragment_name() {
                    if let Some(name_token) = name.name() {
                        fragments.push(name_token.text().to_string());
                    }
                }
            }
            cst::Selection::Field(field) => {
                if let Some(nested_set) = field.selection_set() {
                    collect_fragment_spreads(&nested_set, fragments);
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested_set) = inline.selection_set() {
                    collect_fragment_spreads(&nested_set, fragments);
                }
            }
        }
    }
}

/// Collect fragment spreads from a selection set into a `HashSet`
///
/// Similar to `collect_fragment_spreads` but uses a `HashSet` for deduplication.
pub fn collect_fragment_spreads_from_selection_set(
    selection_set: &apollo_parser::cst::SelectionSet,
    used_fragments: &mut HashSet<String>,
) {
    use apollo_parser::cst;

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(nested_selection_set) = field.selection_set() {
                    collect_fragment_spreads_from_selection_set(
                        &nested_selection_set,
                        used_fragments,
                    );
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                if let Some(fragment_name) = spread.fragment_name() {
                    if let Some(name) = fragment_name.name() {
                        used_fragments.insert(name.text().to_string());
                    }
                }
            }
            cst::Selection::InlineFragment(inline_fragment) => {
                if let Some(nested_selection_set) = inline_fragment.selection_set() {
                    collect_fragment_spreads_from_selection_set(
                        &nested_selection_set,
                        used_fragments,
                    );
                }
            }
        }
    }
}

/// Extract a specific fragment definition from GraphQL source by name
///
/// Parses the content and returns only the text of the named fragment definition.
/// Returns None if the fragment is not found.
pub fn extract_fragment_from_content(content: &str, fragment_name: &str) -> Option<String> {
    use apollo_parser::cst;
    use apollo_parser::cst::CstNode;
    use apollo_parser::Parser;

    let parsed = Parser::new(content).parse();

    for def in parsed.document().definitions() {
        if let cst::Definition::FragmentDefinition(frag) = def {
            if let Some(name) = frag.fragment_name() {
                if let Some(name_token) = name.name() {
                    if name_token.text() == fragment_name {
                        return Some(frag.syntax().text().to_string());
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_fragment_only() {
        let fragment_only = "fragment Foo on User { id name }";
        assert!(is_fragment_only(fragment_only));

        let with_query = "query Q { user { id } } fragment Foo on User { id }";
        assert!(!is_fragment_only(with_query));

        let query_only = "query Q { user { id } }";
        assert!(!is_fragment_only(query_only));
    }

    #[test]
    fn test_is_fragment_only_simple() {
        assert!(is_fragment_only_simple("fragment Foo on User { id }"));
        assert!(!is_fragment_only_simple("query Q { user { id } }"));
        assert!(!is_fragment_only_simple(
            "fragment Foo on User { id } query Q { user { id } }"
        ));
    }

    #[test]
    fn test_collect_fragment_definitions() {
        let source = "fragment A on User { id } fragment B on Post { title }";
        let defs = collect_fragment_definitions(source);
        assert_eq!(defs.len(), 2);
        assert!(defs.contains("A"));
        assert!(defs.contains("B"));
    }

    #[test]
    fn test_collect_referenced_fragments() {
        let source = "query Q { user { ...UserFields posts { ...PostFields } } }";
        let refs = collect_referenced_fragments(source);
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&"UserFields".to_string()));
        assert!(refs.contains(&"PostFields".to_string()));
    }

    #[test]
    fn test_extract_fragment_from_content() {
        let source = "fragment A on User { id } fragment B on Post { title }";
        let extracted = extract_fragment_from_content(source, "A");
        assert!(extracted.is_some());
        assert!(extracted.unwrap().contains("fragment A on User"));

        let not_found = extract_fragment_from_content(source, "C");
        assert!(not_found.is_none());
    }
}
