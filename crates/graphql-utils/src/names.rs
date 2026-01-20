//! Name extraction utilities for GraphQL CST nodes.
//!
//! This module provides extension traits that make it easier to extract
//! names and text from CST nodes without long option chains.
//!
//! # Example
//!
//! ```
//! use graphql_utils::NameExt;
//! use apollo_parser::Parser;
//!
//! let source = "fragment UserFields on User { name }";
//! let tree = Parser::new(source).parse();
//! let doc = tree.document();
//!
//! for def in doc.definitions() {
//!     if let apollo_parser::cst::Definition::FragmentDefinition(frag) = def {
//!         // Instead of: frag.fragment_name().and_then(|n| n.name()).map(|n| n.text().to_string())
//!         assert_eq!(frag.name_text(), Some("UserFields".to_string()));
//!     }
//! }
//! ```

use apollo_parser::cst::{self, CstNode};

/// Byte range of a CST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Check if an offset is within this range.
    #[must_use]
    pub const fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Check if an offset is within this range (inclusive end).
    #[must_use]
    pub const fn contains_inclusive(&self, offset: usize) -> bool {
        offset >= self.start && offset <= self.end
    }
}

/// Extension trait for extracting byte ranges from CST nodes.
pub trait RangeExt: CstNode {
    /// Get the byte range of this node.
    fn byte_range(&self) -> ByteRange {
        let range = self.syntax().text_range();
        ByteRange::new(range.start().into(), range.end().into())
    }

    /// Check if the given byte offset is within this node.
    fn contains_offset(&self, offset: usize) -> bool {
        self.byte_range().contains_inclusive(offset)
    }
}

// Implement RangeExt for all CstNode types
impl<T: CstNode> RangeExt for T {}

/// Extension trait for extracting names from CST nodes.
///
/// Provides convenient methods to get name text without long option chains.
pub trait NameExt {
    /// Get the name text as a String, if available.
    fn name_text(&self) -> Option<String>;

    /// Get the name's byte range, if available.
    fn name_range(&self) -> Option<ByteRange>;
}

impl NameExt for cst::OperationDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::FragmentDefinition {
    fn name_text(&self) -> Option<String> {
        self.fragment_name()
            .and_then(|n| n.name())
            .map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.fragment_name()
            .and_then(|n| n.name())
            .map(|n| n.byte_range())
    }
}

impl NameExt for cst::FragmentSpread {
    fn name_text(&self) -> Option<String> {
        self.fragment_name()
            .and_then(|n| n.name())
            .map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.fragment_name()
            .and_then(|n| n.name())
            .map(|n| n.byte_range())
    }
}

impl NameExt for cst::Field {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::Variable {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::Argument {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::Directive {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::ObjectTypeDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::InterfaceTypeDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::UnionTypeDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::EnumTypeDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::ScalarTypeDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::InputObjectTypeDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::FieldDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::InputValueDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::EnumValueDefinition {
    fn name_text(&self) -> Option<String> {
        self.enum_value()
            .and_then(|v| v.name())
            .map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.enum_value()
            .and_then(|v| v.name())
            .map(|n| n.byte_range())
    }
}

impl NameExt for cst::DirectiveDefinition {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::NamedType {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

// Extension types
impl NameExt for cst::ObjectTypeExtension {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::InterfaceTypeExtension {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::UnionTypeExtension {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::EnumTypeExtension {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::ScalarTypeExtension {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

impl NameExt for cst::InputObjectTypeExtension {
    fn name_text(&self) -> Option<String> {
        self.name().map(|n| n.text().to_string())
    }

    fn name_range(&self) -> Option<ByteRange> {
        self.name().map(|n| n.byte_range())
    }
}

/// Extension trait for extracting type condition names.
pub trait TypeConditionExt {
    /// Get the type condition name as a String.
    fn type_condition_name(&self) -> Option<String>;

    /// Get the type condition's byte range.
    fn type_condition_range(&self) -> Option<ByteRange>;
}

impl TypeConditionExt for cst::FragmentDefinition {
    fn type_condition_name(&self) -> Option<String> {
        self.type_condition()
            .and_then(|tc| tc.named_type())
            .and_then(|nt| nt.name())
            .map(|n| n.text().to_string())
    }

    fn type_condition_range(&self) -> Option<ByteRange> {
        self.type_condition()
            .and_then(|tc| tc.named_type())
            .and_then(|nt| nt.name())
            .map(|n| n.byte_range())
    }
}

impl TypeConditionExt for cst::InlineFragment {
    fn type_condition_name(&self) -> Option<String> {
        self.type_condition()
            .and_then(|tc| tc.named_type())
            .and_then(|nt| nt.name())
            .map(|n| n.text().to_string())
    }

    fn type_condition_range(&self) -> Option<ByteRange> {
        self.type_condition()
            .and_then(|tc| tc.named_type())
            .and_then(|nt| nt.name())
            .map(|n| n.byte_range())
    }
}

/// Extension trait for extracting the base type name from a Type reference.
///
/// This unwraps `NonNull` and `List` wrappers to get the underlying named type.
pub trait BaseTypeExt {
    /// Get the base type name (unwrapping `NonNull` and `List`).
    fn base_type_name(&self) -> Option<String>;

    /// Get the base type's byte range.
    fn base_type_range(&self) -> Option<ByteRange>;
}

impl BaseTypeExt for cst::Type {
    fn base_type_name(&self) -> Option<String> {
        match self {
            cst::Type::NamedType(named) => named.name().map(|n| n.text().to_string()),
            cst::Type::ListType(list) => list.ty().and_then(|t| t.base_type_name()),
            cst::Type::NonNullType(non_null) => {
                if let Some(named) = non_null.named_type() {
                    named.name().map(|n| n.text().to_string())
                } else if let Some(list) = non_null.list_type() {
                    list.ty().and_then(|t| t.base_type_name())
                } else {
                    None
                }
            }
        }
    }

    fn base_type_range(&self) -> Option<ByteRange> {
        match self {
            cst::Type::NamedType(named) => named.name().map(|n| n.byte_range()),
            cst::Type::ListType(list) => list.ty().and_then(|t| t.base_type_range()),
            cst::Type::NonNullType(non_null) => {
                if let Some(named) = non_null.named_type() {
                    named.name().map(|n| n.byte_range())
                } else if let Some(list) = non_null.list_type() {
                    list.ty().and_then(|t| t.base_type_range())
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollo_parser::Parser;

    #[test]
    fn test_operation_name() {
        let source = "query GetUser { user { id } }";
        let tree = Parser::new(source).parse();
        let doc = tree.document();

        for def in doc.definitions() {
            if let cst::Definition::OperationDefinition(op) = def {
                assert_eq!(op.name_text(), Some("GetUser".to_string()));
            }
        }
    }

    #[test]
    fn test_fragment_name() {
        let source = "fragment UserFields on User { name }";
        let tree = Parser::new(source).parse();
        let doc = tree.document();

        for def in doc.definitions() {
            if let cst::Definition::FragmentDefinition(frag) = def {
                assert_eq!(frag.name_text(), Some("UserFields".to_string()));
                assert_eq!(frag.type_condition_name(), Some("User".to_string()));
            }
        }
    }

    #[test]
    fn test_field_name() {
        let source = "query { user { name } }";
        let tree = Parser::new(source).parse();
        let doc = tree.document();

        for def in doc.definitions() {
            if let cst::Definition::OperationDefinition(op) = def {
                if let Some(ss) = op.selection_set() {
                    for sel in ss.selections() {
                        if let cst::Selection::Field(field) = sel {
                            assert_eq!(field.name_text(), Some("user".to_string()));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_base_type_extraction() {
        let source = "type User { friends: [User!]! }";
        let tree = Parser::new(source).parse();
        let doc = tree.document();

        for def in doc.definitions() {
            if let cst::Definition::ObjectTypeDefinition(obj) = def {
                if let Some(fields) = obj.fields_definition() {
                    for field in fields.field_definitions() {
                        if let Some(ty) = field.ty() {
                            // [User!]! should extract "User"
                            assert_eq!(ty.base_type_name(), Some("User".to_string()));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_byte_range() {
        let source = "query GetUser { user }";
        let tree = Parser::new(source).parse();
        let doc = tree.document();

        for def in doc.definitions() {
            if let cst::Definition::OperationDefinition(op) = def {
                let range = op.name_range().unwrap();
                assert_eq!(range.start, 6);
                assert_eq!(range.end, 13);
                assert_eq!(&source[range.start..range.end], "GetUser");
            }
        }
    }

    #[test]
    fn test_contains_offset() {
        let source = "query { user }";
        let tree = Parser::new(source).parse();
        let doc = tree.document();

        for def in doc.definitions() {
            if let cst::Definition::OperationDefinition(op) = def {
                // Operation spans the whole query
                assert!(op.contains_offset(0));
                assert!(op.contains_offset(7));
                assert!(!op.contains_offset(100));
            }
        }
    }
}
