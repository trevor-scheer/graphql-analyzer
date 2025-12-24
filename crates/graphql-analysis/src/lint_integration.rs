// Integration with graphql-linter

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use graphql_db::{FileContent, FileKind, FileMetadata};
use std::sync::Arc;

/// Run lints on a file
/// This integrates with the existing graphql-linter crate
#[salsa::tracked]
pub fn lint_file(
    db: &dyn GraphQLAnalysisDatabase,
    content: FileContent,
    metadata: FileMetadata,
) -> Arc<Vec<Diagnostic>> {
    let lint_config = db.lint_config();
    let mut diagnostics = Vec::new();

    // Get content text and file URI
    let content_text = content.text(db);
    let file_uri = metadata.uri(db);
    let file_name = file_uri.as_str();
    let file_kind = metadata.kind(db);

    // Parse the file
    let parse = graphql_syntax::parse(db, content, metadata);

    // Skip linting if there are parse errors
    if !parse.errors.is_empty() {
        tracing::debug!("Skipping linting due to parse errors");
        return Arc::new(diagnostics);
    }

    // Convert our LintConfig to graphql_linter::LintConfig
    let mut rules = std::collections::HashMap::new();
    for (rule_name, severity) in &lint_config.enabled_rules {
        let lint_severity = match severity {
            Severity::Error => graphql_linter::LintSeverity::Error,
            Severity::Warning | Severity::Info => graphql_linter::LintSeverity::Warn,
        };
        rules.insert(
            rule_name.clone(),
            graphql_linter::LintRuleConfig::Severity(lint_severity),
        );
    }

    let linter_config = graphql_linter::LintConfig::Rules { rules };
    let linter = graphql_linter::Linter::new(linter_config);

    // Run lints based on file kind
    match file_kind {
        FileKind::ExecutableGraphQL | FileKind::TypeScript | FileKind::JavaScript => {
            // For executable GraphQL, run standalone document lints
            // TODO: Also run document+schema lints when we have SchemaIndex available
            let lint_diagnostics = linter.lint_standalone_document(
                content_text.as_ref(),
                file_name,
                None, // TODO: Pass DocumentIndex for fragment resolution
                Some(&parse.tree),
            );

            // Convert graphql_project::Diagnostic to our Diagnostic type
            for lint_diag in lint_diagnostics {
                diagnostics.push(convert_lint_diagnostic(&lint_diag));
            }
        }
        FileKind::Schema => {
            // TODO: Run schema lints when we have support for that
            tracing::debug!("Schema linting not yet implemented");
        }
    }

    tracing::debug!(
        file = file_name,
        diagnostics = diagnostics.len(),
        "Linting complete"
    );

    Arc::new(diagnostics)
}

/// Convert `graphql_project::Diagnostic` to our Diagnostic type
#[allow(clippy::cast_possible_truncation)]
fn convert_lint_diagnostic(lint_diag: &graphql_project::Diagnostic) -> Diagnostic {
    let severity = match lint_diag.severity {
        graphql_project::Severity::Error => Severity::Error,
        graphql_project::Severity::Warning => Severity::Warning,
        graphql_project::Severity::Information | graphql_project::Severity::Hint => Severity::Info,
    };

    Diagnostic {
        severity,
        message: lint_diag.message.clone().into(),
        range: DiagnosticRange {
            start: Position {
                line: lint_diag.range.start.line as u32,
                character: lint_diag.range.start.character as u32,
            },
            end: Position {
                line: lint_diag.range.end.line as u32,
                character: lint_diag.range.end.character as u32,
            },
        },
        source: lint_diag.source.clone().into(),
        code: lint_diag.code.as_ref().map(|c| c.clone().into()),
    }
}
