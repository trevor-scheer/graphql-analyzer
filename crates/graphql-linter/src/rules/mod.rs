mod deprecated;
mod redundant_fields;
mod unique_names;
mod unused_fields;
mod unused_fragments;

pub use deprecated::DeprecatedFieldRule;
pub use redundant_fields::RedundantFieldsRule;
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
    vec![Box::new(RedundantFieldsRule)]
}

/// Get all available document+schema lint rules
pub fn all_document_schema_rules() -> Vec<Box<dyn DocumentSchemaRule>> {
    vec![Box::new(DeprecatedFieldRule)]
}

/// Get all available standalone schema lint rules
pub fn all_standalone_schema_rules() -> Vec<Box<dyn StandaloneSchemaRule>> {
    vec![]
}

/// Get all available project-wide lint rules
pub fn all_project_rules() -> Vec<Box<dyn ProjectRule>> {
    vec![
        Box::new(UniqueNamesRule),
        Box::new(UnusedFieldsRule),
        Box::new(UnusedFragmentsRule),
    ]
}
