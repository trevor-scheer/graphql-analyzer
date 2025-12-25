use crate::diagnostics::{LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, ProjectLintRule};
use graphql_db::{FileId, ProjectFiles};
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
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        // Collect all operations with their locations
        let document_files = project_files.document_files(db);
        let mut operations_by_name: HashMap<String, Vec<(FileId, usize)>> = HashMap::new();

        for (file_id, content, metadata) in document_files.iter() {
            let structure = graphql_hir::file_structure(db, *file_id, *content, *metadata);
            for operation in &structure.operations {
                if let Some(ref name) = operation.name {
                    operations_by_name
                        .entry(name.to_string())
                        .or_default()
                        .push((*file_id, operation.index));
                }
            }
        }

        // Check for duplicate operation names
        for (name, locations) in &operations_by_name {
            if locations.len() > 1 {
                // Found duplicate operation names
                for (file_id, _operation_index) in locations {
                    let message = format!(
                        "Operation name '{name}' is not unique across the project. Found {} definitions.",
                        locations.len()
                    );

                    // For now, use offset 0 - we'll need to extract position from AST
                    let diag = LintDiagnostic {
                        message,
                        offset_range: crate::diagnostics::OffsetRange {
                            start: 0,
                            end: name.len(),
                        },
                        severity: self.default_severity(),
                        rule: self.name().to_string(),
                    };

                    diagnostics_by_file.entry(*file_id).or_default().push(diag);
                }
            }
        }

        // Collect all fragments with their locations
        let mut fragments_by_name: HashMap<String, Vec<FileId>> = HashMap::new();

        for (file_id, content, metadata) in document_files.iter() {
            let structure = graphql_hir::file_structure(db, *file_id, *content, *metadata);
            for fragment in &structure.fragments {
                fragments_by_name
                    .entry(fragment.name.to_string())
                    .or_default()
                    .push(*file_id);
            }
        }

        // Check for duplicate fragment names
        for (name, file_ids) in &fragments_by_name {
            if file_ids.len() > 1 {
                // Found duplicate fragment names
                for file_id in file_ids {
                    let message = format!(
                        "Fragment name '{name}' is not unique across the project. Found {} definitions.",
                        file_ids.len()
                    );

                    let diag = LintDiagnostic {
                        message,
                        offset_range: crate::diagnostics::OffsetRange {
                            start: 0,
                            end: name.len(),
                        },
                        severity: self.default_severity(),
                        rule: self.name().to_string(),
                    };

                    diagnostics_by_file.entry(*file_id).or_default().push(diag);
                }
            }
        }

        diagnostics_by_file
    }
}
