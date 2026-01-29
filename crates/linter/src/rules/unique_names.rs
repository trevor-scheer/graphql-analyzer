use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::{FragmentNameInfo, OperationNameInfo};
use std::collections::HashMap;

/// Trait implementation for `unique_names` rule
pub struct UniqueNamesRuleImpl;

impl LintRule for UniqueNamesRuleImpl {
    fn name(&self) -> &'static str {
        "unique_names"
    }

    fn description(&self) -> &'static str {
        "Ensures operation and fragment names are unique across the project"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Error
    }
}

impl ProjectLintRule for UniqueNamesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Collect all operations with their locations using per-file cached queries
        let doc_ids = project_files.document_file_ids(db).ids(db);
        let mut operations_by_name: HashMap<String, Vec<(FileId, OperationNameInfo)>> =
            HashMap::new();

        for file_id in doc_ids.iter() {
            // Use per-file lookup for granular caching
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            // Per-file cached query - only recomputes if THIS file changed
            let op_names = graphql_hir::file_operation_names(db, *file_id, content, metadata);
            for op_info in op_names.iter() {
                operations_by_name
                    .entry(op_info.name.to_string())
                    .or_default()
                    .push((*file_id, op_info.clone()));
            }
        }

        // Check for duplicate operation names
        for (name, locations) in &operations_by_name {
            if locations.len() > 1 {
                // Found duplicate operation names
                for (file_id, op_info) in locations {
                    let message = format!(
                        "Operation name '{name}' is not unique across the project. Found {} definitions.",
                        locations.len()
                    );

                    // Use the actual name range if available, otherwise fall back to start of file
                    let offset_range = op_info.name_range.map_or_else(
                        || crate::diagnostics::OffsetRange::new(0, name.len()),
                        |range| {
                            crate::diagnostics::OffsetRange::new(
                                range.start().into(),
                                range.end().into(),
                            )
                        },
                    );

                    let mut diag = LintDiagnostic::new(
                        offset_range,
                        self.default_severity(),
                        message,
                        self.name().to_string(),
                    );

                    // For embedded GraphQL, add block context for proper position calculation
                    if let (Some(line_offset), Some(byte_offset), Some(source)) = (
                        op_info.block_line_offset,
                        op_info.block_byte_offset,
                        &op_info.block_source,
                    ) {
                        diag = diag.with_block_context(line_offset, byte_offset, source.clone());
                    }

                    diagnostics_by_file.entry(*file_id).or_default().push(diag);
                }
            }
        }

        // Collect all fragments with their locations using per-file cached queries
        let mut fragments_by_name: HashMap<String, Vec<(FileId, FragmentNameInfo)>> =
            HashMap::new();

        for file_id in doc_ids.iter() {
            // Use per-file lookup for granular caching
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, *file_id)
            else {
                continue;
            };
            // Per-file cached query - only recomputes if THIS file changed
            let frag_info = graphql_hir::file_fragment_info(db, *file_id, content, metadata);
            for info in frag_info.iter() {
                fragments_by_name
                    .entry(info.name.to_string())
                    .or_default()
                    .push((*file_id, info.clone()));
            }
        }

        // Check for duplicate fragment names
        for (name, locations) in &fragments_by_name {
            if locations.len() > 1 {
                // Found duplicate fragment names
                for (file_id, frag_info) in locations {
                    let message = format!(
                        "Fragment name '{name}' is not unique across the project. Found {} definitions.",
                        locations.len()
                    );

                    // Use the actual name range
                    let offset_range = crate::diagnostics::OffsetRange::new(
                        frag_info.name_range.start().into(),
                        frag_info.name_range.end().into(),
                    );

                    let mut diag = LintDiagnostic::new(
                        offset_range,
                        self.default_severity(),
                        message,
                        self.name().to_string(),
                    );

                    // For embedded GraphQL, add block context for proper position calculation
                    if let (Some(line_offset), Some(byte_offset), Some(source)) = (
                        frag_info.block_line_offset,
                        frag_info.block_byte_offset,
                        &frag_info.block_source,
                    ) {
                        diag = diag.with_block_context(line_offset, byte_offset, source.clone());
                    }

                    diagnostics_by_file.entry(*file_id).or_default().push(diag);
                }
            }
        }

        diagnostics_by_file
    }
}
