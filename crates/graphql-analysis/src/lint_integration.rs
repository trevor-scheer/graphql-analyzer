use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use std::collections::HashMap;
use std::sync::Arc;

/// Convert `LintSeverity` to Severity
#[allow(clippy::match_same_arms)]
fn convert_severity(lint_severity: graphql_linter::LintSeverity) -> Severity {
    match lint_severity {
        graphql_linter::LintSeverity::Error => Severity::Error,
        graphql_linter::LintSeverity::Warn => Severity::Warning,
        graphql_linter::LintSeverity::Off => Severity::Info,
        _ => Severity::Info, // fallback for future severity levels
    }
}

/// Run lints on a file
///
/// This integrates with the new trait-based graphql-linter API.
/// When `project_files` is `None`, no lints are run.
/// Memoization happens at the inner function level.
pub fn lint_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: Option<ProjectFiles>,
) -> Arc<Vec<Diagnostic>> {
    let Some(project_files) = project_files else {
        tracing::debug!("project_files is None, skipping all lints");
        return Arc::new(Vec::new());
    };

    lint_file_impl(db, content, metadata, project_files)
}

/// Run lints on a file with known project files
///
/// This is a tracked function for use when `ProjectFiles` is already known.
/// Use `lint_file` when you have an `Option<ProjectFiles>`.
pub fn lint_file_with_project(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    lint_file_impl(db, content, metadata, project_files)
}

/// Internal tracked function for linting with project files
#[salsa::tracked]
fn lint_file_impl(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: ProjectFiles,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    let parse = graphql_syntax::parse(db, content, metadata);

    let uri = metadata.uri(db);
    tracing::debug!(uri = %uri, parse_errors = parse.errors().len(), "lint_file called");

    if parse.has_errors() {
        tracing::debug!(uri = %uri, "Skipping linting due to parse errors");
        return Arc::new(diagnostics);
    }

    let file_id = metadata.file_id(db);
    let file_kind = metadata.kind(db);

    tracing::debug!(uri = %uri, ?file_kind, "Checking file kind");

    if file_kind.is_document() {
        tracing::debug!(uri = %uri, "Running standalone document lints");
        diagnostics.extend(standalone_document_lints(
            db,
            file_id,
            content,
            metadata,
            project_files,
        ));

        tracing::debug!(uri = %uri, "Running document+schema lints");
        diagnostics.extend(document_schema_lints(
            db,
            file_id,
            content,
            metadata,
            project_files,
        ));
    } else if file_kind.is_schema() {
        tracing::debug!(uri = %uri, "Running schema lints");
        diagnostics.extend(schema_lints(db, file_id, content, project_files));
    }

    tracing::debug!(diagnostics = diagnostics.len(), "Linting complete");

    Arc::new(diagnostics)
}

/// Run standalone document lint rules (no schema required)
fn standalone_document_lints(
    db: &dyn GraphQLAnalysisDatabase,
    file_id: FileId,
    content: FileContent,
    metadata: FileMetadata,
    project_files: ProjectFiles,
) -> Vec<Diagnostic> {
    let lint_config = db.lint_config();
    let mut diagnostics = Vec::new();

    for rule in graphql_linter::standalone_document_rules() {
        let enabled = lint_config.is_enabled(rule.name());
        tracing::debug!(
            rule = rule.name(),
            enabled = enabled,
            "Checking standalone document rule"
        );

        if !enabled {
            continue;
        }

        let lint_diags = rule.check(db, file_id, content, metadata, project_files);

        if !lint_diags.is_empty() {
            tracing::debug!(
                rule = rule.name(),
                count = lint_diags.len(),
                "Found lint issues"
            );
        }

        let severity = lint_config
            .get_severity(rule.name())
            .map_or(Severity::Warning, convert_severity);
        diagnostics.extend(convert_lint_diagnostics(
            db,
            content,
            lint_diags,
            rule.name(),
            severity,
        ));
    }

    diagnostics
}

/// Run document+schema lint rules
fn document_schema_lints(
    db: &dyn GraphQLAnalysisDatabase,
    file_id: FileId,
    content: FileContent,
    metadata: FileMetadata,
    project_files: ProjectFiles,
) -> Vec<Diagnostic> {
    let lint_config = db.lint_config();
    let mut diagnostics = Vec::new();

    for rule in graphql_linter::document_schema_rules() {
        let enabled = lint_config.is_enabled(rule.name());
        tracing::debug!(
            rule = rule.name(),
            enabled = enabled,
            "Checking document+schema rule"
        );

        if !enabled {
            continue;
        }

        let lint_diags = rule.check(db, file_id, content, metadata, project_files);

        if !lint_diags.is_empty() {
            tracing::debug!(
                rule = rule.name(),
                count = lint_diags.len(),
                "Found lint issues"
            );
        }

        let severity = lint_config
            .get_severity(rule.name())
            .map_or(Severity::Warning, convert_severity);
        diagnostics.extend(convert_lint_diagnostics(
            db,
            content,
            lint_diags,
            rule.name(),
            severity,
        ));
    }

    diagnostics
}

/// Run schema lint rules
#[allow(clippy::unnecessary_wraps)] // Future schema rules will return diagnostics
fn schema_lints(
    db: &dyn GraphQLAnalysisDatabase,
    _file_id: FileId,
    _content: FileContent,
    _project_files: ProjectFiles,
) -> Vec<Diagnostic> {
    let _lint_config = db.lint_config();
    let diagnostics = Vec::new();

    // Placeholder for future schema design rules (naming conventions, descriptions, etc.)

    tracing::debug!(
        enabled_rules = 0,
        "Schema linting complete (no schema rules available yet)"
    );

    diagnostics
}

/// Run project-wide lint rules
///
/// When `project_files` is `None`, returns an empty map.
/// Memoization happens at the inner function level.
pub fn project_lint_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: Option<ProjectFiles>,
) -> Arc<HashMap<FileId, Vec<Diagnostic>>> {
    let Some(project_files) = project_files else {
        return Arc::new(HashMap::new());
    };

    project_lint_diagnostics_impl(db, project_files)
}

/// Internal tracked function for project-wide linting
#[salsa::tracked]
fn project_lint_diagnostics_impl(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: ProjectFiles,
) -> Arc<HashMap<FileId, Vec<Diagnostic>>> {
    let lint_config = db.lint_config();
    let mut diagnostics_by_file: HashMap<FileId, Vec<Diagnostic>> = HashMap::new();

    tracing::info!("Running project-wide lint rules");

    for rule in graphql_linter::project_rules() {
        let enabled = lint_config.is_enabled(rule.name());
        tracing::info!(
            rule = rule.name(),
            enabled = enabled,
            "Checking project-wide rule"
        );

        if !enabled {
            continue;
        }

        let lint_diags = rule.check(db, project_files);

        tracing::info!(
            rule = rule.name(),
            file_count = lint_diags.len(),
            "Project-wide rule returned diagnostics"
        );

        for (file_id, file_lint_diags) in lint_diags {
            let Some((content, _)) = find_file_content_and_metadata(db, project_files, file_id)
            else {
                tracing::warn!(?file_id, "Could not find content for file");
                continue;
            };

            let severity = lint_config
                .get_severity(rule.name())
                .map_or(Severity::Warning, convert_severity);
            let converted =
                convert_lint_diagnostics(db, content, file_lint_diags, rule.name(), severity);
            diagnostics_by_file
                .entry(file_id)
                .or_default()
                .extend(converted);
        }
    }

    tracing::info!(
        files = diagnostics_by_file.len(),
        "Project-wide linting complete"
    );

    Arc::new(diagnostics_by_file)
}

/// Helper to find `FileContent` and `FileMetadata` for a given `FileId` from `ProjectFiles`
fn find_file_content_and_metadata(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: ProjectFiles,
    file_id: FileId,
) -> Option<(FileContent, FileMetadata)> {
    graphql_base_db::file_lookup(db, project_files, file_id)
}

/// Get raw lint diagnostics for a file (with fix information preserved)
///
/// This function returns the raw `LintDiagnostic` objects from the linter,
/// which include fix information. Use this for implementing auto-fix functionality.
pub fn lint_file_with_fixes(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: Option<ProjectFiles>,
) -> Vec<graphql_linter::LintDiagnostic> {
    let Some(project_files) = project_files else {
        return Vec::new();
    };

    let lint_config = db.lint_config();
    let mut all_diagnostics = Vec::new();
    let file_id = metadata.file_id(db);
    let file_kind = metadata.kind(db);

    // Parse and check for errors
    let parse = graphql_syntax::parse(db, content, metadata);
    if parse.has_errors() {
        return all_diagnostics;
    }

    // Run lints based on file kind
    if file_kind.is_document() {
        // Standalone document lints
        for rule in graphql_linter::standalone_document_rules() {
            if lint_config.is_enabled(rule.name()) {
                all_diagnostics.extend(rule.check(db, file_id, content, metadata, project_files));
            }
        }

        // Document+schema lints
        for rule in graphql_linter::document_schema_rules() {
            if lint_config.is_enabled(rule.name()) {
                all_diagnostics.extend(rule.check(db, file_id, content, metadata, project_files));
            }
        }
    }
    // Schema lints (none currently)

    all_diagnostics
}

/// Get project-wide raw lint diagnostics (with fix information preserved)
pub fn project_lint_diagnostics_with_fixes(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: Option<ProjectFiles>,
) -> HashMap<FileId, Vec<graphql_linter::LintDiagnostic>> {
    let Some(project_files) = project_files else {
        return HashMap::new();
    };

    let lint_config = db.lint_config();
    let mut diagnostics_by_file: HashMap<FileId, Vec<graphql_linter::LintDiagnostic>> =
        HashMap::new();

    // Get all project rules from registry
    for rule in graphql_linter::project_rules() {
        if !lint_config.is_enabled(rule.name()) {
            continue;
        }

        // Run the project-wide rule
        let lint_diags = rule.check(db, project_files);

        // Merge into result
        for (file_id, file_lint_diags) in lint_diags {
            diagnostics_by_file
                .entry(file_id)
                .or_default()
                .extend(file_lint_diags);
        }
    }

    diagnostics_by_file
}

/// Convert `LintDiagnostic` (byte offsets) to `Diagnostic` (line/column)
///
/// For TypeScript/JavaScript files with extracted blocks, each `LintDiagnostic` may have
/// `block_line_offset` and `block_source` set. When present:
/// - `offset_range` is relative to `block_source`, not the full file
/// - We build a `LineIndex` from `block_source` to convert byte offsets to line/column
/// - We add `block_line_offset` to get the correct position in the original file
///
/// For pure GraphQL files (no block context), we use the full file's `LineIndex`.
#[allow(clippy::cast_possible_truncation)]
fn convert_lint_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    lint_diags: Vec<graphql_linter::LintDiagnostic>,
    rule_name: &str,
    configured_severity: Severity,
) -> Vec<Diagnostic> {
    use graphql_linter::DiagnosticSeverity as LintSev;

    let file_line_index = graphql_syntax::line_index(db, content);

    lint_diags
        .into_iter()
        .map(|ld| {
            let (line_offset, start_line, start_col, end_line, end_col) =
                if let (Some(block_line_offset), Some(ref block_source)) =
                    (ld.block_line_offset, &ld.block_source)
                {
                    let block_line_index = graphql_syntax::LineIndex::new(block_source);
                    let (sl, sc) = block_line_index.line_col(ld.offset_range.start);
                    let (el, ec) = block_line_index.line_col(ld.offset_range.end);
                    tracing::trace!(
                        block_line_offset,
                        offset_start = ld.offset_range.start,
                        offset_end = ld.offset_range.end,
                        start_line_in_block = sl,
                        start_col_in_block = sc,
                        final_line = sl + block_line_offset,
                        message = %ld.message,
                        "Converting block diagnostic"
                    );
                    (block_line_offset, sl, sc, el, ec)
                } else {
                    let (sl, sc) = file_line_index.line_col(ld.offset_range.start);
                    let (el, ec) = file_line_index.line_col(ld.offset_range.end);
                    (0, sl, sc, el, ec)
                };

            let severity = match ld.severity {
                LintSev::Error => Severity::Error,
                LintSev::Warning => configured_severity,
                LintSev::Info => Severity::Info,
            };

            Diagnostic {
                severity,
                message: ld.message.into(),
                range: DiagnosticRange {
                    start: Position {
                        line: start_line as u32 + line_offset as u32,
                        character: start_col as u32,
                    },
                    end: Position {
                        line: end_line as u32 + line_offset as u32,
                        character: end_col as u32,
                    },
                },
                source: "graphql-linter".into(),
                code: Some(rule_name.to_string().into()),
            }
        })
        .collect()
}
