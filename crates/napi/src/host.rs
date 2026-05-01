use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::Mutex;

use graphql_ide::{AnalysisHost, DocumentKind, FilePath, Language};

/// One project's analysis state. We keep a separate `AnalysisHost` per
/// project so each project's schema, documents, and lint config stay
/// isolated — files in `apps/web` don't accidentally see schemas from
/// `apps/server` and vice versa.
struct ProjectState {
    config: graphql_config::ProjectConfig,
    host: AnalysisHost,
    /// Canonicalized paths of files loaded during init. On subsequent
    /// `lint_file` calls for these paths we skip re-adding the file,
    /// preserving the document kind (Schema vs Executable) set at init.
    known_files: HashSet<PathBuf>,
}

pub struct NapiAnalysisHost {
    /// Per-project analysis state. Empty until `init_from_config` runs.
    projects: Vec<ProjectState>,
    /// Workspace root resolved from the config file's parent directory.
    /// Used to make file paths relative for `ProjectConfig::matches_file`.
    workspace_root: PathBuf,
    initialized: bool,
}

static HOST: OnceLock<Mutex<NapiAnalysisHost>> = OnceLock::new();

pub fn get_host() -> &'static Mutex<NapiAnalysisHost> {
    HOST.get_or_init(|| {
        Mutex::new(NapiAnalysisHost {
            projects: Vec::new(),
            workspace_root: PathBuf::new(),
            initialized: false,
        })
    })
}

impl NapiAnalysisHost {
    pub fn init_from_config(&mut self, config_path: &Path) -> anyhow::Result<()> {
        // Reset host state so a second init for a *different* config doesn't
        // leave the prior project's schema/documents resident. The JS adapter
        // calls `init` once per resolved config path, so monorepos with
        // multiple configs (and parity tests that spin up many throwaway
        // projects in a single process) need each init to start fresh.
        self.projects.clear();
        self.initialized = false;

        let config = graphql_config::load_config(config_path)?;
        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        self.workspace_root = base_dir.to_path_buf();

        for (name, project) in config.projects() {
            let mut host = AnalysisHost::new();

            if let Some(lint_value) = project.lint() {
                let lint_config = serde_json::from_value::<graphql_linter::LintConfig>(lint_value)?;
                if let Err(e) = lint_config.validate() {
                    return Err(anyhow::anyhow!(
                        "Invalid lint configuration in project '{name}':\n\n{e}"
                    ));
                }
                host.set_lint_config(lint_config);
            }

            // Files matched by the project's `documents:` config are explicit
            // GraphQL sources, so a bare `gql` tag without an `import` should
            // still be extracted (issue #1035). User overrides come from the
            // modern `extensions.graphql-analyzer.extractConfig` namespace.
            let extract_value = project.extract_config();
            let extract_config = graphql_extract::resolve_for_documents(extract_value.as_ref());
            host.set_extract_config(extract_config);

            let mut known_files = HashSet::new();
            let schema_result = host.load_schemas_from_config(project, base_dir)?;
            for path in &schema_result.loaded_paths {
                known_files.insert(canonicalize_or(path));
            }

            let extract_config = host.get_extract_config();
            let (loaded, _result) =
                host.load_documents_from_config(project, base_dir, &extract_config);
            for file in &loaded {
                if let Some(path) = file_path_to_pathbuf(&file.path) {
                    known_files.insert(canonicalize_or(&path));
                }
            }

            let _ = name; // currently only surfaced in error messages above
            self.projects.push(ProjectState {
                config: project.clone(),
                host,
                known_files,
            });
        }

        if self.projects.is_empty() {
            return Err(anyhow::anyhow!("No projects found in config"));
        }

        self.initialized = true;
        Ok(())
    }

    pub fn extract_config(&self) -> graphql_extract::ExtractConfig {
        // Extract config is only used for embedded-GraphQL extraction in the
        // processor, before any per-file routing happens. Use the first
        // project's config — projects in a single workspace typically share
        // pluck conventions, and a stricter API would force the processor to
        // know which file it's looking at before extracting.
        self.projects
            .first()
            .map_or_else(graphql_extract::ExtractConfig::default, |p| {
                p.host.get_extract_config()
            })
    }

    pub fn lint_file(
        &mut self,
        path: &str,
        source: &str,
        overrides: Option<std::collections::HashMap<String, graphql_linter::LintRuleConfig>>,
    ) -> Vec<graphql_ide::Diagnostic> {
        let file_path = FilePath::from_path(Path::new(path));
        let canonical = canonicalize_or(Path::new(path));

        let Some(project_idx) = self.project_for_file(&canonical) else {
            // No project claims this file. Return empty rather than guessing
            // — emitting a diagnostic from the wrong project's schema would
            // be worse than emitting none.
            return Vec::new();
        };
        let project = &mut self.projects[project_idx];

        if !project.known_files.contains(&canonical) {
            let (language, document_kind) = language_and_kind_from_path(path);
            project
                .host
                .add_file(&file_path, source, language, document_kind);
        }
        // Known files were loaded during init; ESLint passes the on-disk
        // content, which matches. Live-editor updates would need a separate
        // path here.

        // Apply per-call rule overrides on top of the persistent config for
        // the duration of this lint call, then restore. ESLint passes per-
        // rule options through `rules: { rule: [severity, options] }` and
        // the shim forwards them here so they take precedence over whatever
        // `.graphqlrc.yaml` provided. Restoration keeps subsequent calls
        // (and other consumers of the same host) on the persistent config.
        let restore = if let Some(overrides) = overrides.filter(|m| !m.is_empty()) {
            let original = project.host.lint_config();
            let merged = (*original).clone().with_overrides(overrides);
            project.host.set_lint_config(merged);
            Some(original)
        } else {
            None
        };

        let diagnostics = {
            let snapshot = project.host.snapshot();
            snapshot.all_diagnostics_for_file(&file_path)
        };

        if let Some(original) = restore {
            project.host.set_lint_config((*original).clone());
        }

        diagnostics
    }

    /// Resolve a file path to the project that owns it, mirroring
    /// graphql-config's `getProjectForFile` semantics:
    ///
    /// 1. First project whose `matches_file` returns true wins.
    /// 2. If no project matches and exactly one project has no
    ///    include/exclude/schema/document constraints, that catch-all
    ///    project wins.
    /// 3. Otherwise, return `None` and let the caller produce an empty
    ///    result.
    ///
    /// The single-project case ends up at branch 1 or 2 trivially, so this
    /// stays a no-op fast path for non-monorepo users.
    fn project_for_file(&self, canonical: &Path) -> Option<usize> {
        for (idx, p) in self.projects.iter().enumerate() {
            if p.config.matches_file(canonical, &self.workspace_root) {
                return Some(idx);
            }
        }
        let unconstrained: Vec<usize> = self
            .projects
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.config.has_file_constraints())
            .map(|(i, _)| i)
            .collect();
        if unconstrained.len() == 1 {
            return Some(unconstrained[0]);
        }
        // Single-project default: even if it has constraints, treat it as
        // the catch-all (matches the legacy single-project behavior where
        // any unmatched file still got linted against the only project).
        if self.projects.len() == 1 {
            return Some(0);
        }
        let _ = canonical; // suppress unused-var when no project claims the file
        None
    }
}

fn language_and_kind_from_path(path: &str) -> (Language, DocumentKind) {
    match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("ts" | "tsx") => (Language::TypeScript, DocumentKind::Executable),
        Some("js" | "jsx" | "mjs" | "cjs") => (Language::JavaScript, DocumentKind::Executable),
        Some("vue") => (Language::Vue, DocumentKind::Executable),
        Some("svelte") => (Language::Svelte, DocumentKind::Executable),
        Some("astro") => (Language::Astro, DocumentKind::Executable),
        _ => (Language::GraphQL, DocumentKind::Executable),
    }
}

/// Canonicalize for stable equality in `known_files`. Falls back to the input
/// path when the file doesn't exist on disk (e.g., virtual paths from tests).
fn canonicalize_or(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn file_path_to_pathbuf(fp: &FilePath) -> Option<PathBuf> {
    let s = fp.as_str();
    let rest = s.strip_prefix("file://").unwrap_or(s);
    // On Windows, `file:///C:/foo` leaves `/C:/foo` after the strip — drop the
    // extra leading slash so `C:/foo` round-trips as a valid path.
    if cfg!(windows) && rest.starts_with('/') && rest.len() > 3 && rest.as_bytes()[2] == b':' {
        Some(PathBuf::from(&rest[1..]))
    } else {
        Some(PathBuf::from(rest))
    }
}
