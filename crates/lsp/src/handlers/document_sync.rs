use crate::conversions::convert_ide_diagnostic;
use crate::server::GraphQLLanguageServer;
use graphql_ide::{DocumentKind, Language};
use lsp_types::{
    Diagnostic, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    FileChangeType, MessageType, Uri,
};
use std::path::Path;
use std::str::FromStr;
use tower_lsp_server::ls_types as lsp_types;

pub(crate) async fn handle_did_open(
    server: &GraphQLLanguageServer,
    params: DidOpenTextDocumentParams,
) {
    let uri = params.text_document.uri;
    let content = params.text_document.text;
    let version = params.text_document.version;

    tracing::info!("File opened: {}", uri.path());

    let uri_string = uri.to_string();
    server
        .workspace
        .document_versions
        .insert(uri_string.clone(), version);
    server
        .workspace
        .document_contents
        .insert(uri_string.clone(), content.clone());

    let Some((workspace_uri, project_name)) = server.workspace.find_workspace_and_project(&uri)
    else {
        // File is not covered by any project's schema or documents patterns - ignore it
        tracing::debug!("File not covered by any project config, ignoring");
        return;
    };

    server
        .workspace
        .file_to_project
        .insert(uri_string, (workspace_uri.clone(), project_name.clone()));

    let host = server
        .workspace
        .get_or_create_host(&workspace_uri, &project_name);

    // Determine language from file extension
    let language = Language::from_path(Path::new(uri.path().as_str())).unwrap_or(Language::GraphQL);

    // Get DocumentKind from config (schema vs documents pattern)
    let document_kind = server
        .workspace
        .get_file_type(&uri, &workspace_uri, &project_name)
        .map_or(DocumentKind::Executable, |ft| match ft {
            graphql_config::FileType::Schema => DocumentKind::Schema,
            graphql_config::FileType::Document => DocumentKind::Executable,
        });

    let final_content = content;

    // Update file and get snapshot in one lock
    // Uses add_file_and_snapshot which properly rebuilds project files if needed
    let file_path = graphql_ide::FilePath::new(uri.to_string());
    let (is_new, snapshot) = host
        .add_file_and_snapshot(&file_path, &final_content, language, document_kind)
        .await;

    // Only publish diagnostics if this is a new file (not already loaded during init).
    // Files loaded during init already have diagnostics published (including project-wide).
    // Re-publishing here would overwrite project-wide diagnostics with only per-file ones.
    if is_new {
        if let Some(lsp_diagnostics) = GraphQLLanguageServer::blocking(move || {
            snapshot
                .all_diagnostics_for_file(&file_path)
                .into_iter()
                .map(convert_ide_diagnostic)
                .collect::<Vec<Diagnostic>>()
        })
        .await
        {
            server
                .client
                .publish_diagnostics(uri, lsp_diagnostics, None)
                .await;
        }
    }
}

pub(crate) async fn handle_did_change(
    server: &GraphQLLanguageServer,
    params: DidChangeTextDocumentParams,
) {
    let uri = params.text_document.uri;
    let version = params.text_document.version;

    tracing::info!("File changed: {} (v{})", uri.path(), version);

    let uri_string = uri.to_string();
    if let Some(current_version) = server.workspace.document_versions.get(&uri_string) {
        if version <= *current_version {
            tracing::warn!(
                "Ignoring stale document update: version {} <= current {}",
                version,
                *current_version
            );
            return;
        }
    }
    server
        .workspace
        .document_versions
        .insert(uri_string.clone(), version);

    let current_content = {
        let _span =
            tracing::info_span!("apply_changes", changes = params.content_changes.len()).entered();

        let mut current_content = server
            .workspace
            .document_contents
            .get(&uri_string)
            .map(|v| v.clone())
            .unwrap_or_default();

        for change in &params.content_changes {
            current_content = crate::workspace::apply_content_change(&current_content, change);
        }

        server
            .workspace
            .document_contents
            .insert(uri_string.clone(), current_content.clone());

        current_content
    };

    let Some((workspace_uri, project_name)) = server.workspace.find_workspace_and_project(&uri)
    else {
        return;
    };

    let host = server
        .workspace
        .get_or_create_host(&workspace_uri, &project_name);

    // Determine language from file extension
    let language = Language::from_path(Path::new(uri.path().as_str())).unwrap_or(Language::GraphQL);

    // Get DocumentKind from config (schema vs documents pattern)
    let document_kind = server
        .workspace
        .get_file_type(&uri, &workspace_uri, &project_name)
        .map_or(DocumentKind::Executable, |ft| match ft {
            graphql_config::FileType::Schema => DocumentKind::Schema,
            graphql_config::FileType::Document => DocumentKind::Executable,
        });

    let file_path = graphql_ide::FilePath::new(uri.to_string());
    let (_is_new, snapshot) = {
        use tracing::Instrument;
        async {
            host.add_file_and_snapshot(&file_path, &current_content, language, document_kind)
                .await
        }
        .instrument(tracing::info_span!(
            "update_file_and_snapshot",
            document_kind = ?document_kind,
        ))
        .await
    };

    let file_path_clone = graphql_ide::FilePath::new(uri.as_str());
    if let Some(lsp_diagnostics) = GraphQLLanguageServer::blocking(move || {
        snapshot
            .diagnostics(&file_path_clone)
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect::<Vec<Diagnostic>>()
    })
    .await
    {
        server
            .client
            .publish_diagnostics(uri, lsp_diagnostics, None)
            .await;
    }
}

pub(crate) async fn handle_did_save(
    server: &GraphQLLanguageServer,
    params: DidSaveTextDocumentParams,
) {
    // Check auto-stop for trace capture (prevents forgotten captures)
    if let Some(ref manager) = server.trace_capture {
        if manager.check_auto_stop() {
            server
                .client
                .log_message(
                    MessageType::INFO,
                    "Trace capture auto-stopped after 60s timeout",
                )
                .await;
        }
    }

    let uri = params.text_document.uri;

    tracing::info!("File saved: {}", uri.path());

    // Find the workspace and project for this file
    let Some((workspace_uri, project_name)) = server.workspace.find_workspace_and_project(&uri)
    else {
        tracing::debug!("No workspace/project found for saved file, skipping project-wide lints");
        return;
    };

    let Some(host) = server.workspace.get_host(&workspace_uri, &project_name) else {
        tracing::debug!("No analysis host found for workspace/project");
        return;
    };

    let snapshot = {
        use tracing::Instrument;
        let result = async { host.try_snapshot().await }
            .instrument(tracing::info_span!("acquire_snapshot"))
            .await;
        let Some(snapshot) = result else {
            tracing::debug!("Could not acquire snapshot");
            return;
        };
        snapshot
    };

    let changed_file = graphql_ide::FilePath::new(uri.as_str());
    let Some(all_diagnostics) =
        GraphQLLanguageServer::blocking(move || snapshot.all_diagnostics_for_change(&changed_file))
            .await
    else {
        return;
    };

    tracing::info!(
        affected_file_count = all_diagnostics.len(),
        "Diagnostics computed for save"
    );

    for (file_path, file_diagnostics) in all_diagnostics {
        let Ok(file_uri) = Uri::from_str(file_path.as_str()) else {
            tracing::warn!("Invalid URI in diagnostics: {}", file_path.as_str());
            continue;
        };

        let lsp_diagnostics: Vec<Diagnostic> = file_diagnostics
            .into_iter()
            .map(convert_ide_diagnostic)
            .collect();

        server
            .client
            .publish_diagnostics(file_uri, lsp_diagnostics, None)
            .await;
    }
}

#[allow(clippy::unused_async)]
pub(crate) async fn handle_did_close(
    server: &GraphQLLanguageServer,
    params: DidCloseTextDocumentParams,
) {
    tracing::info!("File closed: {}", params.text_document.uri.path());
    // NOTE: We intentionally do NOT remove the file from AnalysisHost or clear diagnostics.
    // The file is still part of the project on disk, and other files may reference
    // fragments/types defined in it. Diagnostics should remain visible.
    // Only files deleted from disk should be removed (handled by did_change_watched_files).
    let uri_string = params.text_document.uri.to_string();
    server.workspace.document_versions.remove(&uri_string);
    server.workspace.document_contents.remove(&uri_string);
}

pub(crate) async fn handle_did_change_watched_files(
    server: &GraphQLLanguageServer,
    params: DidChangeWatchedFilesParams,
) {
    tracing::debug!("Watched files changed: {} file(s)", params.changes.len());

    for change in params.changes {
        let uri = change.uri;
        tracing::debug!("File changed: {} (type: {:?})", uri.path(), change.typ);

        let Some(config_path) = uri.to_file_path() else {
            tracing::warn!("Failed to convert URI to file path: {:?}", uri);
            continue;
        };

        let workspace_uri: Option<String> = server
            .workspace
            .config_paths
            .iter()
            .find(|entry| entry.value() == &config_path)
            .map(|entry| entry.key().clone());

        if let Some(workspace_uri) = workspace_uri {
            match change.typ {
                FileChangeType::CREATED | FileChangeType::CHANGED => {
                    tracing::info!("Config file changed for workspace: {}", workspace_uri);
                    server.reload_workspace_config(&workspace_uri).await;
                }
                FileChangeType::DELETED => {
                    tracing::warn!("Config file deleted for workspace: {}", workspace_uri);
                    server
                        .client
                        .show_message(MessageType::WARNING, "GraphQL config file was deleted")
                        .await;
                }
                _ => {}
            }
            continue;
        }

        // Check if the changed file is a resolved schema
        let resolved_match: Option<(String, String)> = server
            .workspace
            .resolved_schema_paths
            .iter()
            .find(|entry| entry.value() == &config_path)
            .map(|entry| {
                let (ws, proj) = entry.key();
                (ws.clone(), proj.clone())
            });

        if let Some((ws_uri, proj_name)) = resolved_match {
            if matches!(
                change.typ,
                FileChangeType::CREATED | FileChangeType::CHANGED
            ) {
                server
                    .reload_resolved_schema(&ws_uri, &proj_name, &config_path)
                    .await;
            }
        } else {
            tracing::debug!(
                "Changed file is not a tracked config or resolved schema file: {:?}",
                config_path
            );
        }
    }
}
