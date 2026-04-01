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
    let uri = metadata.uri(db);
    let _span = tracing::debug_span!("lint_file", uri = %uri).entered();
    let mut diagnostics = Vec::new();

    let parse = graphql_syntax::parse(db, content, metadata);

    tracing::debug!(parse_errors = parse.errors().len(), "lint_file called");

    if parse.has_errors() {
        tracing::debug!(uri = %uri, "Skipping linting due to parse errors");
        return Arc::new(diagnostics);
    }

    let file_id = metadata.file_id(db);
    let document_kind = metadata.document_kind(db);

    tracing::debug!(uri = %uri, ?document_kind, "Checking document kind");

    if metadata.is_document(db) {
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
    } else if metadata.is_schema(db) {
        tracing::debug!(uri = %uri, "Running schema lints");
        diagnostics.extend(schema_lints(db, file_id, content, project_files));
    }

    diagnostics.extend(unused_ignore_diagnostics(
        db,
        content,
        metadata,
        project_files,
    ));

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

        let options = lint_config.get_options(rule.name());
        let lint_diags = rule.check(db, file_id, content, metadata, project_files, options);

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

        let options = lint_config.get_options(rule.name());
        let lint_diags = rule.check(db, file_id, content, metadata, project_files, options);

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
fn schema_lints(
    db: &dyn GraphQLAnalysisDatabase,
    file_id: FileId,
    content: FileContent,
    project_files: ProjectFiles,
) -> Vec<Diagnostic> {
    let lint_config = db.lint_config();
    let mut diagnostics = Vec::new();
    let mut enabled_count = 0;

    for rule in graphql_linter::standalone_schema_rules() {
        let enabled = lint_config.is_enabled(rule.name());
        tracing::debug!(
            rule = rule.name(),
            enabled = enabled,
            "Checking schema rule"
        );

        if !enabled {
            continue;
        }

        enabled_count += 1;
        let options = lint_config.get_options(rule.name());
        let lint_diags = rule.check(db, project_files, options);

        if let Some(file_lint_diags) = lint_diags.get(&file_id) {
            if !file_lint_diags.is_empty() {
                tracing::debug!(
                    rule = rule.name(),
                    count = file_lint_diags.len(),
                    "Found schema lint issues"
                );
            }

            let severity = lint_config
                .get_severity(rule.name())
                .map_or(Severity::Warning, convert_severity);
            diagnostics.extend(convert_lint_diagnostics(
                db,
                content,
                file_lint_diags.clone(),
                rule.name(),
                severity,
            ));
        }
    }

    tracing::debug!(enabled_rules = enabled_count, "Schema linting complete");

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
    let _span = tracing::debug_span!("project_lint_diagnostics").entered();
    let lint_config = db.lint_config();
    let mut diagnostics_by_file: HashMap<FileId, Vec<Diagnostic>> = HashMap::new();

    for rule in graphql_linter::project_rules() {
        let enabled = lint_config.is_enabled(rule.name());
        if !enabled {
            tracing::debug!(rule = rule.name(), "Project rule disabled, skipping");
            continue;
        }

        let _rule_span = tracing::debug_span!("project_rule", rule_name = rule.name()).entered();

        let options = lint_config.get_options(rule.name());
        let lint_diags = rule.check(db, project_files, options);

        tracing::debug!(
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

    // Parse and check for errors
    let parse = graphql_syntax::parse(db, content, metadata);
    if parse.has_errors() {
        return all_diagnostics;
    }

    // Run lints based on document kind
    if metadata.is_document(db) {
        // Standalone document lints
        for rule in graphql_linter::standalone_document_rules() {
            if lint_config.is_enabled(rule.name()) {
                let options = lint_config.get_options(rule.name());
                all_diagnostics.extend(rule.check(
                    db,
                    file_id,
                    content,
                    metadata,
                    project_files,
                    options,
                ));
            }
        }

        // Document+schema lints
        for rule in graphql_linter::document_schema_rules() {
            if lint_config.is_enabled(rule.name()) {
                let options = lint_config.get_options(rule.name());
                all_diagnostics.extend(rule.check(
                    db,
                    file_id,
                    content,
                    metadata,
                    project_files,
                    options,
                ));
            }
        }
    }

    // Schema lints
    if metadata.is_schema(db) {
        for rule in graphql_linter::standalone_schema_rules() {
            if lint_config.is_enabled(rule.name()) {
                let options = lint_config.get_options(rule.name());
                let lint_diags = rule.check(db, project_files, options);
                if let Some(file_lint_diags) = lint_diags.get(&file_id) {
                    all_diagnostics.extend(file_lint_diags.iter().cloned());
                }
            }
        }
    }

    filter_suppressed_diagnostics(db, content, all_diagnostics)
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
        let options = lint_config.get_options(rule.name());
        let lint_diags = rule.check(db, project_files, options);

        // Merge into result
        for (file_id, file_lint_diags) in lint_diags {
            diagnostics_by_file
                .entry(file_id)
                .or_default()
                .extend(file_lint_diags);
        }
    }

    // Filter suppressed diagnostics per file
    for (file_id, diags) in &mut diagnostics_by_file {
        if let Some((content, _)) = find_file_content_and_metadata(db, project_files, *file_id) {
            let filtered = filter_suppressed_diagnostics(db, content, std::mem::take(diags));
            *diags = filtered;
        }
    }

    diagnostics_by_file
}

/// Produce diagnostics for unused `# graphql-analyzer-ignore` comments.
///
/// Collects all raw lint diagnostics (before filtering) to determine which
/// ignore directives actually suppressed something. Any directive that didn't
/// match a diagnostic is reported as a warning.
fn unused_ignore_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
    project_files: ProjectFiles,
) -> Vec<Diagnostic> {
    let file_text = content.text(db);
    let file_line_index = graphql_syntax::line_index(db, content);
    let file_ignores = graphql_linter::ignore::parse_ignore_directives(&file_text);

    if file_ignores.is_empty() {
        return Vec::new();
    }

    // Collect all raw lint diagnostics to determine what each ignore could match
    let lint_config = db.lint_config();
    let file_id = metadata.file_id(db);
    let mut all_raw_diags: Vec<(usize, String)> = Vec::new();

    let collect_line_and_rule = |diag: &graphql_linter::LintDiagnostic| -> (usize, String) {
        if let Some(ref block_source) = diag.span.source {
            let idx = graphql_syntax::LineIndex::new(block_source);
            let (sl, _) = idx.line_col(diag.span.start);
            (sl, diag.rule.clone())
        } else {
            let (sl, _) = file_line_index.line_col(diag.span.start);
            (sl, diag.rule.clone())
        }
    };

    if metadata.is_document(db) {
        for rule in graphql_linter::standalone_document_rules() {
            if lint_config.is_enabled(rule.name()) {
                let options = lint_config.get_options(rule.name());
                for d in rule.check(db, file_id, content, metadata, project_files, options) {
                    all_raw_diags.push(collect_line_and_rule(&d));
                }
            }
        }
        for rule in graphql_linter::document_schema_rules() {
            if lint_config.is_enabled(rule.name()) {
                let options = lint_config.get_options(rule.name());
                for d in rule.check(db, file_id, content, metadata, project_files, options) {
                    all_raw_diags.push(collect_line_and_rule(&d));
                }
            }
        }
    }

    let diag_refs: Vec<(usize, &str)> = all_raw_diags
        .iter()
        .map(|(line, rule)| (*line, rule.as_str()))
        .collect();

    let unused = graphql_linter::ignore::find_unused_rules(&file_ignores, &diag_refs);

    unused
        .into_iter()
        .flat_map(|u| match u {
            graphql_linter::ignore::UnusedIgnore::EntireDirective(d) => {
                let (start_line, start_col) = file_line_index.line_col(d.byte_offset);
                let (end_line, end_col) = file_line_index.line_col(d.byte_end);
                vec![Diagnostic {
                    severity: Severity::Warning,
                    message: "Unused graphql-analyzer-ignore directive".into(),
                    range: DiagnosticRange {
                        start: Position {
                            line: start_line as u32,
                            character: start_col as u32,
                        },
                        end: Position {
                            line: end_line as u32,
                            character: end_col as u32,
                        },
                    },
                    source: "graphql-linter".into(),
                    code: Some("unused_ignore".into()),
                    help: None,
                    url: None,
                    tags: vec![crate::DiagnosticTag::Unnecessary],
                }]
            }
            graphql_linter::ignore::UnusedIgnore::UnusedRules { rules, .. } => rules
                .into_iter()
                .map(|r| {
                    let (start_line, start_col) = file_line_index.line_col(r.byte_offset);
                    let (end_line, end_col) = file_line_index.line_col(r.byte_end);
                    Diagnostic {
                        severity: Severity::Warning,
                        message: format!(
                            "Unused rule '{}' in graphql-analyzer-ignore directive",
                            r.name
                        )
                        .into(),
                        range: DiagnosticRange {
                            start: Position {
                                line: start_line as u32,
                                character: start_col as u32,
                            },
                            end: Position {
                                line: end_line as u32,
                                character: end_col as u32,
                            },
                        },
                        source: "graphql-linter".into(),
                        code: Some("unused_ignore".into()),
                        help: None,
                        url: None,
                        tags: vec![crate::DiagnosticTag::Unnecessary],
                    }
                })
                .collect(),
        })
        .collect()
}

/// Filter raw `LintDiagnostic`s, removing those suppressed by ignore comments.
fn filter_suppressed_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    diagnostics: Vec<graphql_linter::LintDiagnostic>,
) -> Vec<graphql_linter::LintDiagnostic> {
    let file_text = content.text(db);
    let file_line_index = graphql_syntax::line_index(db, content);
    let file_ignores = graphql_linter::ignore::parse_ignore_directives(&file_text);

    diagnostics
        .into_iter()
        .filter(|ld| {
            if let Some(ref block_source) = ld.span.source {
                let block_line_index = graphql_syntax::LineIndex::new(block_source);
                let (sl, _) = block_line_index.line_col(ld.span.start);
                let block_ignores = graphql_linter::ignore::parse_ignore_directives(block_source);
                !graphql_linter::ignore::is_suppressed(&block_ignores, sl, &ld.rule)
            } else {
                let (sl, _) = file_line_index.line_col(ld.span.start);
                !graphql_linter::ignore::is_suppressed(&file_ignores, sl, &ld.rule)
            }
        })
        .collect()
}

/// Convert `LintDiagnostic` (byte offsets) to `Diagnostic` (line/column),
/// filtering out diagnostics suppressed by ignore comments.
///
/// Each `LintDiagnostic` carries a `SourceSpan` which bundles byte offsets with block context
/// (for embedded GraphQL in TS/JS). When block context is present:
/// - `span.start/end` are relative to `span.source`, not the full file
/// - We build a `LineIndex` from `span.source` to convert byte offsets to line/column
/// - We add `span.line_offset` to get the correct position in the original file
///
/// For pure GraphQL files (no block context), we use the full file's `LineIndex`.
///
/// Diagnostics preceded by `# graphql-analyzer-ignore` comments are filtered out.
fn convert_lint_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    lint_diags: Vec<graphql_linter::LintDiagnostic>,
    rule_name: &str,
    configured_severity: Severity,
) -> Vec<Diagnostic> {
    use graphql_linter::DiagnosticSeverity as LintSev;

    let file_text = content.text(db);
    let file_line_index = graphql_syntax::line_index(db, content);
    let file_ignores = graphql_linter::ignore::parse_ignore_directives(&file_text);

    lint_diags
        .into_iter()
        .filter_map(|ld| {
            let (line_offset, start_line, start_col, end_line, end_col, suppressed) =
                if let Some(ref block_source) = ld.span.source {
                    let block_line_index = graphql_syntax::LineIndex::new(block_source);
                    let (sl, sc) = block_line_index.line_col(ld.span.start);
                    let (el, ec) = block_line_index.line_col(ld.span.end);
                    tracing::trace!(
                        line_offset = ld.span.line_offset,
                        offset_start = ld.span.start,
                        offset_end = ld.span.end,
                        start_line_in_block = sl,
                        start_col_in_block = sc,
                        final_line = sl + ld.span.line_offset as usize,
                        message = %ld.message,
                        "Converting block diagnostic"
                    );
                    // For embedded blocks, parse ignore directives from the block source
                    let block_ignores =
                        graphql_linter::ignore::parse_ignore_directives(block_source);
                    let suppressed =
                        graphql_linter::ignore::is_suppressed(&block_ignores, sl, rule_name);
                    (ld.span.line_offset, sl, sc, el, ec, suppressed)
                } else {
                    let (sl, sc) = file_line_index.line_col(ld.span.start);
                    let (el, ec) = file_line_index.line_col(ld.span.end);
                    let suppressed =
                        graphql_linter::ignore::is_suppressed(&file_ignores, sl, rule_name);
                    (0u32, sl, sc, el, ec, suppressed)
                };

            if suppressed {
                tracing::debug!(
                    rule = rule_name,
                    line = start_line,
                    "Lint diagnostic suppressed by ignore comment"
                );
                return None;
            }

            let severity = match ld.severity {
                LintSev::Error => Severity::Error,
                LintSev::Warning => configured_severity,
                LintSev::Info => Severity::Info,
            };

            Some(Diagnostic {
                severity,
                message: ld.message.into(),
                range: DiagnosticRange {
                    start: Position {
                        line: start_line as u32 + line_offset,
                        character: start_col as u32,
                    },
                    end: Position {
                        line: end_line as u32 + line_offset,
                        character: end_col as u32,
                    },
                },
                source: "graphql-linter".into(),
                code: Some(rule_name.to_string().into()),
                help: ld.help.map(Into::into),
                url: ld.url.map(Into::into),
                tags: ld
                    .tags
                    .into_iter()
                    .map(|t| match t {
                        graphql_linter::DiagnosticTag::Unnecessary => {
                            crate::DiagnosticTag::Unnecessary
                        }
                        graphql_linter::DiagnosticTag::Deprecated => {
                            crate::DiagnosticTag::Deprecated
                        }
                    })
                    .collect(),
            })
        })
        .collect()
}
