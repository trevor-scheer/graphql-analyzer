use graphql_project::{DocumentIndex, SchemaIndex};

/// Context for linting a standalone document (no schema)
pub struct StandaloneDocumentContext<'a> {
    pub document: &'a str,
    pub file_name: &'a str,
}

/// Context for linting a document against a schema
pub struct DocumentSchemaContext<'a> {
    pub document: &'a str,
    pub file_name: &'a str,
    pub schema: &'a SchemaIndex,
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
