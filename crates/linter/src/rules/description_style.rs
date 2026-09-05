use crate::diagnostics::{CodeSuggestion, LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use apollo_compiler::Node;
use graphql_base_db::{FileId, ProjectFiles};
use graphql_syntax::DocumentRef;
use serde::Deserialize;
use std::collections::HashMap;

/// The expected description style
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DescriptionStyleKind {
    /// Inline string: "description"
    Inline,
    /// Block string: """description"""
    Block,
}

/// Options for the `description_style` rule
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct DescriptionStyleOptions {
    /// The expected style for descriptions
    pub style: DescriptionStyleKind,
}

impl Default for DescriptionStyleOptions {
    fn default() -> Self {
        Self {
            style: DescriptionStyleKind::Block,
        }
    }
}

impl DescriptionStyleOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces consistent description style in schema
pub struct DescriptionStyleRuleImpl;

impl LintRule for DescriptionStyleRuleImpl {
    fn name(&self) -> &'static str {
        "descriptionStyle"
    }

    fn description(&self) -> &'static str {
        "Enforces consistent description style (block vs inline)"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for DescriptionStyleRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = DescriptionStyleOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_ids = project_files.schema_file_ids(db).ids(db);

        for file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };

            let parse = graphql_syntax::parse(db, content, metadata);
            if parse.has_errors() {
                continue;
            }

            let mut diagnostics: Vec<LintDiagnostic> = Vec::new();
            for doc in parse.documents() {
                let mut visitor = DescriptionVisitor {
                    diagnostics: &mut diagnostics,
                    doc: &doc,
                    opts,
                };
                visitor.walk_document();
            }
            if !diagnostics.is_empty() {
                diagnostics_by_file
                    .entry(*file_id)
                    .or_default()
                    .extend(diagnostics);
            }
        }

        diagnostics_by_file
    }
}

/// Walks a document and emits a diagnostic for every `.description` whose
/// style (block vs inline) doesn't match the configured style. Mirrors
/// `@graphql-eslint/eslint-plugin`'s `description-style`, which selects
/// `.description[type=StringValue]` on every AST node — so we visit
/// `FieldDefinition`, `InputValueDefinition` (input fields + arguments),
/// `EnumValueDefinition`, and `DirectiveDefinition` in addition to the
/// top-level type definitions.
struct DescriptionVisitor<'a> {
    diagnostics: &'a mut Vec<LintDiagnostic>,
    doc: &'a DocumentRef<'a>,
    opts: DescriptionStyleOptions,
}

impl DescriptionVisitor<'_> {
    fn walk_document(&mut self) {
        for definition in &self.doc.ast.definitions {
            match definition {
                apollo_compiler::ast::Definition::ObjectTypeDefinition(d) => {
                    let parent = format!("type \"{}\"", d.name);
                    self.check(d.description.as_ref(), &parent);
                    for field in &d.fields {
                        self.walk_field(field, &parent);
                    }
                }
                apollo_compiler::ast::Definition::InterfaceTypeDefinition(d) => {
                    let parent = format!("interface \"{}\"", d.name);
                    self.check(d.description.as_ref(), &parent);
                    for field in &d.fields {
                        self.walk_field(field, &parent);
                    }
                }
                apollo_compiler::ast::Definition::UnionTypeDefinition(d) => {
                    self.check(d.description.as_ref(), &format!("union \"{}\"", d.name));
                }
                apollo_compiler::ast::Definition::EnumTypeDefinition(d) => {
                    let parent = format!("enum \"{}\"", d.name);
                    self.check(d.description.as_ref(), &parent);
                    for value in &d.values {
                        self.check(
                            value.description.as_ref(),
                            &format!("enum value \"{}\" in {parent}", value.value),
                        );
                    }
                }
                apollo_compiler::ast::Definition::ScalarTypeDefinition(d) => {
                    self.check(d.description.as_ref(), &format!("scalar \"{}\"", d.name));
                }
                apollo_compiler::ast::Definition::InputObjectTypeDefinition(d) => {
                    let parent = format!("input \"{}\"", d.name);
                    self.check(d.description.as_ref(), &parent);
                    for field in &d.fields {
                        self.check(
                            field.description.as_ref(),
                            &format!("input value \"{}\" in {parent}", field.name),
                        );
                    }
                }
                apollo_compiler::ast::Definition::DirectiveDefinition(d) => {
                    let parent = format!("directive \"{}\"", d.name);
                    self.check(d.description.as_ref(), &parent);
                    for arg in &d.arguments {
                        self.check(
                            arg.description.as_ref(),
                            &format!("input value \"{}\" in {parent}", arg.name),
                        );
                    }
                }
                // Type extensions don't carry descriptions in the spec.
                _ => {}
            }
        }
    }

    fn walk_field(
        &mut self,
        field: &Node<apollo_compiler::ast::FieldDefinition>,
        parent_label: &str,
    ) {
        let field_label = format!("field \"{}\" in {parent_label}", field.name);
        self.check(field.description.as_ref(), &field_label);
        for arg in &field.arguments {
            self.check(
                arg.description.as_ref(),
                &format!("input value \"{}\" in {field_label}", arg.name),
            );
        }
    }

    fn check(&mut self, description: Option<&Node<str>>, parent_label: &str) {
        let Some(desc) = description else {
            return;
        };
        let Some(location) = desc.location() else {
            return;
        };

        let start = location.offset();
        let end = location.end_offset();
        let source = self.doc.source;
        if start >= source.len() || end > source.len() {
            return;
        }

        let raw = &source[start..end];
        let is_block = raw.starts_with("\"\"\"");

        let wrong_style = match self.opts.style {
            DescriptionStyleKind::Block => !is_block,
            DescriptionStyleKind::Inline => is_block,
        };

        if !wrong_style {
            return;
        }

        let unexpected = match self.opts.style {
            DescriptionStyleKind::Block => "inline",
            DescriptionStyleKind::Inline => "block",
        };
        let suggested = match self.opts.style {
            DescriptionStyleKind::Block => "block",
            DescriptionStyleKind::Inline => "inline",
        };

        // Cap the highlighted span so multi-line block descriptions don't
        // produce noisy underlines; matches the original behavior.
        let desc_str = desc.as_str();
        let span_end = end.min(start + desc_str.len().min(30) + 6);

        // Mirror upstream: replace the entire StringValue with the converted
        // text. inline → block: replace leading/trailing `"` with `"""`.
        // block → inline: replace leading/trailing `"""` with `"` AND
        // collapse whitespace runs to a single space.
        let new_text = match self.opts.style {
            DescriptionStyleKind::Block => {
                // current is inline, target is block
                let mut s = raw.to_string();
                if s.starts_with('"') {
                    s.replace_range(0..1, "\"\"\"");
                }
                if s.ends_with('"') {
                    let new_len = s.len();
                    s.replace_range(new_len - 1..new_len, "\"\"\"");
                }
                s
            }
            DescriptionStyleKind::Inline => {
                // current is block, target is inline
                let mut s = raw.to_string();
                if s.starts_with("\"\"\"") {
                    s.replace_range(0..3, "\"");
                }
                if s.ends_with("\"\"\"") {
                    let new_len = s.len();
                    s.replace_range(new_len - 3..new_len, "\"");
                }
                // Collapse all whitespace runs to a single space.
                collapse_whitespace(&s)
            }
        };
        let suggestion = CodeSuggestion::replace(
            format!("Change to {suggested} style description"),
            start,
            end,
            new_text,
        );

        self.diagnostics.push(
            LintDiagnostic::new(
                self.doc.span(start, span_end),
                LintSeverity::Warning,
                format!("Unexpected {unexpected} description for {parent_label}"),
                "descriptionStyle",
            )
            .with_message_id("description-style")
            .with_help(format!("Change to {suggested} style description"))
            .with_suggestion(suggestion),
        );
    }
}

/// Collapse runs of whitespace (spaces, tabs, newlines) into a single space.
/// Mirrors upstream's `originalText.replace(/\s+/g, " ")`.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneSchemaLintRule;
    use graphql_base_db::{
        DocumentFileIds, DocumentKind, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata,
        FileUri, Language, ProjectFiles, SchemaFileIds,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_schema_project(db: &RootDatabase, schema: &str) -> ProjectFiles {
        let file_id = FileId::new(0);
        let content = FileContent::new(db, Arc::from(schema));
        let metadata = FileMetadata::new(
            db,
            file_id,
            FileUri::new("file:///schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let entry = FileEntry::new(db, content, metadata);
        let mut entries = std::collections::HashMap::new();
        entries.insert(file_id, entry);
        let schema_file_ids = SchemaFileIds::new(db, Arc::new(vec![file_id]));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map = FileEntryMap::new(db, Arc::new(entries));
        ProjectFiles::new(
            db,
            schema_file_ids,
            document_file_ids,
            graphql_base_db::ResolvedSchemaFileIds::new(db, std::sync::Arc::new(vec![])),
            file_entry_map,
            graphql_base_db::FilePathMap::new(
                db,
                Arc::new(std::collections::HashMap::new()),
                Arc::new(std::collections::HashMap::new()),
            ),
        )
    }

    #[test]
    fn test_block_style_with_block_option() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"""A user"""
type User {
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None); // default is block
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_inline_style_with_block_option() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"A user"
type User {
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None); // default is block
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Unexpected inline description for type \"User\""
        );
    }

    #[test]
    fn test_inline_field_description_flagged() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"""A user"""
type User {
    "The user id"
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Unexpected inline description for field \"id\" in type \"User\""
        );
    }

    #[test]
    fn test_inline_argument_description_flagged() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"""A query"""
type Query {
    """Lookup a user"""
    user(
        "The user id"
        id: ID!
    ): String
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Unexpected inline description for input value \"id\" in field \"user\" in type \"Query\""
        );
    }

    #[test]
    fn test_inline_input_field_description_flagged() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"""Filter input"""
input UserFilter {
    "Match users by id"
    id: ID
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Unexpected inline description for input value \"id\" in input \"UserFilter\""
        );
    }

    #[test]
    fn test_inline_enum_value_description_flagged() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"""Color"""
enum Color {
    "The color red"
    RED
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Unexpected inline description for enum value \"RED\" in enum \"Color\""
        );
    }

    #[test]
    fn test_inline_directive_definition_description_flagged() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"Mark a field as cached"
directive @cached(
    "How long to cache"
    seconds: Int
) on FIELD_DEFINITION
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let messages: Vec<&str> = diagnostics
            .values()
            .flatten()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(messages.len(), 2);
        assert!(messages.contains(&"Unexpected inline description for directive \"cached\""));
        assert!(messages.contains(
            &"Unexpected inline description for input value \"seconds\" in directive \"cached\""
        ));
    }

    #[test]
    fn test_block_style_on_nested_nodes_does_not_warn() {
        let db = RootDatabase::default();
        let rule = DescriptionStyleRuleImpl;
        let schema = r#"
"""A user"""
type User {
    """Field id"""
    id(
        """Argument something"""
        something: String
    ): ID!
}

"""Color"""
enum Color {
    """Red"""
    RED
}

"""Filter"""
input UserFilter {
    """Filter by id"""
    id: ID
}

"""Cache directive"""
directive @cached(
    """seconds"""
    seconds: Int
) on FIELD_DEFINITION
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty(), "expected no diagnostics, got: {all:?}");
    }
}
