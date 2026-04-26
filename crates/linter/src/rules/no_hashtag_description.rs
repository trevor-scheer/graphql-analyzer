use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use std::collections::HashMap;

/// Lint rule that disallows using `#` comments as type or field descriptions.
///
/// In GraphQL SDL, descriptions should use string literals (double quotes or
/// triple quotes), not hash comments. Hash comments are not part of the
/// schema and won't appear in introspection results.
///
/// Behavior mirrors `@graphql-eslint/eslint-plugin`'s `no-hashtag-description`:
/// for each block of consecutive `#` lines that is immediately followed by a
/// schema definition (type, interface, field, etc.), one diagnostic is emitted
/// at the **last** comment line of the block, naming the entity the comment is
/// attached to.
pub struct NoHashtagDescriptionRuleImpl;

impl LintRule for NoHashtagDescriptionRuleImpl {
    fn name(&self) -> &'static str {
        "noHashtagDescription"
    }

    fn description(&self) -> &'static str {
        "Disallows using # comments as type or field descriptions in schema"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for NoHashtagDescriptionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
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

            for doc in parse.documents() {
                let source = doc.source;
                let lines: Vec<&str> = source.lines().collect();

                // Track the parent type/interface/etc. as we walk the source so
                // diagnostics on field comments can name `field "X" in type "Y"`.
                let mut current_parent: Option<(String, String)> = None;
                let mut depth: i32 = 0;

                let mut i = 0;
                while i < lines.len() {
                    let trimmed = lines[i].trim();

                    // Update parent-tracking for type/interface/input/etc. blocks.
                    if depth == 0 {
                        if let Some(parent) = parse_definition_header(trimmed) {
                            current_parent = Some(parent);
                        }
                    }
                    depth += brace_delta(trimmed);
                    if depth == 0 {
                        // Reset parent when we close the outer definition.
                        if trimmed.contains('}') {
                            current_parent = None;
                        }
                    }

                    if !trimmed.starts_with('#') {
                        i += 1;
                        continue;
                    }

                    // Find the end of this consecutive `#` block.
                    let mut block_end = i;
                    while block_end + 1 < lines.len()
                        && lines[block_end + 1].trim().starts_with('#')
                    {
                        block_end += 1;
                    }

                    // Find the next non-blank, non-comment line.
                    let mut next_idx = block_end + 1;
                    while next_idx < lines.len() && lines[next_idx].trim().is_empty() {
                        next_idx += 1;
                    }

                    if next_idx >= lines.len() {
                        i = block_end + 1;
                        continue;
                    }

                    // Only fire when the block is **immediately** followed by the
                    // next definition (no blank line between). graphql-eslint
                    // requires `linesAfter < 2` between comment and name.
                    let comment_to_def_gap = next_idx - block_end;
                    if comment_to_def_gap > 1 {
                        i = block_end + 1;
                        continue;
                    }

                    let next_line = lines[next_idx].trim();
                    let attached_node = classify_attached_node(next_line, current_parent.as_ref());

                    if let Some(node_name) = attached_node {
                        // Diagnostic position: last `#` line of the block (matches
                        // graphql-eslint, which reports on the comment immediately
                        // preceding the NAME token).
                        let last_comment_line = lines[block_end];
                        let line_start: usize =
                            lines[..block_end].iter().map(|l| l.len() + 1).sum();
                        let comment_start = line_start + last_comment_line.len()
                            - last_comment_line.trim_start().len();
                        let comment_end = line_start + last_comment_line.trim_end().len();

                        diagnostics_by_file.entry(*file_id).or_default().push(
                            LintDiagnostic::new(
                                doc.span(comment_start, comment_end),
                                LintSeverity::Warning,
                                format!(
                                    "Unexpected GraphQL descriptions as hashtag `#` for {node_name}.\nPrefer using `\"\"\"` for multiline, or `\"` for a single line description."
                                ),
                                "noHashtagDescription",
                            )
                            .with_message_id("HASHTAG_COMMENT")
                            .with_help(
                                "Replace the hashtag comment with a string or block string description above the definition",
                            ),
                        );
                    }

                    // Skip past the whole block so consecutive `#` lines aren't
                    // re-processed as additional one-line blocks.
                    i = block_end + 1;
                }
            }
        }

        diagnostics_by_file
    }
}

/// Parse a top-level definition header line and return (kind, name).
/// e.g. `type Post implements Node {` → `("type", "Post")`.
fn parse_definition_header(line: &str) -> Option<(String, String)> {
    let prefixes = [
        ("type ", "type"),
        ("interface ", "interface"),
        ("union ", "union"),
        ("enum ", "enum"),
        ("scalar ", "scalar"),
        ("input ", "input"),
        ("directive ", "directive"),
        ("extend type ", "type"),
        ("extend interface ", "interface"),
        ("extend union ", "union"),
        ("extend enum ", "enum"),
        ("extend scalar ", "scalar"),
        ("extend input ", "input"),
    ];
    for (prefix, kind) in prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            // Name ends at first whitespace or `{`/`(`/`@`.
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '{' || c == '(' || c == '@' || c == ':')
                .unwrap_or(rest.len());
            let name = rest[..end].trim_end_matches('!').to_string();
            if !name.is_empty() {
                return Some((kind.to_string(), name));
            }
        }
    }
    None
}

/// Heuristic: a non-blank, non-comment, non-`}` indented line that looks like
/// `fieldName(...): Type` or `fieldName: Type` is a field definition.
fn parse_field_name(line: &str) -> Option<String> {
    if line.starts_with('}') || line.is_empty() {
        return None;
    }
    let colon = line.find(':')?;
    let before = &line[..colon];
    // Field name ends at `(` if there are arguments.
    let name_end = before.find('(').unwrap_or(before.len());
    let name = before[..name_end].trim();
    if name.chars().next().is_some_and(char::is_alphabetic)
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        Some(name.to_string())
    } else {
        None
    }
}

/// Determine the `nodeName` template for the comment block's diagnostic, given
/// the next definition line and (if inside a type body) the current parent.
/// Returns `None` if the comment isn't attached to a recognizable AST node.
fn classify_attached_node(line: &str, parent: Option<&(String, String)>) -> Option<String> {
    if let Some((kind, name)) = parse_definition_header(line) {
        return Some(format!("{kind} \"{name}\""));
    }
    if let Some((parent_kind, parent_name)) = parent {
        if let Some(field_name) = parse_field_name(line) {
            return Some(format!(
                "field \"{field_name}\" in {parent_kind} \"{parent_name}\""
            ));
        }
        // Enum-value heuristic: a bare identifier inside an `enum` body.
        if parent_kind == "enum" {
            let bare = line.trim_end_matches(',');
            if !bare.is_empty()
                && bare.chars().all(|c| c.is_alphanumeric() || c == '_')
                && bare.chars().next().is_some_and(char::is_alphabetic)
            {
                return Some(format!("enum value \"{bare}\" in enum \"{parent_name}\""));
            }
        }
    }
    None
}

fn brace_delta(line: &str) -> i32 {
    let mut delta: i32 = 0;
    let mut in_string = false;
    for c in line.chars() {
        if c == '"' {
            in_string = !in_string;
        }
        if in_string {
            continue;
        }
        if c == '{' {
            delta += 1;
        } else if c == '}' {
            delta -= 1;
        }
    }
    delta
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
    fn test_proper_description() {
        let db = RootDatabase::default();
        let rule = NoHashtagDescriptionRuleImpl;
        let schema = r#"
"A user in the system"
type User {
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn test_hashtag_description_on_type() {
        let db = RootDatabase::default();
        let rule = NoHashtagDescriptionRuleImpl;
        let schema = r"
# A user in the system
type User {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("type \"User\""));
        assert!(all[0].message.contains("Unexpected GraphQL descriptions"));
    }

    #[test]
    fn test_consecutive_comments_collapse_to_one_diagnostic() {
        // Multiple `#` lines describing the same entity should produce one
        // diagnostic, not one per line.
        let db = RootDatabase::default();
        let rule = NoHashtagDescriptionRuleImpl;
        let schema = r"
# Represents a user
# More about the user
type User {
    id: ID!
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1, "expected one diagnostic for the whole block");
    }

    #[test]
    fn test_hashtag_description_on_field() {
        let db = RootDatabase::default();
        let rule = NoHashtagDescriptionRuleImpl;
        let schema = r"
type User {
    # The user's display name
    name: String!
}
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("field \"name\" in type \"User\""));
    }

    #[test]
    fn test_inline_comment_not_attached_when_separated_by_blank_line() {
        // A blank line between the comment and the next definition means the
        // comment is documentation, not a description.
        let db = RootDatabase::default();
        let rule = NoHashtagDescriptionRuleImpl;
        let schema = r#"
# This is a file-level note about the schema.

"A user"
type User {
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }
}
