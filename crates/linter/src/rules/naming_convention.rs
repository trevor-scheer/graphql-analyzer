use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::rules::{get_operation_kind, OperationKind};
use crate::traits::{LintRule, StandaloneDocumentLintRule, StandaloneSchemaLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use graphql_hir::{TextRange, TypeDefKind};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;

/// Convention for names. Accepts the same string forms as graphql-eslint:
/// `"camelCase"`, `"PascalCase"`, `"snake_case"`, `"UPPER_CASE"`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum NamingCase {
    /// camelCase
    #[serde(rename = "camelCase")]
    Camel,
    /// `PascalCase`
    #[serde(rename = "PascalCase")]
    Pascal,
    /// `snake_case`
    #[serde(rename = "snake_case")]
    Snake,
    /// `UPPER_CASE`
    #[serde(rename = "UPPER_CASE")]
    Upper,
}

impl NamingCase {
    fn check(self, name: &str) -> bool {
        match self {
            NamingCase::Camel => is_camel_case(name),
            NamingCase::Pascal => is_pascal_case(name),
            NamingCase::Snake => is_snake_case(name),
            NamingCase::Upper => is_upper_case(name),
        }
    }

    fn label(self) -> &'static str {
        match self {
            NamingCase::Camel => "camelCase",
            NamingCase::Pascal => "PascalCase",
            NamingCase::Snake => "snake_case",
            NamingCase::Upper => "UPPER_CASE",
        }
    }
}

fn is_camel_case(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let first = s.chars().next().unwrap();
    first.is_lowercase() && !s.contains('_')
}

fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let first = s.chars().next().unwrap();
    first.is_uppercase() && !s.contains('_')
}

fn is_snake_case(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_upper_case(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Per-kind rule entry. Accepts the bare `"camelCase"` form *and*
/// the object form upstream uses in its recommended preset
/// (`{ style: "PascalCase", forbiddenPrefixes: ..., forbiddenSuffixes: ..., ... }`).
///
/// The `Detailed` variant is much larger than `Case`; both rare-and-Boxed
/// alternatives weren't worth the extra indirection because instances live
/// behind `Option<NamingRule>` slots that are themselves rare to populate.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum NamingRule {
    /// `Kind: "camelCase"`
    Case(NamingCase),
    /// `Kind: { style: "camelCase", forbiddenPrefixes: [...], ... }`
    Detailed {
        #[serde(default)]
        style: Option<NamingCase>,
        #[serde(default, rename = "prefix")]
        prefix: Option<String>,
        #[serde(default, rename = "suffix")]
        suffix: Option<String>,
        #[serde(default, rename = "forbiddenPrefixes")]
        forbidden_prefixes: Vec<String>,
        #[serde(default, rename = "forbiddenSuffixes")]
        forbidden_suffixes: Vec<String>,
        #[serde(default, rename = "requiredPrefixes")]
        required_prefixes: Vec<String>,
        #[serde(default, rename = "requiredSuffixes")]
        required_suffixes: Vec<String>,
        #[serde(default, rename = "requiredPattern")]
        required_pattern: Option<String>,
        #[serde(default, rename = "forbiddenPatterns")]
        forbidden_patterns: Vec<String>,
        #[serde(default, rename = "ignorePattern")]
        ignore_pattern: Option<String>,
        #[serde(default, rename = "allowLeadingUnderscore")]
        allow_leading_underscore: bool,
        #[serde(default, rename = "allowTrailingUnderscore")]
        allow_trailing_underscore: bool,
    },
}

/// Borrowed view onto a normalized `NamingRule`. Both `Case` and `Detailed`
/// variants of `NamingRule` collapse into this for uniform processing — the
/// matching closely follows graphql-eslint's `checkNode` / `getError` impl.
struct NormalizedRule<'a> {
    style: Option<NamingCase>,
    prefix: Option<&'a str>,
    suffix: Option<&'a str>,
    forbidden_prefixes: &'a [String],
    forbidden_suffixes: &'a [String],
    required_prefixes: &'a [String],
    required_suffixes: &'a [String],
    required_pattern: Option<&'a str>,
    forbidden_patterns: &'a [String],
    ignore_pattern: Option<&'a str>,
    allow_leading_underscore: bool,
    allow_trailing_underscore: bool,
}

impl<'a> NormalizedRule<'a> {
    fn from_rule(rule: &'a NamingRule) -> Self {
        match rule {
            NamingRule::Case(c) => Self {
                style: Some(*c),
                prefix: None,
                suffix: None,
                forbidden_prefixes: &[],
                forbidden_suffixes: &[],
                required_prefixes: &[],
                required_suffixes: &[],
                required_pattern: None,
                forbidden_patterns: &[],
                ignore_pattern: None,
                allow_leading_underscore: false,
                allow_trailing_underscore: false,
            },
            NamingRule::Detailed {
                style,
                prefix,
                suffix,
                forbidden_prefixes,
                forbidden_suffixes,
                required_prefixes,
                required_suffixes,
                required_pattern,
                forbidden_patterns,
                ignore_pattern,
                allow_leading_underscore,
                allow_trailing_underscore,
            } => Self {
                style: *style,
                prefix: prefix.as_deref(),
                suffix: suffix.as_deref(),
                forbidden_prefixes,
                forbidden_suffixes,
                required_prefixes,
                required_suffixes,
                required_pattern: required_pattern.as_deref(),
                forbidden_patterns,
                ignore_pattern: ignore_pattern.as_deref(),
                allow_leading_underscore: *allow_leading_underscore,
                allow_trailing_underscore: *allow_trailing_underscore,
            },
        }
    }

    fn strip_underscores<'b>(&self, name: &'b str) -> &'b str {
        let mut s = name;
        if self.allow_leading_underscore {
            s = s.trim_start_matches('_');
        }
        if self.allow_trailing_underscore {
            s = s.trim_end_matches('_');
        }
        s
    }
}

/// Joins a list of strings using `, ` as separator and `, or ` before the
/// last item — mirrors `Intl.ListFormat("en-US", {type: "disjunction"})`.
fn english_join(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [single] => single.clone(),
        [a, b] => format!("{a} or {b}"),
        _ => {
            let head = &words[..words.len() - 1];
            let tail = &words[words.len() - 1];
            format!("{}, or {tail}", head.join(", "))
        }
    }
}

/// Options for the `naming_convention` rule.
///
/// Mirrors graphql-eslint: with no options the rule no-ops. Each AST kind
/// must be opted into explicitly. Per-kind entries accept either the bare
/// `"camelCase"` string or the detailed object with `style`, `prefix`,
/// `suffix`, `forbiddenPrefixes`/`forbiddenSuffixes`, `requiredPrefixes`/
/// `requiredSuffixes`, `requiredPattern`, `forbiddenPatterns`, `ignorePattern`,
/// `allowLeadingUnderscore`, and `allowTrailingUnderscore`.
///
/// Unknown keys (including ESLint-style selectors like
/// `"FieldDefinition[parent.name.value=Query]"`) are captured into
/// `selector_overrides` so deserialization doesn't fail; their content is
/// not enforced today (selector parsing is deferred).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NamingConventionOptions {
    /// Convention for operation names (no default; must be set to fire)
    #[serde(rename = "OperationDefinition")]
    pub operation_definition: Option<NamingRule>,
    /// Convention for fragment names (no default; must be set to fire)
    #[serde(rename = "FragmentDefinition")]
    pub fragment_definition: Option<NamingRule>,
    /// Convention for variable names (no default; must be set to fire)
    #[serde(rename = "VariableDefinition", alias = "Variable")]
    pub variable: Option<NamingRule>,
    /// Umbrella default applied to every type-system kind (object, interface,
    /// enum, scalar, input, union) that doesn't have its own override.
    #[serde(default, rename = "types")]
    pub types: Option<NamingRule>,
    #[serde(default, rename = "FieldDefinition")]
    pub field_definition: Option<NamingRule>,
    #[serde(default, rename = "InputValueDefinition")]
    pub input_value_definition: Option<NamingRule>,
    #[serde(default, rename = "Argument")]
    pub argument: Option<NamingRule>,
    #[serde(default, rename = "DirectiveDefinition")]
    pub directive_definition: Option<NamingRule>,
    #[serde(default, rename = "EnumValueDefinition")]
    pub enum_value_definition: Option<NamingRule>,
    #[serde(default, rename = "ObjectTypeDefinition")]
    pub object_type_definition: Option<NamingRule>,
    #[serde(default, rename = "InterfaceTypeDefinition")]
    pub interface_type_definition: Option<NamingRule>,
    #[serde(default, rename = "EnumTypeDefinition")]
    pub enum_type_definition: Option<NamingRule>,
    #[serde(default, rename = "UnionTypeDefinition")]
    pub union_type_definition: Option<NamingRule>,
    #[serde(default, rename = "ScalarTypeDefinition")]
    pub scalar_type_definition: Option<NamingRule>,
    #[serde(default, rename = "InputObjectTypeDefinition")]
    pub input_object_type_definition: Option<NamingRule>,
    /// Catches ESLint-style selector keys (`"FieldDefinition[parent.name.value=Query]"`,
    /// `"EnumTypeDefinition,EnumTypeExtension"`) so deserialization
    /// doesn't fail when a user pastes upstream's preset config. Selector
    /// parsing is intentionally not implemented — these keys are accepted
    /// for drop-in config compatibility but no diagnostics are emitted from
    /// them. Documented and intentional follow-up.
    #[serde(flatten)]
    #[allow(dead_code)]
    pub selector_overrides: std::collections::HashMap<String, serde_json::Value>,
}

impl NamingConventionOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Pick the rule that applies to `kind`, falling back to the `types`
    /// umbrella for type-system kinds with no explicit override.
    fn rule_for_type_kind(&self, kind: TypeDefKind) -> Option<&NamingRule> {
        let explicit = match kind {
            TypeDefKind::Object => self.object_type_definition.as_ref(),
            TypeDefKind::Interface => self.interface_type_definition.as_ref(),
            TypeDefKind::Enum => self.enum_type_definition.as_ref(),
            TypeDefKind::Union => self.union_type_definition.as_ref(),
            TypeDefKind::Scalar => self.scalar_type_definition.as_ref(),
            TypeDefKind::InputObject => self.input_object_type_definition.as_ref(),
            // `TypeDefKind` is `#[non_exhaustive]`; future variants fall
            // through to the `types` umbrella with no explicit override.
            _ => None,
        };
        explicit.or(self.types.as_ref())
    }
}

/// Result of running all `NormalizedRule` checks against a name. The first
/// failing check is reported, mirroring upstream's short-circuiting `getError`.
struct CheckFailure {
    /// The body of the message that follows `<Kind> "<name>" should `.
    /// Examples: `be in camelCase format`, `not have "Query" prefix`,
    /// `contain the required pattern: /^get/`.
    message_body: String,
}

/// Run the per-kind checks in the same order as upstream's `getError`:
/// 1. `ignorePattern` short-circuits to no error.
/// 2. `prefix`
/// 3. `suffix`
/// 4. `requiredPattern`
/// 5. `forbiddenPatterns` (first match wins)
/// 6. `forbiddenPrefixes` (first match wins)
/// 7. `forbiddenSuffixes` (first match wins)
/// 8. `requiredPrefixes` (none-of-them match)
/// 9. `requiredSuffixes` (none-of-them match)
/// 10. `style`
fn check_name(rule: &NormalizedRule<'_>, name: &str) -> Option<CheckFailure> {
    let stripped = rule.strip_underscores(name);

    if let Some(re) = rule.ignore_pattern.and_then(|p| Regex::new(p).ok()) {
        if re.is_match(stripped) {
            return None;
        }
    }

    if let Some(prefix) = rule.prefix {
        if !stripped.starts_with(prefix) {
            return Some(CheckFailure {
                message_body: format!("have \"{prefix}\" prefix"),
            });
        }
    }
    if let Some(suffix) = rule.suffix {
        if !stripped.ends_with(suffix) {
            return Some(CheckFailure {
                message_body: format!("have \"{suffix}\" suffix"),
            });
        }
    }

    if let Some(pat) = rule.required_pattern {
        match Regex::new(pat) {
            Ok(re) if !re.is_match(stripped) => {
                return Some(CheckFailure {
                    message_body: format!("contain the required pattern: /{pat}/"),
                });
            }
            _ => {}
        }
    }

    for pat in rule.forbidden_patterns {
        if let Ok(re) = Regex::new(pat) {
            if re.is_match(stripped) {
                return Some(CheckFailure {
                    message_body: format!("not contain the forbidden pattern \"/{pat}/\""),
                });
            }
        }
    }

    if let Some(forbidden) = rule
        .forbidden_prefixes
        .iter()
        .find(|p| stripped.starts_with(p.as_str()))
    {
        return Some(CheckFailure {
            message_body: format!("not have \"{forbidden}\" prefix"),
        });
    }
    if let Some(forbidden) = rule
        .forbidden_suffixes
        .iter()
        .find(|s| stripped.ends_with(s.as_str()))
    {
        return Some(CheckFailure {
            message_body: format!("not have \"{forbidden}\" suffix"),
        });
    }

    if !rule.required_prefixes.is_empty()
        && !rule
            .required_prefixes
            .iter()
            .any(|p| stripped.starts_with(p.as_str()))
    {
        return Some(CheckFailure {
            message_body: format!(
                "have one of the following prefixes: {}",
                english_join(rule.required_prefixes)
            ),
        });
    }
    if !rule.required_suffixes.is_empty()
        && !rule
            .required_suffixes
            .iter()
            .any(|s| stripped.ends_with(s.as_str()))
    {
        return Some(CheckFailure {
            message_body: format!(
                "have one of the following suffixes: {}",
                english_join(rule.required_suffixes)
            ),
        });
    }

    if let Some(style) = rule.style {
        if !style.check(stripped) {
            return Some(CheckFailure {
                message_body: format!("be in {} format", style.label()),
            });
        }
    }

    None
}

/// Compose the full diagnostic message in upstream's exact form:
/// `${KindLabel} "${name}" should ${body}`. The kind label has its first
/// character uppercased (matching `${name[0].toUpperCase()}${name.slice(1)}`
/// in upstream).
fn format_message(kind_label: &str, name: &str, body: &str) -> String {
    let mut chars = kind_label.chars();
    let first = chars
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let rest: String = chars.collect();
    format!("{first}{rest} \"{name}\" should {body}")
}

/// Lint rule that enforces naming conventions for operations, fragments,
/// variables, and (on the schema side) types, fields, arguments, enum values,
/// and directives.
///
/// Like graphql-eslint, the rule no-ops with no options — each kind must be
/// explicitly configured.
///
/// ESLint-style selector keys (e.g. `"FieldDefinition[parent.name.value=Query]"`)
/// are accepted for drop-in config compatibility but no diagnostics are
/// emitted from them; selector parsing is a follow-up.
pub struct NamingConventionRuleImpl;

impl LintRule for NamingConventionRuleImpl {
    fn name(&self) -> &'static str {
        "namingConvention"
    }

    fn description(&self) -> &'static str {
        "Enforces naming conventions for operations, fragments, variables, and schema definitions"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for NamingConventionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let opts = NamingConventionOptions::from_json(options);
        let mut diagnostics = Vec::new();

        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        for doc in parse.documents() {
            let doc_cst = doc.tree.document();
            for definition in doc_cst.definitions() {
                match definition {
                    cst::Definition::OperationDefinition(op) => {
                        if let (Some(rule), Some(name_node)) =
                            (opts.operation_definition.as_ref(), op.name())
                        {
                            let normalized = NormalizedRule::from_rule(rule);
                            let name = name_node.text();
                            if let Some(failure) = check_name(&normalized, &name) {
                                let op_kind =
                                    op.operation_type().map_or(OperationKind::Query, |op_type| {
                                        get_operation_kind(&op_type)
                                    });
                                let op_label = match op_kind {
                                    OperationKind::Query => "query",
                                    OperationKind::Mutation => "mutation",
                                    OperationKind::Subscription => "subscription",
                                };
                                let start: usize = name_node.syntax().text_range().start().into();
                                let end: usize = name_node.syntax().text_range().end().into();
                                diagnostics.push(LintDiagnostic::new(
                                    doc.span(start, end),
                                    LintSeverity::Warning,
                                    format_message(op_label, &name, &failure.message_body),
                                    "namingConvention",
                                ));
                            }
                        }

                        if let Some(rule) = opts.variable.as_ref() {
                            let normalized = NormalizedRule::from_rule(rule);
                            if let Some(var_defs) = op.variable_definitions() {
                                for var_def in var_defs.variable_definitions() {
                                    if let Some(var) = var_def.variable() {
                                        if let Some(name_node) = var.name() {
                                            let name = name_node.text();
                                            if let Some(failure) = check_name(&normalized, &name) {
                                                let start: usize =
                                                    name_node.syntax().text_range().start().into();
                                                let end: usize =
                                                    name_node.syntax().text_range().end().into();
                                                diagnostics.push(LintDiagnostic::new(
                                                    doc.span(start, end),
                                                    LintSeverity::Warning,
                                                    format_message(
                                                        "variable",
                                                        &name,
                                                        &failure.message_body,
                                                    ),
                                                    "namingConvention",
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        if let (Some(rule), Some(frag_name)) = (
                            opts.fragment_definition.as_ref(),
                            frag.fragment_name().and_then(|fn_| fn_.name()),
                        ) {
                            let normalized = NormalizedRule::from_rule(rule);
                            let name = frag_name.text();
                            if let Some(failure) = check_name(&normalized, &name) {
                                let start: usize = frag_name.syntax().text_range().start().into();
                                let end: usize = frag_name.syntax().text_range().end().into();
                                diagnostics.push(LintDiagnostic::new(
                                    doc.span(start, end),
                                    LintSeverity::Warning,
                                    format_message("fragment", &name, &failure.message_body),
                                    "namingConvention",
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        diagnostics
    }
}

/// Schema-side enforcement. Walks every type definition (object, interface,
/// union, enum, scalar, input object), every field/input value, every
/// argument, every enum value, and every directive definition, applying the
/// matching per-kind rule (with the `types` umbrella as fallback for the
/// type-system kinds).
///
/// NOTE: this impl is not yet registered in `STANDALONE_SCHEMA_RULES` — the
/// registry change is a follow-up. Schema-side enforcement is reachable via
/// `StandaloneSchemaLintRule::check` directly (and via the unit tests below).
impl StandaloneSchemaLintRule for NamingConventionRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = NamingConventionOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        // Source schema file ids — used to filter out builtins and resolved-schema
        // entries that the user didn't write themselves.
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
            if !source_file_ids.contains(&type_def.file_id) {
                continue;
            }

            // Type/interface/enum/union/scalar/input own name (uses `types`
            // umbrella as fallback).
            if let Some(rule) = opts.rule_for_type_kind(type_def.kind) {
                let normalized = NormalizedRule::from_rule(rule);
                if let Some(failure) = check_name(&normalized, &type_def.name) {
                    let kind_label = match type_def.kind {
                        TypeDefKind::Interface => "interface",
                        TypeDefKind::Enum => "enum",
                        TypeDefKind::Union => "union",
                        TypeDefKind::Scalar => "scalar",
                        TypeDefKind::InputObject => "input",
                        // `Object` plus future `#[non_exhaustive]` variants.
                        _ => "type",
                    };
                    push_diagnostic(
                        &mut diagnostics_by_file,
                        type_def.file_id,
                        type_def.name_range,
                        format_message(kind_label, &type_def.name, &failure.message_body),
                    );
                }
            }

            // Field definitions (object/interface) and input value definitions
            // (input object). The two share `FieldSignature` in our HIR, but
            // upstream uses different ESLint kinds for them.
            for field in &type_def.fields {
                if !source_file_ids.contains(&field.file_id) {
                    continue;
                }

                let (field_rule, field_label) = if type_def.kind == TypeDefKind::InputObject {
                    (opts.input_value_definition.as_ref(), "input value")
                } else {
                    (opts.field_definition.as_ref(), "field")
                };

                if let Some(rule) = field_rule {
                    let normalized = NormalizedRule::from_rule(rule);
                    if let Some(failure) = check_name(&normalized, &field.name) {
                        push_diagnostic(
                            &mut diagnostics_by_file,
                            field.file_id,
                            field.name_range,
                            format_message(field_label, &field.name, &failure.message_body),
                        );
                    }
                }

                // Field arguments use the `Argument` slot. (Upstream's `Argument`
                // selector applies to argument definitions on field definitions.)
                if type_def.kind != TypeDefKind::InputObject {
                    if let Some(rule) = opts.argument.as_ref() {
                        let normalized = NormalizedRule::from_rule(rule);
                        for arg in &field.arguments {
                            if !source_file_ids.contains(&arg.file_id) {
                                continue;
                            }
                            if let Some(failure) = check_name(&normalized, &arg.name) {
                                push_diagnostic(
                                    &mut diagnostics_by_file,
                                    arg.file_id,
                                    arg.name_range,
                                    format_message("argument", &arg.name, &failure.message_body),
                                );
                            }
                        }
                    }
                }
            }

            // Enum values
            if let Some(rule) = opts.enum_value_definition.as_ref() {
                if type_def.kind == TypeDefKind::Enum {
                    let normalized = NormalizedRule::from_rule(rule);
                    for value in &type_def.enum_values {
                        if let Some(failure) = check_name(&normalized, &value.name) {
                            push_diagnostic(
                                &mut diagnostics_by_file,
                                type_def.file_id,
                                value.name_range,
                                format_message("enum value", &value.name, &failure.message_body),
                            );
                        }
                    }
                }
            }
        }

        // Directive definitions and their arguments.
        let directives = graphql_hir::schema_directives(db, project_files);
        for dir_def in directives.values() {
            if !source_file_ids.contains(&dir_def.file_id) {
                continue;
            }
            if let Some(rule) = opts.directive_definition.as_ref() {
                let normalized = NormalizedRule::from_rule(rule);
                if let Some(failure) = check_name(&normalized, &dir_def.name) {
                    push_diagnostic(
                        &mut diagnostics_by_file,
                        dir_def.file_id,
                        dir_def.name_range,
                        format_message("directive", &dir_def.name, &failure.message_body),
                    );
                }
            }
            if let Some(rule) = opts.argument.as_ref() {
                let normalized = NormalizedRule::from_rule(rule);
                for arg in &dir_def.arguments {
                    if !source_file_ids.contains(&arg.file_id) {
                        continue;
                    }
                    if let Some(failure) = check_name(&normalized, &arg.name) {
                        push_diagnostic(
                            &mut diagnostics_by_file,
                            arg.file_id,
                            arg.name_range,
                            format_message("argument", &arg.name, &failure.message_body),
                        );
                    }
                }
            }
        }

        diagnostics_by_file
    }
}

fn push_diagnostic(
    diagnostics_by_file: &mut HashMap<FileId, Vec<LintDiagnostic>>,
    file_id: FileId,
    name_range: TextRange,
    message: String,
) {
    let span = graphql_syntax::SourceSpan {
        start: name_range.start().into(),
        end: name_range.end().into(),
        line_offset: 0,
        byte_offset: 0,
        source: None,
    };
    diagnostics_by_file
        .entry(file_id)
        .or_default()
        .push(LintDiagnostic::new(
            span,
            LintSeverity::Warning,
            message,
            "namingConvention",
        ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{StandaloneDocumentLintRule, StandaloneSchemaLintRule};
    use graphql_base_db::{
        DocumentFileIds, DocumentKind, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata,
        FileUri, Language, SchemaFileIds,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map = FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
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

    fn check_with_options(
        source: &str,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = NamingConventionRuleImpl;
        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);
        StandaloneDocumentLintRule::check(
            &rule,
            &db,
            file_id,
            content,
            metadata,
            project_files,
            options,
        )
    }

    fn check(source: &str) -> Vec<LintDiagnostic> {
        check_with_options(source, None)
    }

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

    fn schema_messages(diags: &HashMap<FileId, Vec<LintDiagnostic>>) -> Vec<String> {
        diags
            .values()
            .flatten()
            .map(|d| d.message.clone())
            .collect()
    }

    fn check_schema(
        schema: &str,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let db = RootDatabase::default();
        let project_files = create_schema_project(&db, schema);
        StandaloneSchemaLintRule::check(&NamingConventionRuleImpl, &db, project_files, options)
    }

    #[test]
    fn test_no_options_is_noop() {
        let diagnostics = check("query lowercaseOp { user { id } }");
        assert!(diagnostics.is_empty());
        let diagnostics = check("fragment lowercase_frag on User { id }");
        assert!(diagnostics.is_empty());
        let diagnostics = check("query Q($Bad: ID!) { user(id: $Bad) { id } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_valid_operation_name() {
        let opts = serde_json::json!({ "OperationDefinition": "PascalCase" });
        let diagnostics = check_with_options("query GetUser { user { id } }", Some(&opts));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_operation_name() {
        let opts = serde_json::json!({ "OperationDefinition": "PascalCase" });
        let diagnostics = check_with_options("query get_user { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"get_user\" should be in PascalCase format"
        );
    }

    #[test]
    fn test_valid_fragment_name() {
        let opts = serde_json::json!({ "FragmentDefinition": "PascalCase" });
        let diagnostics = check_with_options("fragment UserFields on User { id }", Some(&opts));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_fragment_name() {
        let opts = serde_json::json!({ "FragmentDefinition": "PascalCase" });
        let diagnostics = check_with_options("fragment user_fields on User { id }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Fragment \"user_fields\" should be in PascalCase format"
        );
    }

    #[test]
    fn test_valid_variable_name() {
        let opts = serde_json::json!({ "VariableDefinition": "camelCase" });
        let diagnostics = check_with_options(
            "query Q($userId: ID!) { user(id: $userId) { id } }",
            Some(&opts),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_variable_name() {
        let opts = serde_json::json!({ "VariableDefinition": "camelCase" });
        let diagnostics = check_with_options(
            "query Q($UserId: ID!) { user(id: $UserId) { id } }",
            Some(&opts),
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Variable \"UserId\" should be in camelCase format"
        );
    }

    #[test]
    fn test_object_form_with_style() {
        let opts = serde_json::json!({
            "OperationDefinition": {
                "style": "PascalCase",
                "forbiddenPrefixes": ["Query"],
                "forbiddenSuffixes": ["Query"]
            }
        });
        let diagnostics = check_with_options("query lowercaseOp { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("PascalCase"));
    }

    #[test]
    fn test_selector_keys_dont_break_deserialization() {
        let opts = serde_json::json!({
            "FieldDefinition[parent.name.value=Query]": {
                "forbiddenPrefixes": ["query"]
            },
            "EnumTypeDefinition,EnumTypeExtension": {
                "forbiddenPrefixes": ["Enum"]
            }
        });
        let diagnostics = check_with_options("query GetUser { user { id } }", Some(&opts));
        assert!(diagnostics.is_empty());
    }

    // ----- new prefix/suffix/pattern coverage on the document side -----

    #[test]
    fn test_forbidden_prefix() {
        let opts = serde_json::json!({
            "OperationDefinition": { "forbiddenPrefixes": ["Get"] }
        });
        let diagnostics = check_with_options("query GetUser { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"GetUser\" should not have \"Get\" prefix"
        );
    }

    #[test]
    fn test_forbidden_suffix() {
        let opts = serde_json::json!({
            "OperationDefinition": { "forbiddenSuffixes": ["Query"] }
        });
        let diagnostics = check_with_options("query GetUserQuery { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"GetUserQuery\" should not have \"Query\" suffix"
        );
    }

    #[test]
    fn test_required_prefix_singular() {
        let opts = serde_json::json!({
            "OperationDefinition": { "prefix": "Get" }
        });
        let diagnostics = check_with_options("query FetchUser { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"FetchUser\" should have \"Get\" prefix"
        );
    }

    #[test]
    fn test_required_suffix_singular() {
        let opts = serde_json::json!({
            "OperationDefinition": { "suffix": "Query" }
        });
        let diagnostics = check_with_options("query GetUser { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"GetUser\" should have \"Query\" suffix"
        );
    }

    #[test]
    fn test_required_prefixes_list_message() {
        let opts = serde_json::json!({
            "OperationDefinition": { "requiredPrefixes": ["Get", "List"] }
        });
        let diagnostics = check_with_options("query FetchUser { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"FetchUser\" should have one of the following prefixes: Get or List"
        );
    }

    #[test]
    fn test_required_suffixes_three_items_uses_or() {
        let opts = serde_json::json!({
            "OperationDefinition": { "requiredSuffixes": ["Query", "Mutation", "Subscription"] }
        });
        let diagnostics = check_with_options("query GetUser { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"GetUser\" should have one of the following suffixes: Query, Mutation, or Subscription"
        );
    }

    #[test]
    fn test_required_pattern() {
        let opts = serde_json::json!({
            "OperationDefinition": { "requiredPattern": "^Get" }
        });
        let diagnostics = check_with_options("query FetchUser { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"FetchUser\" should contain the required pattern: /^Get/"
        );
    }

    #[test]
    fn test_forbidden_patterns() {
        let opts = serde_json::json!({
            "OperationDefinition": { "forbiddenPatterns": ["^Get"] }
        });
        let diagnostics = check_with_options("query GetUser { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Query \"GetUser\" should not contain the forbidden pattern \"/^Get/\""
        );
    }

    #[test]
    fn test_ignore_pattern_skips_check() {
        let opts = serde_json::json!({
            "OperationDefinition": {
                "style": "camelCase",
                "ignorePattern": "^EAN13"
            }
        });
        // Operation name fails camelCase but matches ignorePattern → no diag.
        let diagnostics = check_with_options("query EAN13 { user { id } }", Some(&opts));
        assert!(diagnostics.is_empty(), "got: {:?}", diagnostics);
    }

    #[test]
    fn test_allow_leading_underscore_strips_before_case_check() {
        let opts = serde_json::json!({
            "OperationDefinition": {
                "style": "PascalCase",
                "allowLeadingUnderscore": true
            }
        });
        // After stripping the leading `_`, `_GetUser` → `GetUser`, valid PascalCase.
        let diagnostics = check_with_options("query _GetUser { user { id } }", Some(&opts));
        assert!(diagnostics.is_empty(), "got: {:?}", diagnostics);
    }

    #[test]
    fn test_allow_trailing_underscore_strips_before_case_check() {
        let opts = serde_json::json!({
            "OperationDefinition": {
                "style": "PascalCase",
                "allowTrailingUnderscore": true
            }
        });
        let diagnostics = check_with_options("query GetUser_ { user { id } }", Some(&opts));
        assert!(diagnostics.is_empty(), "got: {:?}", diagnostics);
    }

    #[test]
    fn test_disallow_leading_underscore_when_not_opted_in() {
        // Without `allowLeadingUnderscore`, the leading `_` is part of the
        // name and PascalCase still rejects it (first char must be uppercase).
        let opts = serde_json::json!({
            "OperationDefinition": "PascalCase"
        });
        let diagnostics = check_with_options("query _GetUser { user { id } }", Some(&opts));
        assert_eq!(diagnostics.len(), 1);
    }

    // ----- schema-side tests -----

    #[test]
    fn test_schema_field_definition_camelcase() {
        let opts = serde_json::json!({ "FieldDefinition": "camelCase" });
        let diags = check_schema("type User { first_name: String }\n", Some(&opts));
        let msgs = schema_messages(&diags);
        assert!(
            msgs.contains(&"Field \"first_name\" should be in camelCase format".to_string()),
            "got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_object_type_pascalcase() {
        let opts = serde_json::json!({ "ObjectTypeDefinition": "PascalCase" });
        let diags = check_schema("type user { id: ID! }\n", Some(&opts));
        let msgs = schema_messages(&diags);
        assert!(
            msgs.contains(&"Type \"user\" should be in PascalCase format".to_string()),
            "got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_enum_value_upper_case() {
        let opts = serde_json::json!({ "EnumValueDefinition": "UPPER_CASE" });
        let diags = check_schema("enum Color { red GREEN }\n", Some(&opts));
        let msgs = schema_messages(&diags);
        assert!(
            msgs.iter()
                .any(|m| m == "Enum value \"red\" should be in UPPER_CASE format"),
            "got: {:?}",
            msgs
        );
        assert!(
            !msgs.iter().any(|m| m.contains("\"GREEN\"")),
            "GREEN should pass UPPER_CASE; got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_input_value_definition() {
        let opts = serde_json::json!({ "InputValueDefinition": "camelCase" });
        let diags = check_schema("input UserFilter { ID_field: String }\n", Some(&opts));
        let msgs = schema_messages(&diags);
        assert!(
            msgs.contains(&"Input value \"ID_field\" should be in camelCase format".to_string()),
            "got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_argument() {
        let opts = serde_json::json!({ "Argument": "camelCase" });
        let diags = check_schema("type Query { user(User_Id: ID!): String }\n", Some(&opts));
        let msgs = schema_messages(&diags);
        assert!(
            msgs.contains(&"Argument \"User_Id\" should be in camelCase format".to_string()),
            "got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_directive_definition() {
        let opts = serde_json::json!({ "DirectiveDefinition": "camelCase" });
        let diags = check_schema("directive @My_Cool_Dir on FIELD_DEFINITION\n", Some(&opts));
        let msgs = schema_messages(&diags);
        assert!(
            msgs.contains(&"Directive \"My_Cool_Dir\" should be in camelCase format".to_string()),
            "got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_interface_union_scalar_input_each_check() {
        let opts = serde_json::json!({
            "InterfaceTypeDefinition": "PascalCase",
            "UnionTypeDefinition": "PascalCase",
            "ScalarTypeDefinition": "PascalCase",
            "InputObjectTypeDefinition": "PascalCase"
        });
        let schema = r"
            interface my_iface { id: ID! }
            type A implements my_iface { id: ID! }
            type B { id: ID! }
            union my_union = A | B
            scalar my_scalar
            input my_input { id: ID! }
        ";
        let diags = check_schema(schema, Some(&opts));
        let msgs = schema_messages(&diags);
        assert!(
            msgs.iter()
                .any(|m| m == "Interface \"my_iface\" should be in PascalCase format"),
            "got: {:?}",
            msgs
        );
        assert!(
            msgs.iter()
                .any(|m| m == "Union \"my_union\" should be in PascalCase format"),
            "got: {:?}",
            msgs
        );
        assert!(
            msgs.iter()
                .any(|m| m == "Scalar \"my_scalar\" should be in PascalCase format"),
            "got: {:?}",
            msgs
        );
        assert!(
            msgs.iter()
                .any(|m| m == "Input \"my_input\" should be in PascalCase format"),
            "got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_types_umbrella_applies_to_all_unmapped_kinds() {
        let opts = serde_json::json!({ "types": "PascalCase" });
        let schema = r"
            type good_type { id: ID! }
            interface good_iface { id: ID! }
            enum good_enum { A B }
            scalar good_scalar
            input good_input { id: ID! }
        ";
        let diags = check_schema(schema, Some(&opts));
        let msgs = schema_messages(&diags);
        for expected_kind in ["Type", "Interface", "Enum", "Scalar", "Input"] {
            assert!(
                msgs.iter()
                    .any(|m| m.starts_with(expected_kind) && m.contains("PascalCase")),
                "expected umbrella diagnostic for {expected_kind}, got: {:?}",
                msgs
            );
        }
    }

    #[test]
    fn test_schema_explicit_override_wins_over_types_umbrella() {
        // umbrella says PascalCase but ObjectTypeDefinition explicitly allows
        // snake_case for object types only.
        let opts = serde_json::json!({
            "types": "PascalCase",
            "ObjectTypeDefinition": "snake_case"
        });
        let schema = r"
            type my_type { id: ID! }
            interface my_iface { id: ID! }
        ";
        let diags = check_schema(schema, Some(&opts));
        let msgs = schema_messages(&diags);
        // `my_type` passes snake_case → no diag.
        assert!(
            !msgs.iter().any(|m| m.contains("\"my_type\"")),
            "object should pass override, got: {:?}",
            msgs
        );
        // `my_iface` falls through to umbrella (PascalCase) → diag.
        assert!(
            msgs.iter()
                .any(|m| m == "Interface \"my_iface\" should be in PascalCase format"),
            "interface should fall through to umbrella, got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_field_forbidden_prefix() {
        let opts = serde_json::json!({
            "FieldDefinition": { "forbiddenPrefixes": ["get"] }
        });
        let diags = check_schema("type User { getName: String }\n", Some(&opts));
        let msgs = schema_messages(&diags);
        assert!(
            msgs.contains(&"Field \"getName\" should not have \"get\" prefix".to_string()),
            "got: {:?}",
            msgs
        );
    }

    #[test]
    fn test_schema_ignore_pattern_skips() {
        let opts = serde_json::json!({
            "FieldDefinition": {
                "style": "camelCase",
                "ignorePattern": "^(EAN13|UPC)"
            }
        });
        let schema = "type Product { EAN13: String UPC: String NotIgnored: String }\n";
        let diags = check_schema(schema, Some(&opts));
        let msgs = schema_messages(&diags);
        // EAN13/UPC ignored, NotIgnored fails camelCase.
        assert_eq!(msgs.len(), 1, "got: {:?}", msgs);
        assert_eq!(
            msgs[0],
            "Field \"NotIgnored\" should be in camelCase format"
        );
    }
}
