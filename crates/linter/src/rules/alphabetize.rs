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

        if !opts.definitions && !opts.fields.enabled() && !opts.values && !opts.arguments.enabled()
        {
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

/// One top-level definition's ordering metadata. Unnamed definitions (anonymous
/// operations, schema blocks) still participate as prev-sentinels but have no name
/// to order by.
enum DefinitionEntry {
    /// Named definition: `(kind_label, name_node)`. Named operations carry the
    /// specific operation type (`"query"`, `"mutation"`, `"subscription"`)
    /// rather than the generic `"operation"`, matching graphql-eslint's
    /// `getNodeTypeName` helper.
    Named(&'static str, cst::Name),
    /// Unnamed definition that still participates in the definitions ordering
    /// sequence as a prev-sentinel. When a named definition immediately follows
    /// an unnamed one, graphql-eslint always fires regardless of alphabetical
    /// order (upstream `checkNodes` skips the comparison when `prevName` is
    /// empty and falls through to `context.report`).
    ///
    /// The carried `&'static str` is the display label for the message (e.g.
    /// `"operation definition"`, `"schema definition"`), matching
    /// `lowerCase(prevNode.kind)` in upstream.
    Unnamed(&'static str),
}

/// Returns the ordering entry for a top-level definition, or `None` only in
/// unreachable branches (everything now maps to `Named` or `Unnamed`).
fn definition_entry(definition: &cst::Definition) -> Option<DefinitionEntry> {
    match definition {
        cst::Definition::ObjectTypeDefinition(d) => Some(DefinitionEntry::Named("type", d.name()?)),
        cst::Definition::ObjectTypeExtension(d) => Some(DefinitionEntry::Named("type", d.name()?)),
        cst::Definition::InterfaceTypeDefinition(d) => {
            Some(DefinitionEntry::Named("interface", d.name()?))
        }
        cst::Definition::InterfaceTypeExtension(d) => {
            Some(DefinitionEntry::Named("interface", d.name()?))
        }
        cst::Definition::UnionTypeDefinition(d) => Some(DefinitionEntry::Named("union", d.name()?)),
        cst::Definition::UnionTypeExtension(d) => Some(DefinitionEntry::Named("union", d.name()?)),
        cst::Definition::EnumTypeDefinition(d) => Some(DefinitionEntry::Named("enum", d.name()?)),
        cst::Definition::EnumTypeExtension(d) => Some(DefinitionEntry::Named("enum", d.name()?)),
        cst::Definition::ScalarTypeDefinition(d) => {
            Some(DefinitionEntry::Named("scalar", d.name()?))
        }
        cst::Definition::ScalarTypeExtension(d) => {
            Some(DefinitionEntry::Named("scalar", d.name()?))
        }
        cst::Definition::InputObjectTypeDefinition(d) => {
            Some(DefinitionEntry::Named("input", d.name()?))
        }
        cst::Definition::InputObjectTypeExtension(d) => {
            Some(DefinitionEntry::Named("input", d.name()?))
        }
        cst::Definition::DirectiveDefinition(d) => {
            Some(DefinitionEntry::Named("directive", d.name()?))
        }
        cst::Definition::OperationDefinition(d) => {
            // Use the specific operation type as the label, matching
            // graphql-eslint's `getNodeTypeName` which returns `node.operation`
            // ("query", "mutation", "subscription") for OperationDefinition.
            // Anonymous operations (no name) still participate as prev-sentinels.
            let kind_label = match d.operation_type() {
                Some(op) => match op.mutation_token() {
                    Some(_) => "mutation",
                    None => match op.subscription_token() {
                        Some(_) => "subscription",
                        None => "query",
                    },
                },
                None => "query", // bare `{ ... }` shorthand is an anonymous query
            };
            match d.name() {
                Some(name) => Some(DefinitionEntry::Named(kind_label, name)),
                None => Some(DefinitionEntry::Unnamed("operation definition")),
            }
        }
        cst::Definition::FragmentDefinition(d) => Some(DefinitionEntry::Named(
            "fragment",
            d.fragment_name()?.name()?,
        )),
        // Schema definitions have no name but still participate as prev-sentinels:
        // a named definition immediately following a schema block is always reported
        // (mirrors upstream's `!prevName → always report` path in `checkNodes`).
        cst::Definition::SchemaDefinition(_) | cst::Definition::SchemaExtension(_) => {
            Some(DefinitionEntry::Unnamed("schema definition"))
        }
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
    //
    // `prev` carries `(sort_name, display_kind, range)`. For unnamed definitions
    // (anonymous operations, schema blocks), `sort_name` is `""` which triggers
    // "always report" when a named definition follows.
    if opts.definitions {
        let mut prev: Option<(String, &'static str, (usize, usize))> = None;
        for definition in doc_cst.definitions() {
            let entry = definition_entry(&definition);
            match entry {
                None => {}
                Some(DefinitionEntry::Unnamed(display_label)) => {
                    // Unnamed definitions update prev with an empty sort name so
                    // that the next named definition always fires (upstream's
                    // `if (!prevName) → always report` path).
                    let curr_range = {
                        let r = definition.syntax().text_range();
                        let s: usize = r.start().into();
                        let e: usize = r.end().into();
                        (s, e)
                    };
                    prev = Some((String::new(), display_label, curr_range));
                }
                Some(DefinitionEntry::Named(kind_label, name_node)) => {
                    let curr_name = name_node.text().to_string();
                    let curr_range = {
                        let r = definition.syntax().text_range();
                        let s: usize = r.start().into();
                        let e: usize = r.end().into();
                        (s, e)
                    };
                    if let Some((ref prev_name, prev_kind, prev_range)) = prev {
                        // When prev has no name (empty), always report — mirrors
                        // upstream skipping the alphabetical comparison when
                        // `prevName` is falsy and falling through to `report`.
                        let should_report = if prev_name.is_empty() {
                            true
                        } else {
                            is_misordered(&curr_name, prev_name, &opts.groups)
                        };
                        if should_report {
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
            cst::Definition::DirectiveDefinition(d) => {
                // Directive arguments. Selector context is `DirectiveDefinition`
                // — narrow with `arguments: ["DirectiveDefinition"]` (mirrors
                // upstream's array form). Bool `true` enables every context.
                // These are `InputValueDefinition` nodes, so upstream calls them
                // "input value" in its messages (same as field/input-type nodes).
                if opts.arguments.includes_kind("DirectiveDefinition") {
                    if let Some(args) = d.arguments_definition() {
                        check_input_value_definition_order(
                            args.input_value_definitions(),
                            "input value",
                            &opts.groups,
                            doc,
                            diagnostics,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    // 3. Field-argument ordering on object/interface fields. Selector
    // context is `FieldDefinition` (the field that owns the argument list);
    // upstream's `arguments: ["FieldDefinition", ...]` enables this slice
    // independently of the bare-field ordering above.
    if opts.arguments.includes_kind("FieldDefinition") {
        for definition in doc_cst.definitions() {
            let fd_iter = match definition {
                cst::Definition::ObjectTypeDefinition(d) => d.fields_definition(),
                cst::Definition::ObjectTypeExtension(d) => d.fields_definition(),
                cst::Definition::InterfaceTypeDefinition(d) => d.fields_definition(),
                cst::Definition::InterfaceTypeExtension(d) => d.fields_definition(),
                _ => None,
            };
            let Some(fields) = fd_iter else {
                continue;
            };
            for field in fields.field_definitions() {
                if let Some(args) = field.arguments_definition() {
                    // Schema-side field arguments are `InputValueDefinition` nodes;
                    // upstream labels them "input value" (same as field/input-type nodes).
                    check_input_value_definition_order(
                        args.input_value_definitions(),
                        "input value",
                        &opts.groups,
                        doc,
                        diagnostics,
                    );
                }
            }
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
    // When prev has no name (e.g. anonymous operation, schema block), upstream
    // formats the prev side without quotes, matching its
    // `prevName ? displayNodeName(prevNode) : lowerCase(prevNode.kind)` path.
    let message = if prev_name.is_empty() {
        format!("{curr_kind} \"{curr_name}\" should be before {prev_kind}")
    } else {
        format!("{curr_kind} \"{curr_name}\" should be before {prev_kind} \"{prev_name}\"")
    };
    let mut diag = LintDiagnostic::new(
        doc.span(start, end),
        LintSeverity::Warning,
        message,
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
    /// Inline fragments have no name; they participate in ordering as a
    /// sentinel using `lowerCase(node.kind)` = `"inline fragment"` for both
    /// the sort key and the message. This mirrors upstream's `checkNodes`
    /// path where `prevName` is empty and the message is formed with
    /// `lowerCase(prevNode.kind)` instead of `displayNodeName`.
    InlineFragment,
}

impl SelectionKind {
    fn label(self) -> &'static str {
        match self {
            SelectionKind::Field => "field",
            SelectionKind::FragmentSpread => "fragment spread",
            SelectionKind::InlineFragment => "inline fragment",
        }
    }
}

/// One selection's ordering metadata, tracking enough info for group-aware
/// comparison.
struct SelectionInfo {
    /// Empty string for inline fragments — signals the "no name → always
    /// report next named item" path from upstream's `checkNodes`.
    name: String,
    kind: SelectionKind,
    /// True when the selection is a `Field` that has a sub-selection set (`{`
    /// group bucket in graphql-eslint's `getIndex`).
    has_selection_set: bool,
    full_range: (usize, usize),
    /// Byte range of the name/alias node itself (used as the diagnostic anchor).
    /// For inline fragments this is the start of the `...` token.
    name_range: (usize, usize),
}

/// Compute the group rank for a selection, mirroring graphql-eslint's `getIndex`:
///   1. Exact name match in `groups`
///   2. `{` if the selection has a sub-selection set
///   3. `...` if the selection is a fragment spread
///   4. `*` catch-all
fn selection_group_rank(info: &SelectionInfo, groups: &[String]) -> Option<usize> {
    if groups.is_empty() {
        return None;
    }
    if let Some(i) = groups.iter().position(|g| g == &info.name) {
        return Some(i);
    }
    if info.has_selection_set {
        if let Some(i) = groups.iter().position(|g| g == "{") {
            return Some(i);
        }
    }
    if matches!(info.kind, SelectionKind::FragmentSpread) {
        if let Some(i) = groups.iter().position(|g| g == "...") {
            return Some(i);
        }
    }
    groups.iter().position(|g| g == "*")
}

/// True when `curr` selection is misordered relative to `prev` selection.
fn selection_is_misordered(curr: &SelectionInfo, prev: &SelectionInfo, groups: &[String]) -> bool {
    if groups.is_empty() {
        return locale_compare(&curr.name, &prev.name) == std::cmp::Ordering::Less;
    }
    let curr_rank = selection_group_rank(curr, groups);
    let prev_rank = selection_group_rank(prev, groups);
    match (prev_rank, curr_rank) {
        (Some(pr), Some(cr)) if pr != cr => pr > cr,
        _ => locale_compare(&curr.name, &prev.name) == std::cmp::Ordering::Less,
    }
}

fn check_selection_set_order(
    selection_set: &cst::SelectionSet,
    opts: &AlphabetizeOptions,
    scan_selections: bool,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // Single-pass walk: check ordering inline, then descend immediately.
    // This mirrors upstream's ESTree visitor where each SelectionSet node
    // fires in document order (outer before inner), but within one SelectionSet
    // the walk descends into a nested set before processing later siblings.
    // The net effect is that errors inside an inline fragment appear before
    // errors on fields that follow the inline fragment in the parent set.
    let mut last: Option<SelectionInfo> = None;

    for selection in selection_set.selections() {
        match &selection {
            cst::Selection::Field(field) => {
                let name_node = field
                    .alias()
                    .and_then(|a| a.name())
                    .or_else(|| field.name());
                let Some(n) = name_node else {
                    continue;
                };

                let full_range = {
                    let r = selection.syntax().text_range();
                    (r.start().into(), r.end().into())
                };
                let curr_info = SelectionInfo {
                    name: n.text().to_string(),
                    kind: SelectionKind::Field,
                    has_selection_set: field.selection_set().is_some(),
                    full_range,
                    name_range: (
                        n.syntax().text_range().start().into(),
                        n.syntax().text_range().end().into(),
                    ),
                };

                if scan_selections {
                    if let Some(ref prev_info) = last {
                        report_if_misordered(&curr_info, prev_info, opts, doc, diagnostics);
                    }
                }
                last = Some(curr_info);

                // Descend into nested selection set before moving to the next
                // sibling — matches upstream's depth-first ESTree walk order.
                if opts.arguments.enabled() {
                    if let Some(arguments) = field.arguments() {
                        check_argument_order(&arguments, doc, diagnostics);
                    }
                }
                if let Some(nested) = field.selection_set() {
                    check_selection_set_order(&nested, opts, scan_selections, doc, diagnostics);
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                let name_node = spread.fragment_name().and_then(|fn_| fn_.name());
                let Some(n) = name_node else {
                    continue;
                };

                let full_range = {
                    let r = selection.syntax().text_range();
                    (r.start().into(), r.end().into())
                };
                let curr_info = SelectionInfo {
                    name: n.text().to_string(),
                    kind: SelectionKind::FragmentSpread,
                    has_selection_set: false,
                    full_range,
                    name_range: (
                        n.syntax().text_range().start().into(),
                        n.syntax().text_range().end().into(),
                    ),
                };

                if scan_selections {
                    if let Some(ref prev_info) = last {
                        report_if_misordered(&curr_info, prev_info, opts, doc, diagnostics);
                    }
                }
                last = Some(curr_info);
            }
            cst::Selection::InlineFragment(inline) => {
                // Inline fragments are sentinels in the ordering sequence.
                // Descend first so inner errors precede errors on fields that
                // follow this fragment in the parent set. Then update `last`
                // to the sentinel — any subsequent named field fires when it
                // alphabetically precedes "inline fragment".
                if let Some(nested) = inline.selection_set() {
                    check_selection_set_order(&nested, opts, scan_selections, doc, diagnostics);
                }

                if scan_selections {
                    let full_range = {
                        let r = selection.syntax().text_range();
                        (r.start().into(), r.end().into())
                    };
                    // Empty name signals upstream's "no prevName → always
                    // report next named curr" path in checkNodes.
                    last = Some(SelectionInfo {
                        name: String::new(),
                        kind: SelectionKind::InlineFragment,
                        has_selection_set: true,
                        full_range,
                        name_range: (full_range.0, full_range.0),
                    });
                }
            }
        }
    }
}

fn report_if_misordered(
    curr_info: &SelectionInfo,
    prev_info: &SelectionInfo,
    opts: &AlphabetizeOptions,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // When the previous selection is an inline fragment sentinel (empty name),
    // always report — mirrors upstream's `if (!prevName) → always report` path
    // in checkNodes, which skips the alphabetical comparison entirely.
    let should_report = if prev_info.name.is_empty() {
        true
    } else {
        selection_is_misordered(curr_info, prev_info, &opts.groups)
    };

    if !should_report {
        return;
    }

    let fix = swap_fix(doc.source, prev_info.full_range, curr_info.full_range);
    let message = if prev_info.name.is_empty() {
        format!(
            "{curr_label} \"{name}\" should be before {prev_label}",
            curr_label = curr_info.kind.label(),
            name = curr_info.name,
            prev_label = prev_info.kind.label(),
        )
    } else {
        format!(
            "{curr_label} \"{name}\" should be before {prev_label} \"{prev_name}\"",
            curr_label = curr_info.kind.label(),
            name = curr_info.name,
            prev_label = prev_info.kind.label(),
            prev_name = prev_info.name,
        )
    };
    diagnostics.push(
        LintDiagnostic::new(
            doc.span(curr_info.name_range.0, curr_info.name_range.1),
            LintSeverity::Warning,
            message,
            "alphabetize",
        )
        .with_message_id("alphabetize")
        .with_fix(fix)
        .with_help("Reorder selections alphabetically by their response name"),
    );
}

/// Locale-aware case-insensitive compare matching JS `String.prototype.localeCompare`.
///
/// Primary comparison is case-insensitive (fold both to lowercase). When
/// names are equal case-insensitively, the tiebreaker puts lowercase before
/// uppercase, mirroring the en-US locale where `"bar" < "Bar"`. This is
/// required to match graphql-eslint's `prevName.localeCompare(currName)` for
/// enum values and field names that differ only in case.
fn locale_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    match a_lower.cmp(&b_lower) {
        std::cmp::Ordering::Equal => {
            // Tiebreaker: lowercase before uppercase. In the Unicode/en-US locale
            // `"a" < "A"`, so a name with all-lowercase chars sorts before its
            // titlecase/uppercase variant.
            a.cmp(b).reverse()
        }
        other => other,
    }
}

/// Compare two names under the `groups` rule. Mirrors graphql-eslint's
/// `getRank` exactly: prefer the explicit group index from the array, fall
/// back to `"*"` (catch-all) when the name isn't listed, then break ties
/// using locale-aware comparison. When `groups` is empty the comparator
/// degrades to plain locale-aware alphabetical.
fn group_compare(a: &str, b: &str, groups: &[String]) -> std::cmp::Ordering {
    if groups.is_empty() {
        return locale_compare(a, b);
    }
    let rank = |name: &str| -> Option<usize> {
        if let Some(i) = groups.iter().position(|g| g == name) {
            return Some(i);
        }
        groups.iter().position(|g| g == "*")
    };
    match (rank(a), rank(b)) {
        (Some(ar), Some(br)) if ar != br => ar.cmp(&br),
        // Same group (or both ungrouped) — locale-aware alphabetical within bucket.
        _ => locale_compare(a, b),
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

    // ----- schema-side `arguments` per-context narrowing -----

    #[test]
    fn test_arguments_field_definition_unordered_fires() {
        // `arguments: ["FieldDefinition"]` enables sorting field-arg
        // definitions on object/interface types. `name(b, a)` is
        // misordered → 1 diagnostic.
        let opts = serde_json::json!({ "arguments": ["FieldDefinition"] });
        let schema = "type Query { user(b: Int, a: Int): String }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    }

    #[test]
    fn test_arguments_directive_definition_unordered_fires() {
        // `arguments: ["DirectiveDefinition"]` enables sorting
        // arguments on directive defs.
        let opts = serde_json::json!({ "arguments": ["DirectiveDefinition"] });
        let schema = "directive @demo(b: Int, a: Int) on FIELD\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    }

    #[test]
    fn test_arguments_per_context_narrowing_excludes_other_kinds() {
        // `arguments: ["FieldDefinition"]` should NOT fire on
        // directive args (they're a different context). `["DirectiveDefinition"]`
        // similarly should not fire on field args.
        let opts = serde_json::json!({ "arguments": ["FieldDefinition"] });
        let schema = "directive @demo(b: Int, a: Int) on FIELD\n\
                      type Query { user(b: Int, a: Int): String }\n";
        let diagnostics = check_schema(schema, &opts);
        // Only the field args fire; the directive args are skipped.
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    }

    #[test]
    fn test_arguments_bool_true_covers_both_contexts() {
        // Bool form enables every context; both field args and directive
        // args should fire.
        let opts = serde_json::json!({ "arguments": true });
        let schema = "directive @demo(b: Int, a: Int) on FIELD\n\
                      type Query { user(b: Int, a: Int): String }\n";
        let diagnostics = check_schema(schema, &opts);
        assert_eq!(diagnostics.len(), 2, "got: {diagnostics:?}");
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
