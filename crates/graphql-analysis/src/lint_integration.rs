// Integration with graphql-linter using new Salsa-based architecture

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use graphql_db::{FileContent, FileId, FileKind, FileMetadata, ProjectFiles};
use std::collections::HashMap;
use std::sync::Arc;

/// Convert `LintSeverity` to Severity
const fn convert_severity(lint_severity: graphql_linter::LintSeverity) -> Severity {
    match lint_severity {
        graphql_linter::LintSeverity::Error => Severity::Error,
        graphql_linter::LintSeverity::Warn => Severity::Warning,
        graphql_linter::LintSeverity::Off => Severity::Info,
    }
}

/// Run lints on a file
///
/// This integrates with the new trait-based graphql-linter API.
/// This query is automatically memoized by Salsa - it will only re-run when:
/// - The file content changes
/// - The file metadata changes
/// - The lint configuration changes
#[salsa::tracked]
pub fn lint_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    // Parse the file (cached by Salsa!)
    let parse = graphql_syntax::parse(db, content, metadata);

    let uri = metadata.uri(db);
    tracing::debug!(uri = %uri, parse_errors = parse.errors.len(), "lint_file called");

    // Skip linting if there are parse errors
    if !parse.errors.is_empty() {
        tracing::debug!(uri = %uri, "Skipping linting due to parse errors");
        return Arc::new(diagnostics);
    }

    let file_id = metadata.file_id(db);
    let file_kind = metadata.kind(db);

    tracing::debug!(uri = %uri, ?file_kind, "Checking file kind");

    // Run lints based on file kind
    match file_kind {
        FileKind::ExecutableGraphQL | FileKind::TypeScript | FileKind::JavaScript => {
            // Get project files for linting
            let project_files_opt = db.project_files();
            tracing::debug!(uri = %uri, has_project_files = project_files_opt.is_some(), "Checking project_files");

            if let Some(project_files) = project_files_opt {
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
            } else {
                tracing::warn!(uri = %uri, "project_files is None, skipping lints!");
            }
        }
        FileKind::Schema => {
            // TODO: Run schema lints (naming conventions, etc.)
            tracing::trace!(uri = %uri, "Schema linting not yet implemented");
        }
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

    // Get all standalone document rules from registry
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

        // Run the rule (it will access parse via Salsa)
        let lint_diags = rule.check(db, file_id, content, metadata, project_files);

        if !lint_diags.is_empty() {
            tracing::debug!(
                rule = rule.name(),
                count = lint_diags.len(),
                "Found lint issues"
            );
        }

        // Convert to analysis Diagnostic format
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

    // Get all document+schema rules from registry
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

        // Run the rule (it has access to schema via project_files)
        let lint_diags = rule.check(db, file_id, content, metadata, project_files);

        if !lint_diags.is_empty() {
            tracing::debug!(
                rule = rule.name(),
                count = lint_diags.len(),
                "Found lint issues"
            );
        }

        // Convert to analysis Diagnostic format
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

#[salsa::tracked]
pub fn project_lint_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
) -> Arc<HashMap<FileId, Vec<Diagnostic>>> {
    let Some(project_files) = db.project_files() else {
        return Arc::new(HashMap::new());
    };

    let lint_config = db.lint_config();
    let mut diagnostics_by_file: HashMap<FileId, Vec<Diagnostic>> = HashMap::new();

    tracing::info!("Running project-wide lint rules");

    // Get all project rules from registry
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

        // Run the project-wide rule
        let lint_diags = rule.check(db, project_files);

        tracing::info!(
            rule = rule.name(),
            file_count = lint_diags.len(),
            "Project-wide rule returned diagnostics"
        );

        // Merge into result
        for (file_id, file_lint_diags) in lint_diags {
            // Find the FileContent for this FileId from project_files
            let Some(content) = find_file_content(db, project_files, file_id) else {
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

/// Helper to find `FileContent` for a given `FileId` from `ProjectFiles`
fn find_file_content(
    db: &dyn GraphQLAnalysisDatabase,
    project_files: ProjectFiles,
    file_id: FileId,
) -> Option<FileContent> {
    // Search in document files
    for (fid, content, _) in project_files.document_files(db).iter() {
        if *fid == file_id {
            return Some(*content);
        }
    }

    // Search in schema files
    for (fid, content, _) in project_files.schema_files(db).iter() {
        if *fid == file_id {
            return Some(*content);
        }
    }

    None
}

/// Convert `LintDiagnostic` (byte offsets) to `Diagnostic` (line/column)
#[allow(clippy::cast_possible_truncation)]
fn convert_lint_diagnostics(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    lint_diags: Vec<graphql_linter::LintDiagnostic>,
    rule_name: &str,
    configured_severity: Severity,
) -> Vec<Diagnostic> {
    use graphql_linter::DiagnosticSeverity as LintSev;

    let line_index = graphql_syntax::line_index(db, content);

    lint_diags
        .into_iter()
        .map(|ld| {
            // Convert byte offsets to line/column (0-based)
            let (start_line, start_col) = line_index.line_col(ld.offset_range.start);
            let (end_line, end_col) = line_index.line_col(ld.offset_range.end);

            // Use configured severity (allows override from config)
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
                        line: start_line as u32,
                        character: start_col as u32,
                    },
                    end: Position {
                        line: end_line as u32,
                        character: end_col as u32,
                    },
                },
                source: "graphql-linter".into(),
                code: Some(rule_name.to_string().into()),
            }
        })
        .collect()
}
