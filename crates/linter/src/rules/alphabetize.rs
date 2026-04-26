use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::traits::{LintRule, StandaloneDocumentLintRule, StandaloneSchemaLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use serde::Deserialize;
use std::collections::HashMap;

/// Selection-set owners that `alphabetize.selections` may restrict to. Mirrors
/// graphql-eslint's `selectionsEnum` (`OperationDefinition`, `FragmentDefinition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SelectionsOwner {
    OperationDefinition,
    FragmentDefinition,
}

/// `selections` accepts either a boolean (legacy) or an array of owner kinds
/// (matching graphql-eslint). `true` is treated as "both owner kinds enabled".
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SelectionsConfig {
    Bool(bool),
    Owners(Vec<SelectionsOwner>),
}

impl SelectionsConfig {
    fn includes(&self, owner: SelectionsOwner) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Owners(list) => list.contains(&owner),
        }
    }
}

impl Default for SelectionsConfig {
    fn default() -> Self {
        Self::Bool(true)
    }
}

/// Tri-state acceptor for graphql-eslint options that historically were
/// booleans and now accept an array of AST kind names. We accept all three
/// shapes so upstream's preset configs deserialize without error:
///
/// - `false` → disabled
/// - `true` → enabled (legacy bool form)
/// - `["FieldDefinition", "Field", ...]` → enabled and scoped to the listed
///   AST kinds (full per-kind filtering is `PARITY_TODO` item 4c — for now we
///   treat any non-empty list as "on" and ignore the kind filter).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum BoolOrKindList {
    Bool(bool),
    Kinds(Vec<String>),
}

impl Default for BoolOrKindList {
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl BoolOrKindList {
    fn enabled(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Kinds(list) => !list.is_empty(),
        }
    }

    /// True when the option is enabled and the given AST kind name is included.
    /// `Bool(true)` matches every kind; `Kinds([...])` matches only listed kinds.
    fn includes_kind(&self, kind: &str) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Kinds(list) => list.iter().any(|k| k == kind),
        }
    }
}

/// Options for the `alphabetize` rule
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AlphabetizeOptions {
    /// Check selection sets for alphabetical order. Either a boolean or an
    /// array of selection-set owner kinds (`OperationDefinition`,
    /// `FragmentDefinition`).
    pub selections: SelectionsConfig,
    /// Check arguments for alphabetical order. Accepts both the legacy
    /// boolean form and the array form upstream uses in `flat/schema-all` /
    /// `flat/operations-all` (`["FieldDefinition", "Field", ...]`); per-kind
    /// filtering of the array form is `PARITY_TODO` item 4c.
    pub arguments: BoolOrKindList,
    /// Check variable definitions for alphabetical order
    pub variables: bool,
    /// Check top-level definitions for alphabetical order across the whole
    /// document (matches graphql-eslint's `definitions` listener on
    /// `Document.definitions`). Schema-side only.
    #[serde(default)]
    pub definitions: bool,
    /// Check field declarations in the listed type kinds. `Bool(true)`
    /// enables all of `ObjectTypeDefinition`, `InterfaceTypeDefinition`,
    /// `InputObjectTypeDefinition`; the array form narrows to the listed
    /// kinds (and their corresponding `*Extension` kinds, mirroring
    /// graphql-eslint).
    #[serde(default)]
    pub fields: BoolOrKindList,
    /// Check enum value declarations for alphabetical order. Fires on
    /// `EnumTypeDefinition` and `EnumTypeExtension`.
    #[serde(default)]
    pub values: bool,
    /// Explicit ordering groups (e.g. `["id", "*", "createdAt"]`). When
    /// non-empty, items are sorted by group rank first (lower index =
    /// earlier), with within-group ties broken alphabetically. `"*"` is
    /// the catch-all bucket for names not otherwise listed.
    ///
    /// graphql-eslint also recognizes `"..."` (fragment spread bucket) and
    /// `"{"` (nodes with a selection set) on the operations side. Those
    /// match the document-side `check_selection_set_order` path; the
    /// schema-side helpers don't see fragment spreads or selection sets,
    /// so only `"*"` and named groups are relevant here.
    #[serde(default)]
    pub groups: Vec<String>,
}

impl AlphabetizeOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    fn check_owner(&self, owner: SelectionsOwner) -> bool {
        self.selections.includes(owner)
    }
}

/// Lint rule that enforces alphabetical ordering of selections, arguments, and variables
pub struct AlphabetizeRuleImpl;

impl LintRule for AlphabetizeRuleImpl {
    fn name(&self) -> &'static str {
        "alphabetize"
    }

    fn description(&self) -> &'static str {
        "Enforces alphabetical ordering of fields, arguments, and variables"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for AlphabetizeRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let opts = AlphabetizeOptions::from_json(options);
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
                        if opts.variables {
                            if let Some(var_defs) = op.variable_definitions() {
                                check_variable_order(&var_defs, &doc, &mut diagnostics);
                            }
                        }
                        let scan = opts.check_owner(SelectionsOwner::OperationDefinition);
                        if let Some(selection_set) = op.selection_set() {
                            check_selection_set_order(
                                &selection_set,
                                &opts,
                                scan,
                                &doc,
                                &mut diagnostics,
                            );
                        }
                    }
                    cst::Definition::FragmentDefinition(frag) => {
                        let scan = opts.check_owner(SelectionsOwner::FragmentDefinition);
                        if let Some(selection_set) = frag.selection_set() {
                            check_selection_set_order(
                                &selection_set,
                                &opts,
                                scan,
                                &doc,
                                &mut diagnostics,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        diagnostics
    }
}

impl StandaloneSchemaLintRule for AlphabetizeRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = AlphabetizeOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        if !opts.definitions && !opts.fields.enabled() && !opts.values {
            return diagnostics_by_file;
        }

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
                let mut local_diagnostics: Vec<LintDiagnostic> = Vec::new();
                check_schema_document(&doc, &opts, &mut local_diagnostics);
                if !local_diagnostics.is_empty() {
                    diagnostics_by_file
                        .entry(*file_id)
                        .or_default()
                        .extend(local_diagnostics);
                }
            }
        }

        diagnostics_by_file
    }
}

/// Returns `(kind_label, name_node)` for each top-level definition that has
/// a name. `Some` means it's orderable; `None` (e.g. anonymous schema
/// definition) is skipped for ordering purposes.
fn definition_label_and_name(definition: &cst::Definition) -> Option<(&'static str, cst::Name)> {
    match definition {
        cst::Definition::ObjectTypeDefinition(d) => Some(("type", d.name()?)),
        cst::Definition::ObjectTypeExtension(d) => Some(("type", d.name()?)),
        cst::Definition::InterfaceTypeDefinition(d) => Some(("interface", d.name()?)),
        cst::Definition::InterfaceTypeExtension(d) => Some(("interface", d.name()?)),
        cst::Definition::UnionTypeDefinition(d) => Some(("union", d.name()?)),
        cst::Definition::UnionTypeExtension(d) => Some(("union", d.name()?)),
        cst::Definition::EnumTypeDefinition(d) => Some(("enum", d.name()?)),
        cst::Definition::EnumTypeExtension(d) => Some(("enum", d.name()?)),
        cst::Definition::ScalarTypeDefinition(d) => Some(("scalar", d.name()?)),
        cst::Definition::ScalarTypeExtension(d) => Some(("scalar", d.name()?)),
        cst::Definition::InputObjectTypeDefinition(d) => Some(("input", d.name()?)),
        cst::Definition::InputObjectTypeExtension(d) => Some(("input", d.name()?)),
        cst::Definition::DirectiveDefinition(d) => Some(("directive", d.name()?)),
        cst::Definition::OperationDefinition(d) => {
            // OperationDefinitions can appear in mixed schemas; an anonymous
            // operation has no name to order by.
            Some(("operation", d.name()?))
        }
        cst::Definition::FragmentDefinition(d) => Some(("fragment", d.fragment_name()?.name()?)),
        cst::Definition::SchemaDefinition(_) | cst::Definition::SchemaExtension(_) => None,
    }
}

fn check_schema_document(
    doc: &graphql_syntax::DocumentRef<'_>,
    opts: &AlphabetizeOptions,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let doc_cst = doc.tree.document();

    // 1. Top-level definition ordering. Track each definition's full byte
    // range so misordered pairs get a swap fix matching graphql-eslint's.
    if opts.definitions {
        let mut prev: Option<(String, &'static str, (usize, usize))> = None;
        for definition in doc_cst.definitions() {
            let Some((kind_label, name_node)) = definition_label_and_name(&definition) else {
                continue;
            };
            let curr_name = name_node.text().to_string();
            let curr_range = {
                let r = definition.syntax().text_range();
                let s: usize = r.start().into();
                let e: usize = r.end().into();
                (s, e)
            };
            if let Some((ref prev_name, prev_kind, prev_range)) = prev {
                if is_misordered(&curr_name, prev_name, &opts.groups) {
                    let fix = swap_fix(doc.source, prev_range, curr_range);
                    push_alphabetize_diagnostic_with_fix(
                        doc,
                        diagnostics,
                        &name_node,
                        kind_label,
                        &curr_name,
                        prev_kind,
                        prev_name,
                        Some(fix),
                    );
                }
            }
            prev = Some((curr_name, kind_label, curr_range));
        }
    }

    // 2. Per-kind field / enum-value ordering inside each definition.
    for definition in doc_cst.definitions() {
        match definition {
            cst::Definition::ObjectTypeDefinition(d) => {
                if opts.fields.includes_kind("ObjectTypeDefinition") {
                    if let Some(fd) = d.fields_definition() {
                        check_field_definition_order(&fd, &opts.groups, doc, diagnostics);
                    }
                }
            }
            cst::Definition::ObjectTypeExtension(d) => {
                if opts.fields.includes_kind("ObjectTypeDefinition") {
                    if let Some(fd) = d.fields_definition() {
                        check_field_definition_order(&fd, &opts.groups, doc, diagnostics);
                    }
                }
            }
            cst::Definition::InterfaceTypeDefinition(d) => {
                if opts.fields.includes_kind("InterfaceTypeDefinition") {
                    if let Some(fd) = d.fields_definition() {
                        check_field_definition_order(&fd, &opts.groups, doc, diagnostics);
                    }
                }
            }
            cst::Definition::InterfaceTypeExtension(d) => {
                if opts.fields.includes_kind("InterfaceTypeDefinition") {
                    if let Some(fd) = d.fields_definition() {
                        check_field_definition_order(&fd, &opts.groups, doc, diagnostics);
                    }
                }
            }
            cst::Definition::InputObjectTypeDefinition(d) => {
                if opts.fields.includes_kind("InputObjectTypeDefinition") {
                    if let Some(fd) = d.input_fields_definition() {
                        check_input_value_definition_order(
                            fd.input_value_definitions(),
                            "input value",
                            &opts.groups,
                            doc,
                            diagnostics,
                        );
                    }
                }
            }
            cst::Definition::InputObjectTypeExtension(d) => {
                if opts.fields.includes_kind("InputObjectTypeDefinition") {
                    if let Some(fd) = d.input_fields_definition() {
                        check_input_value_definition_order(
                            fd.input_value_definitions(),
                            "input value",
                            &opts.groups,
                            doc,
                            diagnostics,
                        );
                    }
                }
            }
            cst::Definition::EnumTypeDefinition(d) => {
                if opts.values {
                    if let Some(values) = d.enum_values_definition() {
                        check_enum_value_order(&values, &opts.groups, doc, diagnostics);
                    }
                }
            }
            cst::Definition::EnumTypeExtension(d) => {
                if opts.values {
                    if let Some(values) = d.enum_values_definition() {
                        check_enum_value_order(&values, &opts.groups, doc, diagnostics);
                    }
                }
            }
            _ => {}
        }
    }
}

fn check_field_definition_order(
    fields: &cst::FieldsDefinition,
    groups: &[String],
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut prev: Option<(String, (usize, usize))> = None;
    for field in fields.field_definitions() {
        let Some(name_node) = field.name() else {
            continue;
        };
        let name = name_node.text().to_string();
        let curr_range = {
            let r = field.syntax().text_range();
            let s: usize = r.start().into();
            let e: usize = r.end().into();
            (s, e)
        };
        if let Some((ref prev_name, prev_range)) = prev {
            if is_misordered(&name, prev_name, groups) {
                let fix = swap_fix(doc.source, prev_range, curr_range);
                push_alphabetize_diagnostic_with_fix(
                    doc,
                    diagnostics,
                    &name_node,
                    "field",
                    &name,
                    "field",
                    prev_name,
                    Some(fix),
                );
            }
        }
        prev = Some((name, curr_range));
    }
}

fn check_input_value_definition_order(
    values: cst::CstChildren<cst::InputValueDefinition>,
    label: &'static str,
    groups: &[String],
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut prev: Option<(String, (usize, usize))> = None;
    for value in values {
        let Some(name_node) = value.name() else {
            continue;
        };
        let name = name_node.text().to_string();
        let curr_range = {
            let r = value.syntax().text_range();
            let s: usize = r.start().into();
            let e: usize = r.end().into();
            (s, e)
        };
        if let Some((ref prev_name, prev_range)) = prev {
            if is_misordered(&name, prev_name, groups) {
                let fix = swap_fix(doc.source, prev_range, curr_range);
                push_alphabetize_diagnostic_with_fix(
                    doc,
                    diagnostics,
                    &name_node,
                    label,
                    &name,
                    label,
                    prev_name,
                    Some(fix),
                );
            }
        }
        prev = Some((name, curr_range));
    }
}

fn check_enum_value_order(
    values: &cst::EnumValuesDefinition,
    groups: &[String],
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut prev: Option<(String, (usize, usize))> = None;
    for value in values.enum_value_definitions() {
        let Some(enum_value) = value.enum_value() else {
            continue;
        };
        let Some(name_node) = enum_value.name() else {
            continue;
        };
        let name = name_node.text().to_string();
        let curr_range = {
            let r = value.syntax().text_range();
            let s: usize = r.start().into();
            let e: usize = r.end().into();
            (s, e)
        };
        if let Some((ref prev_name, prev_range)) = prev {
            if is_misordered(&name, prev_name, groups) {
                let fix = swap_fix(doc.source, prev_range, curr_range);
                push_alphabetize_diagnostic_with_fix(
                    doc,
                    diagnostics,
                    &name_node,
                    "enum value",
                    &name,
                    "enum value",
                    prev_name,
                    Some(fix),
                );
            }
        }
        prev = Some((name, curr_range));
    }
}

#[allow(clippy::too_many_arguments)]
fn push_alphabetize_diagnostic_with_fix(
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
    name_node: &cst::Name,
    curr_kind: &str,
    curr_name: &str,
    prev_kind: &str,
    prev_name: &str,
    fix: Option<CodeFix>,
) {
    let start: usize = name_node.syntax().text_range().start().into();
    let end: usize = name_node.syntax().text_range().end().into();
    let mut diag = LintDiagnostic::new(
        doc.span(start, end),
        LintSeverity::Warning,
        format!("{curr_kind} \"{curr_name}\" should be before {prev_kind} \"{prev_name}\""),
        "alphabetize",
    )
    .with_message_id("alphabetize")
    .with_help("Reorder alphabetically by name");
    if let Some(fix) = fix {
        diag = diag.with_fix(fix);
    }
    diagnostics.push(diag);
}

#[allow(dead_code)]
fn push_alphabetize_diagnostic(
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
    name_node: &cst::Name,
    curr_kind: &str,
    curr_name: &str,
    prev_kind: &str,
    prev_name: &str,
) {
    let start: usize = name_node.syntax().text_range().start().into();
    let end: usize = name_node.syntax().text_range().end().into();
    diagnostics.push(
        LintDiagnostic::new(
            doc.span(start, end),
            LintSeverity::Warning,
            format!("{curr_kind} \"{curr_name}\" should be before {prev_kind} \"{prev_name}\""),
            "alphabetize",
        )
        .with_message_id("alphabetize")
        .with_help("Reorder alphabetically by name"),
    );
}

#[derive(Debug, Clone, Copy)]
enum SelectionKind {
    Field,
    FragmentSpread,
}

impl SelectionKind {
    fn label(self) -> &'static str {
        match self {
            SelectionKind::Field => "field",
            SelectionKind::FragmentSpread => "fragment spread",
        }
    }
}

fn check_selection_set_order(
    selection_set: &cst::SelectionSet,
    opts: &AlphabetizeOptions,
    scan_selections: bool,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if scan_selections {
        // Track the previous selection's full byte range too so we can emit a
        // graphql-eslint-compatible swap fix.
        let mut last: Option<(String, SelectionKind, (usize, usize))> = None;

        for selection in selection_set.selections() {
            let current = match &selection {
                cst::Selection::Field(field) => field
                    .alias()
                    .and_then(|a| a.name())
                    .or_else(|| field.name())
                    .map(|n| (n.text().to_string(), SelectionKind::Field)),
                cst::Selection::FragmentSpread(spread) => spread
                    .fragment_name()
                    .and_then(|fn_| fn_.name())
                    .map(|n| (n.text().to_string(), SelectionKind::FragmentSpread)),
                cst::Selection::InlineFragment(_) => None, // Inline fragments don't have a name to order by
            };

            // Full byte range of the entire current selection — used both as
            // the diagnostic anchor's containing range and as half of the
            // swap fix.
            let curr_full_range = {
                let r = selection.syntax().text_range();
                let s: usize = r.start().into();
                let e: usize = r.end().into();
                (s, e)
            };

            if let Some((name, curr_kind)) = current {
                if let Some((prev_name, prev_kind, prev_full_range)) = &last {
                    if name.to_lowercase() < prev_name.to_lowercase() {
                        let start_offset = match &selection {
                            cst::Selection::Field(f) => f
                                .alias()
                                .and_then(|a| a.name())
                                .or_else(|| f.name())
                                .map(|n| {
                                    let s: usize = n.syntax().text_range().start().into();
                                    let e: usize = n.syntax().text_range().end().into();
                                    (s, e)
                                }),
                            cst::Selection::FragmentSpread(s) => {
                                s.fragment_name().and_then(|fn_| fn_.name()).map(|n| {
                                    let s: usize = n.syntax().text_range().start().into();
                                    let e: usize = n.syntax().text_range().end().into();
                                    (s, e)
                                })
                            }
                            cst::Selection::InlineFragment(_) => None,
                        };

                        if let Some((start, end)) = start_offset {
                            // Build the swap fix matching graphql-eslint:
                            // replace [prev.start, curr.end] with
                            // <curr_text><whitespace_between><prev_text>.
                            let fix = swap_fix(doc.source, *prev_full_range, curr_full_range);

                            diagnostics.push(
                                LintDiagnostic::new(
                                    doc.span(start, end),
                                    LintSeverity::Warning,
                                    format!(
                                        "{curr_label} \"{name}\" should be before {prev_label} \"{prev_name}\"",
                                        curr_label = curr_kind.label(),
                                        prev_label = prev_kind.label(),
                                    ),
                                    "alphabetize",
                                )
                                .with_message_id("alphabetize")
                                .with_fix(fix)
                                .with_help(
                                    "Reorder selections alphabetically by their response name",
                                ),
                            );
                        }
                    }
                }
                last = Some((name, curr_kind, curr_full_range));
            }
        }
    }

    // Recurse into nested selection sets
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if opts.arguments.enabled() {
                    if let Some(arguments) = field.arguments() {
                        check_argument_order(&arguments, doc, diagnostics);
                    }
                }
                if let Some(nested) = field.selection_set() {
                    check_selection_set_order(&nested, opts, scan_selections, doc, diagnostics);
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(nested) = inline.selection_set() {
                    check_selection_set_order(&nested, opts, scan_selections, doc, diagnostics);
                }
            }
            cst::Selection::FragmentSpread(_) => {}
        }
    }
}

/// Compare two names under the `groups` rule. Mirrors graphql-eslint's
/// `getRank` exactly: prefer the explicit group index from the array, fall
/// back to `"*"` (catch-all) when the name isn't listed, then break ties
/// alphabetically (case-insensitive). When `groups` is empty the comparator
/// degrades to plain alphabetical, matching the legacy default.
fn group_compare(a: &str, b: &str, groups: &[String]) -> std::cmp::Ordering {
    if groups.is_empty() {
        return a.to_lowercase().cmp(&b.to_lowercase());
    }
    let rank = |name: &str| -> Option<usize> {
        if let Some(i) = groups.iter().position(|g| g == name) {
            return Some(i);
        }
        groups.iter().position(|g| g == "*")
    };
    match (rank(a), rank(b)) {
        (Some(ar), Some(br)) if ar != br => ar.cmp(&br),
        // Same group (or both ungrouped) — alphabetical within the bucket.
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

/// True when `curr` is misordered relative to `prev` under the configured
/// `groups`. Wraps `group_compare` for the common "should I report?" check.
fn is_misordered(curr: &str, prev: &str, groups: &[String]) -> bool {
    group_compare(curr, prev, groups) == std::cmp::Ordering::Less
}

/// Build a graphql-eslint-style swap fix: replaces `[prev.start, curr.end]`
/// with `<curr_text><between><prev_text>`. graphql-eslint emits two replace
/// ops which `ESLint` coalesces into a single edit covering both ranges; we
/// produce the equivalent collapsed edit directly so the napi shim can
/// surface a single `fix` to `ESLint`.
fn swap_fix(source: &str, prev: (usize, usize), curr: (usize, usize)) -> CodeFix {
    let prev_text = &source[prev.0..prev.1];
    let curr_text = &source[curr.0..curr.1];
    let between = &source[prev.1..curr.0];
    let new_text = format!("{curr_text}{between}{prev_text}");
    CodeFix::new(
        "Reorder alphabetically",
        vec![TextEdit::new(prev.0, curr.1, new_text)],
    )
}

fn check_argument_order(
    arguments: &cst::Arguments,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut last_name: Option<String> = None;

    for arg in arguments.arguments() {
        if let Some(name_node) = arg.name() {
            let name = name_node.text().to_string();
            if let Some(ref prev) = last_name {
                if name.to_lowercase() < prev.to_lowercase() {
                    let start: usize = name_node.syntax().text_range().start().into();
                    let end: usize = name_node.syntax().text_range().end().into();
                    diagnostics.push(
                        LintDiagnostic::new(
                            doc.span(start, end),
                            LintSeverity::Warning,
                            format!("argument \"{name}\" should be before argument \"{prev}\""),
                            "alphabetize",
                        )
                        .with_message_id("alphabetize")
                        .with_help("Reorder arguments alphabetically by name"),
                    );
                }
            }
            last_name = Some(name);
        }
    }
}

fn check_variable_order(
    var_defs: &cst::VariableDefinitions,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut last_name: Option<String> = None;

    for var_def in var_defs.variable_definitions() {
        if let Some(var) = var_def.variable() {
            if let Some(name_node) = var.name() {
                let name = name_node.text().to_string();
                if let Some(ref prev) = last_name {
                    if name.to_lowercase() < prev.to_lowercase() {
                        let start: usize = name_node.syntax().text_range().start().into();
                        let end: usize = name_node.syntax().text_range().end().into();
                        diagnostics.push(
                            LintDiagnostic::new(
                                doc.span(start, end),
                                LintSeverity::Warning,
                                format!("variable \"{name}\" should be before variable \"{prev}\""),
                                "alphabetize",
                            )
                            .with_message_id("alphabetize")
                            .with_help("Reorder variable definitions alphabetically by name"),
                        );
                    }
                }
                last_name = Some(name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneDocumentLintRule;
    use graphql_base_db::{DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language};
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_base_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
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

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = AlphabetizeRuleImpl;
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
            None,
        )
    }

    #[test]
    fn test_alphabetical_selections() {
        let diagnostics = check("query Q { user { age email name } }");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_non_alphabetical_selections() {
        let diagnostics = check("query Q { user { name age email } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("field \"age\" should be before field \"name\""));
    }

    #[test]
    fn test_nested_non_alphabetical() {
        let diagnostics = check("query Q { user { posts { title id } } }");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("field \"id\" should be before field \"title\""));
    }

    #[test]
    fn test_mixed_field_after_fragment_spread() {
        // Fragment spread `Zed` then field `age` — current is field, previous is fragment spread.
        let diagnostics = check("query Q { user { ...Zed age } }");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "field \"age\" should be before fragment spread \"Zed\""
        );
    }

    #[test]
    fn test_mixed_fragment_spread_after_field() {
        // Field `name` then fragment spread `Avatar` — current is fragment spread, previous is field.
        let diagnostics = check("query Q { user { name ...Avatar } }");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "fragment spread \"Avatar\" should be before field \"name\""
        );
    }

    fn check_with_options(source: &str, options: &serde_json::Value) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = AlphabetizeRuleImpl;
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
            Some(options),
        )
    }

    #[test]
    fn test_selections_array_only_operation_definition() {
        // With `selections: ["OperationDefinition"]`, fragment definitions
        // are NOT scanned for selection-set ordering.
        let opts = serde_json::json!({ "selections": ["OperationDefinition"] });
        let source = "fragment F on User { name age id }\nquery Q { user { name age id } }\n";
        let diagnostics = check_with_options(source, &opts);
        assert_eq!(diagnostics.len(), 1, "expected only the query to fire");
        assert!(diagnostics[0]
            .message
            .contains("field \"age\" should be before field \"name\""));
    }

    #[test]
    fn test_selections_array_only_fragment_definition() {
        let opts = serde_json::json!({ "selections": ["FragmentDefinition"] });
        let source = "fragment F on User { name age id }\nquery Q { user { name age id } }\n";
        let diagnostics = check_with_options(source, &opts);
        assert_eq!(diagnostics.len(), 1, "expected only the fragment to fire");
    }

    #[test]
    fn test_selections_array_both_kinds() {
        let opts = serde_json::json!({
            "selections": ["OperationDefinition", "FragmentDefinition"]
        });
        let source = "fragment F on User { name age id }\nquery Q { user { name age id } }\n";
        let diagnostics = check_with_options(source, &opts);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_selections_false_disables_check() {
        let opts = serde_json::json!({ "selections": false });
        let diagnostics = check_with_options("query Q { user { name age } }", &opts);
        assert!(diagnostics.is_empty());
    }

    // ----- Schema-side tests -----

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
        let entry = graphql_base_db::FileEntry::new(db, content, metadata);
        let mut entries = std::collections::HashMap::new();
        entries.insert(file_id, entry);
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![file_id]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map = graphql_base_db::FileEntryMap::new(db, Arc::new(entries));
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

    fn check_schema(schema: &str, options: &serde_json::Value) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = AlphabetizeRuleImpl;
        let project_files = create_schema_project(&db, schema);
        StandaloneSchemaLintRule::check(&rule, &db, project_files, Some(options))
            .into_values()
            .flatten()
            .collect()
    }

    #[test]
    fn test_definitions_unordered_fires() {
        let opts = serde_json::json!({ "definitions": true });
        let schema = "type Zebra { id: ID! }\ntype Apple { id: ID! }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one definition-order diagnostic"
        );
        assert_eq!(
            diagnostics[0].message,
            "type \"Apple\" should be before type \"Zebra\""
        );
    }

    #[test]
    fn test_definitions_ordered_clean() {
        let opts = serde_json::json!({ "definitions": true });
        let schema = "type Apple { id: ID! }\ntype Zebra { id: ID! }\n";
        let diagnostics = check_schema(schema, &opts);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics, got {diagnostics:?}"
        );
    }

    #[test]
    fn test_definitions_off_does_not_fire() {
        let opts = serde_json::json!({ "values": true });
        let schema = "type Zebra { id: ID! }\ntype Apple { id: ID! }\n";
        let diagnostics = check_schema(schema, &opts);
        // No `definitions` flag, so out-of-order types are fine.
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_fields_in_object_type_unordered_fires() {
        let opts = serde_json::json!({ "fields": ["ObjectTypeDefinition"] });
        // `name age id`: only the (name, age) pair is misordered (id > age, so
        // no diagnostic on the second pair). Mirrors graphql-eslint's
        // consecutive-pair scan.
        let schema = "type User { name: String age: Int id: ID! }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "field \"age\" should be before field \"name\""
        );
    }

    #[test]
    fn test_fields_in_input_object_unordered_fires() {
        let opts = serde_json::json!({ "fields": ["InputObjectTypeDefinition"] });
        let schema = "input UserFilter { name: String age: Int }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "input value \"age\" should be before input value \"name\""
        );
    }

    #[test]
    fn test_values_in_enum_unordered_fires() {
        let opts = serde_json::json!({ "values": true });
        let schema = "enum Role { SUPER_ADMIN ADMIN USER GOD }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 2, "got: {diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "enum value \"ADMIN\" should be before enum value \"SUPER_ADMIN\""
        );
        assert_eq!(
            diagnostics[1].message,
            "enum value \"GOD\" should be before enum value \"USER\""
        );
    }

    #[test]
    fn test_values_off_does_not_fire() {
        let opts = serde_json::json!({ "definitions": true });
        let schema = "enum Role { Z A }\n";
        let diagnostics = check_schema(schema, &opts);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_fields_only_for_interface_when_object_excluded() {
        // `fields` array form narrows to InterfaceTypeDefinition only — the
        // object type's misordered fields must NOT be flagged.
        let opts = serde_json::json!({ "fields": ["InterfaceTypeDefinition"] });
        let schema =
            "type User { name: String age: Int }\ninterface Node { name: String age: Int }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "field \"age\" should be before field \"name\""
        );
        // The diagnostic must point at the interface's `age`, not the object's.
        // The interface appears second in the source, so its `age` byte-position
        // is > the object's `age`.
        let object_age = schema.find("type User { name: String age").unwrap()
            + "type User { name: String ".len();
        assert!(
            diagnostics[0].span.start > object_age,
            "diagnostic should point at the interface's `age`, not the object's"
        );
    }

    #[test]
    fn test_fields_bool_true_covers_all_kinds() {
        let opts = serde_json::json!({ "fields": true });
        let schema =
            "type User { name: String age: Int }\ninput Filter { name: String age: Int }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 2, "got: {diagnostics:?}");
    }

    #[test]
    fn test_schema_message_id_is_alphabetize() {
        let opts = serde_json::json!({ "values": true });
        let schema = "enum Role { B A }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message_id.as_deref(), Some("alphabetize"));
    }

    // ----- groups: explicit ordering buckets -----

    #[test]
    fn test_groups_ordering_id_first_wildcard_middle() {
        // `groups: ["id", "*", "createdAt"]` → expected order is:
        //   id (rank 0) < {anything else} (rank 1, alphabetical) < createdAt (rank 2)
        // The schema has fields out of that order (`name` before `id`,
        // `createdAt` before `email`); both should fire.
        let opts = serde_json::json!({
            "fields": ["ObjectTypeDefinition"],
            "groups": ["id", "*", "createdAt"]
        });
        let schema = "type User { name: String id: ID! createdAt: String email: String }\n";
        let diagnostics = check_schema(schema, &opts);
        // 2 misorderings under the group rule:
        //   - `id` (rank 0) should be before `name` (rank 1, *)
        //   - `email` (rank 1, *) should be before `createdAt` (rank 2)
        assert_eq!(diagnostics.len(), 2, "got: {diagnostics:?}");
    }

    #[test]
    fn test_groups_within_bucket_falls_back_to_alphabetical() {
        // No name in `groups`, so everything lands in `*` and the within-
        // bucket order is alphabetical — same as legacy behavior.
        let opts = serde_json::json!({
            "fields": ["ObjectTypeDefinition"],
            "groups": ["*"]
        });
        let schema = "type User { name: String age: Int }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_groups_unconfigured_uses_alphabetical() {
        // Empty `groups` (the default) keeps the legacy alphabetical
        // comparator — backwards-compatible for existing configs.
        let opts = serde_json::json!({
            "fields": ["ObjectTypeDefinition"],
        });
        let schema = "type User { name: String age: Int }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_groups_named_bucket_pulls_to_front() {
        // Even with `*` first, an explicit named bucket later in the list
        // wins for that name. `["*", "id"]` means id is LAST.
        let opts = serde_json::json!({
            "fields": ["ObjectTypeDefinition"],
            "groups": ["*", "id"]
        });
        let schema = "type User { id: ID! name: String age: Int }\n";
        let diagnostics = check_schema(schema, &opts);
        // `id` (rank 1) appearing first is a violation; `age` < `name`
        // alphabetically within `*` is also a violation.
        assert_eq!(diagnostics.len(), 2, "got: {diagnostics:?}");
    }
}
