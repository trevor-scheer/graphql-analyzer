//! Type definitions for GraphQL introspection responses.
//!
//! These types mirror the structure of GraphQL introspection query responses
//! and can be deserialized from JSON using serde.

use serde::{Deserialize, Serialize};

/// Top-level introspection response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionResponse {
    pub data: IntrospectionData,
}

/// Data field of the introspection response containing the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionData {
    #[serde(rename = "__schema")]
    pub schema: IntrospectionSchema,
}

/// Complete GraphQL schema information from introspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionSchema {
    pub query_type: Option<IntrospectionTypeRef>,
    pub mutation_type: Option<IntrospectionTypeRef>,
    pub subscription_type: Option<IntrospectionTypeRef>,
    pub types: Vec<IntrospectionType>,
    pub directives: Vec<IntrospectionDirective>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionTypeRef {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IntrospectionType {
    #[serde(rename = "SCALAR")]
    Scalar(IntrospectionScalarType),
    #[serde(rename = "OBJECT")]
    Object(IntrospectionObjectType),
    #[serde(rename = "INTERFACE")]
    Interface(IntrospectionInterfaceType),
    #[serde(rename = "UNION")]
    Union(IntrospectionUnionType),
    #[serde(rename = "ENUM")]
    Enum(IntrospectionEnumType),
    #[serde(rename = "INPUT_OBJECT")]
    InputObject(IntrospectionInputObjectType),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionScalarType {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionObjectType {
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<IntrospectionField>,
    pub interfaces: Vec<IntrospectionTypeRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionInterfaceType {
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<IntrospectionField>,
    pub interfaces: Vec<IntrospectionTypeRef>,
    pub possible_types: Option<Vec<IntrospectionTypeRef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionUnionType {
    pub name: String,
    pub description: Option<String>,
    pub possible_types: Vec<IntrospectionTypeRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionEnumType {
    pub name: String,
    pub description: Option<String>,
    pub enum_values: Vec<IntrospectionEnumValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionInputObjectType {
    pub name: String,
    pub description: Option<String>,
    pub input_fields: Vec<IntrospectionInputValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionField {
    pub name: String,
    pub description: Option<String>,
    pub args: Vec<IntrospectionInputValue>,
    #[serde(rename = "type")]
    pub type_ref: IntrospectionTypeRefFull,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionInputValue {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub type_ref: IntrospectionTypeRefFull,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionEnumValue {
    pub name: String,
    pub description: Option<String>,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionDirective {
    pub name: String,
    pub description: Option<String>,
    pub locations: Vec<String>,
    pub args: Vec<IntrospectionInputValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionTypeRefFull {
    pub kind: TypeKind,
    pub name: Option<String>,
    pub of_type: Option<Box<IntrospectionTypeRefFull>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum TypeKind {
    Scalar,
    Object,
    Interface,
    Union,
    Enum,
    InputObject,
    List,
    NonNull,
}

impl IntrospectionTypeRefFull {
    /// Converts the type reference to a GraphQL type string.
    ///
    /// Handles type wrappers like `NonNull` and `List` to generate strings like:
    /// - `String` for a simple scalar
    /// - `String!` for a non-null scalar
    /// - `[String]` for a list
    /// - `[String!]!` for a non-null list of non-null strings
    ///
    /// # Examples
    ///
    /// ```
    /// # use graphql_introspect::{IntrospectionTypeRefFull, TypeKind};
    /// let type_ref = IntrospectionTypeRefFull {
    ///     kind: TypeKind::NonNull,
    ///     name: None,
    ///     of_type: Some(Box::new(IntrospectionTypeRefFull {
    ///         kind: TypeKind::Scalar,
    ///         name: Some("String".to_string()),
    ///         of_type: None,
    ///     })),
    /// };
    /// assert_eq!(type_ref.to_type_string(), "String!");
    /// ```
    #[must_use]
    pub fn to_type_string(&self) -> String {
        match self.kind {
            TypeKind::NonNull => self.of_type.as_ref().map_or_else(
                || "!".to_string(),
                |of_type| format!("{}!", of_type.to_type_string()),
            ),
            TypeKind::List => self.of_type.as_ref().map_or_else(
                || "[]".to_string(),
                |of_type| format!("[{}]", of_type.to_type_string()),
            ),
            _ => self.name.as_deref().unwrap_or_default().to_string(),
        }
    }
}

impl std::fmt::Display for IntrospectionTypeRefFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_type_string())
    }
}
