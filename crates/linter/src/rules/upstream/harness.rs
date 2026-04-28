//! Shared test harness for upstream-ported rule cases.

use std::collections::HashMap;

use graphql_base_db::FileId;
use graphql_test_utils::{TestProject, TestProjectBuilder};
use serde_json::Value;

use crate::diagnostics::LintDiagnostic;
use crate::traits::{
    DocumentSchemaLintRule, ProjectLintRule, StandaloneDocumentLintRule, StandaloneSchemaLintRule,
};

/// One upstream test case, pre-execution. Built fluently in `#[test] fn`s.
///
/// Construct with [`Case::valid`] or [`Case::invalid`], chain configuration
/// methods, then end with one of the `run_against_*` methods (see the
/// "runners" section of this module) to execute and assert.
#[allow(dead_code)]
pub(crate) struct Case {
    pub(super) permalink: String,
    pub(super) is_valid: bool,
    /// Inline schema. `None` falls back to `PLACEHOLDER_SCHEMA` (a tiny
    /// no-op schema: `type Query { _placeholder: Boolean }`). Some upstream
    /// tests assert on rules that need *some* schema even though the rule
    /// itself doesn't read it (e.g. document-side rules with operations
    /// that reference a Query type).
    pub(super) schema: Option<String>,
    /// The primary `code:` from upstream — written to `query.graphql` for
    /// document-rule runners and to `schema.graphql` for schema-rule
    /// runners. Each runner picks the right placement.
    pub(super) code: String,
    /// Additional documents the case needs (e.g. fragment files for
    /// `no-unused-fragments` / `require-import-fragment`). Written under
    /// `extra/<name>` so the primary document slot stays at index 0.
    pub(super) extra_documents: Vec<(String, String)>,
    /// Rule options forwarded as the `options` parameter to `rule.check`.
    pub(super) options: Option<Value>,
    /// Expected diagnostics (only consulted when `is_valid == false`).
    pub(super) expected_errors: Vec<ExpectedError>,
    /// Upstream's `output:` field, if present. After diagnostics are
    /// produced, the harness applies the autofix (`LintDiagnostic.fix`) to
    /// the input code and asserts the result equals this string.
    pub(super) expected_output: Option<String>,
}

/// One expected error in upstream's `errors: [{...}]`. Fields are optional
/// because upstream omits ones it doesn't pin (e.g. cases that assert only
/// `messageId` and don't care about column).
#[allow(dead_code)]
#[derive(Default, Debug)]
pub(crate) struct ExpectedError {
    pub(super) message_id: Option<String>,
    /// Set when upstream pins the literal message text. Most cases use
    /// `messageId` instead and leave this `None`.
    pub(super) message: Option<String>,
    pub(super) line: Option<u32>,
    pub(super) column: Option<u32>,
    pub(super) end_line: Option<u32>,
    pub(super) end_column: Option<u32>,
    pub(super) suggestions: Vec<ExpectedSuggestion>,
}

#[allow(dead_code)]
#[derive(Default, Debug)]
pub(crate) struct ExpectedSuggestion {
    pub(super) desc: String,
    /// Source after applying this suggestion's `fix` to the input `code`.
    /// Empty `String` means upstream didn't pin the post-fix output for
    /// this suggestion (rare).
    pub(super) output: String,
}

#[allow(dead_code)]
impl Case {
    pub(crate) fn valid(permalink: impl Into<String>) -> Self {
        Self {
            permalink: permalink.into(),
            is_valid: true,
            schema: None,
            code: String::new(),
            extra_documents: Vec::new(),
            options: None,
            expected_errors: Vec::new(),
            expected_output: None,
        }
    }

    pub(crate) fn invalid(permalink: impl Into<String>) -> Self {
        Self {
            permalink: permalink.into(),
            is_valid: false,
            schema: None,
            code: String::new(),
            extra_documents: Vec::new(),
            options: None,
            expected_errors: Vec::new(),
            expected_output: None,
        }
    }

    pub(crate) fn schema(mut self, s: impl Into<String>) -> Self {
        self.schema = Some(s.into());
        self
    }

    pub(crate) fn code(mut self, c: impl Into<String>) -> Self {
        self.code = c.into();
        self
    }

    pub(crate) fn document(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        self.extra_documents.push((name.into(), content.into()));
        self
    }

    pub(crate) fn options(mut self, opts: Value) -> Self {
        self.options = Some(opts);
        self
    }

    pub(crate) fn errors(mut self, errors: Vec<ExpectedError>) -> Self {
        self.expected_errors = errors;
        self
    }

    pub(crate) fn output(mut self, out: impl Into<String>) -> Self {
        self.expected_output = Some(out.into());
        self
    }
}

#[allow(dead_code)]
impl ExpectedError {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn message_id(mut self, id: impl Into<String>) -> Self {
        self.message_id = Some(id.into());
        self
    }

    pub(crate) fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    pub(crate) fn line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    pub(crate) fn column(mut self, column: u32) -> Self {
        self.column = Some(column);
        self
    }

    pub(crate) fn end_line(mut self, line: u32) -> Self {
        self.end_line = Some(line);
        self
    }

    pub(crate) fn end_column(mut self, column: u32) -> Self {
        self.end_column = Some(column);
        self
    }

    pub(crate) fn suggestions(mut self, sugs: Vec<ExpectedSuggestion>) -> Self {
        self.suggestions = sugs;
        self
    }
}

#[allow(dead_code)]
impl ExpectedSuggestion {
    pub(crate) fn new(desc: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            desc: desc.into(),
            output: output.into(),
        }
    }
}

#[allow(dead_code)]
pub(super) const PLACEHOLDER_SCHEMA: &str = "type Query { _placeholder: Boolean }\n";

/// Where the case's primary `code:` should be written.
#[allow(dead_code)]
#[derive(Clone, Copy)]
enum CodePlacement {
    /// `code:` goes to `schema.graphql`. Used for schema-side rule runners.
    Schema,
    /// `code:` goes to `query.graphql`. Used for document-side rule runners.
    /// A placeholder schema is added if the case didn't supply one.
    Document,
}

#[allow(dead_code)]
impl Case {
    fn build_project(&self, placement: CodePlacement) -> TestProject {
        let mut builder = TestProjectBuilder::new();
        match placement {
            CodePlacement::Schema => {
                builder = builder.with_schema("schema.graphql", &self.code);
            }
            CodePlacement::Document => {
                let schema = self.schema.as_deref().unwrap_or(PLACEHOLDER_SCHEMA);
                builder = builder
                    .with_schema("schema.graphql", schema)
                    .with_document("query.graphql", &self.code);
            }
        }
        for (name, content) in &self.extra_documents {
            builder = builder.with_document(name, content);
        }
        builder.build_detailed()
    }
}

#[allow(dead_code)]
impl Case {
    /// Run the case against a `StandaloneDocumentLintRule`. The case's
    /// `code:` is placed at `query.graphql`; `schema.graphql` is the
    /// placeholder schema unless the case supplied one.
    pub(crate) fn run_against_standalone_document<R: StandaloneDocumentLintRule>(self, rule: R) {
        let project = self.build_project(CodePlacement::Document);
        let target = project.documents.first().expect("document slot");
        let opts = self.options.as_ref();
        let diagnostics = rule.check(
            &project.db,
            target.id,
            target.content,
            target.metadata,
            project.project_files,
            opts,
        );
        self.assert_outcome(&diagnostics, &self.code);
    }

    /// Run the case against a `DocumentSchemaLintRule`. Same placement as
    /// `StandaloneDocumentLintRule`; the rule reads schema via the
    /// project's HIR queries.
    pub(crate) fn run_against_document_schema<R: DocumentSchemaLintRule>(self, rule: R) {
        let project = self.build_project(CodePlacement::Document);
        let target = project.documents.first().expect("document slot");
        let opts = self.options.as_ref();
        let diagnostics = rule.check(
            &project.db,
            target.id,
            target.content,
            target.metadata,
            project.project_files,
            opts,
        );
        self.assert_outcome(&diagnostics, &self.code);
    }

    /// Run the case against a `StandaloneSchemaLintRule`. The case's
    /// `code:` is placed at `schema.graphql`. Diagnostics are filtered to
    /// the schema file before assertion.
    pub(crate) fn run_against_standalone_schema<R: StandaloneSchemaLintRule>(self, rule: R) {
        let project = self.build_project(CodePlacement::Schema);
        let opts = self.options.as_ref();
        let by_file = rule.check(&project.db, project.project_files, opts);
        let target_id = project.schemas.first().expect("schema slot").id;
        let diagnostics = by_file.get(&target_id).cloned().unwrap_or_default();
        self.assert_outcome(&diagnostics, &self.code);
    }

    /// Run the case against a `ProjectLintRule`. Project rules can fire on
    /// any file in the project; the harness flattens diagnostics across
    /// every file before assertion. `code:` goes to `schema.graphql`.
    pub(crate) fn run_against_project_schema<R: ProjectLintRule>(self, rule: R) {
        self.run_project_inner(CodePlacement::Schema, rule);
    }

    /// Document-side variant of `run_against_project_schema`.
    pub(crate) fn run_against_project_document<R: ProjectLintRule>(self, rule: R) {
        self.run_project_inner(CodePlacement::Document, rule);
    }

    fn run_project_inner<R: ProjectLintRule>(self, placement: CodePlacement, rule: R) {
        let project = self.build_project(placement);
        let opts = self.options.as_ref();
        let by_file: HashMap<FileId, Vec<LintDiagnostic>> =
            rule.check(&project.db, project.project_files, opts);
        let mut diagnostics: Vec<LintDiagnostic> = by_file.into_values().flatten().collect();
        // Flattening across files loses cross-file ordering — sort by
        // (start byte, message) so assertion order is deterministic.
        diagnostics.sort_by(|a, b| (a.span.start, &a.message).cmp(&(b.span.start, &b.message)));
        self.assert_outcome(&diagnostics, &self.code);
    }
}

/// Convert a 0-based byte offset in `source` to (line, column) using
/// 1-based line/column with column counted in chars-from-start-of-line.
/// Matches ESLint's positional convention for our diagnostics.
#[allow(dead_code)]
fn byte_to_line_col(source: &str, byte_offset: usize) -> (u32, u32) {
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    for (i, ch) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Apply a single `CodeFix` to `source` and return the resulting string.
/// Edits are applied right-to-left so earlier offsets aren't invalidated.
#[allow(dead_code)]
fn apply_fix(source: &str, fix: &crate::diagnostics::CodeFix) -> String {
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|e| std::cmp::Reverse(e.offset_range.start));
    let mut out = source.to_string();
    for edit in edits {
        let start = edit.offset_range.start;
        let end = edit.offset_range.end;
        out.replace_range(start..end, &edit.new_text);
    }
    out
}

#[allow(dead_code)]
impl Case {
    fn assert_outcome(&self, diagnostics: &[LintDiagnostic], source: &str) {
        if self.is_valid {
            assert!(
                diagnostics.is_empty(),
                "{}: expected valid (no diagnostics) but got {} diagnostic(s):\n{:#?}",
                self.permalink,
                diagnostics.len(),
                diagnostics,
            );
            return;
        }

        assert_eq!(
            diagnostics.len(),
            self.expected_errors.len(),
            "{}: expected {} diagnostic(s) but got {}:\n{:#?}",
            self.permalink,
            self.expected_errors.len(),
            diagnostics.len(),
            diagnostics,
        );

        for (i, (got, want)) in diagnostics
            .iter()
            .zip(self.expected_errors.iter())
            .enumerate()
        {
            if let Some(ref want_id) = want.message_id {
                let got_id = got.message_id.as_deref().unwrap_or("<none>");
                assert_eq!(
                    got_id, want_id,
                    "{} #{i}: messageId mismatch (want {want_id:?}, got {got_id:?})",
                    self.permalink,
                );
            }
            if let Some(ref want_msg) = want.message {
                assert_eq!(
                    &got.message, want_msg,
                    "{} #{i}: message mismatch",
                    self.permalink,
                );
            }
            let (got_line, got_col) = byte_to_line_col(source, got.span.start);
            if let Some(want_line) = want.line {
                assert_eq!(
                    got_line, want_line,
                    "{} #{i}: line mismatch (want {want_line}, got {got_line})",
                    self.permalink,
                );
            }
            if let Some(want_col) = want.column {
                assert_eq!(
                    got_col, want_col,
                    "{} #{i}: column mismatch (want {want_col}, got {got_col})",
                    self.permalink,
                );
            }
            if want.end_line.is_some() || want.end_column.is_some() {
                let (got_end_line, got_end_col) = byte_to_line_col(source, got.span.end);
                if let Some(want_end_line) = want.end_line {
                    assert_eq!(
                        got_end_line, want_end_line,
                        "{} #{i}: endLine mismatch",
                        self.permalink,
                    );
                }
                if let Some(want_end_col) = want.end_column {
                    assert_eq!(
                        got_end_col, want_end_col,
                        "{} #{i}: endColumn mismatch",
                        self.permalink,
                    );
                }
            }
            assert_eq!(
                got.suggestions.len(),
                want.suggestions.len(),
                "{} #{i}: suggestion count mismatch (want {}, got {}):\n{:#?}",
                self.permalink,
                want.suggestions.len(),
                got.suggestions.len(),
                got.suggestions,
            );
            for (j, (got_sug, want_sug)) in got
                .suggestions
                .iter()
                .zip(want.suggestions.iter())
                .enumerate()
            {
                assert_eq!(
                    got_sug.desc, want_sug.desc,
                    "{} #{i}.suggest[{j}]: desc mismatch",
                    self.permalink,
                );
                if !want_sug.output.is_empty() {
                    let got_output = apply_fix(source, &got_sug.fix);
                    assert_eq!(
                        got_output, want_sug.output,
                        "{} #{i}.suggest[{j}]: output mismatch",
                        self.permalink,
                    );
                }
            }
        }

        if let Some(ref want_output) = self.expected_output {
            let mut applied = source.to_string();
            for d in diagnostics {
                if let Some(ref fix) = d.fix {
                    applied = apply_fix(&applied, fix);
                }
            }
            assert_eq!(
                &applied, want_output,
                "{}: post-fix output mismatch",
                self.permalink,
            );
        }
    }
}
