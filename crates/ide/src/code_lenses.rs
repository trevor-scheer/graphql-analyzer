//! Code lens feature implementation.
//!
//! This module provides IDE code lens functionality:
//! - Fragment reference counts
//! - Deprecated field usage counts

use crate::helpers::{adjust_range_for_line_offset, offset_range_to_range};
use crate::references::find_field_references;
use crate::symbol::find_fragment_definition_full_range;
use crate::types::{CodeLens, CodeLensCommand, CodeLensInfo, FilePath, FragmentUsage};
use crate::FileRegistry;

/// Get code lenses for a file.
///
/// Returns code lenses for fragment definitions showing reference counts.
pub fn code_lenses(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    file: &FilePath,
    fragment_usages: &[FragmentUsage],
) -> Vec<CodeLens> {
    let (content, metadata, file_id) = {
        let Some(file_id) = registry.get_file_id(file) else {
            return Vec::new();
        };

        let Some(content) = registry.get_content(file_id) else {
            return Vec::new();
        };
        let Some(metadata) = registry.get_metadata(file_id) else {
            return Vec::new();
        };

        (content, metadata, file_id)
    };

    if project_files.is_none() {
        return Vec::new();
    }

    let structure = graphql_hir::file_structure(db, file_id, content, metadata);

    let mut lenses = Vec::new();
    let parse = graphql_syntax::parse(db, content, metadata);

    for fragment in structure.fragments.iter() {
        let usage_count = fragment_usages
            .iter()
            .find(|u| u.name == fragment.name.as_ref())
            .map_or(0, FragmentUsage::usage_count);

        for doc in parse.documents() {
            if let Some(ranges) = find_fragment_definition_full_range(doc.tree, &fragment.name) {
                let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                #[allow(clippy::cast_possible_truncation)]
                let range = adjust_range_for_line_offset(
                    offset_range_to_range(&doc_line_index, ranges.def_start, ranges.def_start),
                    doc.line_offset as u32,
                );

                let title = if usage_count == 1 {
                    "1 reference".to_string()
                } else {
                    format!("{usage_count} references")
                };

                let command = CodeLensCommand::new("editor.action.showReferences", &title)
                    .with_arguments(vec![
                        file.as_str().to_string(),
                        format!("{}:{}", range.start.line, range.start.character),
                        fragment.name.to_string(),
                    ]);

                lenses.push(CodeLens::new(range, title).with_command(command));
                break;
            }
        }
    }

    tracing::debug!(lens_count = lenses.len(), "code_lenses: returning");
    lenses
}

/// Get code lenses for deprecated fields in a schema file.
///
/// Returns code lens information for each deprecated field definition,
/// including the usage count and locations for navigation.
pub fn deprecated_field_code_lenses(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    file: &FilePath,
) -> Vec<CodeLensInfo> {
    let mut code_lenses = Vec::new();

    let Some(project_files) = project_files else {
        return code_lenses;
    };

    let file_id = registry.get_file_id(file);

    let Some(file_id) = file_id else {
        return code_lenses;
    };

    let schema_types = graphql_hir::schema_types(db, project_files);

    let content = registry.get_content(file_id);
    let metadata = registry.get_metadata(file_id);

    let (Some(content), Some(_metadata)) = (content, metadata) else {
        return code_lenses;
    };

    let line_index = graphql_syntax::line_index(db, content);

    for type_def in schema_types.values() {
        if type_def.file_id != file_id {
            continue;
        }

        for field in &type_def.fields {
            if !field.is_deprecated {
                continue;
            }

            let usage_locations = find_field_references(
                db,
                registry,
                Some(project_files),
                type_def.name.as_ref(),
                field.name.as_ref(),
                false,
            );

            let name_start = field.name_range.start().into();
            let name_end = field.name_range.end().into();
            let range = offset_range_to_range(&line_index, name_start, name_end);

            let mut code_lens = CodeLensInfo::new(
                range,
                type_def.name.as_ref(),
                field.name.as_ref(),
                usage_locations.len(),
                usage_locations,
            );

            if let Some(ref reason) = field.deprecation_reason {
                code_lens = code_lens.with_deprecation_reason(reason.as_ref());
            }

            code_lenses.push(code_lens);
        }
    }

    code_lenses
}
