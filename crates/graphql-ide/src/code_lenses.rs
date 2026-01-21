//! Code lens feature implementation.
//!
//! This module provides IDE code lens functionality:
//! - Fragment reference counts
//! - Deprecated field usage counts
//! - Operation actions (Run, Copy as cURL)

use crate::helpers::{adjust_range_for_line_offset, offset_range_to_range};
use crate::references::find_field_references;
use crate::symbol::find_fragment_definition_full_range;
use crate::types::{
    CodeLens, CodeLensCommand, CodeLensInfo, FilePath, FragmentUsage, OperationCodeLens,
    OperationCodeLensKind, OperationType,
};
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

/// Get code lenses for operations in a file.
///
/// Returns code lenses for operation definitions with "Run" and "Copy as cURL" actions.
/// The "Run" lens is only included if `include_run` is true (requires endpoint configuration).
pub fn operation_code_lenses(
    db: &dyn graphql_analysis::GraphQLAnalysisDatabase,
    registry: &FileRegistry,
    file: &FilePath,
    include_run: bool,
) -> Vec<OperationCodeLens> {
    let Some(file_id) = registry.get_file_id(file) else {
        return Vec::new();
    };

    let Some(content) = registry.get_content(file_id) else {
        return Vec::new();
    };
    let Some(metadata) = registry.get_metadata(file_id) else {
        return Vec::new();
    };

    let structure = graphql_hir::file_structure(db, file_id, content, metadata);
    let parse = graphql_syntax::parse(db, content, metadata);

    let mut lenses = Vec::new();

    for operation in structure.operations.iter() {
        // Find the operation source text from the parse result
        let mut operation_source = String::new();

        for doc in parse.documents() {
            // Use the operation range to extract the source
            let start_offset: usize = operation.operation_range.start().into();
            let end_offset: usize = operation.operation_range.end().into();

            if start_offset < doc.source.len() && end_offset <= doc.source.len() {
                operation_source = doc.source[start_offset..end_offset].to_string();
                break;
            }
        }

        // Skip if we couldn't extract the operation source
        if operation_source.is_empty() {
            continue;
        }

        // Convert the operation range to editor coordinates
        if let Some(doc) = parse.documents().next() {
            let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
            let start_offset: usize = operation.operation_range.start().into();

            #[allow(clippy::cast_possible_truncation)]
            let range = adjust_range_for_line_offset(
                offset_range_to_range(
                    &doc_line_index,
                    start_offset,
                    start_offset, // Just the start position for the lens
                ),
                operation.block_line_offset.unwrap_or(0) as u32,
            );

            let op_type = match operation.operation_type {
                graphql_hir::OperationType::Mutation => OperationType::Mutation,
                graphql_hir::OperationType::Subscription => OperationType::Subscription,
                _ => OperationType::Query,
            };

            let op_name = operation
                .name
                .as_ref()
                .map(std::string::ToString::to_string);

            // Add "Copy as cURL" lens (always available)
            lenses.push(OperationCodeLens::new(
                range,
                op_name.clone(),
                op_type,
                operation_source.clone(),
                OperationCodeLensKind::CopyAsCurl,
            ));

            // Add "Run" lens (only if endpoint is configured)
            if include_run {
                lenses.push(OperationCodeLens::new(
                    range,
                    op_name,
                    op_type,
                    operation_source.clone(),
                    OperationCodeLensKind::Run,
                ));
            }
        }
    }

    tracing::debug!(
        lens_count = lenses.len(),
        "operation_code_lenses: returning"
    );
    lenses
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AnalysisHost;
    use graphql_base_db::FileKind;

    fn setup_test(schema: &str, document: &str) -> (AnalysisHost, FilePath) {
        let mut host = AnalysisHost::new();
        let schema_path = FilePath::new("file:///schema.graphql");
        let doc_path = FilePath::new("file:///query.graphql");
        host.add_file(&schema_path, schema, FileKind::Schema);
        host.add_file(&doc_path, document, FileKind::ExecutableGraphQL);
        host.rebuild_project_files();
        (host, doc_path)
    }

    #[test]
    fn test_operation_code_lenses_query() {
        let (host, file_path) = setup_test("type Query { hello: String }", "query Hello { hello }");
        let analysis = host.snapshot();

        let lenses = analysis.operation_code_lenses(&file_path, false);

        // Should have 1 lens (Copy as cURL, no Run since include_run is false)
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].kind, OperationCodeLensKind::CopyAsCurl);
        assert_eq!(lenses[0].operation_name, Some("Hello".to_string()));
        assert_eq!(lenses[0].operation_type, OperationType::Query);
    }

    #[test]
    fn test_operation_code_lenses_with_run() {
        let (host, file_path) = setup_test("type Query { hello: String }", "query Hello { hello }");
        let analysis = host.snapshot();

        let lenses = analysis.operation_code_lenses(&file_path, true);

        // Should have 2 lenses (Copy as cURL and Run)
        assert_eq!(lenses.len(), 2);
        assert!(lenses
            .iter()
            .any(|l| l.kind == OperationCodeLensKind::CopyAsCurl));
        assert!(lenses.iter().any(|l| l.kind == OperationCodeLensKind::Run));
    }

    #[test]
    fn test_operation_code_lenses_mutation() {
        let (host, file_path) = setup_test(
            "type Query { dummy: String } type Mutation { updateHello(msg: String!): String }",
            "mutation Update($msg: String!) { updateHello(msg: $msg) }",
        );
        let analysis = host.snapshot();

        let lenses = analysis.operation_code_lenses(&file_path, false);

        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].operation_type, OperationType::Mutation);
        assert_eq!(lenses[0].operation_name, Some("Update".to_string()));
    }

    #[test]
    fn test_operation_code_lenses_anonymous() {
        let (host, file_path) = setup_test("type Query { hello: String }", "{ hello }");
        let analysis = host.snapshot();

        let lenses = analysis.operation_code_lenses(&file_path, false);

        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].operation_name, None);
        assert_eq!(lenses[0].operation_type, OperationType::Query);
    }

    #[test]
    fn test_operation_code_lenses_multiple_operations() {
        let (host, file_path) = setup_test(
            "type Query { hello: String world: String }",
            "query Hello { hello } query World { world }",
        );
        let analysis = host.snapshot();

        let lenses = analysis.operation_code_lenses(&file_path, true);

        // 2 operations x 2 lens types = 4 lenses
        assert_eq!(lenses.len(), 4);

        let hello_lenses: Vec<_> = lenses
            .iter()
            .filter(|l| l.operation_name == Some("Hello".to_string()))
            .collect();
        let world_lenses: Vec<_> = lenses
            .iter()
            .filter(|l| l.operation_name == Some("World".to_string()))
            .collect();

        assert_eq!(hello_lenses.len(), 2);
        assert_eq!(world_lenses.len(), 2);
    }
}
