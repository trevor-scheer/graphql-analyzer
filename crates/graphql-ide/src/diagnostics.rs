//! Diagnostics feature implementation.
//!
//! This module provides IDE diagnostics functionality including:
//! - Syntax error reporting
//! - GraphQL validation errors
//! - Lint rule violations
//! - Project-wide lint diagnostics

use std::collections::HashMap;

use crate::helpers::convert_diagnostic;
use crate::types::{Diagnostic, FilePath};
use crate::FileRegistry;

/// Get all diagnostics for a file (syntax + validation + lint)
pub fn file_diagnostics(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
    file: &FilePath,
) -> Vec<Diagnostic> {
    let (content, metadata) = {
        let Some(file_id) = registry.get_file_id(file) else {
            return Vec::new();
        };

        let Some(content) = registry.get_content(file_id) else {
            return Vec::new();
        };
        let Some(metadata) = registry.get_metadata(file_id) else {
            return Vec::new();
        };

        (content, metadata)
    };

    let analysis_diagnostics =
        graphql_analysis::file_diagnostics(db, content, metadata, project_files);

    analysis_diagnostics
        .iter()
        .map(convert_diagnostic)
        .collect()
}

/// Get only validation diagnostics for a file (excludes custom lint rules)
///
/// Returns only GraphQL spec validation errors, not custom lint rule violations.
/// Use this for the `validate` command to avoid duplicating lint checks.
pub fn validation_diagnostics(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
    file: &FilePath,
) -> Vec<Diagnostic> {
    let (content, metadata) = {
        let Some(file_id) = registry.get_file_id(file) else {
            return Vec::new();
        };

        let Some(content) = registry.get_content(file_id) else {
            return Vec::new();
        };
        let Some(metadata) = registry.get_metadata(file_id) else {
            return Vec::new();
        };

        (content, metadata)
    };

    let analysis_diagnostics =
        graphql_analysis::file_validation_diagnostics(db, content, metadata, project_files);

    analysis_diagnostics
        .iter()
        .map(convert_diagnostic)
        .collect()
}

/// Get only lint diagnostics for a file (excludes validation errors)
///
/// Returns only custom lint rule violations, not GraphQL spec validation errors.
pub fn lint_diagnostics(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
    file: &FilePath,
) -> Vec<Diagnostic> {
    let (content, metadata) = {
        let Some(file_id) = registry.get_file_id(file) else {
            return Vec::new();
        };

        let Some(content) = registry.get_content(file_id) else {
            return Vec::new();
        };
        let Some(metadata) = registry.get_metadata(file_id) else {
            return Vec::new();
        };

        (content, metadata)
    };

    let lint_diagnostics =
        graphql_analysis::lint_integration::lint_file(db, content, metadata, project_files);

    lint_diagnostics.iter().map(convert_diagnostic).collect()
}

/// Get project-wide lint diagnostics (e.g., unused fields, unique names)
///
/// Returns a map of file paths -> diagnostics for project-wide lint rules.
/// These are expensive rules that analyze the entire project.
pub fn project_lint_diagnostics(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
) -> HashMap<FilePath, Vec<Diagnostic>> {
    let diagnostics_by_file_id =
        graphql_analysis::lint_integration::project_lint_diagnostics(db, project_files);

    let mut results = HashMap::new();

    for (file_id, diagnostics) in diagnostics_by_file_id.iter() {
        if let Some(file_path) = registry.get_path(*file_id) {
            let converted: Vec<Diagnostic> = diagnostics.iter().map(convert_diagnostic).collect();

            if !converted.is_empty() {
                results.insert(file_path, converted);
            }
        }
    }

    results
}
