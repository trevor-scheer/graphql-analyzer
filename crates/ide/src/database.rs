use std::sync::Arc;

/// Input: Lint configuration
///
/// This is a Salsa input so that config changes properly invalidate dependent queries.
/// Wrapping in Arc allows queries to access the config without cloning the entire config object.
///
/// Using Salsa inputs instead of `Arc<RwLock<...>>` ensures:
/// - Proper dependency tracking (Salsa knows which queries depend on config)
/// - Automatic invalidation (only config-dependent queries re-run on changes)
/// - No deadlock risk (Salsa manages all locking internally)
/// - Snapshot isolation (config is immutable in Analysis snapshots)
#[salsa::input]
pub(crate) struct LintConfigInput {
    pub config: Arc<graphql_linter::LintConfig>,
}

/// Input: Extract configuration for TypeScript/JavaScript extraction
///
/// This is a Salsa input so that config changes properly invalidate dependent queries.
///
/// Using Salsa inputs instead of `Arc<RwLock<...>>` ensures:
/// - Proper dependency tracking (Salsa knows which queries depend on config)
/// - Automatic invalidation (only config-dependent queries re-run on changes)
/// - No deadlock risk (Salsa manages all locking internally)
/// - Snapshot isolation (config is immutable in Analysis snapshots)
#[salsa::input]
pub(crate) struct ExtractConfigInput {
    pub config: Arc<graphql_extract::ExtractConfig>,
}

/// Custom database that implements config traits
///
/// All configuration is now stored as Salsa inputs (`LintConfigInput`, `ExtractConfigInput`,
/// and `ProjectFiles`) instead of `Arc<RwLock<...>>` wrappers. This allows Salsa to properly
/// track config dependencies and only invalidate affected queries when inputs change.
///
/// Queries can access `project_files` via `db.project_files()` and Salsa will automatically
/// track dependencies when the query calls getters like `project_files.schema_file_ids(db)`.
#[salsa::db]
#[derive(Clone)]
pub(crate) struct IdeDatabase {
    pub(crate) storage: salsa::Storage<Self>,
    pub(crate) lint_config_input: Option<LintConfigInput>,
    pub(crate) extract_config_input: Option<ExtractConfigInput>,
    /// Project files input - stores the current `ProjectFiles` Salsa input directly.
    /// Unlike the old `Arc<RwLock<...>>` approach, this enables proper Salsa dependency
    /// tracking: queries that call `db.project_files()` and then access fields like
    /// `project_files.schema_file_ids(db)` will have their dependencies tracked.
    pub(crate) project_files_input: Option<graphql_base_db::ProjectFiles>,
}

impl Default for IdeDatabase {
    fn default() -> Self {
        let mut db = Self {
            storage: salsa::Storage::new(Some(Box::new(|event: salsa::Event| match event.kind {
                salsa::EventKind::WillExecute { database_key, .. } => {
                    tracing::debug!("query cache miss (executing): {database_key:?}");
                }
                salsa::EventKind::DidValidateMemoizedValue { database_key } => {
                    tracing::debug!("query cache hit (memoized): {database_key:?}");
                }
                _ => {}
            }))),
            lint_config_input: None,
            extract_config_input: None,
            project_files_input: None,
        };

        // Initialize with default configs as Salsa inputs
        db.lint_config_input = Some(LintConfigInput::new(
            &db,
            Arc::new(graphql_linter::LintConfig::default()),
        ));
        db.extract_config_input = Some(ExtractConfigInput::new(
            &db,
            Arc::new(graphql_extract::ExtractConfig::default()),
        ));

        db
    }
}

#[salsa::db]
impl salsa::Database for IdeDatabase {}

#[salsa::db]
impl graphql_syntax::GraphQLSyntaxDatabase for IdeDatabase {
    fn extract_config(&self) -> Option<Arc<graphql_extract::ExtractConfig>> {
        self.extract_config_input
            .map(|input| input.config(self).clone())
    }
}

#[salsa::db]
impl graphql_hir::GraphQLHirDatabase for IdeDatabase {
    fn project_files(&self) -> Option<graphql_base_db::ProjectFiles> {
        self.project_files_input
    }
}

#[salsa::db]
impl graphql_analysis::GraphQLAnalysisDatabase for IdeDatabase {
    fn lint_config(&self) -> Arc<graphql_linter::LintConfig> {
        self.lint_config_input.map_or_else(
            || Arc::new(graphql_linter::LintConfig::default()),
            |input| input.config(self).clone(),
        )
    }
}
