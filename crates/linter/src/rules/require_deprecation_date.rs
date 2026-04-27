use crate::diagnostics::{CodeSuggestion, LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::{DirectiveUsage, TextRange};
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `requireDeprecationDate` rule
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RequireDeprecationDateOptions {
    /// The directive argument name to read the deletion date from. Defaults
    /// to `"deletionDate"`.
    #[serde(rename = "argumentName")]
    pub argument_name: String,
}

impl Default for RequireDeprecationDateOptions {
    fn default() -> Self {
        Self {
            argument_name: "deletionDate".to_string(),
        }
    }
}

impl RequireDeprecationDateOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that requires a deletion date on `@deprecated` directives.
///
/// Mirrors graphql-eslint:
///   - Reads the date from a dedicated `deletionDate` argument (configurable)
///     on the directive — not from a substring of the `reason`.
///   - Validates the date is in `DD/MM/YYYY` format.
///   - Validates the date is real (e.g. rejects `99/13/2025`).
///   - When the deletion date is in the past, emits a "can be removed"
///     diagnostic so teams know the field is overdue for removal.
pub struct RequireDeprecationDateRuleImpl;

impl LintRule for RequireDeprecationDateRuleImpl {
    fn name(&self) -> &'static str {
        "requireDeprecationDate"
    }

    fn description(&self) -> &'static str {
        "Requires @deprecated directives to include a deletion date"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

/// Find the `@deprecated` directive in a directive list, if any.
fn find_deprecated(directives: &[DirectiveUsage]) -> Option<&DirectiveUsage> {
    directives.iter().find(|d| d.name.as_ref() == "deprecated")
}

/// Validate `DD/MM/YYYY`. Mirrors graphql-eslint's `DATE_REGEX`.
fn is_valid_date_format(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let bytes = s.as_bytes();
    bytes[2] == b'/'
        && bytes[5] == b'/'
        && bytes[0..2].iter().all(u8::is_ascii_digit)
        && bytes[3..5].iter().all(u8::is_ascii_digit)
        && bytes[6..10].iter().all(u8::is_ascii_digit)
}

/// Parse a `DD/MM/YYYY` date and return whether it is a real calendar date,
/// plus a millisecond-since-UNIX-epoch timestamp suitable for past/future
/// comparison. Mirrors graphql-eslint's `Date.parse("${y}-${m}-${d}")`
/// roundtrip — including the `padStart` so e.g. `1/2/2025` is accepted.
fn parse_dd_mm_yyyy(s: &str) -> Option<i64> {
    let mut parts = s.split('/');
    let day = parts.next()?.parse::<u32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let year = parts.next()?.parse::<i32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&month) {
        return None;
    }
    let max_day = days_in_month(year, month)?;
    if day < 1 || day > max_day {
        return None;
    }
    Some(days_from_epoch(year, month, day) * 86_400_000)
}

/// Days in the given month/year (Gregorian).
const fn days_in_month(year: i32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if is_leap_year(year) { 29 } else { 28 }),
        _ => None,
    }
}

const fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days from 1970-01-01 (UTC) to the given date. Negative for dates before
/// the epoch. Used only for past/future comparison so a simple Howard
/// Hinnant-style civil-from-days isn't needed.
fn days_from_epoch(year: i32, month: u32, day: u32) -> i64 {
    let mut total: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            total += if is_leap_year(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            total -= if is_leap_year(y) { 366 } else { 365 };
        }
    }
    let mut m = 1u32;
    while m < month {
        total += i64::from(days_in_month(year, m).unwrap_or(0));
        m += 1;
    }
    total + i64::from(day) - 1
}

/// Strip surrounding double quotes from a directive argument's serialized
/// value (apollo-compiler stringifies String values as `"foo"`).
fn unquote_string(s: &str) -> Option<&str> {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

/// Format the `nodeName` portion of graphql-eslint's diagnostic messages,
/// matching `displayNodeName` + `getNodeName`.
enum NodeKind<'a> {
    Field { field: &'a str, parent: &'a str },
    InputValue { arg: &'a str, field: &'a str },
    EnumValue { value: &'a str, parent: &'a str },
}

impl NodeKind<'_> {
    fn render(&self) -> String {
        match self {
            NodeKind::Field { field, parent } => {
                format!("field \"{field}\" in type \"{parent}\"")
            }
            NodeKind::InputValue { arg, field } => {
                format!("input value \"{arg}\" in field \"{field}\"")
            }
            NodeKind::EnumValue { value, parent } => {
                format!("enum value \"{value}\" in enum \"{parent}\"")
            }
        }
    }

    /// The bare name used in upstream's "Remove `${nodeName}`" suggestion
    /// description (just the field/argument/enum value identifier).
    fn bare_name(&self) -> &str {
        match self {
            NodeKind::Field { field, .. } => field,
            NodeKind::InputValue { arg, .. } => arg,
            NodeKind::EnumValue { value, .. } => value,
        }
    }
}

fn span_from_range(range: TextRange) -> graphql_syntax::SourceSpan {
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    graphql_syntax::SourceSpan {
        start,
        end,
        line_offset: 0,
        byte_offset: 0,
        source: None,
    }
}

/// Diagnose a single `@deprecated` directive against the configured
/// argument name. Returns the diagnostic to append, if any. Mirrors the
/// branching in graphql-eslint's `Directive[name.value=deprecated]` handler.
fn diagnose(
    deprecated: &DirectiveUsage,
    argument_name: &str,
    node: &NodeKind<'_>,
    parent_def_range: TextRange,
) -> Option<LintDiagnostic> {
    let node_name = node.render();
    let directive_span = span_from_range(deprecated.name_range);

    // 1) No deletionDate argument — MESSAGE_REQUIRE_DATE, points at @deprecated name.
    let Some(arg) = deprecated
        .arguments
        .iter()
        .find(|a| a.name.as_ref() == argument_name)
    else {
        return Some(
            LintDiagnostic::new(
                directive_span,
                LintSeverity::Warning,
                format!("Directive \"@deprecated\" must have a deletion date for {node_name}"),
                "requireDeprecationDate",
            )
            .with_message_id("MESSAGE_REQUIRE_DATE"),
        );
    };

    // 2) deletionDate exists but isn't a string in DD/MM/YYYY form —
    //    MESSAGE_INVALID_FORMAT, points at the argument's value.
    let value_span = span_from_range(arg.value_range);
    let value_str = unquote_string(&arg.value);
    let date_str = match value_str {
        Some(s) if is_valid_date_format(s) => s,
        _ => {
            return Some(
                LintDiagnostic::new(
                    value_span,
                    LintSeverity::Warning,
                    format!("Deletion date must be in format \"DD/MM/YYYY\" for {node_name}"),
                    "requireDeprecationDate",
                )
                .with_message_id("MESSAGE_INVALID_FORMAT"),
            );
        }
    };

    // 3) Format ok but not a real calendar date — MESSAGE_INVALID_DATE.
    let Some(deletion_ms) = parse_dd_mm_yyyy(date_str) else {
        return Some(
            LintDiagnostic::new(
                value_span,
                LintSeverity::Warning,
                format!("Invalid \"{date_str}\" deletion date for {node_name}"),
                "requireDeprecationDate",
            )
            .with_message_id("MESSAGE_INVALID_DATE"),
        );
    };

    // 4) Date is in the past — MESSAGE_CAN_BE_REMOVED.
    if now_ms() > deletion_ms {
        // Mirror upstream's `fixer.remove(parent)`: remove the entire
        // field/argument/enum value definition.
        let bare = node.bare_name();
        let def_start: usize = parent_def_range.start().into();
        let def_end: usize = parent_def_range.end().into();
        let suggestion = if def_start < def_end {
            Some(CodeSuggestion::delete(
                format!("Remove `{bare}`"),
                def_start,
                def_end,
            ))
        } else {
            None
        };
        let mut diag = LintDiagnostic::new(
            directive_span,
            LintSeverity::Warning,
            format!("{node_name} \u{0441}an be removed"),
            "requireDeprecationDate",
        )
        .with_message_id("MESSAGE_CAN_BE_REMOVED");
        if let Some(s) = suggestion {
            diag = diag.with_suggestion(s);
        }
        return Some(diag);
    }

    None
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl StandaloneSchemaLintRule for RequireDeprecationDateRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = RequireDeprecationDateOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);

        for type_def in schema_types.values() {
            for field in &type_def.fields {
                if let Some(d) = find_deprecated(&field.directives) {
                    if let Some(diag) = diagnose(
                        d,
                        &opts.argument_name,
                        &NodeKind::Field {
                            field: &field.name,
                            parent: &type_def.name,
                        },
                        field.definition_range,
                    ) {
                        diagnostics_by_file
                            .entry(type_def.file_id)
                            .or_default()
                            .push(diag);
                    }
                }

                for arg in &field.arguments {
                    if let Some(d) = find_deprecated(&arg.directives) {
                        if let Some(diag) = diagnose(
                            d,
                            &opts.argument_name,
                            &NodeKind::InputValue {
                                arg: &arg.name,
                                field: &field.name,
                            },
                            arg.definition_range,
                        ) {
                            diagnostics_by_file
                                .entry(type_def.file_id)
                                .or_default()
                                .push(diag);
                        }
                    }
                }
            }

            for ev in &type_def.enum_values {
                if let Some(d) = find_deprecated(&ev.directives) {
                    if let Some(diag) = diagnose(
                        d,
                        &opts.argument_name,
                        &NodeKind::EnumValue {
                            value: &ev.name,
                            parent: &type_def.name,
                        },
                        ev.definition_range,
                    ) {
                        diagnostics_by_file
                            .entry(type_def.file_id)
                            .or_default()
                            .push(diag);
                    }
                }
            }
        }

        diagnostics_by_file
    }
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

    fn check_with_options(
        schema: &str,
        options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let db = RootDatabase::default();
        let rule = RequireDeprecationDateRuleImpl;
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, options);
        diagnostics.into_values().flatten().collect()
    }

    fn check(schema: &str) -> Vec<LintDiagnostic> {
        check_with_options(schema, None)
    }

    #[test]
    fn test_deprecated_with_future_deletion_date() {
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField", deletionDate: "01/01/2999")
}
"#,
        );
        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn test_deprecated_without_deletion_date() {
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField instead")
}
"#,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Directive \"@deprecated\" must have a deletion date for field \"oldField\" in type \"User\""
        );
    }

    #[test]
    fn test_deprecated_without_reason() {
        let diagnostics = check(
            r"
type User {
    id: ID!
    oldField: String @deprecated
}
",
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("must have a deletion date"));
    }

    #[test]
    fn test_enum_deprecated_without_date() {
        let diagnostics = check(
            r#"
enum Status {
    ACTIVE
    LEGACY @deprecated(reason: "No longer used")
}
"#,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Directive \"@deprecated\" must have a deletion date for enum value \"LEGACY\" in enum \"Status\""
        );
    }

    #[test]
    fn test_enum_deprecated_with_future_date() {
        let diagnostics = check(
            r#"
enum Status {
    ACTIVE
    LEGACY @deprecated(reason: "No longer used", deletionDate: "01/06/2999")
}
"#,
        );
        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn test_no_deprecated_fields() {
        let diagnostics = check(
            r"
type User {
    id: ID!
    name: String
}
",
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_format() {
        // Date in YYYY-MM-DD form — graphql-eslint requires DD/MM/YYYY.
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField", deletionDate: "2025-06-01")
}
"#,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Deletion date must be in format \"DD/MM/YYYY\" for field \"oldField\" in type \"User\""
        );
    }

    #[test]
    fn test_invalid_date() {
        // Format matches but the date is impossible (no Feb 31).
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "x", deletionDate: "31/02/2999")
}
"#,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Invalid \"31/02/2999\" deletion date for field \"oldField\" in type \"User\""
        );
    }

    #[test]
    fn test_can_be_removed_when_in_past() {
        let diagnostics = check(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "x", deletionDate: "01/01/1990")
}
"#,
        );
        assert_eq!(diagnostics.len(), 1);
        // The middle byte is U+0441 (Cyrillic small letter es), matching
        // graphql-eslint exactly.
        assert_eq!(
            diagnostics[0].message,
            "field \"oldField\" in type \"User\" \u{0441}an be removed"
        );
    }

    #[test]
    fn test_custom_argument_name() {
        let opts = serde_json::json!({ "argumentName": "removalDate" });
        let diagnostics = check_with_options(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField", removalDate: "01/03/2999")
}
"#,
            Some(&opts),
        );
        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn test_custom_argument_name_missing() {
        let opts = serde_json::json!({ "argumentName": "removalDate" });
        let diagnostics = check_with_options(
            r#"
type User {
    id: ID!
    oldField: String @deprecated(reason: "Use newField", deletionDate: "01/03/2999")
}
"#,
            Some(&opts),
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_position_points_at_directive_name() {
        let schema = "type Query { _: Boolean }\ntype User {\n  id: ID!\n  oldField: String @deprecated(reason: \"use newField\")\n}\n";
        let diagnostics = check(schema);
        assert_eq!(diagnostics.len(), 1);
        // Range should cover `deprecated` (the directive name token), not
        // the field, the directive arguments, or the field type.
        let span = &diagnostics[0].span;
        let text = &schema[span.start..span.end];
        assert_eq!(text, "deprecated");
    }
}
