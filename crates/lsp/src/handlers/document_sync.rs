#![allow(clippy::needless_pass_by_value)]

use crate::conversions::convert_ide_diagnostic;
use crate::global_state::GlobalState;
#[cfg(feature = "native")]
use crate::loading;
use graphql_ide::{DocumentKind, Language};
#[cfg(feature = "native")]
use lsp_types::FileChangeType;
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, Uri,
};
use std::path::Path;
use std::str::FromStr;

pub(crate) fn handle_did_open(state: &mut GlobalState, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri;
    let content = params.text_document.text;
    let version = params.text_document.version;

    tracing::info!("File opened: {}", uri.path());

    let uri_string = uri.to_string();
    state
        .workspace
        .document_versions
        .insert(uri_string.clone(), version);
    state
        .workspace
        .document_contents
        .insert(uri_string.clone(), content.clone());

    let Some((workspace_uri, project_name)) = state.workspace.find_workspace_and_project(&uri)
    else {
        tracing::debug!("File not covered by any project config, ignoring");
        return;
    };

    state
        .workspace
        .file_to_project
        .insert(uri_string, (workspace_uri.clone(), project_name.clone()));

    let language = Language::from_path(Path::new(uri.path().as_str())).unwrap_or(Language::GraphQL);

    let document_kind = state
        .workspace
        .get_file_type(&uri, &workspace_uri, &project_name)
        .map_or(DocumentKind::Executable, |ft| match ft {
            graphql_config::FileType::Schema => DocumentKind::Schema,
            graphql_config::FileType::Document => DocumentKind::Executable,
        });

    let file_path = graphql_ide::FilePath::new(uri.to_string());
    let host = state
        .workspace
        .get_or_create_host(&workspace_uri, &project_name);
    let (is_new, snapshot) =
        host.update_file_and_snapshot(&file_path, &content, language, document_kind);

    // Only publish diagnostics if this is a new file (not already loaded during init).
    if is_new {
        let diagnostics: Vec<lsp_types::Diagnostic> = snapshot
            .all_diagnostics_for_file(&file_path)
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect();
        state.publish_diagnostics(uri, diagnostics, None);
    }
}

pub(crate) fn handle_did_change(state: &mut GlobalState, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri;
    let version = params.text_document.version;

    tracing::info!("File changed: {} (v{})", uri.path(), version);

    let uri_string = uri.to_string();
    if let Some(&current_version) = state.workspace.document_versions.get(&uri_string) {
        if version <= current_version {
            tracing::warn!(
                "Ignoring stale document update: version {} <= current {}",
                version,
                current_version
            );
            return;
        }
    }
    state
        .workspace
        .document_versions
        .insert(uri_string.clone(), version);

    let current_content = {
        let mut current_content = state
            .workspace
            .document_contents
            .get(&uri_string)
            .cloned()
            .unwrap_or_default();

        for change in &params.content_changes {
            current_content = crate::workspace::apply_content_change(&current_content, change);
        }

        state
            .workspace
            .document_contents
            .insert(uri_string.clone(), current_content.clone());

        current_content
    };

    let Some((workspace_uri, project_name)) = state.workspace.find_workspace_and_project(&uri)
    else {
        return;
    };

    let language = Language::from_path(Path::new(uri.path().as_str())).unwrap_or(Language::GraphQL);

    let document_kind = state
        .workspace
        .get_file_type(&uri, &workspace_uri, &project_name)
        .map_or(DocumentKind::Executable, |ft| match ft {
            graphql_config::FileType::Schema => DocumentKind::Schema,
            graphql_config::FileType::Document => DocumentKind::Executable,
        });

    let file_path = graphql_ide::FilePath::new(uri.to_string());
    let host = state
        .workspace
        .get_or_create_host(&workspace_uri, &project_name);
    let (_is_new, snapshot) =
        host.update_file_and_snapshot(&file_path, &current_content, language, document_kind);

    let file_path_clone = graphql_ide::FilePath::new(uri.as_str());
    state.spawn_diagnostics_for_uri(uri, move || {
        snapshot
            .diagnostics(&file_path_clone)
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect()
    });
}

pub(crate) fn handle_did_save(state: &mut GlobalState, params: DidSaveTextDocumentParams) {
    if let Some(ref manager) = state.trace_capture {
        if manager.check_auto_stop() {
            state.send_notification::<lsp_types::notification::LogMessage>(
                lsp_types::LogMessageParams {
                    typ: lsp_types::MessageType::INFO,
                    message: "Trace capture auto-stopped after 60s timeout".to_owned(),
                },
            );
        }
    }

    let uri = params.text_document.uri;
    tracing::info!("File saved: {}", uri.path());

    let Some((workspace_uri, project_name)) = state.workspace.find_workspace_and_project(&uri)
    else {
        return;
    };

    let Some(host) = state.workspace.get_host(&workspace_uri, &project_name) else {
        return;
    };

    let snapshot = host.snapshot();
    let changed_file = graphql_ide::FilePath::new(uri.as_str());

    state.spawn_diagnostics_batch(move || {
        let all_diagnostics = snapshot.all_diagnostics_for_change(&changed_file);
        all_diagnostics
            .into_iter()
            .filter_map(|(file_path, diags)| {
                let file_uri = Uri::from_str(file_path.as_str()).ok()?;
                let lsp_diagnostics = diags.into_iter().map(convert_ide_diagnostic).collect();
                Some((file_uri, lsp_diagnostics))
            })
            .collect()
    });
}

pub(crate) fn handle_did_close(state: &mut GlobalState, params: DidCloseTextDocumentParams) {
    tracing::info!("File closed: {}", params.text_document.uri.path());
    let uri_string = params.text_document.uri.to_string();
    state.workspace.document_versions.remove(&uri_string);
    state.workspace.document_contents.remove(&uri_string);
}

pub(crate) fn handle_did_change_watched_files(
    state: &mut GlobalState,
    params: DidChangeWatchedFilesParams,
) {
    #[cfg(feature = "native")]
    {
        tracing::debug!("Watched files changed: {} file(s)", params.changes.len());

        for change in params.changes {
            let uri = change.uri;
            tracing::debug!("File changed: {} (type: {:?})", uri.path(), change.typ);

            let Some(config_path) = crate::conversions::uri_to_file_path(&uri) else {
                tracing::warn!("Failed to convert URI to file path: {:?}", uri);
                continue;
            };

            let workspace_uri: Option<String> = state
                .workspace
                .config_paths
                .iter()
                .find(|(_, path)| **path == config_path)
                .map(|(ws_uri, _)| ws_uri.clone());

            if let Some(workspace_uri) = workspace_uri {
                match change.typ {
                    FileChangeType::CREATED | FileChangeType::CHANGED => {
                        tracing::info!("Config file changed for workspace: {}", workspace_uri);
                        loading::reload_workspace_config(state, &workspace_uri);
                    }
                    FileChangeType::DELETED => {
                        tracing::warn!("Config file deleted for workspace: {}", workspace_uri);
                        state.send_notification::<lsp_types::notification::ShowMessage>(
                            lsp_types::ShowMessageParams {
                                typ: lsp_types::MessageType::WARNING,
                                message: "GraphQL config file was deleted".to_owned(),
                            },
                        );
                    }
                    _ => {}
                }
                continue;
            }

            let resolved_match: Option<(String, String)> = state
                .workspace
                .resolved_schema_paths
                .iter()
                .find(|(_, path)| **path == config_path)
                .map(|((ws, proj), _)| (ws.clone(), proj.clone()));

            if let Some((ws_uri, proj_name)) = resolved_match {
                if matches!(
                    change.typ,
                    FileChangeType::CREATED | FileChangeType::CHANGED
                ) {
                    loading::reload_resolved_schema(state, &ws_uri, &proj_name, &config_path);
                }
            }
        }
    }

    // Under wasm the server has no access to the host filesystem, so watched-file
    // notifications are a no-op. Config loading happens via init options instead.
    #[cfg(not(feature = "native"))]
    let _ = (state, params);
}
