//! Workspace loading: config discovery, file loading, and diagnostics publishing.
//!
//! Extracted from the old `server.rs` async methods into free sync functions
//! that take `&mut GlobalState`.

use std::path::Path;
#[cfg(feature = "native")]
use std::str::FromStr;

#[cfg(feature = "native")]
use lsp_types::{Diagnostic, MessageType, Uri};

#[cfg(feature = "native")]
use crate::conversions::convert_ide_diagnostic;
use crate::global_state::GlobalState;
#[cfg(feature = "native")]
use crate::global_state::IntrospectionRequest;
#[cfg(feature = "native")]
use crate::server::validation_errors_to_diagnostics;

/// Load a workspace config and all its projects.
#[cfg(feature = "native")]
pub fn load_workspace_config(state: &mut GlobalState, workspace_uri: &str, workspace_path: &Path) {
    tracing::debug!(path = ?workspace_path, "Loading GraphQL config");

    state
        .workspace
        .workspace_roots
        .insert(workspace_uri.to_string(), workspace_path.to_path_buf());

    match graphql_config::find_config(workspace_path) {
        Ok(Some(config_path)) => {
            state
                .workspace
                .config_paths
                .insert(workspace_uri.to_string(), config_path.clone());

            match graphql_config::load_config(&config_path) {
                Ok(config) => {
                    let lint_rule_names = graphql_linter::all_rule_names();
                    let lint_context = graphql_config::LintValidationContext {
                        valid_rule_names: &lint_rule_names,
                        valid_presets: &["recommended"],
                    };
                    let errors =
                        graphql_config::validate(&config, workspace_path, Some(&lint_context));
                    let config_uri = Uri::from_str(&graphql_ide::path_to_file_uri(&config_path))
                        .expect("valid config path");

                    let has_errors = errors
                        .iter()
                        .any(|e| e.severity() == graphql_config::Severity::Error);

                    if errors.is_empty() {
                        state.publish_diagnostics(config_uri, vec![], None);
                    } else {
                        let config_content =
                            std::fs::read_to_string(&config_path).unwrap_or_default();
                        let diagnostics =
                            validation_errors_to_diagnostics(&errors, &config_content);
                        state.publish_diagnostics(config_uri.clone(), diagnostics, None);

                        if has_errors {
                            let error_count = errors
                                .iter()
                                .filter(|e| e.severity() == graphql_config::Severity::Error)
                                .count();

                            state.send_notification::<lsp_types::notification::ShowMessage>(
                                lsp_types::ShowMessageParams {
                                    typ: MessageType::ERROR,
                                    message: format!(
                                        "GraphQL config has {error_count} validation error(s). \
                                        Please fix the configuration before continuing.",
                                    ),
                                },
                            );
                            return;
                        }
                    }

                    state.send_notification::<lsp_types::notification::LogMessage>(
                        lsp_types::LogMessageParams {
                            typ: MessageType::INFO,
                            message: "GraphQL config found, loading files...".to_owned(),
                        },
                    );

                    state
                        .workspace
                        .configs
                        .insert(workspace_uri.to_string(), config.clone());

                    load_all_project_files(
                        state,
                        workspace_uri,
                        workspace_path,
                        &config,
                        &config_path,
                    );
                }
                Err(e) => {
                    tracing::error!("Error loading config: {}", e);
                    state.send_notification::<lsp_types::notification::LogMessage>(
                        lsp_types::LogMessageParams {
                            typ: MessageType::ERROR,
                            message: format!("Failed to load GraphQL config: {e}"),
                        },
                    );
                }
            }
        }
        Ok(None) => {
            state.send_notification::<lsp_types::notification::ShowMessage>(
                lsp_types::ShowMessageParams {
                    typ: MessageType::WARNING,
                    message: "No GraphQL config found. Schema validation and full IDE features require a config file.".to_owned(),
                },
            );
        }
        Err(e) => {
            tracing::error!("Error searching for config: {}", e);
        }
    }
}

/// Load all project files from a config into their respective `AnalysisHost` instances.
#[cfg(feature = "native")]
fn load_all_project_files(
    state: &mut GlobalState,
    workspace_uri: &str,
    workspace_path: &Path,
    config: &graphql_config::GraphQLConfig,
    config_path: &Path,
) {
    let start = std::time::Instant::now();
    let projects: Vec<_> = config.projects().collect();
    tracing::debug!("Loading files for {} project(s)", projects.len());

    let mut content_mismatch_errors: Vec<graphql_config::ConfigValidationError> = Vec::new();

    for (project_name, project_config) in projects {
        let project_start = std::time::Instant::now();
        tracing::debug!("Loading project: {}", project_name);

        let extract_config = project_config
            .extract_config()
            .and_then(
                |v| match serde_json::from_value::<graphql_extract::ExtractConfig>(v) {
                    Ok(config) => Some(config),
                    Err(e) => {
                        tracing::warn!("Failed to parse extract config: {e}, using defaults");
                        None
                    }
                },
            )
            .unwrap_or_default();

        let lint_config =
            project_config
                .lint()
                .map_or_else(graphql_linter::LintConfig::default, |lint_value| {
                    match serde_json::from_value::<graphql_linter::LintConfig>(lint_value) {
                        Ok(cfg) => cfg,
                        Err(e) => {
                            tracing::warn!(
                                "Failed to parse lint config for project '{}': {}. Using default.",
                                project_name,
                                e
                            );
                            graphql_linter::LintConfig::default()
                        }
                    }
                });

        let host = state
            .workspace
            .get_or_create_host(workspace_uri, project_name);

        host.set_extract_config(extract_config.clone());
        host.set_lint_config(lint_config);

        // Load local schemas AND documents in a single pass
        let (schema_result, loaded_files, _doc_result) = {
            let schema_result = match host.load_schemas_from_config(project_config, workspace_path)
            {
                Ok(result) => {
                    tracing::debug!(
                        "Loaded {} local schema file(s), {} remote schema(s) pending",
                        result.loaded_count,
                        result.pending_introspections.len()
                    );
                    result
                }
                Err(e) => {
                    tracing::error!("Failed to load schemas: {}", e);
                    graphql_ide::SchemaLoadResult::default()
                }
            };

            let (docs, doc_result) =
                host.load_documents_from_config(project_config, workspace_path, &extract_config);

            (schema_result, docs, doc_result)
        };

        // Track resolved schema path for file watching
        if let Some(resolved_path) = project_config.resolved_schema() {
            let resolved_full = workspace_path.join(&resolved_path);
            state.workspace.resolved_schema_paths.insert(
                (workspace_uri.to_string(), project_name.to_string()),
                resolved_full,
            );
        }

        let no_user_schema = schema_result.has_no_user_schema();
        let schema_errors = schema_result.content_errors.clone();

        if no_user_schema {
            tracing::warn!(
                "Project '{}': no schema files found matching configured patterns",
                project_name
            );
            state.send_notification::<lsp_types::notification::ShowMessage>(
                lsp_types::ShowMessageParams {
                    typ: MessageType::WARNING,
                    message: format!(
                        "GraphQL: No schema files found for project '{project_name}'. \
                         Schema validation will be skipped."
                    ),
                },
            );
        }

        // Convert schema content mismatch errors
        for error in &schema_errors {
            tracing::warn!(
                "Content mismatch in '{}': file in schema config contains executable definitions: {}",
                error.file_path.display(),
                error.unexpected_definitions.join(", ")
            );
            content_mismatch_errors.push(graphql_config::ConfigValidationError::ContentMismatch {
                project: project_name.to_string(),
                pattern: error.pattern.clone(),
                expected: graphql_config::FileType::Schema,
                file_path: error.file_path.clone(),
                unexpected_definitions: error.unexpected_definitions.clone(),
            });
        }

        // Send introspection requests to the async thread
        for pending in &schema_result.pending_introspections {
            let _ = state
                .introspection_request_sender
                .send(IntrospectionRequest {
                    workspace_uri: workspace_uri.to_string(),
                    project_name: project_name.to_string(),
                    pending: pending.clone(),
                });
        }

        if !loaded_files.is_empty() {
            let total_files_loaded = loaded_files.len();
            tracing::debug!(
                "Collected {} document files for project '{}'",
                total_files_loaded,
                project_name
            );

            for loaded_file in &loaded_files {
                state.workspace.file_to_project.insert(
                    loaded_file.path.as_str().to_string(),
                    (workspace_uri.to_string(), project_name.to_string()),
                );
            }

            // The earlier &mut borrow ended above, so we can re-borrow the host
            // immutably to take a snapshot for diagnostics.
            let host = state
                .workspace
                .get_host(workspace_uri, project_name)
                .expect("host was just created");
            let snapshot = host.snapshot();

            let loaded_file_paths: Vec<graphql_ide::FilePath> =
                loaded_files.iter().map(|f| f.path.clone()).collect();

            let all_diagnostics_map = snapshot.all_diagnostics_for_files(&loaded_file_paths);

            for (file_path, diagnostics) in &all_diagnostics_map {
                let Ok(file_uri) = Uri::from_str(file_path.as_str()) else {
                    continue;
                };
                let lsp_diagnostics: Vec<Diagnostic> = diagnostics
                    .iter()
                    .cloned()
                    .map(convert_ide_diagnostic)
                    .collect();
                state.publish_diagnostics(file_uri, lsp_diagnostics, None);
            }

            // Clear stale diagnostics for files with no issues
            for loaded_file in &loaded_files {
                if !all_diagnostics_map.contains_key(&loaded_file.path) {
                    if let Ok(file_uri) = Uri::from_str(loaded_file.path.as_str()) {
                        state.publish_diagnostics(file_uri, vec![], None);
                    }
                }
            }
        }

        let project_msg = format!(
            "Project '{}' loaded: {} schema file(s), {} document file(s) in {:.1}s",
            project_name,
            schema_result.loaded_count,
            loaded_files.len(),
            project_start.elapsed().as_secs_f64()
        );
        tracing::info!("{}", project_msg);
        state.send_notification::<lsp_types::notification::LogMessage>(
            lsp_types::LogMessageParams {
                typ: MessageType::INFO,
                message: project_msg,
            },
        );
    }

    // Publish config file diagnostics (content mismatches)
    if !content_mismatch_errors.is_empty() {
        let config_uri =
            Uri::from_str(&graphql_ide::path_to_file_uri(config_path)).expect("valid config path");
        let config_content = std::fs::read_to_string(config_path).unwrap_or_default();
        let diagnostics =
            validation_errors_to_diagnostics(&content_mismatch_errors, &config_content);

        tracing::warn!(
            "Found {} content mismatch error(s) in config",
            content_mismatch_errors.len()
        );

        state.publish_diagnostics(config_uri, diagnostics, None);
    }

    let elapsed = start.elapsed();
    let total_files = state.workspace.file_to_project.len();
    let init_message = format!(
        "Project initialization complete: {} files loaded in {:.1}s",
        total_files,
        elapsed.as_secs_f64()
    );
    tracing::info!("{}", init_message);
    state.send_notification::<lsp_types::notification::LogMessage>(lsp_types::LogMessageParams {
        typ: MessageType::INFO,
        message: init_message,
    });
}

/// Reload configuration for a workspace.
#[cfg(feature = "native")]
pub fn reload_workspace_config(state: &mut GlobalState, workspace_uri: &str) {
    tracing::debug!("Reloading configuration for workspace: {}", workspace_uri);

    let Some(workspace_path) = state.workspace.workspace_roots.get(workspace_uri).cloned() else {
        tracing::error!(
            "Cannot reload config: workspace root not found for {}",
            workspace_uri
        );
        return;
    };

    state.workspace.clear_workspace(workspace_uri);
    state.workspace.configs.remove(workspace_uri);
    load_workspace_config(state, workspace_uri, &workspace_path);

    if state.workspace.configs.contains_key(workspace_uri) {
        state.send_notification::<lsp_types::notification::ShowMessage>(
            lsp_types::ShowMessageParams {
                typ: MessageType::INFO,
                message: "GraphQL configuration reloaded successfully".to_owned(),
            },
        );
    }
}

/// Reload a resolved schema file that changed on disk.
#[cfg(feature = "native")]
pub fn reload_resolved_schema(
    state: &mut GlobalState,
    workspace_uri: &str,
    project_name: &str,
    resolved_path: &Path,
) {
    tracing::info!(
        "Reloading resolved schema for project '{}': {}",
        project_name,
        resolved_path.display()
    );

    let Ok(content) = std::fs::read_to_string(resolved_path) else {
        tracing::warn!(
            "Failed to read resolved schema file: {}",
            resolved_path.display()
        );
        return;
    };

    let file_uri = graphql_ide::path_to_file_uri(resolved_path);
    let file_path = graphql_ide::FilePath::new(file_uri);

    let Some(host) = state.workspace.get_host_mut(workspace_uri, project_name) else {
        return;
    };

    host.add_file(
        &file_path,
        &content,
        graphql_ide::Language::GraphQL,
        graphql_ide::DocumentKind::Schema,
    );

    // Republish diagnostics for all files
    let host = state
        .workspace
        .get_host(workspace_uri, project_name)
        .expect("host exists");
    let snapshot = host.snapshot();

    let diag_map = snapshot.all_diagnostics();
    for (fp, diagnostics) in &diag_map {
        let Ok(file_uri) = Uri::from_str(fp.as_str()) else {
            continue;
        };
        let lsp_diagnostics: Vec<Diagnostic> = diagnostics
            .iter()
            .cloned()
            .map(convert_ide_diagnostic)
            .collect();
        state.publish_diagnostics(file_uri, lsp_diagnostics, None);
    }
}

/// Install a workspace from LSP `initializationOptions` JSON, bypassing the on-disk
/// `.graphqlrc.yaml` lookup. The JSON shape must deserialize to `graphql_config::GraphQLConfig`.
///
/// Used by the wasm entrypoint where the host page declares the project shape (schema +
/// documents URIs) up front, rather than relying on filesystem discovery. Native callers
/// can still use `load_workspace_config` to load from disk.
pub fn install_workspace_from_init_options(
    state: &mut GlobalState,
    workspace_uri: &str,
    workspace_path: &Path,
    init_options: serde_json::Value,
) -> Result<(), String> {
    let config: graphql_config::GraphQLConfig = serde_json::from_value(init_options)
        .map_err(|e| format!("`initializationOptions` is not a valid graphql_config::GraphQLConfig: {e}"))?;
    state
        .workspace
        .workspace_roots
        .insert(workspace_uri.to_string(), workspace_path.to_path_buf());
    state
        .workspace
        .configs
        .insert(workspace_uri.to_string(), config);
    Ok(())
}
