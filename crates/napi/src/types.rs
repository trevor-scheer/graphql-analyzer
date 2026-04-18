use napi_derive::napi;

#[napi(object)]
pub struct JsDiagnostic {
    pub rule: String,
    pub message: String,
    pub severity: String,
    /// 1-based line number (ESLint convention)
    pub line: u32,
    /// 1-based column number (ESLint convention)
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub fix: Option<JsFix>,
    pub help: Option<String>,
    pub url: Option<String>,
    pub source: String,
}

#[napi(object)]
pub struct JsFix {
    pub description: String,
    pub edits: Vec<JsTextEdit>,
}

#[napi(object)]
pub struct JsTextEdit {
    pub range_start_line: u32,
    pub range_start_column: u32,
    pub range_end_line: u32,
    pub range_end_column: u32,
    pub new_text: String,
}

#[napi(object)]
pub struct JsExtractedBlock {
    pub source: String,
    pub offset: u32,
    pub tag: Option<String>,
}

#[napi(object)]
pub struct JsRuleMeta {
    pub name: String,
    pub description: String,
    pub default_severity: String,
    pub category: String,
}

// -- From conversions ---------------------------------------------------------

impl From<graphql_ide::Diagnostic> for JsDiagnostic {
    fn from(d: graphql_ide::Diagnostic) -> Self {
        Self {
            rule: d.code.unwrap_or_default(),
            message: d.message,
            severity: match d.severity {
                graphql_ide::DiagnosticSeverity::Error => "error".to_string(),
                graphql_ide::DiagnosticSeverity::Warning => "warning".to_string(),
                graphql_ide::DiagnosticSeverity::Information => "information".to_string(),
                graphql_ide::DiagnosticSeverity::Hint => "hint".to_string(),
            },
            // IDE positions are 0-indexed; ESLint expects 1-based
            line: d.range.start.line + 1,
            column: d.range.start.character + 1,
            end_line: d.range.end.line + 1,
            end_column: d.range.end.character + 1,
            fix: d.fix.map(JsFix::from),
            help: d.help,
            url: d.url,
            source: d.source,
        }
    }
}

impl From<graphql_ide::CodeFix> for JsFix {
    fn from(f: graphql_ide::CodeFix) -> Self {
        Self {
            description: f.label,
            edits: f.edits.into_iter().map(JsTextEdit::from).collect(),
        }
    }
}

impl From<graphql_ide::TextEdit> for JsTextEdit {
    fn from(e: graphql_ide::TextEdit) -> Self {
        Self {
            range_start_line: e.range.start.line + 1,
            range_start_column: e.range.start.character + 1,
            range_end_line: e.range.end.line + 1,
            range_end_column: e.range.end.character + 1,
            new_text: e.new_text,
        }
    }
}

impl From<graphql_extract::ExtractedGraphQL> for JsExtractedBlock {
    #[allow(clippy::cast_possible_truncation)]
    fn from(e: graphql_extract::ExtractedGraphQL) -> Self {
        Self {
            source: e.source,
            offset: e.location.offset as u32,
            tag: e.tag_name,
        }
    }
}

impl From<graphql_linter::RuleInfo> for JsRuleMeta {
    fn from(r: graphql_linter::RuleInfo) -> Self {
        Self {
            name: r.name.to_string(),
            description: r.description.to_string(),
            default_severity: match r.default_severity {
                graphql_linter::DiagnosticSeverity::Error => "error",
                graphql_linter::DiagnosticSeverity::Warning => "warn",
                graphql_linter::DiagnosticSeverity::Info => "info",
            }
            .to_string(),
            category: r.category.to_string(),
        }
    }
}
