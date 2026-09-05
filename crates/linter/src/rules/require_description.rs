use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneDocumentLintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use graphql_hir::{TextRange, TypeDefKind};
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `require-description` rule. Mirrors graphql-eslint's
/// schema where each kind is opt-in by default (the schema requires
/// `minProperties: 1`, but we accept missing options as "all kinds enabled"
/// to preserve existing behaviour for callers that didn't specify options).
///
/// graphql-eslint exposes one boolean per AST kind, so several `bool` fields
/// here is the natural shape — clippy's lint suggesting an enum doesn't fit.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Deserialize)]
pub struct RequireDescriptionOptions {
    /// Umbrella — enables description requirement for all type definition kinds:
    /// `ObjectTypeDefinition`, `InterfaceTypeDefinition`, `EnumTypeDefinition`,
    /// `ScalarTypeDefinition`, `InputObjectTypeDefinition`, `UnionTypeDefinition`.
    /// Per-kind booleans below take precedence when explicitly set.
    #[serde(default)]
    pub types: bool,

    /// Per-kind overrides. `Some(true)` opts that kind in (overrides `types:
    /// false`); `Some(false)` explicitly opts it out (overrides `types: true`).
    /// `None` defers to the `types` umbrella.
    #[serde(default, rename = "ObjectTypeDefinition")]
    pub object_type_definition: Option<bool>,
    #[serde(default, rename = "InterfaceTypeDefinition")]
    pub interface_type_definition: Option<bool>,
    #[serde(default, rename = "EnumTypeDefinition")]
    pub enum_type_definition: Option<bool>,
    #[serde(default, rename = "ScalarTypeDefinition")]
    pub scalar_type_definition: Option<bool>,
    #[serde(default, rename = "InputObjectTypeDefinition")]
    pub input_object_type_definition: Option<bool>,
    #[serde(default, rename = "UnionTypeDefinition")]
    pub union_type_definition: Option<bool>,

    /// Require descriptions on fields of the root operation types (Query /
    /// Mutation / Subscription). Root types are resolved via the schema
    /// definition or by convention (a type named `Query`, `Mutation`, or
    /// `Subscription` with no explicit `schema` block).
    #[serde(default, rename = "rootField")]
    pub root_field: bool,

    /// Per-kind opt-ins for non-type-system nodes.
    #[serde(default, rename = "FieldDefinition")]
    pub field_definition: bool,
    #[serde(default, rename = "InputValueDefinition")]
    pub input_value_definition: bool,
    #[serde(default, rename = "EnumValueDefinition")]
    pub enum_value_definition: bool,
    #[serde(default, rename = "DirectiveDefinition")]
    pub directive_definition: bool,
    #[serde(default, rename = "OperationDefinition")]
    pub operation_definition: bool,

    /// ESLint-style selectors that suppress description checks on matching
    /// nodes. Currently supports the `[type=Kind][name.value=X]` form used by
    /// graphql-eslint's recommended presets; `X` may be a plain identifier or a
    /// `/regex/` literal. Unrecognised or unsupported selector strings are
    /// silently skipped with a `tracing::warn!`.
    #[serde(default, rename = "ignoredSelectors")]
    pub ignored_selectors: Vec<String>,
}

impl Default for RequireDescriptionOptions {
    fn default() -> Self {
        // Without explicit options, enable every kind — preserves the
        // historical behaviour of this rule for callers not supplying options.
        Self {
            types: true,
            object_type_definition: None,
            interface_type_definition: None,
            enum_type_definition: None,
            scalar_type_definition: None,
            input_object_type_definition: None,
            union_type_definition: None,
            root_field: true,
            field_definition: true,
            input_value_definition: true,
            enum_value_definition: true,
            directive_definition: true,
            operation_definition: true,
            ignored_selectors: Vec::new(),
        }
    }
}

impl RequireDescriptionOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Resolve whether description checking is enabled for a specific type-definition kind.
    /// Per-kind `Some(bool)` wins; falls back to the `types` umbrella.
    fn kind_enabled(&self, kind: TypeDefKind) -> bool {
        let per_kind = match kind {
            TypeDefKind::Object => self.object_type_definition,
            TypeDefKind::Interface => self.interface_type_definition,
            TypeDefKind::Enum => self.enum_type_definition,
            TypeDefKind::Scalar => self.scalar_type_definition,
            TypeDefKind::InputObject => self.input_object_type_definition,
            TypeDefKind::Union => self.union_type_definition,
            _ => None,
        };
        per_kind.unwrap_or(self.types)
    }
}

/// A parsed form of a single `ignoredSelectors` entry.
///
/// Only the subset of `ESLint` selector syntax used by graphql-eslint's
/// recommended presets is supported: `[type=Kind][name.value=X]` where `X` is
/// either a plain identifier or a `/pattern/` JavaScript-style regex.
/// Anything else is logged and skipped.
#[derive(Debug, Clone)]
enum IgnoredSelector {
    /// Match `[type=<ast_kind>][name.value=<literal>]`
    ExactName { ast_kind: String, name: String },
    /// Match `[type=<ast_kind>][name.value=/<pattern>/]`
    Regex {
        ast_kind: String,
        pattern: regex::Regex,
    },
}

impl IgnoredSelector {
    fn parse(raw: &str) -> Option<Self> {
        // Expected form: `[type=ObjectTypeDefinition][name.value=PageInfo]`
        // or `[type=ObjectTypeDefinition][name.value=/(Connection|Edge)$/]`
        let s = raw.trim();

        // Both attributes must be present as `[key=value]` pairs.
        let (type_attr, rest) = parse_bracketed_attr(s)?;
        let (name_attr, leftover) = parse_bracketed_attr(rest)?;
        if !leftover.trim().is_empty() {
            tracing::warn!(
                target: "require-description",
                "ignoredSelectors: unsupported selector (trailing content): {raw}"
            );
            return None;
        }

        let ast_kind = if let Some(kind) = type_attr.strip_prefix("type=") {
            kind.trim().to_string()
        } else {
            tracing::warn!(
                target: "require-description",
                "ignoredSelectors: first attribute must be `type=<Kind>`: {raw}"
            );
            return None;
        };

        let name_value = if let Some(v) = name_attr.strip_prefix("name.value=") {
            v.trim()
        } else {
            tracing::warn!(
                target: "require-description",
                "ignoredSelectors: second attribute must be `name.value=<X>`: {raw}"
            );
            return None;
        };

        if name_value.starts_with('/') {
            // JavaScript regex literal — strip leading `/` and trailing `/flags`.
            let inner = name_value.trim_start_matches('/');
            // Find the last `/` that ends the pattern (flags after it are
            // optional and unused since we only care about matching, not
            // case-insensitivity etc.).
            let end = inner.rfind('/').unwrap_or(inner.len());
            let pattern_str = &inner[..end];
            match regex::Regex::new(pattern_str) {
                Ok(re) => Some(Self::Regex {
                    ast_kind,
                    pattern: re,
                }),
                Err(e) => {
                    tracing::warn!(
                        target: "require-description",
                        "ignoredSelectors: invalid regex `{pattern_str}` in `{raw}`: {e}"
                    );
                    None
                }
            }
        } else {
            Some(Self::ExactName {
                ast_kind,
                name: name_value.to_string(),
            })
        }
    }

    /// Return true when this selector suppresses checking of a type with the
    /// given GraphQL AST kind string and name.
    fn matches(&self, ast_kind: &str, name: &str) -> bool {
        match self {
            Self::ExactName {
                ast_kind: k,
                name: n,
            } => k == ast_kind && n == name,
            Self::Regex {
                ast_kind: k,
                pattern,
            } => k == ast_kind && pattern.is_match(name),
        }
    }
}

/// Extract `[content]` from the start of `s`. Returns `(content, rest_of_s)`.
fn parse_bracketed_attr(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if !s.starts_with('[') {
        return None;
    }
    let close = s.find(']')?;
    Some((&s[1..close], &s[close + 1..]))
}

/// Convert a `TypeDefKind` to the `ESLint` AST kind string that graphql-eslint
/// uses, so `ignoredSelectors` entries (which use `ESLint` kind names) can be
/// matched against our HIR type.
fn type_def_kind_to_ast_kind(kind: TypeDefKind) -> &'static str {
    match kind {
        TypeDefKind::Interface => "InterfaceTypeDefinition",
        TypeDefKind::Enum => "EnumTypeDefinition",
        TypeDefKind::Scalar => "ScalarTypeDefinition",
        TypeDefKind::InputObject => "InputObjectTypeDefinition",
        TypeDefKind::Union => "UnionTypeDefinition",
        // Object and any other/future variants map to ObjectTypeDefinition
        _ => "ObjectTypeDefinition",
    }
}

/// Parse the `ignoredSelectors` strings once, logging and dropping any we
/// can't handle.
fn parse_ignored_selectors(raw: &[String]) -> Vec<IgnoredSelector> {
    raw.iter()
        .filter_map(|s| IgnoredSelector::parse(s))
        .collect()
}

/// Lint rule that requires descriptions on schema definitions and operations.
///
/// Descriptions serve as documentation for schema consumers and operation
/// readers. Mirrors `@graphql-eslint/eslint-plugin`'s `require-description`,
/// which fires on every AST node that can carry a `.description`:
/// type/interface/union/enum/scalar/input definitions, field definitions,
/// input value definitions (input fields and arguments), enum value
/// definitions, directive definitions, and operation definitions.
///
/// `OperationDefinition` description support uses the GraphQL `#` comment
/// immediately above the operation (operations don't have a syntactic
/// `description` slot in the spec).
pub struct RequireDescriptionRuleImpl;

impl LintRule for RequireDescriptionRuleImpl {
    fn name(&self) -> &'static str {
        "requireDescription"
    }

    fn description(&self) -> &'static str {
        "Requires descriptions on type definitions, fields, arguments, enum values, directives, and operations"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for RequireDescriptionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = RequireDescriptionOptions::from_json(options);
        let ignored = parse_ignored_selectors(&opts.ignored_selectors);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Source schema file ids — used to filter out builtins and resolved-schema
        // entries that the user didn't write themselves and can't add descriptions to.
        let source_file_ids: std::collections::HashSet<FileId> = project_files
            .schema_file_ids(db)
            .ids(db)
            .iter()
            .copied()
            .filter(|fid| {
                graphql_base_db::file_lookup(db, project_files, *fid).is_some_and(|(_, meta)| {
                    let uri = meta.uri(db);
                    let s = uri.as_str();
                    !s.ends_with("schema_builtins.graphql")
                        && !s.ends_with("client_builtins.graphql")
                })
            })
            .collect();

        // Resolve root operation type names for the `rootField` check.
        let root_type_names = if opts.root_field {
            Some(crate::schema_utils::extract_root_type_names(
                db,
                project_files,
                schema_types,
            ))
        } else {
            None
        };

        for type_def in schema_types.values() {
            // Skip built-in scalars
            if type_def.kind == TypeDefKind::Scalar
                && matches!(
                    type_def.name.as_ref(),
                    "String" | "Int" | "Float" | "Boolean" | "ID"
                )
            {
                continue;
            }

            // Only flag entities defined in source files (skip resolved schema/builtins)
            if !source_file_ids.contains(&type_def.file_id) {
                continue;
            }

            // Suppress checking for this type entirely if it matches an ignoredSelector.
            let ast_kind = type_def_kind_to_ast_kind(type_def.kind);
            let is_ignored = ignored
                .iter()
                .any(|sel| sel.matches(ast_kind, type_def.name.as_ref()));

            if !is_ignored && opts.kind_enabled(type_def.kind) && type_def.description.is_none() {
                let kind_name = match type_def.kind {
                    TypeDefKind::Interface => "interface",
                    TypeDefKind::Union => "union",
                    TypeDefKind::Enum => "enum",
                    TypeDefKind::Scalar => "scalar",
                    TypeDefKind::InputObject => "input",
                    _ => "type",
                };

                push_missing_description(
                    &mut diagnostics_by_file,
                    type_def.file_id,
                    type_def.name_range,
                    &format!("{kind_name} \"{}\"", type_def.name),
                );
            }

            let parent_label = match type_def.kind {
                TypeDefKind::Interface => format!("interface \"{}\"", type_def.name),
                TypeDefKind::InputObject => format!("input \"{}\"", type_def.name),
                TypeDefKind::Enum => format!("enum \"{}\"", type_def.name),
                _ => format!("type \"{}\"", type_def.name),
            };

            // Whether this type's fields should be checked under `rootField`.
            let is_root = root_type_names
                .as_ref()
                .is_some_and(|rtn| rtn.is_root_type(type_def.name.as_ref()));

            // Field definitions (object/interface) and input value definitions (input).
            for field in &type_def.fields {
                let field_kind = if type_def.kind == TypeDefKind::InputObject {
                    "input value"
                } else {
                    "field"
                };
                let field_label = format!("{field_kind} \"{}\" in {parent_label}", field.name);

                if !source_file_ids.contains(&field.file_id) {
                    continue;
                }

                let field_kind_enabled = if type_def.kind == TypeDefKind::InputObject {
                    opts.input_value_definition
                } else {
                    // A root-type field fires when either `FieldDefinition` or
                    // `rootField` is enabled; non-root fields only fire for
                    // `FieldDefinition`.
                    opts.field_definition || (is_root && opts.root_field)
                };

                if field_kind_enabled && field.description.is_none() {
                    push_missing_description(
                        &mut diagnostics_by_file,
                        field.file_id,
                        field.name_range,
                        &field_label,
                    );
                }

                // Arguments only apply to object/interface fields, not input values.
                if opts.input_value_definition && type_def.kind != TypeDefKind::InputObject {
                    for arg in &field.arguments {
                        if !source_file_ids.contains(&arg.file_id) {
                            continue;
                        }
                        if arg.description.is_none() {
                            push_missing_description(
                                &mut diagnostics_by_file,
                                arg.file_id,
                                arg.name_range,
                                &format!("input value \"{}\" in {field_label}", arg.name),
                            );
                        }
                    }
                }
            }

            // Enum value definitions
            if opts.enum_value_definition {
                for value in &type_def.enum_values {
                    if value.description.is_none() {
                        push_missing_description(
                            &mut diagnostics_by_file,
                            type_def.file_id,
                            value.name_range,
                            &format!("enum value \"{}\" in {parent_label}", value.name),
                        );
                    }
                }
            }
        }

        // Directive definitions (and their arguments).
        if opts.directive_definition || opts.input_value_definition {
            let directives = graphql_hir::schema_directives(db, project_files);
            for dir_def in directives.values() {
                if !source_file_ids.contains(&dir_def.file_id) {
                    continue;
                }

                let dir_label = format!("directive \"{}\"", dir_def.name);

                if opts.directive_definition && dir_def.description.is_none() {
                    push_missing_description(
                        &mut diagnostics_by_file,
                        dir_def.file_id,
                        dir_def.name_range,
                        &dir_label,
                    );
                }

                if opts.input_value_definition {
                    for arg in &dir_def.arguments {
                        if !source_file_ids.contains(&arg.file_id) {
                            continue;
                        }
                        if arg.description.is_none() {
                            push_missing_description(
                                &mut diagnostics_by_file,
                                arg.file_id,
                                arg.name_range,
                                &format!("input value \"{}\" in {dir_label}", arg.name),
                            );
                        }
                    }
                }
            }
        }

        diagnostics_by_file
    }
}

impl StandaloneDocumentLintRule for RequireDescriptionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let opts = RequireDescriptionOptions::from_json(options);
        let mut diagnostics = Vec::new();
        if !opts.operation_definition {
            return diagnostics;
        }
        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        // graphql-eslint treats the `#` comment immediately above an operation
        // (no blank line between) as that operation's description.
        for doc in parse.documents() {
            let source = doc.source;
            for definition in &doc.ast.definitions {
                let apollo_compiler::ast::Definition::OperationDefinition(op) = definition else {
                    continue;
                };
                let Some(loc) = op.location() else { continue };
                let op_start = loc.offset();
                let op_end = loc.end_offset();
                if op_start >= source.len() || op_end > source.len() {
                    continue;
                }

                if has_immediate_comment(source, op_start) {
                    continue;
                }

                // Find the operation keyword (`query`/`mutation`/`subscription`)
                // or for shorthand queries, the opening `{` brace. Mirrors
                // `getLocation(node.loc.start, node.operation)` in graphql-eslint.
                let op_keyword = match op.operation_type {
                    apollo_compiler::ast::OperationType::Query => "query",
                    apollo_compiler::ast::OperationType::Mutation => "mutation",
                    apollo_compiler::ast::OperationType::Subscription => "subscription",
                };
                let raw = &source[op_start..op_end];
                let (rel_start, rel_end) =
                    if let Some(pos) = raw.find(op_keyword).filter(|p| *p < 8) {
                        (pos, pos + op_keyword.len())
                    } else if let Some(pos) = raw.find('{') {
                        (pos, pos + 1)
                    } else {
                        (0, raw.len().min(1))
                    };
                let span_start = op_start + rel_start;
                let span_end = op_start + rel_end;

                let label = match (op.name.as_ref(), op.operation_type) {
                    (Some(name), apollo_compiler::ast::OperationType::Query) => {
                        format!("query \"{name}\"")
                    }
                    (Some(name), apollo_compiler::ast::OperationType::Mutation) => {
                        format!("mutation \"{name}\"")
                    }
                    (Some(name), apollo_compiler::ast::OperationType::Subscription) => {
                        format!("subscription \"{name}\"")
                    }
                    (None, _) => op_keyword.to_string(),
                };

                diagnostics.push(
                    LintDiagnostic::new(
                        doc.span(span_start, span_end),
                        LintSeverity::Warning,
                        format!("Description is required for {label}"),
                        "requireDescription",
                    )
                    .with_message_id("require-description")
                    .with_help(
                        "Add a `# comment` line directly above the operation to document its purpose",
                    ),
                );
            }
        }

        diagnostics
    }
}

fn push_missing_description(
    diagnostics_by_file: &mut HashMap<FileId, Vec<LintDiagnostic>>,
    file_id: FileId,
    name_range: TextRange,
    label: &str,
) {
    let span = graphql_syntax::SourceSpan {
        start: name_range.start().into(),
        end: name_range.end().into(),
        line_offset: 0,
        byte_offset: 0,
        source: None,
    };

    diagnostics_by_file.entry(file_id).or_default().push(
        LintDiagnostic::new(
            span,
            LintSeverity::Warning,
            format!("Description is required for {label}"),
            "requireDescription",
        )
        .with_message_id("require-description")
        .with_help("Add a description string above the definition to document its purpose"),
    );
}

/// Returns true when the byte at `op_start` is preceded by a `#` comment
/// whose final newline is on the line directly before the operation
/// (matches graphql-eslint's `linesBefore === 1` check).
fn has_immediate_comment(source: &str, op_start: usize) -> bool {
    let before = &source[..op_start];

    // Walk back over whitespace on the operation's start line.
    let mut idx = before.len();
    while idx > 0 {
        let b = before.as_bytes()[idx - 1];
        if b == b'\n' {
            break;
        }
        if !b.is_ascii_whitespace() {
            // Non-whitespace before the operation on its own line — anything
            // else here means this isn't a leading-comment scenario.
            return false;
        }
        idx -= 1;
    }

    // `idx` now points at the newline at the end of the previous line, or 0.
    // Skip back past blank lines — graphql-eslint requires the comment to be
    // on the line *immediately* preceding the operation.
    if idx == 0 {
        return false;
    }
    // Step over the newline at idx-1, then look at the previous line.
    let prev_line_end = idx - 1; // position of '\n'
    let prev_line_start = before[..prev_line_end].rfind('\n').map_or(0, |nl| nl + 1);
    let prev_line = &before[prev_line_start..prev_line_end];
    let trimmed = prev_line.trim_start();
    if !trimmed.starts_with('#') {
        return false;
    }
    // Skip ESLint directive comments — `# eslint-disable …` etc.
    let body = trimmed.trim_start_matches('#').trim();
    !body.starts_with("eslint")
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn create_document_project(
        db: &RootDatabase,
        source: &str,
    ) -> (FileId, FileContent, FileMetadata, ProjectFiles) {
        let file_id = FileId::new(0);
        let content = FileContent::new(db, Arc::from(source));
        let metadata = FileMetadata::new(
            db,
            file_id,
            FileUri::new("file:///query.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let entry = FileEntry::new(db, content, metadata);
        let mut entries = std::collections::HashMap::new();
        entries.insert(file_id, entry);

        let schema_file_ids = SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(vec![file_id]));
        let file_entry_map = FileEntryMap::new(db, Arc::new(entries));
        let project_files = ProjectFiles::new(
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
        );
        (file_id, content, metadata, project_files)
    }

    fn messages(diagnostics: &HashMap<FileId, Vec<LintDiagnostic>>) -> Vec<&str> {
        diagnostics
            .values()
            .flatten()
            .map(|d| d.message.as_str())
            .collect()
    }

    #[test]
    fn test_type_with_description() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;

        let schema = r#"
"A user in the system"
type User {
    "An id"
    id: ID!
    "A name"
    name: String!
}
"#;

        let project_files = create_schema_project(&db, schema);
        let diagnostics = StandaloneSchemaLintRule::check(&rule, &db, project_files, None);

        let user_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("\"User\""))
            .collect();
        assert!(user_warnings.is_empty());
    }

    #[test]
    fn test_type_without_description() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;

        let schema = r"
type User {
    id: ID!
    name: String!
}
";

        let project_files = create_schema_project(&db, schema);
        let diagnostics = StandaloneSchemaLintRule::check(&rule, &db, project_files, None);

        let user_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message == "Description is required for type \"User\"")
            .collect();
        assert_eq!(user_warnings.len(), 1);
    }

    #[test]
    fn test_field_without_description_flagged() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let schema = r#"
"A user"
type User {
    id: ID!
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = StandaloneSchemaLintRule::check(&rule, &db, project_files, None);
        let msgs = messages(&diagnostics);
        assert!(
            msgs.contains(&"Description is required for field \"id\" in type \"User\""),
            "missing field diagnostic, got: {msgs:?}"
        );
    }

    #[test]
    fn test_argument_without_description_flagged() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let schema = r#"
"A query"
type Query {
    "Lookup a user"
    user(id: ID!): String
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = StandaloneSchemaLintRule::check(&rule, &db, project_files, None);
        let msgs = messages(&diagnostics);
        assert!(
            msgs.contains(
                &"Description is required for input value \"id\" in field \"user\" in type \"Query\""
            ),
            "missing arg diagnostic, got: {msgs:?}"
        );
    }

    #[test]
    fn test_input_field_without_description_flagged() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let schema = r#"
"Filter input"
input UserFilter {
    id: ID
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = StandaloneSchemaLintRule::check(&rule, &db, project_files, None);
        let msgs = messages(&diagnostics);
        assert!(
            msgs.contains(
                &"Description is required for input value \"id\" in input \"UserFilter\""
            ),
            "missing input field diagnostic, got: {msgs:?}"
        );
    }

    #[test]
    fn test_enum_value_without_description_flagged() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let schema = r#"
"A color"
enum Color {
    RED
}
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = StandaloneSchemaLintRule::check(&rule, &db, project_files, None);
        let msgs = messages(&diagnostics);
        assert!(
            msgs.contains(&"Description is required for enum value \"RED\" in enum \"Color\""),
            "missing enum value diagnostic, got: {msgs:?}"
        );
    }

    #[test]
    fn test_directive_without_description_flagged() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let schema = r"
directive @cached(seconds: Int) on FIELD_DEFINITION
";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = StandaloneSchemaLintRule::check(&rule, &db, project_files, None);
        let msgs = messages(&diagnostics);
        assert!(
            msgs.contains(&"Description is required for directive \"cached\""),
            "missing directive diagnostic, got: {msgs:?}"
        );
        assert!(
            msgs.contains(
                &"Description is required for input value \"seconds\" in directive \"cached\""
            ),
            "missing directive arg diagnostic, got: {msgs:?}"
        );
    }

    #[test]
    fn test_fully_documented_schema_clean() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let schema = r#"
"A user"
type User {
    "The id"
    id: ID!
}

"Filter input"
input UserFilter {
    "Filter by id"
    id: ID
}

"Color"
enum Color {
    "Red"
    RED
}

"Cache directive"
directive @cached(
    "How long to cache"
    seconds: Int
) on FIELD_DEFINITION
"#;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = StandaloneSchemaLintRule::check(&rule, &db, project_files, None);
        let msgs = messages(&diagnostics);
        assert!(msgs.is_empty(), "expected no diagnostics, got: {msgs:?}");
    }

    #[test]
    fn test_operation_without_comment_flagged() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let source = "query GetUser { user { id } }\n";
        let (file_id, content, metadata, project_files) = create_document_project(&db, source);
        let diagnostics = StandaloneDocumentLintRule::check(
            &rule,
            &db,
            file_id,
            content,
            metadata,
            project_files,
            None,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Description is required for query \"GetUser\""
        );
    }

    #[test]
    fn test_operation_with_leading_comment_clean() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let source = "# Fetches the current user\nquery GetUser { user { id } }\n";
        let (file_id, content, metadata, project_files) = create_document_project(&db, source);
        let diagnostics = StandaloneDocumentLintRule::check(
            &rule,
            &db,
            file_id,
            content,
            metadata,
            project_files,
            None,
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_operation_with_blank_line_after_comment_flagged() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        // Blank line between comment and operation — graphql-eslint requires
        // `linesBefore === 1`, so this is NOT a description.
        let source = "# Fetches the current user\n\nquery GetUser { user { id } }\n";
        let (file_id, content, metadata, project_files) = create_document_project(&db, source);
        let diagnostics = StandaloneDocumentLintRule::check(
            &rule,
            &db,
            file_id,
            content,
            metadata,
            project_files,
            None,
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_anonymous_operation_uses_keyword_label() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let source = "mutation { updateUser(id: \"1\") { id } }\n";
        let (file_id, content, metadata, project_files) = create_document_project(&db, source);
        let diagnostics = StandaloneDocumentLintRule::check(
            &rule,
            &db,
            file_id,
            content,
            metadata,
            project_files,
            None,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Description is required for mutation"
        );
    }

    #[test]
    fn test_eslint_disable_comment_does_not_satisfy() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let source = "# eslint-disable-next-line\nquery GetUser { user { id } }\n";
        let (file_id, content, metadata, project_files) = create_document_project(&db, source);
        let diagnostics = StandaloneDocumentLintRule::check(
            &rule,
            &db,
            file_id,
            content,
            metadata,
            project_files,
            None,
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_fragment_definitions_not_flagged() {
        let db = RootDatabase::default();
        let rule = RequireDescriptionRuleImpl;
        let source = "fragment UserFields on User { id }\n";
        let (file_id, content, metadata, project_files) = create_document_project(&db, source);
        let diagnostics = StandaloneDocumentLintRule::check(
            &rule,
            &db,
            file_id,
            content,
            metadata,
            project_files,
            None,
        );
        assert!(diagnostics.is_empty());
    }
}
