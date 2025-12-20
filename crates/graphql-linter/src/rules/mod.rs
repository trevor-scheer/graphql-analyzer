mod deprecated;
mod enum_values_should_be_screaming_snake_case;
mod field_names_should_be_camel_case;
mod no_anonymous_operations;
mod redundant_fields;
mod require_id_field;
mod type_names_should_be_pascal_case;
mod unique_names;
mod unused_fields;
mod unused_fragments;

pub use deprecated::DeprecatedFieldRule;
pub use enum_values_should_be_screaming_snake_case::EnumValuesShouldBeScreamingSnakeCaseRule;
pub use field_names_should_be_camel_case::FieldNamesShouldBeCamelCaseRule;
pub use no_anonymous_operations::NoAnonymousOperationsRule;
pub use redundant_fields::RedundantFieldsRule;
pub use require_id_field::RequireIdFieldRule;
pub use type_names_should_be_pascal_case::TypeNamesShouldBePascalCaseRule;
pub use unique_names::UniqueNamesRule;
pub use unused_fields::UnusedFieldsRule;
pub use unused_fragments::UnusedFragmentsRule;

use crate::context::{
    DocumentSchemaContext, ProjectContext, StandaloneDocumentContext, StandaloneSchemaContext,
};
use graphql_project::Diagnostic;
use std::collections::HashMap;

/// Trait for implementing standalone document lint rules (no schema)
pub trait StandaloneDocumentRule {
    /// Unique identifier for this rule (e.g., "operation-naming-convention")
    fn name(&self) -> &'static str;

    /// Human-readable description
    #[allow(dead_code)]
    fn description(&self) -> &'static str;

    /// Run the lint check on a standalone document
    fn check(&self, ctx: &StandaloneDocumentContext) -> Vec<Diagnostic>;
}

/// Trait for implementing document+schema lint rules
pub trait DocumentSchemaRule {
    /// Unique identifier for this rule (e.g., "deprecated-field")
    fn name(&self) -> &'static str;

    /// Human-readable description
    #[allow(dead_code)]
    fn description(&self) -> &'static str;

    /// Run the lint check on a document with schema access
    fn check(&self, ctx: &DocumentSchemaContext) -> Vec<Diagnostic>;
}

/// Trait for implementing standalone schema lint rules
pub trait StandaloneSchemaRule {
    /// Unique identifier for this rule (e.g., "schema-naming-convention")
    fn name(&self) -> &'static str;

    /// Human-readable description
    #[allow(dead_code)]
    fn description(&self) -> &'static str;

    /// Run the lint check on a schema
    fn check(&self, ctx: &StandaloneSchemaContext) -> Vec<Diagnostic>;
}

/// Trait for implementing project-wide lint rules that need access to all documents
pub trait ProjectRule {
    /// Unique identifier for this rule (e.g., "unused-fields")
    fn name(&self) -> &'static str;

    /// Human-readable description
    #[allow(dead_code)]
    fn description(&self) -> &'static str;

    /// Run the lint check across the entire project
    /// Returns a `HashMap` where keys are file paths and values are diagnostics for that file
    fn check(&self, ctx: &ProjectContext) -> HashMap<String, Vec<Diagnostic>>;
}

/// Get all available standalone document lint rules
pub fn all_standalone_document_rules() -> Vec<Box<dyn StandaloneDocumentRule>> {
    vec![
        Box::new(NoAnonymousOperationsRule),
        Box::new(RedundantFieldsRule),
    ]
}

/// Get all available document+schema lint rules
pub fn all_document_schema_rules() -> Vec<Box<dyn DocumentSchemaRule>> {
    vec![Box::new(DeprecatedFieldRule), Box::new(RequireIdFieldRule)]
}

/// Get all available standalone schema lint rules
pub fn all_standalone_schema_rules() -> Vec<Box<dyn StandaloneSchemaRule>> {
    vec![
        Box::new(FieldNamesShouldBeCamelCaseRule),
        Box::new(TypeNamesShouldBePascalCaseRule),
        Box::new(EnumValuesShouldBeScreamingSnakeCaseRule),
    ]
}

/// Get all available project-wide lint rules
pub fn all_project_rules() -> Vec<Box<dyn ProjectRule>> {
    vec![
        Box::new(UniqueNamesRule),
        Box::new(UnusedFieldsRule),
        Box::new(UnusedFragmentsRule),
    ]
}
