use graphql_project::{DocumentIndex, SchemaIndex};

/// Context for linting a standalone document (no schema)
pub struct StandaloneDocumentContext<'a> {
    pub document: &'a str,
    pub file_name: &'a str,
    /// Optional access to the global fragment index for cross-file fragment resolution
    pub fragments: Option<&'a DocumentIndex>,
    /// Pre-parsed syntax tree to avoid repeated parsing
    pub parsed: &'a apollo_parser::SyntaxTree,
}

/// Context for linting a document against a schema
pub struct DocumentSchemaContext<'a> {
    pub document: &'a str,
    pub file_name: &'a str,
    pub schema: &'a SchemaIndex,
    /// Pre-parsed syntax tree to avoid repeated parsing
    pub parsed: &'a apollo_parser::SyntaxTree,
}

/// Context for linting a standalone schema
pub struct StandaloneSchemaContext<'a> {
    pub schema: &'a SchemaIndex,
}

/// Context for project-wide linting
pub struct ProjectContext<'a> {
    pub documents: &'a DocumentIndex,
    pub schema: &'a SchemaIndex,
}
