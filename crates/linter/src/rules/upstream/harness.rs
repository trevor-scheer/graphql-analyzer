//! Shared test harness for upstream-ported rule cases.

use serde_json::Value;

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
