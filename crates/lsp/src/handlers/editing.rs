use crate::conversions::{
    convert_ide_completion_item, convert_ide_diagnostic, convert_ide_range,
    convert_ide_signature_help, convert_lsp_position,
};
use crate::server::GraphQLLanguageServer;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    CompletionParams, CompletionResponse, ExecuteCommandParams, MessageType, PrepareRenameResponse,
    RenameParams, SignatureHelpParams, TextDocumentPositionParams, TextEdit, Uri, WorkspaceEdit,
};
use std::collections::HashMap;
use std::str::FromStr;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types as lsp_types;

pub(crate) async fn handle_completion(
    server: &GraphQLLanguageServer,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let uri = params.text_document_position.text_document.uri;
    let lsp_position = params.text_document_position.position;
    let position = convert_lsp_position(lsp_position);

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let items = analysis.completions(&file_path, position)?;
            let lsp_items: Vec<lsp_types::CompletionItem> =
                items.into_iter().map(convert_ide_completion_item).collect();
            Some(CompletionResponse::Array(lsp_items))
        })
        .await
}

pub(crate) async fn handle_signature_help(
    server: &GraphQLLanguageServer,
    params: SignatureHelpParams,
) -> Result<Option<lsp_types::SignatureHelp>> {
    let uri = params.text_document_position_params.text_document.uri;
    let lsp_position = params.text_document_position_params.position;
    let position = convert_lsp_position(lsp_position);

    server
        .with_analysis(&uri, move |analysis, file_path| {
            analysis
                .signature_help(&file_path, position)
                .map(convert_ide_signature_help)
        })
        .await
}

pub(crate) async fn handle_prepare_rename(
    server: &GraphQLLanguageServer,
    params: TextDocumentPositionParams,
) -> Result<Option<PrepareRenameResponse>> {
    let uri = params.text_document.uri;
    let position = convert_lsp_position(params.position);

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let range = analysis.prepare_rename(&file_path, position)?;
            Some(PrepareRenameResponse::Range(convert_ide_range(range)))
        })
        .await
}

pub(crate) async fn handle_rename(
    server: &GraphQLLanguageServer,
    params: RenameParams,
) -> Result<Option<WorkspaceEdit>> {
    let uri = params.text_document_position.text_document.uri;
    let lsp_position = params.text_document_position.position;
    let position = convert_lsp_position(lsp_position);
    let new_name = params.new_name;

    server
        .with_analysis(&uri, move |analysis, file_path| {
            let result = analysis.rename(&file_path, position, &new_name)?;
            let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
            for (ide_path, edits) in result.changes {
                let uri: Uri = ide_path.as_str().parse().ok()?;
                let lsp_edits = edits
                    .into_iter()
                    .map(|edit| TextEdit {
                        range: convert_ide_range(edit.range),
                        new_text: edit.new_text,
                    })
                    .collect();
                changes.insert(uri, lsp_edits);
            }
            Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            })
        })
        .await
}

#[allow(
    clippy::uninlined_format_args,
    clippy::single_match_else,
    clippy::option_if_let_else,
    clippy::manual_string_new,
    clippy::manual_map
)]
pub(crate) async fn handle_execute_command(
    server: &GraphQLLanguageServer,
    params: ExecuteCommandParams,
) -> Result<Option<serde_json::Value>> {
    tracing::info!("Execute command requested: {}", params.command);

    if params.command.as_str() == "graphql-analyzer.checkStatus" {
        let mut status_lines = Vec::new();
        let mut total_projects = 0;

        for workspace_entry in &server.workspace.workspace_roots {
            let workspace_uri = workspace_entry.key();
            let workspace_path = workspace_entry.value();

            status_lines.push(format!("Workspace: {}", workspace_path.display()));

            if let Some(config_path) = server.workspace.config_paths.get(workspace_uri) {
                status_lines.push(format!(
                    "  Config: {}",
                    config_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                ));
            }

            let workspace_projects = server.workspace.projects_for_workspace(workspace_uri);

            if workspace_projects.is_empty() {
                status_lines.push("  Projects: (none loaded)".to_string());
            } else {
                status_lines.push(format!("  Projects: {}", workspace_projects.len()));
                total_projects += workspace_projects.len();

                for (project_name, host) in workspace_projects {
                    if let Some(snapshot) = host.try_snapshot().await {
                        let status = snapshot.project_status();
                        let schema_status = if status.has_schema() {
                            "loaded"
                        } else {
                            "missing"
                        };
                        status_lines.push(format!(
                            "    - {}: {} schema file(s), {} document(s), schema {}",
                            project_name,
                            status.schema_file_count,
                            status.document_file_count,
                            schema_status
                        ));
                    } else {
                        status_lines.push(format!("    - {}: (busy)", project_name));
                    }
                }
            }
        }

        let status_report = status_lines.join("\n");

        let full_report = format!("\n=== GraphQL LSP Status ===\n{}\n", status_report);
        tracing::info!("{}", full_report);

        server
            .client
            .log_message(MessageType::INFO, full_report)
            .await;

        let summary = if server.workspace.workspace_roots.is_empty() {
            "No workspaces loaded".to_string()
        } else {
            let workspace_count = server.workspace.workspace_roots.len();
            format!(
                "{} workspace(s), {} project(s) - Check output for details",
                workspace_count, total_projects
            )
        };

        server.client.show_message(MessageType::INFO, summary).await;

        Ok(Some(serde_json::json!({ "success": true })))
    } else {
        tracing::warn!("Unknown command: {}", params.command);
        Ok(None)
    }
}

#[allow(clippy::mutable_key_type)]
pub(crate) async fn handle_code_action(
    server: &GraphQLLanguageServer,
    params: CodeActionParams,
) -> Result<Option<CodeActionResponse>> {
    let uri = params.text_document.uri;
    let range = params.range;

    server
        .with_analysis(&uri, move |analysis, file_path| {
            // Get lint diagnostics with fixes for this file (per-file rules)
            let mut lint_diagnostics = analysis.lint_diagnostics_with_fixes(&file_path);

            // Also get project-level diagnostics for this file (e.g., unused_fragments)
            let project_diagnostics = analysis.project_lint_diagnostics_with_fixes();
            if let Some(project_diags_for_file) = project_diagnostics.get(&file_path) {
                lint_diagnostics.extend(project_diags_for_file.iter().cloned());
            }

            if lint_diagnostics.is_empty() {
                return None;
            }

            // Convert LSP range to line/column for comparison
            let start_line = range.start.line as usize;
            let end_line = range.end.line as usize;

            let mut actions: Vec<CodeActionOrCommand> = Vec::new();

            let content = analysis.file_content(&file_path)?;

            let file_line_index = graphql_syntax::LineIndex::new(&content);
            // Reconstruct URI for workspace edit keys. If parsing fails — which
            // can happen for virtual paths or in-memory schemes that don't
            // round-trip cleanly through `Uri::from_str` — log and skip rather
            // than panicking the spawn_blocking worker.
            let uri = match Uri::from_str(&file_path.0) {
                Ok(uri) => uri,
                Err(e) => {
                    tracing::warn!(
                        path = %file_path.0,
                        error = %e,
                        "code_action: failed to parse FilePath as URI, skipping",
                    );
                    return None;
                }
            };

            for diag in lint_diagnostics {
                let Some(ref fix) = diag.fix else {
                    continue;
                };

                // For embedded GraphQL (TypeScript/JavaScript), offsets are relative to the
                // GraphQL block, not the full file. Use block context from SourceSpan.
                let (line_offset, diag_line_index): (
                    u32,
                    std::borrow::Cow<'_, graphql_syntax::LineIndex>,
                ) = if let Some(ref block_source) = diag.span.source {
                    (
                        diag.span.line_offset,
                        std::borrow::Cow::Owned(graphql_syntax::LineIndex::new(block_source)),
                    )
                } else {
                    (0, std::borrow::Cow::Borrowed(&file_line_index))
                };

                let (diag_start_line, _) = diag_line_index.line_col(diag.span.start);
                let (diag_end_line, _) = diag_line_index.line_col(diag.span.end);
                let diag_start_line = diag_start_line + line_offset as usize;
                let diag_end_line = diag_end_line + line_offset as usize;

                if diag_end_line < start_line || diag_start_line > end_line {
                    continue;
                }

                let edits: Vec<TextEdit> = fix
                    .edits
                    .iter()
                    .map(|edit| {
                        let (start_line, start_col) =
                            diag_line_index.line_col(edit.offset_range.start);
                        let (end_line, end_col) = diag_line_index.line_col(edit.offset_range.end);

                        TextEdit {
                            range: lsp_types::Range {
                                start: lsp_types::Position {
                                    line: (start_line + line_offset as usize) as u32,
                                    character: start_col as u32,
                                },
                                end: lsp_types::Position {
                                    line: (end_line + line_offset as usize) as u32,
                                    character: end_col as u32,
                                },
                            },
                            new_text: edit.new_text.clone(),
                        }
                    })
                    .collect();

                let mut changes = HashMap::new();
                changes.insert(uri.clone(), edits);

                let workspace_edit = WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                };

                let action = CodeAction {
                    title: fix.label.clone(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![convert_ide_diagnostic(graphql_ide::Diagnostic {
                        range: graphql_ide::Range {
                            start: graphql_ide::Position {
                                line: diag_start_line as u32,
                                character: 0,
                            },
                            end: graphql_ide::Position {
                                line: diag_end_line as u32,
                                character: 0,
                            },
                        },
                        severity: graphql_ide::DiagnosticSeverity::Warning,
                        message: diag.message.clone(),
                        code: Some(diag.rule.clone()),
                        source: "graphql-linter".to_string(),
                        fix: None,
                        help: diag.help.clone(),
                        url: diag.url.clone(),
                        tags: diag
                            .tags
                            .iter()
                            .map(|t| match t {
                                graphql_linter::DiagnosticTag::Unnecessary => {
                                    graphql_ide::DiagnosticTag::Unnecessary
                                }
                                graphql_linter::DiagnosticTag::Deprecated => {
                                    graphql_ide::DiagnosticTag::Deprecated
                                }
                            })
                            .collect(),
                        related: Vec::new(),
                    })]),
                    edit: Some(workspace_edit),
                    command: None,
                    is_preferred: Some(true),
                    disabled: None,
                    data: None,
                };

                actions.push(CodeActionOrCommand::CodeAction(action));
            }

            if actions.is_empty() {
                None
            } else {
                Some(actions)
            }
        })
        .await
}
