//! Definition filtering utilities for GraphQL documents.
//!
//! This module provides convenient iterators and functions for working with
//! specific types of definitions in a GraphQL document.
//!
//! # Example
//!
//! ```
//! use graphql_apollo_ext::DocumentExt;
//! use apollo_parser::Parser;
//!
//! let source = r"
//!     query GetUser { user { id } }
//!     mutation UpdateUser { updateUser { id } }
//!     fragment UserFields on User { name }
//! ";
//! let tree = Parser::new(source).parse();
//!
//! // Get all operations
//! let ops: Vec<_> = tree.operations().collect();
//! assert_eq!(ops.len(), 2);
//!
//! // Get all fragments
//! let frags: Vec<_> = tree.fragments().collect();
//! assert_eq!(frags.len(), 1);
//! ```

use apollo_parser::cst;
use apollo_parser::SyntaxTree;

/// Extension trait for convenient access to document definitions.
pub trait DocumentExt {
    /// Iterate over all operation definitions in the document.
    fn operations(&self) -> impl Iterator<Item = cst::OperationDefinition>;

    /// Iterate over all fragment definitions in the document.
    fn fragments(&self) -> impl Iterator<Item = cst::FragmentDefinition>;

    /// Iterate over all object type definitions in the document.
    fn object_types(&self) -> impl Iterator<Item = cst::ObjectTypeDefinition>;

    /// Iterate over all interface type definitions in the document.
    fn interface_types(&self) -> impl Iterator<Item = cst::InterfaceTypeDefinition>;

    /// Iterate over all union type definitions in the document.
    fn union_types(&self) -> impl Iterator<Item = cst::UnionTypeDefinition>;

    /// Iterate over all enum type definitions in the document.
    fn enum_types(&self) -> impl Iterator<Item = cst::EnumTypeDefinition>;

    /// Iterate over all scalar type definitions in the document.
    fn scalar_types(&self) -> impl Iterator<Item = cst::ScalarTypeDefinition>;

    /// Iterate over all input object type definitions in the document.
    fn input_object_types(&self) -> impl Iterator<Item = cst::InputObjectTypeDefinition>;

    /// Iterate over all directive definitions in the document.
    fn directive_definitions(&self) -> impl Iterator<Item = cst::DirectiveDefinition>;

    /// Find an operation by name.
    fn find_operation(&self, name: &str) -> Option<cst::OperationDefinition>;

    /// Find a fragment by name.
    fn find_fragment(&self, name: &str) -> Option<cst::FragmentDefinition>;

    /// Find a type definition by name (object, interface, union, enum, scalar, input).
    fn find_type(&self, name: &str) -> Option<TypeDefinition>;
}

impl DocumentExt for SyntaxTree {
    fn operations(&self) -> impl Iterator<Item = cst::OperationDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::OperationDefinition(op) = def {
                Some(op)
            } else {
                None
            }
        })
    }

    fn fragments(&self) -> impl Iterator<Item = cst::FragmentDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::FragmentDefinition(frag) = def {
                Some(frag)
            } else {
                None
            }
        })
    }

    fn object_types(&self) -> impl Iterator<Item = cst::ObjectTypeDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::ObjectTypeDefinition(obj) = def {
                Some(obj)
            } else {
                None
            }
        })
    }

    fn interface_types(&self) -> impl Iterator<Item = cst::InterfaceTypeDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::InterfaceTypeDefinition(iface) = def {
                Some(iface)
            } else {
                None
            }
        })
    }

    fn union_types(&self) -> impl Iterator<Item = cst::UnionTypeDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::UnionTypeDefinition(union) = def {
                Some(union)
            } else {
                None
            }
        })
    }

    fn enum_types(&self) -> impl Iterator<Item = cst::EnumTypeDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::EnumTypeDefinition(enum_def) = def {
                Some(enum_def)
            } else {
                None
            }
        })
    }

    fn scalar_types(&self) -> impl Iterator<Item = cst::ScalarTypeDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::ScalarTypeDefinition(scalar) = def {
                Some(scalar)
            } else {
                None
            }
        })
    }

    fn input_object_types(&self) -> impl Iterator<Item = cst::InputObjectTypeDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::InputObjectTypeDefinition(input) = def {
                Some(input)
            } else {
                None
            }
        })
    }

    fn directive_definitions(&self) -> impl Iterator<Item = cst::DirectiveDefinition> {
        self.document().definitions().filter_map(|def| {
            if let cst::Definition::DirectiveDefinition(dir) = def {
                Some(dir)
            } else {
                None
            }
        })
    }

    fn find_operation(&self, name: &str) -> Option<cst::OperationDefinition> {
        use crate::NameExt;
        self.operations()
            .find(|op| op.name_text().as_deref() == Some(name))
    }

    fn find_fragment(&self, name: &str) -> Option<cst::FragmentDefinition> {
        use crate::NameExt;
        self.fragments()
            .find(|frag| frag.name_text().as_deref() == Some(name))
    }

    fn find_type(&self, name: &str) -> Option<TypeDefinition> {
        use crate::NameExt;

        for def in self.document().definitions() {
            match def {
                cst::Definition::ObjectTypeDefinition(obj)
                    if obj.name_text().as_deref() == Some(name) =>
                {
                    return Some(TypeDefinition::Object(obj));
                }
                cst::Definition::InterfaceTypeDefinition(iface)
                    if iface.name_text().as_deref() == Some(name) =>
                {
                    return Some(TypeDefinition::Interface(iface));
                }
                cst::Definition::UnionTypeDefinition(union)
                    if union.name_text().as_deref() == Some(name) =>
                {
                    return Some(TypeDefinition::Union(union));
                }
                cst::Definition::EnumTypeDefinition(enum_def)
                    if enum_def.name_text().as_deref() == Some(name) =>
                {
                    return Some(TypeDefinition::Enum(enum_def));
                }
                cst::Definition::ScalarTypeDefinition(scalar)
                    if scalar.name_text().as_deref() == Some(name) =>
                {
                    return Some(TypeDefinition::Scalar(scalar));
                }
                cst::Definition::InputObjectTypeDefinition(input)
                    if input.name_text().as_deref() == Some(name) =>
                {
                    return Some(TypeDefinition::InputObject(input));
                }
                _ => {}
            }
        }
        None
    }
}

/// A unified type definition enum for easy handling of different type kinds.
#[derive(Debug, Clone)]
pub enum TypeDefinition {
    Object(cst::ObjectTypeDefinition),
    Interface(cst::InterfaceTypeDefinition),
    Union(cst::UnionTypeDefinition),
    Enum(cst::EnumTypeDefinition),
    Scalar(cst::ScalarTypeDefinition),
    InputObject(cst::InputObjectTypeDefinition),
}

impl TypeDefinition {
    /// Get the type name.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        use crate::NameExt;
        match self {
            Self::Object(obj) => obj.name_text(),
            Self::Interface(iface) => iface.name_text(),
            Self::Union(union) => union.name_text(),
            Self::Enum(enum_def) => enum_def.name_text(),
            Self::Scalar(scalar) => scalar.name_text(),
            Self::InputObject(input) => input.name_text(),
        }
    }

    /// Get the type kind as a string.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Object(_) => "object",
            Self::Interface(_) => "interface",
            Self::Union(_) => "union",
            Self::Enum(_) => "enum",
            Self::Scalar(_) => "scalar",
            Self::InputObject(_) => "input",
        }
    }

    /// Check if this is an object type.
    #[must_use]
    pub const fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    /// Check if this is an interface type.
    #[must_use]
    pub const fn is_interface(&self) -> bool {
        matches!(self, Self::Interface(_))
    }

    /// Check if this is a union type.
    #[must_use]
    pub const fn is_union(&self) -> bool {
        matches!(self, Self::Union(_))
    }

    /// Check if this is an enum type.
    #[must_use]
    pub const fn is_enum(&self) -> bool {
        matches!(self, Self::Enum(_))
    }

    /// Check if this is a scalar type.
    #[must_use]
    pub const fn is_scalar(&self) -> bool {
        matches!(self, Self::Scalar(_))
    }

    /// Check if this is an input object type.
    #[must_use]
    pub const fn is_input_object(&self) -> bool {
        matches!(self, Self::InputObject(_))
    }
}

/// Operation type (query, mutation, subscription).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

impl OperationType {
    /// Get the root type name for this operation type.
    #[must_use]
    pub const fn root_type_name(&self) -> &'static str {
        match self {
            Self::Query => "Query",
            Self::Mutation => "Mutation",
            Self::Subscription => "Subscription",
        }
    }
}

/// Extension trait for operation definitions.
pub trait OperationExt {
    /// Get the operation type (query, mutation, subscription).
    fn operation_kind(&self) -> OperationType;

    /// Check if this is a query operation.
    fn is_query(&self) -> bool;

    /// Check if this is a mutation operation.
    fn is_mutation(&self) -> bool;

    /// Check if this is a subscription operation.
    fn is_subscription(&self) -> bool;
}

impl OperationExt for cst::OperationDefinition {
    fn operation_kind(&self) -> OperationType {
        match self.operation_type() {
            Some(op_type) if op_type.mutation_token().is_some() => OperationType::Mutation,
            Some(op_type) if op_type.subscription_token().is_some() => OperationType::Subscription,
            _ => OperationType::Query,
        }
    }

    fn is_query(&self) -> bool {
        self.operation_kind() == OperationType::Query
    }

    fn is_mutation(&self) -> bool {
        self.operation_kind() == OperationType::Mutation
    }

    fn is_subscription(&self) -> bool {
        self.operation_kind() == OperationType::Subscription
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollo_parser::Parser;

    #[test]
    fn test_operations() {
        let source = r"
            query GetUser { user { id } }
            mutation UpdateUser { updateUser { id } }
            subscription OnUserUpdate { userUpdated { id } }
        ";
        let tree = Parser::new(source).parse();

        let ops: Vec<_> = tree.operations().collect();
        assert_eq!(ops.len(), 3);

        assert!(ops[0].is_query());
        assert!(ops[1].is_mutation());
        assert!(ops[2].is_subscription());
    }

    #[test]
    fn test_fragments() {
        let source = r"
            fragment UserFields on User { name }
            fragment AdminFields on Admin { role }
        ";
        let tree = Parser::new(source).parse();

        let frags: Vec<_> = tree.fragments().collect();
        assert_eq!(frags.len(), 2);
    }

    #[test]
    fn test_type_definitions() {
        let source = r"
            type User { id: ID! }
            interface Node { id: ID! }
            union SearchResult = User | Post
            enum Status { ACTIVE INACTIVE }
            scalar DateTime
            input UserInput { name: String! }
        ";
        let tree = Parser::new(source).parse();

        assert_eq!(tree.object_types().count(), 1);
        assert_eq!(tree.interface_types().count(), 1);
        assert_eq!(tree.union_types().count(), 1);
        assert_eq!(tree.enum_types().count(), 1);
        assert_eq!(tree.scalar_types().count(), 1);
        assert_eq!(tree.input_object_types().count(), 1);
    }

    #[test]
    fn test_find_operation() {
        let source = r"
            query GetUser { user { id } }
            query GetPost { post { id } }
        ";
        let tree = Parser::new(source).parse();

        let op = tree.find_operation("GetUser");
        assert!(op.is_some());

        let op = tree.find_operation("NonExistent");
        assert!(op.is_none());
    }

    #[test]
    fn test_find_fragment() {
        let source = "fragment UserFields on User { name }";
        let tree = Parser::new(source).parse();

        let frag = tree.find_fragment("UserFields");
        assert!(frag.is_some());

        let frag = tree.find_fragment("NonExistent");
        assert!(frag.is_none());
    }

    #[test]
    fn test_find_type() {
        let source = r"
            type User { id: ID! }
            interface Node { id: ID! }
        ";
        let tree = Parser::new(source).parse();

        let type_def = tree.find_type("User");
        assert!(type_def.is_some());
        assert!(type_def.unwrap().is_object());

        let type_def = tree.find_type("Node");
        assert!(type_def.is_some());
        assert!(type_def.unwrap().is_interface());

        let type_def = tree.find_type("NonExistent");
        assert!(type_def.is_none());
    }

    #[test]
    fn test_operation_type() {
        let source = "mutation CreateUser { createUser { id } }";
        let tree = Parser::new(source).parse();

        let op = tree.operations().next().unwrap();
        assert_eq!(op.operation_kind(), OperationType::Mutation);
        assert_eq!(op.operation_kind().root_type_name(), "Mutation");
    }
}
