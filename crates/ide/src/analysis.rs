use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::database::IdeDatabase;
use crate::file_registry::FileRegistry;
use crate::helpers::{adjust_range_for_line_offset, convert_diagnostic, offset_range_to_range};
use crate::symbol::{find_fragment_definition_full_range, find_operation_definition_ranges};
use crate::types::{
    CodeLens, CodeLensInfo, ComplexityAnalysis, Diagnostic, DocumentSymbol, FieldComplexity,
    FieldCoverageReport, FieldUsageInfo, FilePath, FoldingRange, FragmentReference, FragmentUsage,
    HoverResult, InlayHint, Location, Position, ProjectStatus, Range, RenameResult, SchemaStats,
    SelectionRange, SignatureHelp, WorkspaceSymbol,
};
use crate::{
    code_lenses, completion, folding_ranges, goto_definition, hover, inlay_hints, references,
    rename, selection_range, semantic_tokens, signature_help, symbols, CompletionItem,
    SemanticToken,
};

/// Immutable snapshot of the analysis state
///
/// Can be cheaply cloned and used from multiple threads.
/// All IDE feature queries go through this.
///
/// # Lifecycle Warning
///
/// This snapshot shares Salsa storage with its parent [`AnalysisHost`](crate::AnalysisHost).
/// **You must drop all `Analysis` instances before calling any mutating method**
/// on the host (like `add_file`, `remove_file`, etc.). Failure to do so will
/// cause a hang/deadlock due to Salsa's single-writer, multi-reader model.
#[derive(Clone)]
pub struct Analysis {
    pub(crate) db: IdeDatabase,
    pub(crate) registry: Arc<RwLock<FileRegistry>>,
    /// Cached `ProjectFiles` for HIR queries
    /// This is fetched from the registry when the snapshot is created
    pub(crate) project_files: Option<graphql_base_db::ProjectFiles>,
}

impl Analysis {
    /// Get diagnostics for a file
    ///
    /// Returns syntax errors, validation errors, and lint warnings.
    pub fn diagnostics(&self, file: &FilePath) -> Vec<Diagnostic> {
        let (content, metadata) = {
            let registry = self.registry.read();

            let Some(file_id) = registry.get_file_id(file) else {
                return Vec::new();
            };

            let Some(content) = registry.get_content(file_id) else {
                return Vec::new();
            };
            let Some(metadata) = registry.get_metadata(file_id) else {
                return Vec::new();
            };
            drop(registry);

            (content, metadata)
        };

        let analysis_diagnostics =
            graphql_analysis::file_diagnostics(&self.db, content, metadata, self.project_files);

        analysis_diagnostics
            .iter()
            .map(convert_diagnostic)
            .collect()
    }

    /// Get diagnostics for all files affected by a change to `changed_file`.
    ///
    /// Always includes diagnostics for the changed file itself. Additionally:
    /// - If the changed file is a **schema** file, re-validates all document files
    ///   (every operation/fragment validates against the schema).
    /// - If the changed file is a **document** file containing fragments,
    ///   re-validates files that spread those fragments.
    /// - If the changed file has named operations, re-validates files with
    ///   same-named operations (uniqueness checks).
    ///
    /// Salsa memoization ensures unaffected files return cached results instantly.
    pub fn diagnostics_for_change(
        &self,
        changed_file: &FilePath,
    ) -> HashMap<FilePath, Vec<Diagnostic>> {
        let mut result = HashMap::new();

        // Always include diagnostics for the changed file
        result.insert(changed_file.clone(), self.diagnostics(changed_file));

        let Some(project_files) = self.project_files else {
            return result;
        };

        let (is_schema, is_document, changed_file_id) = {
            let registry = self.registry.read();
            let Some(file_id) = registry.get_file_id(changed_file) else {
                return result;
            };
            let Some(metadata) = registry.get_metadata(file_id) else {
                return result;
            };
            (
                metadata.is_schema(&self.db),
                metadata.is_document(&self.db),
                file_id,
            )
        };

        if is_schema {
            // Schema change: re-validate all document files
            let document_files: Vec<FilePath> = {
                let registry = self.registry.read();
                registry
                    .all_file_ids()
                    .into_iter()
                    .filter(|&id| {
                        registry
                            .get_metadata(id)
                            .is_some_and(|m| m.is_document(&self.db))
                    })
                    .filter_map(|id| registry.get_path(id))
                    .collect()
            };

            for doc_file in document_files {
                if doc_file != *changed_file {
                    result.insert(doc_file.clone(), self.diagnostics(&doc_file));
                }
            }
        } else if is_document {
            // Document file change: find affected files via fragment/operation dependencies
            let affected = self.find_affected_document_files(changed_file_id, project_files);
            for affected_file in affected {
                if affected_file != *changed_file {
                    result.insert(affected_file.clone(), self.diagnostics(&affected_file));
                }
            }
        }

        result
    }

    /// Get all diagnostics (per-file + project-wide lints) for files affected by a change.
    ///
    /// Combines [`diagnostics_for_change`] with [`project_lint_diagnostics`] into a single
    /// merged result. Each file in the result has its full set of diagnostics:
    /// per-file validation, per-file lints, and project-wide lints.
    ///
    /// Use this from `did_save` to publish complete diagnostics in one pass.
    pub fn all_diagnostics_for_change(
        &self,
        changed_file: &FilePath,
    ) -> HashMap<FilePath, Vec<Diagnostic>> {
        let mut result = self.diagnostics_for_change(changed_file);

        // Merge project-wide lint diagnostics into the result
        let project_diagnostics = self.project_lint_diagnostics();
        for (file_path, diagnostics) in project_diagnostics {
            result.entry(file_path).or_default().extend(diagnostics);
        }

        result
    }

    /// Find document files affected by a change to the given document file.
    ///
    /// Returns files that:
    /// 1. Spread fragments defined in the changed file (directly or transitively)
    /// 2. Have same-named operations as the changed file (uniqueness checks)
    fn find_affected_document_files(
        &self,
        changed_file_id: graphql_base_db::FileId,
        project_files: graphql_base_db::ProjectFiles,
    ) -> Vec<FilePath> {
        let registry = self.registry.read();

        // Get fragments and operations defined in the changed file
        let (content, metadata) = {
            let Some(content) = registry.get_content(changed_file_id) else {
                return Vec::new();
            };
            let Some(metadata) = registry.get_metadata(changed_file_id) else {
                return Vec::new();
            };
            (content, metadata)
        };

        let changed_fragments =
            graphql_hir::file_fragments(&self.db, changed_file_id, content, metadata);
        let changed_operations =
            graphql_hir::file_operations(&self.db, changed_file_id, content, metadata);

        // If no fragments or named operations, no cross-file dependencies
        let has_fragments = !changed_fragments.is_empty();
        let has_named_ops = changed_operations.iter().any(|op| op.name.is_some());
        if !has_fragments && !has_named_ops {
            return Vec::new();
        }

        // Collect fragment names from this file
        let fragment_names: std::collections::HashSet<std::sync::Arc<str>> =
            changed_fragments.iter().map(|f| f.name.clone()).collect();

        // Collect named operation names from this file
        let operation_names: std::collections::HashSet<std::sync::Arc<str>> = changed_operations
            .iter()
            .filter_map(|op| op.name.clone())
            .collect();

        // Build the set of fragment names we care about:
        // 1. Fragments currently defined in this file
        // 2. Fragments that transitively spread our fragments
        let mut affected_fragment_names = fragment_names.clone();
        let all_fragments_index = if has_fragments {
            let index = graphql_hir::project_fragment_name_index(&self.db, project_files);
            let spreads_index = graphql_hir::fragment_spreads_index(&self.db, project_files);
            // Walk reverse: find fragments that spread OUR fragments
            for (frag_name, spreads) in spreads_index.iter() {
                if spreads.iter().any(|s| fragment_names.contains(s)) {
                    affected_fragment_names.insert(frag_name.clone());
                }
            }
            Some(index)
        } else {
            None
        };

        let mut affected_files = Vec::new();
        let doc_ids = project_files.document_file_ids(&self.db).ids(&self.db);

        for file_id in doc_ids.iter() {
            if *file_id == changed_file_id {
                continue;
            }

            let Some((file_content, file_metadata)) =
                graphql_base_db::file_lookup(&self.db, project_files, *file_id)
            else {
                continue;
            };

            let mut is_affected = false;

            if has_fragments {
                let used_names = graphql_hir::file_used_fragment_names(
                    &self.db,
                    *file_id,
                    file_content,
                    file_metadata,
                );

                // Check 1: does this file spread any of our current/transitive fragments?
                if used_names
                    .iter()
                    .any(|n| affected_fragment_names.contains(n))
                {
                    is_affected = true;
                }

                // Check 2: does this file spread a fragment that no longer exists?
                // This catches the rename/delete case: the old fragment name is gone
                // from the project index, so any file still referencing it needs refresh.
                if !is_affected {
                    if let Some(ref index) = all_fragments_index {
                        if used_names.iter().any(|n| !index.contains_key(n)) {
                            is_affected = true;
                        }
                    }
                }
            }

            // Check if this file has same-named operations (uniqueness)
            if !is_affected && has_named_ops {
                let file_op_names = graphql_hir::file_operation_names(
                    &self.db,
                    *file_id,
                    file_content,
                    file_metadata,
                );
                if file_op_names
                    .iter()
                    .any(|info| operation_names.contains(&info.name))
                {
                    is_affected = true;
                }
            }

            if is_affected {
                if let Some(path) = registry.get_path(*file_id) {
                    affected_files.push(path);
                }
            }
        }

        affected_files
    }

    /// Get only validation diagnostics for a file (excludes custom lint rules)
    ///
    /// Returns only GraphQL spec validation errors, not custom lint rule violations.
    /// Use this for the `validate` command to avoid duplicating lint checks.
    pub fn validation_diagnostics(&self, file: &FilePath) -> Vec<Diagnostic> {
        let (content, metadata) = {
            let registry = self.registry.read();

            let Some(file_id) = registry.get_file_id(file) else {
                return Vec::new();
            };

            let Some(content) = registry.get_content(file_id) else {
                return Vec::new();
            };
            let Some(metadata) = registry.get_metadata(file_id) else {
                return Vec::new();
            };
            drop(registry);

            (content, metadata)
        };

        let analysis_diagnostics = graphql_analysis::file_validation_diagnostics(
            &self.db,
            content,
            metadata,
            self.project_files,
        );

        analysis_diagnostics
            .iter()
            .map(convert_diagnostic)
            .collect()
    }

    /// Get only lint diagnostics for a file (excludes validation errors)
    ///
    /// Returns only custom lint rule violations, not GraphQL spec validation errors.
    pub fn lint_diagnostics(&self, file: &FilePath) -> Vec<Diagnostic> {
        let (content, metadata) = {
            let registry = self.registry.read();

            let Some(file_id) = registry.get_file_id(file) else {
                return Vec::new();
            };

            let Some(content) = registry.get_content(file_id) else {
                return Vec::new();
            };
            let Some(metadata) = registry.get_metadata(file_id) else {
                return Vec::new();
            };
            drop(registry);

            (content, metadata)
        };

        let lint_diagnostics = graphql_analysis::lint_integration::lint_file(
            &self.db,
            content,
            metadata,
            self.project_files,
        );

        lint_diagnostics.iter().map(convert_diagnostic).collect()
    }

    /// Get semantic tokens for a file
    ///
    /// Returns tokens for syntax highlighting with semantic information,
    /// including deprecation status for fields.
    pub fn semantic_tokens(&self, file: &FilePath) -> Vec<SemanticToken> {
        let registry = self.registry.read();
        semantic_tokens::semantic_tokens(&self.db, &registry, self.project_files, file)
    }

    /// Get folding ranges for a file
    ///
    /// Returns foldable regions for:
    /// - Operation definitions (query, mutation, subscription)
    /// - Fragment definitions
    /// - Selection sets
    /// - Block comments
    pub fn folding_ranges(&self, file: &FilePath) -> Vec<FoldingRange> {
        let registry = self.registry.read();
        folding_ranges::folding_ranges(&self.db, &registry, file)
    }

    /// Get inlay hints for a file within an optional range.
    ///
    /// Returns inlay hints showing return types after scalar field selections.
    /// Includes support for the `__typename` introspection field.
    ///
    /// If `range` is provided, only returns hints within that range for efficiency.
    pub fn inlay_hints(&self, file: &FilePath, range: Option<Range>) -> Vec<InlayHint> {
        let registry = self.registry.read();
        inlay_hints::inlay_hints(&self.db, &registry, self.project_files, file, range)
    }

    /// Get project-wide lint diagnostics (e.g., unused fields, unique names)
    ///
    /// Returns a map of file paths -> diagnostics for project-wide lint rules.
    /// These are expensive rules that analyze the entire project.
    pub fn project_lint_diagnostics(&self) -> HashMap<FilePath, Vec<Diagnostic>> {
        let diagnostics_by_file_id = graphql_analysis::lint_integration::project_lint_diagnostics(
            &self.db,
            self.project_files,
        );

        let mut results = HashMap::new();
        let registry = self.registry.read();

        for (file_id, diagnostics) in diagnostics_by_file_id.iter() {
            if let Some(file_path) = registry.get_path(*file_id) {
                let converted: Vec<Diagnostic> =
                    diagnostics.iter().map(convert_diagnostic).collect();

                if !converted.is_empty() {
                    results.insert(file_path, converted);
                }
            }
        }

        results
    }

    /// Get all diagnostics for all files, merging per-file and project-wide diagnostics
    ///
    /// This is a convenience method for publishing diagnostics. It:
    /// 1. Gets per-file diagnostics (parse errors, validation errors, per-file lint rules)
    /// 2. Gets project-wide lint diagnostics (unused fields, etc.)
    /// 3. Merges them per file
    ///
    /// Returns a map of file paths -> all diagnostics for that file.
    pub fn all_diagnostics(&self) -> HashMap<FilePath, Vec<Diagnostic>> {
        let mut results: HashMap<FilePath, Vec<Diagnostic>> = HashMap::new();

        // Get all registered files
        let all_file_paths: Vec<FilePath> = {
            let registry = self.registry.read();
            registry
                .all_file_ids()
                .into_iter()
                .filter_map(|file_id| registry.get_path(file_id))
                .collect()
        };

        // Get per-file diagnostics for all files
        for file_path in &all_file_paths {
            let per_file = self.diagnostics(file_path);
            if !per_file.is_empty() {
                results.insert(file_path.clone(), per_file);
            }
        }

        // Get project-wide diagnostics and merge
        let project_diagnostics = self.project_lint_diagnostics();
        for (file_path, diagnostics) in project_diagnostics {
            results.entry(file_path).or_default().extend(diagnostics);
        }

        results
    }

    /// Get all diagnostics for a single file, merging per-file and project-wide diagnostics
    ///
    /// This returns the complete set of diagnostics for a file:
    /// - Per-file diagnostics (parse errors, validation errors, per-file lint rules)
    /// - Project-wide diagnostics (`unused_fields`, etc.) that apply to this file
    ///
    /// Use this when publishing diagnostics to avoid overwriting project-wide diagnostics
    /// with only per-file diagnostics.
    pub fn all_diagnostics_for_file(&self, file: &FilePath) -> Vec<Diagnostic> {
        let mut results = self.diagnostics(file);

        // Add project-wide diagnostics for this file
        let project_diagnostics = self.project_lint_diagnostics();
        if let Some(project_diags) = project_diagnostics.get(file) {
            results.extend(project_diags.iter().cloned());
        }

        results
    }

    /// Get all diagnostics for a specific set of files, merging per-file and project-wide diagnostics
    ///
    /// This is useful when you want diagnostics for specific files (e.g., loaded document files)
    /// rather than all files in the registry.
    pub fn all_diagnostics_for_files(
        &self,
        files: &[FilePath],
    ) -> HashMap<FilePath, Vec<Diagnostic>> {
        let mut results: HashMap<FilePath, Vec<Diagnostic>> = HashMap::new();

        // Get per-file diagnostics for specified files
        for file_path in files {
            let per_file = self.diagnostics(file_path);
            if !per_file.is_empty() {
                results.insert(file_path.clone(), per_file);
            }
        }

        // Get project-wide diagnostics and merge
        let project_diagnostics = self.project_lint_diagnostics();
        for (file_path, diagnostics) in project_diagnostics {
            // Only include if the file is in our set OR it's a schema file with issues
            // (project-wide lints like unused_fields report on schema files)
            results.entry(file_path).or_default().extend(diagnostics);
        }

        results
    }

    /// Get raw lint diagnostics with fix information for a file
    ///
    /// Returns `LintDiagnostic` objects that include fix information.
    /// Use this for implementing auto-fix functionality.
    pub fn lint_diagnostics_with_fixes(
        &self,
        file: &FilePath,
    ) -> Vec<graphql_linter::LintDiagnostic> {
        let (content, metadata) = {
            let registry = self.registry.read();

            let Some(file_id) = registry.get_file_id(file) else {
                return Vec::new();
            };

            let Some(content) = registry.get_content(file_id) else {
                return Vec::new();
            };
            let Some(metadata) = registry.get_metadata(file_id) else {
                return Vec::new();
            };
            drop(registry);

            (content, metadata)
        };

        graphql_analysis::lint_integration::lint_file_with_fixes(
            &self.db,
            content,
            metadata,
            self.project_files,
        )
    }

    /// Get project-wide raw lint diagnostics with fix information
    ///
    /// Returns a map of file paths -> `LintDiagnostic` objects that include fix information.
    pub fn project_lint_diagnostics_with_fixes(
        &self,
    ) -> HashMap<FilePath, Vec<graphql_linter::LintDiagnostic>> {
        let diagnostics_by_file_id =
            graphql_analysis::lint_integration::project_lint_diagnostics_with_fixes(
                &self.db,
                self.project_files,
            );

        let mut results = HashMap::new();
        let registry = self.registry.read();

        for (file_id, diagnostics) in diagnostics_by_file_id {
            if let Some(file_path) = registry.get_path(file_id) {
                if !diagnostics.is_empty() {
                    results.insert(file_path, diagnostics);
                }
            }
        }

        results
    }

    /// Get the content of a file
    ///
    /// Returns the text content of the file if it exists in the registry.
    pub fn file_content(&self, file: &FilePath) -> Option<String> {
        let registry = self.registry.read();
        let file_id = registry.get_file_id(file)?;
        let content = registry.get_content(file_id)?;
        Some(content.text(&self.db).to_string())
    }

    /// Get the status of the project (file counts, schema loaded, etc.)
    ///
    /// Returns status information for the LSP status command.
    #[must_use]
    pub fn project_status(&self) -> ProjectStatus {
        let Some(project_files) = self.project_files else {
            return ProjectStatus::default();
        };

        let schema_file_count = project_files.schema_file_ids(&self.db).ids(&self.db).len();
        let document_file_count = project_files
            .document_file_ids(&self.db)
            .ids(&self.db)
            .len();

        ProjectStatus::new(schema_file_count, document_file_count)
    }

    /// Get field usage coverage report for the project
    ///
    /// Analyzes which schema fields are used in operations and returns
    /// detailed coverage statistics. This is useful for understanding
    /// schema usage patterns and finding unused fields.
    pub fn field_coverage(&self) -> Option<FieldCoverageReport> {
        let pf = self.project_files?;
        Some(FieldCoverageReport::from(
            graphql_analysis::analyze_field_usage(&self.db, pf),
        ))
    }

    /// Get field usage for a specific field
    ///
    /// Returns usage information for a field if it exists in the schema.
    /// Useful for enhancing hover to show "Used in N operations".
    pub fn field_usage(&self, type_name: &str, field_name: &str) -> Option<FieldUsageInfo> {
        let pf = self.project_files?;
        let coverage = graphql_analysis::analyze_field_usage(&self.db, pf);
        let key = (
            std::sync::Arc::from(type_name),
            std::sync::Arc::from(field_name),
        );
        coverage.field_usages.get(&key).map(|usage| FieldUsageInfo {
            usage_count: usage.usage_count,
            operations: usage.operations.iter().map(ToString::to_string).collect(),
        })
    }

    /// Get complexity analysis for all operations in the project
    ///
    /// Analyzes each operation's selection set to calculate:
    /// - Total complexity score (with list multipliers)
    /// - Maximum depth
    /// - Per-field complexity breakdown
    /// - Connection pattern detection (Relay-style edges/nodes/pageInfo)
    /// - Warnings about potential issues (nested pagination, etc.)
    pub fn complexity_analysis(&self) -> Vec<ComplexityAnalysis> {
        let Some(project_files) = self.project_files else {
            return Vec::new();
        };

        // Get all operations in the project
        let operations = graphql_hir::all_operations(&self.db, project_files);
        let schema_types = graphql_hir::schema_types(&self.db, project_files);

        let mut results = Vec::new();

        for operation in operations.iter() {
            // Get file information for this operation
            let registry = self.registry.read();
            let Some(file_path) = registry.get_path(operation.file_id) else {
                continue;
            };
            let Some(content) = registry.get_content(operation.file_id) else {
                continue;
            };
            let Some(metadata) = registry.get_metadata(operation.file_id) else {
                continue;
            };
            drop(registry);

            // Get operation body
            let body = graphql_hir::operation_body(&self.db, content, metadata, operation.index);

            // Get operation location for the range
            let range = if let Some(ref name) = operation.name {
                let parse = graphql_syntax::parse(&self.db, content, metadata);
                let mut found_range = None;
                for doc in parse.documents() {
                    if let Some(ranges) = find_operation_definition_ranges(doc.tree, name) {
                        let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                        let doc_line_offset = doc.line_offset;
                        found_range = Some(adjust_range_for_line_offset(
                            offset_range_to_range(
                                &doc_line_index,
                                ranges.def_start,
                                ranges.def_end,
                            ),
                            doc_line_offset,
                        ));
                        break;
                    }
                }
                found_range.unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)))
            } else {
                Range::new(Position::new(0, 0), Position::new(0, 0))
            };

            // Create complexity analysis
            let op_name = operation
                .name
                .as_ref()
                .map_or_else(|| "<anonymous>".to_string(), ToString::to_string);

            #[allow(clippy::match_same_arms)]
            let op_type = match operation.operation_type {
                graphql_hir::OperationType::Query => "query",
                graphql_hir::OperationType::Mutation => "mutation",
                graphql_hir::OperationType::Subscription => "subscription",
                _ => "query", // fallback for future operation types
            };

            let mut analysis = ComplexityAnalysis::new(op_name, op_type, file_path, range);

            // Get the root type for this operation
            #[allow(clippy::match_same_arms)]
            let root_type_name = match operation.operation_type {
                graphql_hir::OperationType::Query => "Query",
                graphql_hir::OperationType::Mutation => "Mutation",
                graphql_hir::OperationType::Subscription => "Subscription",
                _ => "Query", // fallback for future operation types
            };

            // Analyze the operation body
            analyze_selections(
                &body.selections,
                schema_types,
                root_type_name,
                "",
                0,
                1,
                &mut analysis,
                false,
            );

            results.push(analysis);
        }

        results
    }

    /// Get completions at a position
    ///
    /// Returns a list of completion items appropriate for the context.
    pub fn completions(&self, file: &FilePath, position: Position) -> Option<Vec<CompletionItem>> {
        let registry = self.registry.read();
        completion::completions(&self.db, &registry, self.project_files, file, position)
    }

    /// Get hover information at a position
    ///
    /// Returns documentation, type information, etc.
    pub fn hover(&self, file: &FilePath, position: Position) -> Option<HoverResult> {
        let registry = self.registry.read();
        hover::hover(&self.db, &registry, self.project_files, file, position)
    }

    /// Get signature help at a position
    ///
    /// Returns argument information when inside a field or directive argument list.
    pub fn signature_help(&self, file: &FilePath, position: Position) -> Option<SignatureHelp> {
        let registry = self.registry.read();
        signature_help::signature_help(&self.db, &registry, self.project_files, file, position)
    }

    /// Get goto definition locations for the symbol at a position
    ///
    /// Returns the definition location(s) for types, fields, fragments, etc.
    pub fn goto_definition(&self, file: &FilePath, position: Position) -> Option<Vec<Location>> {
        let registry = self.registry.read();
        goto_definition::goto_definition(&self.db, &registry, self.project_files, file, position)
    }

    /// Find all references to the symbol at a position
    ///
    /// Returns locations of all usages of types, fields, fragments, etc.
    pub fn find_references(
        &self,
        file: &FilePath,
        position: Position,
        include_declaration: bool,
    ) -> Option<Vec<Location>> {
        let registry = self.registry.read();
        references::find_references(
            &self.db,
            &registry,
            self.project_files,
            file,
            position,
            include_declaration,
        )
    }

    /// Check if the symbol at a position can be renamed, returning its range.
    pub fn prepare_rename(&self, file: &FilePath, position: Position) -> Option<Range> {
        let registry = self.registry.read();
        rename::prepare_rename(&self.db, &registry, file, position)
    }

    /// Rename the symbol at a position to a new name.
    pub fn rename(
        &self,
        file: &FilePath,
        position: Position,
        new_name: &str,
    ) -> Option<RenameResult> {
        let registry = self.registry.read();
        rename::rename(
            &self.db,
            &registry,
            self.project_files,
            file,
            position,
            new_name,
        )
    }

    /// Find all references to a fragment
    pub fn find_fragment_references(
        &self,
        fragment_name: &str,
        include_declaration: bool,
    ) -> Vec<Location> {
        let registry = self.registry.read();
        references::find_fragment_references(
            &self.db,
            &registry,
            self.project_files,
            fragment_name,
            include_declaration,
        )
    }

    /// Get selection ranges for smart expand/shrink selection
    ///
    /// Returns a `SelectionRange` for each input position, forming a linked list
    /// from the innermost syntax element to the outermost (document).
    /// This powers the "Expand Selection" (Shift+Alt+Right) and
    /// "Shrink Selection" (Shift+Alt+Left) features.
    pub fn selection_ranges(
        &self,
        file: &FilePath,
        positions: &[Position],
    ) -> Vec<Option<SelectionRange>> {
        let registry = self.registry.read();
        selection_range::selection_ranges(&self.db, &registry, file, positions)
    }

    /// Get code lenses for deprecated fields in a schema file
    ///
    /// Returns code lens information for each deprecated field definition,
    /// including the usage count and locations for navigation.
    pub fn deprecated_field_code_lenses(&self, file: &FilePath) -> Vec<CodeLensInfo> {
        let registry = self.registry.read();
        code_lenses::deprecated_field_code_lenses(&self.db, &registry, self.project_files, file)
    }

    /// Get document symbols for a file (hierarchical outline)
    ///
    /// Returns types, operations, and fragments with their fields as children.
    /// This powers the "Go to Symbol in Editor" (Cmd+Shift+O) feature.
    pub fn document_symbols(&self, file: &FilePath) -> Vec<DocumentSymbol> {
        let registry = self.registry.read();
        symbols::document_symbols(&self.db, &registry, file)
    }

    /// Search for workspace symbols matching a query
    ///
    /// Returns matching types, operations, and fragments across all files.
    /// This powers the "Go to Symbol in Workspace" (Cmd+T) feature.
    pub fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbol> {
        let registry = self.registry.read();
        symbols::workspace_symbols(&self.db, &registry, self.project_files, query)
    }

    /// Get schema statistics
    ///
    /// Returns counts of types by kind, total fields, and directives.
    /// This uses the HIR layer directly for accurate field counting.
    pub fn schema_stats(&self) -> SchemaStats {
        let Some(project_files) = self.project_files else {
            return SchemaStats::default();
        };

        let types = graphql_hir::schema_types(&self.db, project_files);
        let mut stats = SchemaStats::default();

        for type_def in types.values() {
            match type_def.kind {
                graphql_hir::TypeDefKind::Object => stats.objects += 1,
                graphql_hir::TypeDefKind::Interface => stats.interfaces += 1,
                graphql_hir::TypeDefKind::Union => stats.unions += 1,
                graphql_hir::TypeDefKind::Enum => stats.enums += 1,
                graphql_hir::TypeDefKind::Scalar => stats.scalars += 1,
                graphql_hir::TypeDefKind::InputObject => stats.input_objects += 1,
                _ => {} // ignore future type kinds
            }
            // Count fields for types that have fields
            stats.total_fields += type_def.fields.len();
        }

        // Count directive definitions from schema files (excluding built-ins)
        let schema_ids = project_files.schema_file_ids(&self.db).ids(&self.db);
        for file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(&self.db, project_files, *file_id)
            else {
                continue;
            };

            // Skip built-in directive files
            let registry = self.registry.read();
            if let Some(path) = registry.get_path(*file_id) {
                let path_str = path.as_str();
                if path_str == "client_builtins.graphql" || path_str == "schema_builtins.graphql" {
                    drop(registry);
                    continue;
                }
            }
            drop(registry);

            let parse = graphql_syntax::parse(&self.db, content, metadata);
            // Count directive definitions by checking if the definition is a directive
            // Directives in GraphQL SDL start with "directive @"
            for doc in parse.documents() {
                for definition in &doc.ast.definitions {
                    if definition.as_directive_definition().is_some() {
                        stats.directives += 1;
                    }
                }
            }
        }

        stats
    }

    /// Get fragment usage analysis for the project
    ///
    /// Returns information about each fragment: its definition location,
    /// all usages (fragment spreads), and transitive dependencies.
    pub fn fragment_usages(&self) -> Vec<FragmentUsage> {
        let Some(project_files) = self.project_files else {
            return Vec::new();
        };

        let fragments = graphql_hir::all_fragments(&self.db, project_files);
        let mut results = Vec::new();

        for (name, fragment) in fragments {
            // Get definition location
            let Some((def_file, def_range)) = self.get_fragment_def_info(fragment) else {
                continue;
            };

            // Get all usages (fragment spreads) excluding the definition
            let spread_locations = self.find_fragment_references(name, false);
            let usages: Vec<FragmentReference> = spread_locations
                .into_iter()
                .map(FragmentReference::new)
                .collect();

            // Get transitive dependencies using the fragment spreads index
            let transitive_deps = self.compute_transitive_dependencies(name, project_files);

            results.push(FragmentUsage {
                name: name.to_string(),
                definition_file: def_file,
                definition_range: def_range,
                usages,
                transitive_dependencies: transitive_deps,
            });
        }

        // Sort by name for consistent ordering
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }

    /// Get fragment definition file and range
    fn get_fragment_def_info(
        &self,
        fragment: &graphql_hir::FragmentStructure,
    ) -> Option<(FilePath, Range)> {
        let registry = self.registry.read();
        let file_path = registry.get_path(fragment.file_id)?;
        let content = registry.get_content(fragment.file_id)?;
        let metadata = registry.get_metadata(fragment.file_id)?;
        drop(registry);

        let parse = graphql_syntax::parse(&self.db, content, metadata);

        for doc in parse.documents() {
            if let Some(ranges) = find_fragment_definition_full_range(doc.tree, &fragment.name) {
                let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
                let range = adjust_range_for_line_offset(
                    offset_range_to_range(&doc_line_index, ranges.name_start, ranges.name_end),
                    doc.line_offset,
                );
                return Some((file_path, range));
            }
        }

        None
    }

    /// Compute transitive fragment dependencies
    fn compute_transitive_dependencies(
        &self,
        fragment_name: &str,
        project_files: graphql_base_db::ProjectFiles,
    ) -> Vec<String> {
        let spreads_index = graphql_hir::fragment_spreads_index(&self.db, project_files);

        let mut visited = std::collections::HashSet::new();
        let mut to_visit = Vec::new();

        // Start with direct dependencies
        if let Some(direct_deps) = spreads_index.get(fragment_name) {
            to_visit.extend(direct_deps.iter().cloned());
        }

        while let Some(dep_name) = to_visit.pop() {
            if !visited.insert(dep_name.clone()) {
                continue; // Already visited (handles cycles)
            }

            // Add transitive dependencies
            if let Some(nested_deps) = spreads_index.get(&dep_name) {
                for nested in nested_deps {
                    if !visited.contains(nested) {
                        to_visit.push(nested.clone());
                    }
                }
            }
        }

        let mut deps: Vec<String> = visited.into_iter().map(|s| s.to_string()).collect();
        deps.sort();
        deps
    }

    /// Get code lenses for a file
    ///
    /// Returns code lenses for fragment definitions showing reference counts.
    pub fn code_lenses(&self, file: &FilePath) -> Vec<CodeLens> {
        let fragment_usages = self.fragment_usages();
        let registry = self.registry.read();
        code_lenses::code_lenses(
            &self.db,
            &registry,
            self.project_files,
            file,
            &fragment_usages,
        )
    }
}

// Private helper functions for complexity analysis

/// Analyze selections recursively to calculate complexity
#[allow(clippy::too_many_arguments)]
fn analyze_selections(
    selections: &[graphql_hir::Selection],
    schema_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    parent_type_name: &str,
    path_prefix: &str,
    depth: u32,
    multiplier: u32,
    analysis: &mut ComplexityAnalysis,
    in_connection: bool,
) {
    // Update max depth
    if depth > analysis.depth {
        analysis.depth = depth;
    }

    for selection in selections {
        match selection {
            graphql_hir::Selection::Field {
                name,
                selection_set,
                ..
            } => {
                let field_name = name.to_string();
                let path = if path_prefix.is_empty() {
                    field_name.clone()
                } else {
                    format!("{path_prefix}.{field_name}")
                };

                // Get field type info from schema
                let (is_list, inner_type_name) =
                    get_type_info(schema_types, parent_type_name, &field_name);

                // Calculate field multiplier
                let field_multiplier = if is_list {
                    multiplier * 10 // Default list multiplier
                } else {
                    multiplier
                };

                // Check for connection pattern
                let field_is_connection =
                    is_connection_pattern(&field_name, schema_types, &inner_type_name);

                // Warn about nested pagination
                if in_connection && field_is_connection {
                    analysis.warnings.push(format!(
                        "Nested pagination detected at {path}. This can cause performance issues."
                    ));
                }

                // Calculate complexity for this field
                let field_complexity = field_multiplier;
                analysis.total_complexity += field_complexity;

                // Add to breakdown
                let mut fc = FieldComplexity::new(&path, &field_name, field_complexity)
                    .with_multiplier(if is_list { 10 } else { 1 })
                    .with_depth(depth)
                    .with_connection(field_is_connection);

                if in_connection && field_is_connection {
                    fc = fc.with_warning("Nested pagination");
                }

                analysis.breakdown.push(fc);

                // Recurse into nested selections
                if !selection_set.is_empty() {
                    analyze_selections(
                        selection_set,
                        schema_types,
                        &inner_type_name,
                        &path,
                        depth + 1,
                        field_multiplier,
                        analysis,
                        field_is_connection || in_connection,
                    );
                }
            }
            graphql_hir::Selection::FragmentSpread { .. }
            | graphql_hir::Selection::InlineFragment { .. } => {
                // For simplicity, we don't deeply analyze fragment spreads in this implementation
                // A full implementation would resolve the fragment and analyze its selections
            }
        }
    }
}

/// Check if a field follows the Relay connection pattern (edges/nodes/pageInfo)
fn is_connection_pattern(
    _field_name: &str,
    schema_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    type_name: &str,
) -> bool {
    // Check if the return type has edges, nodes, or pageInfo fields
    if let Some(type_def) = schema_types.get(type_name) {
        if type_def.kind == graphql_hir::TypeDefKind::Object {
            let has_edges = type_def.fields.iter().any(|f| f.name.as_ref() == "edges");
            let has_page_info = type_def
                .fields
                .iter()
                .any(|f| f.name.as_ref() == "pageInfo");
            let has_nodes = type_def.fields.iter().any(|f| f.name.as_ref() == "nodes");

            return (has_edges || has_nodes) && has_page_info;
        }
    }
    false
}

/// Get type information for a field: (`is_list`, `inner_type_name`)
fn get_type_info(
    schema_types: &std::collections::HashMap<Arc<str>, graphql_hir::TypeDef>,
    parent_type_name: &str,
    field_name: &str,
) -> (bool, String) {
    if let Some(type_def) = schema_types.get(parent_type_name) {
        if type_def.kind == graphql_hir::TypeDefKind::Object {
            if let Some(field) = type_def
                .fields
                .iter()
                .find(|f| f.name.as_ref() == field_name)
            {
                let type_ref = &field.type_ref;
                return (type_ref.is_list, type_ref.name.to_string());
            }
        }
    }
    (false, "Unknown".to_string())
}
