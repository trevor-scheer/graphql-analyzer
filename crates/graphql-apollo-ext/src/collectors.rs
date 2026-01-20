//! Collection utilities for gathering data from GraphQL documents.
//!
//! This module provides pre-built collectors for common patterns like
//! collecting all variables, fragment spreads, or field names from a document.
//!
//! # Example
//!
//! ```
//! use graphql_apollo_ext::{collect_fragment_spreads, collect_variables};
//! use apollo_parser::Parser;
//!
//! let source = "query($id: ID!) { ...UserFields user(id: $id) { name } }";
//! let tree = Parser::new(source).parse();
//!
//! let fragments = collect_fragment_spreads(&tree);
//! assert!(fragments.contains("UserFields"));
//!
//! let variables = collect_variables(&tree);
//! assert!(variables.contains("id"));
//! ```

use crate::{walk_document, CstVisitor, NameExt};
use apollo_parser::cst;
use apollo_parser::SyntaxTree;
use std::collections::HashSet;

use crate::ByteRange;

// =============================================================================
// Fragment Collection
// =============================================================================

/// Collect all fragment spread names from a document.
///
/// Returns a set of fragment names that are referenced via `...FragmentName`.
#[must_use]
pub fn collect_fragment_spreads(tree: &SyntaxTree) -> HashSet<String> {
    struct Collector(HashSet<String>);

    impl CstVisitor for Collector {
        fn visit_fragment_spread(&mut self, spread: &cst::FragmentSpread) {
            if let Some(name) = spread.name_text() {
                self.0.insert(name);
            }
        }
    }

    let mut collector = Collector(HashSet::new());
    walk_document(&mut collector, tree);
    collector.0
}

/// Collect all fragment definition names from a document.
///
/// Returns a set of fragment names that are defined in this document.
#[must_use]
pub fn collect_fragment_definitions(tree: &SyntaxTree) -> HashSet<String> {
    struct Collector(HashSet<String>);

    impl CstVisitor for Collector {
        fn enter_fragment_definition(&mut self, frag: &cst::FragmentDefinition) {
            if let Some(name) = frag.name_text() {
                self.0.insert(name);
            }
        }
    }

    let mut collector = Collector(HashSet::new());
    walk_document(&mut collector, tree);
    collector.0
}

/// A located fragment spread with its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedFragmentSpread {
    pub name: String,
    pub range: ByteRange,
}

/// Collect all fragment spreads with their locations.
#[must_use]
pub fn collect_fragment_spread_locations(tree: &SyntaxTree) -> Vec<LocatedFragmentSpread> {
    struct Collector(Vec<LocatedFragmentSpread>);

    impl CstVisitor for Collector {
        fn visit_fragment_spread(&mut self, spread: &cst::FragmentSpread) {
            if let (Some(name), Some(range)) = (spread.name_text(), spread.name_range()) {
                self.0.push(LocatedFragmentSpread { name, range });
            }
        }
    }

    let mut collector = Collector(Vec::new());
    walk_document(&mut collector, tree);
    collector.0
}

// =============================================================================
// Variable Collection
// =============================================================================

/// Collect all variable names used in a document.
///
/// Returns a set of variable names (without the `$` prefix) that are referenced.
#[must_use]
pub fn collect_variables(tree: &SyntaxTree) -> HashSet<String> {
    struct Collector(HashSet<String>);

    impl CstVisitor for Collector {
        fn visit_variable(&mut self, var: &cst::Variable) {
            if let Some(name) = var.name_text() {
                self.0.insert(name);
            }
        }
    }

    let mut collector = Collector(HashSet::new());
    walk_document(&mut collector, tree);
    collector.0
}

/// Collect all variable definitions from operations.
///
/// Returns a set of variable names that are declared in operation definitions.
#[must_use]
pub fn collect_variable_definitions(tree: &SyntaxTree) -> HashSet<String> {
    struct Collector(HashSet<String>);

    impl CstVisitor for Collector {
        fn visit_variable_definition(&mut self, var_def: &cst::VariableDefinition) {
            if let Some(var) = var_def.variable() {
                if let Some(name) = var.name_text() {
                    self.0.insert(name);
                }
            }
        }
    }

    let mut collector = Collector(HashSet::new());
    walk_document(&mut collector, tree);
    collector.0
}

/// A located variable reference with its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedVariable {
    pub name: String,
    pub range: ByteRange,
}

/// Collect all variable references with their locations.
#[must_use]
pub fn collect_variable_locations(tree: &SyntaxTree) -> Vec<LocatedVariable> {
    struct Collector(Vec<LocatedVariable>);

    impl CstVisitor for Collector {
        fn visit_variable(&mut self, var: &cst::Variable) {
            if let (Some(name), Some(range)) = (var.name_text(), var.name_range()) {
                self.0.push(LocatedVariable { name, range });
            }
        }
    }

    let mut collector = Collector(Vec::new());
    walk_document(&mut collector, tree);
    collector.0
}

// =============================================================================
// Field Collection
// =============================================================================

/// Collect all field names from selection sets.
///
/// Returns a set of all field names selected in the document.
#[must_use]
pub fn collect_field_names(tree: &SyntaxTree) -> HashSet<String> {
    struct Collector(HashSet<String>);

    impl CstVisitor for Collector {
        fn visit_field(&mut self, field: &cst::Field) {
            if let Some(name) = field.name_text() {
                self.0.insert(name);
            }
        }
    }

    let mut collector = Collector(HashSet::new());
    walk_document(&mut collector, tree);
    collector.0
}

/// A located field with its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedField {
    pub name: String,
    pub range: ByteRange,
}

/// Collect all fields with their locations.
#[must_use]
pub fn collect_field_locations(tree: &SyntaxTree) -> Vec<LocatedField> {
    struct Collector(Vec<LocatedField>);

    impl CstVisitor for Collector {
        fn visit_field(&mut self, field: &cst::Field) {
            if let (Some(name), Some(range)) = (field.name_text(), field.name_range()) {
                self.0.push(LocatedField { name, range });
            }
        }
    }

    let mut collector = Collector(Vec::new());
    walk_document(&mut collector, tree);
    collector.0
}

// =============================================================================
// Directive Collection
// =============================================================================

/// Collect all directive names used in a document.
#[must_use]
pub fn collect_directives(tree: &SyntaxTree) -> HashSet<String> {
    struct Collector(HashSet<String>);

    impl CstVisitor for Collector {
        fn visit_directive(&mut self, dir: &cst::Directive) {
            if let Some(name) = dir.name_text() {
                self.0.insert(name);
            }
        }
    }

    let mut collector = Collector(HashSet::new());
    walk_document(&mut collector, tree);
    collector.0
}

/// A located directive with its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedDirective {
    pub name: String,
    pub range: ByteRange,
}

/// Collect all directives with their locations.
#[must_use]
pub fn collect_directive_locations(tree: &SyntaxTree) -> Vec<LocatedDirective> {
    struct Collector(Vec<LocatedDirective>);

    impl CstVisitor for Collector {
        fn visit_directive(&mut self, dir: &cst::Directive) {
            if let (Some(name), Some(range)) = (dir.name_text(), dir.name_range()) {
                self.0.push(LocatedDirective { name, range });
            }
        }
    }

    let mut collector = Collector(Vec::new());
    walk_document(&mut collector, tree);
    collector.0
}

// =============================================================================
// Type Reference Collection
// =============================================================================

/// Collect all type names referenced in the document.
///
/// This includes types in field definitions, arguments, variables, etc.
#[must_use]
pub fn collect_type_references(tree: &SyntaxTree) -> HashSet<String> {
    use crate::BaseTypeExt;

    struct Collector(HashSet<String>);

    impl CstVisitor for Collector {
        fn visit_type(&mut self, ty: &cst::Type) {
            if let Some(name) = ty.base_type_name() {
                self.0.insert(name);
            }
        }

        fn visit_named_type(&mut self, named: &cst::NamedType) {
            if let Some(name) = named.name_text() {
                self.0.insert(name);
            }
        }
    }

    let mut collector = Collector(HashSet::new());
    walk_document(&mut collector, tree);
    collector.0
}

// =============================================================================
// Selection Set Statistics
// =============================================================================

/// Statistics about a document's selection sets.
#[derive(Debug, Clone, Default)]
pub struct SelectionStats {
    /// Total number of fields
    pub field_count: usize,
    /// Total number of fragment spreads
    pub fragment_spread_count: usize,
    /// Total number of inline fragments
    pub inline_fragment_count: usize,
    /// Maximum nesting depth
    pub max_depth: usize,
}

/// Collect statistics about selection sets in a document.
#[must_use]
pub fn collect_selection_stats(tree: &SyntaxTree) -> SelectionStats {
    struct Collector {
        stats: SelectionStats,
        current_depth: usize,
    }

    impl CstVisitor for Collector {
        fn enter_selection_set(&mut self, _set: &cst::SelectionSet) {
            self.current_depth += 1;
            self.stats.max_depth = self.stats.max_depth.max(self.current_depth);
        }

        fn exit_selection_set(&mut self, _set: &cst::SelectionSet) {
            self.current_depth -= 1;
        }

        fn visit_field(&mut self, _field: &cst::Field) {
            self.stats.field_count += 1;
        }

        fn visit_fragment_spread(&mut self, _spread: &cst::FragmentSpread) {
            self.stats.fragment_spread_count += 1;
        }

        fn enter_inline_fragment(&mut self, _inline: &cst::InlineFragment) {
            self.stats.inline_fragment_count += 1;
        }
    }

    let mut collector = Collector {
        stats: SelectionStats::default(),
        current_depth: 0,
    };
    walk_document(&mut collector, tree);
    collector.stats
}

// =============================================================================
// Find at Offset
// =============================================================================

/// The kind of element found at an offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementAtOffset {
    Field { name: String },
    FragmentSpread { name: String },
    Variable { name: String },
    TypeName { name: String },
    Directive { name: String },
    Argument { name: String },
    OperationName { name: String },
    FragmentDefinition { name: String },
}

/// Find what element exists at a given byte offset.
///
/// Returns the first element whose range contains the offset.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn find_element_at_offset(tree: &SyntaxTree, offset: usize) -> Option<ElementAtOffset> {
    use crate::{RangeExt, TypeConditionExt};

    struct Finder {
        offset: usize,
        found: Option<ElementAtOffset>,
    }

    impl CstVisitor for Finder {
        fn enter_operation(&mut self, op: &cst::OperationDefinition) {
            if self.found.is_some() {
                return;
            }
            if let Some(name) = op.name() {
                if name.contains_offset(self.offset) {
                    self.found = Some(ElementAtOffset::OperationName {
                        name: name.text().to_string(),
                    });
                }
            }
        }

        fn enter_fragment_definition(&mut self, frag: &cst::FragmentDefinition) {
            if self.found.is_some() {
                return;
            }
            if let Some(name) = frag.name_text() {
                if let Some(range) = frag.name_range() {
                    if range.contains(self.offset) {
                        self.found = Some(ElementAtOffset::FragmentDefinition { name });
                    }
                }
            }
            // Check type condition
            if let Some(type_name) = frag.type_condition_name() {
                if let Some(range) = frag.type_condition_range() {
                    if range.contains(self.offset) {
                        self.found = Some(ElementAtOffset::TypeName { name: type_name });
                    }
                }
            }
        }

        fn visit_field(&mut self, field: &cst::Field) {
            if self.found.is_some() {
                return;
            }
            if let Some(name) = field.name() {
                if name.contains_offset(self.offset) {
                    self.found = Some(ElementAtOffset::Field {
                        name: name.text().to_string(),
                    });
                }
            }
        }

        fn visit_fragment_spread(&mut self, spread: &cst::FragmentSpread) {
            if self.found.is_some() {
                return;
            }
            if let Some(name) = spread.name_text() {
                if let Some(range) = spread.name_range() {
                    if range.contains(self.offset) {
                        self.found = Some(ElementAtOffset::FragmentSpread { name });
                    }
                }
            }
        }

        fn enter_inline_fragment(&mut self, inline: &cst::InlineFragment) {
            if self.found.is_some() {
                return;
            }
            if let Some(type_name) = inline.type_condition_name() {
                if let Some(range) = inline.type_condition_range() {
                    if range.contains(self.offset) {
                        self.found = Some(ElementAtOffset::TypeName { name: type_name });
                    }
                }
            }
        }

        fn visit_variable(&mut self, var: &cst::Variable) {
            if self.found.is_some() {
                return;
            }
            if var.contains_offset(self.offset) {
                if let Some(name) = var.name_text() {
                    self.found = Some(ElementAtOffset::Variable { name });
                }
            }
        }

        fn visit_argument(&mut self, arg: &cst::Argument) {
            if self.found.is_some() {
                return;
            }
            if let Some(name) = arg.name() {
                if name.contains_offset(self.offset) {
                    self.found = Some(ElementAtOffset::Argument {
                        name: name.text().to_string(),
                    });
                }
            }
        }

        fn visit_directive(&mut self, dir: &cst::Directive) {
            if self.found.is_some() {
                return;
            }
            if let Some(name) = dir.name() {
                if name.contains_offset(self.offset) {
                    self.found = Some(ElementAtOffset::Directive {
                        name: name.text().to_string(),
                    });
                }
            }
        }

        fn visit_named_type(&mut self, named: &cst::NamedType) {
            if self.found.is_some() {
                return;
            }
            if let Some(name) = named.name() {
                if name.contains_offset(self.offset) {
                    self.found = Some(ElementAtOffset::TypeName {
                        name: name.text().to_string(),
                    });
                }
            }
        }
    }

    let mut finder = Finder {
        offset,
        found: None,
    };
    walk_document(&mut finder, tree);
    finder.found
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollo_parser::Parser;

    #[test]
    fn test_collect_fragment_spreads() {
        let source = "query { ...UserFields user { ...NameFields } }";
        let tree = Parser::new(source).parse();

        let spreads = collect_fragment_spreads(&tree);
        assert!(spreads.contains("UserFields"));
        assert!(spreads.contains("NameFields"));
        assert_eq!(spreads.len(), 2);
    }

    #[test]
    fn test_collect_fragment_definitions() {
        let source = r#"
            fragment UserFields on User { name }
            fragment AdminFields on Admin { role }
        "#;
        let tree = Parser::new(source).parse();

        let defs = collect_fragment_definitions(&tree);
        assert!(defs.contains("UserFields"));
        assert!(defs.contains("AdminFields"));
        assert_eq!(defs.len(), 2);
    }

    #[test]
    fn test_collect_variables() {
        let source = "query($id: ID!, $name: String) { user(id: $id, name: $name) { id } }";
        let tree = Parser::new(source).parse();

        let vars = collect_variables(&tree);
        assert!(vars.contains("id"));
        assert!(vars.contains("name"));
    }

    #[test]
    fn test_collect_variable_definitions() {
        let source = "query($id: ID!, $name: String) { user { id } }";
        let tree = Parser::new(source).parse();

        let var_defs = collect_variable_definitions(&tree);
        assert!(var_defs.contains("id"));
        assert!(var_defs.contains("name"));
        assert_eq!(var_defs.len(), 2);
    }

    #[test]
    fn test_collect_field_names() {
        let source = "query { user { name email posts { title } } }";
        let tree = Parser::new(source).parse();

        let fields = collect_field_names(&tree);
        assert!(fields.contains("user"));
        assert!(fields.contains("name"));
        assert!(fields.contains("email"));
        assert!(fields.contains("posts"));
        assert!(fields.contains("title"));
    }

    #[test]
    fn test_collect_directives() {
        let source = "query @cached { user @skip(if: true) { name @deprecated } }";
        let tree = Parser::new(source).parse();

        let dirs = collect_directives(&tree);
        assert!(dirs.contains("cached"));
        assert!(dirs.contains("skip"));
        assert!(dirs.contains("deprecated"));
    }

    #[test]
    fn test_collect_type_references() {
        let source = "type User { friends: [User!]! posts: [Post] }";
        let tree = Parser::new(source).parse();

        let types = collect_type_references(&tree);
        assert!(types.contains("User"));
        assert!(types.contains("Post"));
    }

    #[test]
    fn test_selection_stats() {
        let source = "query { user { posts { comments { author { name } } } } }";
        let tree = Parser::new(source).parse();

        let stats = collect_selection_stats(&tree);
        assert_eq!(stats.field_count, 5);
        assert_eq!(stats.max_depth, 5);
        assert_eq!(stats.fragment_spread_count, 0);
        assert_eq!(stats.inline_fragment_count, 0);
    }

    #[test]
    fn test_selection_stats_with_fragments() {
        let source = "query { ...UserFields ... on Admin { role } }";
        let tree = Parser::new(source).parse();

        let stats = collect_selection_stats(&tree);
        assert_eq!(stats.fragment_spread_count, 1);
        assert_eq!(stats.inline_fragment_count, 1);
        assert_eq!(stats.field_count, 1); // role
    }

    #[test]
    fn test_find_element_at_offset() {
        let source = "query { user { name } }";
        let tree = Parser::new(source).parse();

        // "user" starts at offset 8
        let elem = find_element_at_offset(&tree, 8);
        assert_eq!(
            elem,
            Some(ElementAtOffset::Field {
                name: "user".to_string()
            })
        );

        // "name" starts at offset 15
        let elem = find_element_at_offset(&tree, 15);
        assert_eq!(
            elem,
            Some(ElementAtOffset::Field {
                name: "name".to_string()
            })
        );
    }

    #[test]
    fn test_find_fragment_spread_at_offset() {
        let source = "query { ...UserFields }";
        let tree = Parser::new(source).parse();

        // "UserFields" starts at offset 11
        let elem = find_element_at_offset(&tree, 11);
        assert_eq!(
            elem,
            Some(ElementAtOffset::FragmentSpread {
                name: "UserFields".to_string()
            })
        );
    }

    #[test]
    fn test_find_variable_at_offset() {
        let source = "query { user(id: $userId) { name } }";
        let tree = Parser::new(source).parse();

        // "$userId" - the variable reference
        let elem = find_element_at_offset(&tree, 18);
        assert_eq!(
            elem,
            Some(ElementAtOffset::Variable {
                name: "userId".to_string()
            })
        );
    }

    #[test]
    fn test_located_fragment_spreads() {
        let source = "query { ...UserFields }";
        let tree = Parser::new(source).parse();

        let spreads = collect_fragment_spread_locations(&tree);
        assert_eq!(spreads.len(), 1);
        assert_eq!(spreads[0].name, "UserFields");
        assert_eq!(
            &source[spreads[0].range.start..spreads[0].range.end],
            "UserFields"
        );
    }
}
