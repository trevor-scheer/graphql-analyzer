use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::Mutex;

use graphql_ide::{AnalysisHost, DocumentKind, FilePath, Language};

pub struct NapiAnalysisHost {
    host: AnalysisHost,
    /// Canonicalized paths of files loaded during `init_from_config`. On
    /// subsequent `lint_file` calls for these paths we skip re-adding the file,
    /// preserving the document kind (Schema vs Executable) set at init time.
    known_files: HashSet<PathBuf>,
    initialized: bool,
}

static HOST: OnceLock<Mutex<NapiAnalysisHost>> = OnceLock::new();

pub fn get_host() -> &'static Mutex<NapiAnalysisHost> {
    HOST.get_or_init(|| {
        Mutex::new(NapiAnalysisHost {
            host: AnalysisHost::new(),
            known_files: HashSet::new(),
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
        self.host = AnalysisHost::new();
        self.known_files.clear();
        self.initialized = false;

        let config = graphql_config::load_config(config_path)?;
        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

        let project_count = config.projects().count();
        if project_count > 1 {
            // Supporting multi-project configs requires selecting the project
            // whose documents/schema globs match the file being linted. That
            // dispatch isn't wired yet — fail loudly rather than silently
            // dropping the extra projects.
            anyhow::bail!(
                "Multi-project .graphqlrc configs are not yet supported by the ESLint plugin \
                 (found {project_count} projects in {}). Use a single-project config for now.",
                config_path.display()
            );
        }

        let (_name, project) = config
            .projects()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No projects found in config"))?;

        if let Some(lint_value) = project.lint() {
            let lint_config = serde_json::from_value::<graphql_linter::LintConfig>(lint_value)?;
            if let Err(e) = lint_config.validate() {
                return Err(anyhow::anyhow!("Invalid lint configuration:\n\n{e}"));
            }
            self.host.set_lint_config(lint_config);
        }

        if let Some(ref extensions) = project.extensions {
            if let Some(extract_value) = extensions.get("extractConfig") {
                let extract_config =
                    serde_json::from_value::<graphql_extract::ExtractConfig>(extract_value.clone())
                        .map_err(|e| anyhow::anyhow!("Invalid extractConfig:\n\n{e}"))?;
                self.host.set_extract_config(extract_config);
            }
        }

        let schema_result = self.host.load_schemas_from_config(project, base_dir)?;
        for path in &schema_result.loaded_paths {
            self.known_files.insert(canonicalize_or(path));
        }

        let extract_config = self.host.get_extract_config();
        let (loaded, _result) =
            self.host
                .load_documents_from_config(project, base_dir, &extract_config);
        for file in &loaded {
            if let Some(path) = file_path_to_pathbuf(&file.path) {
                self.known_files.insert(canonicalize_or(&path));
            }
        }

        self.initialized = true;
        Ok(())
    }

    pub fn extract_config(&self) -> graphql_extract::ExtractConfig {
        self.host.get_extract_config()
    }

    pub fn lint_file(&mut self, path: &str, source: &str) -> Vec<graphql_ide::Diagnostic> {
        let file_path = FilePath::from_path(Path::new(path));
        let canonical = canonicalize_or(Path::new(path));

        if !self.known_files.contains(&canonical) {
            let (language, document_kind) = language_and_kind_from_path(path);
            self.host
                .add_file(&file_path, source, language, document_kind);
        }
        // Known files were loaded during init; ESLint passes the on-disk
        // content, which matches. Live-editor updates would need a separate
        // path here.

        let snapshot = self.host.snapshot();
        snapshot.all_diagnostics_for_file(&file_path)
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
