use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::Mutex;

use graphql_ide::{AnalysisHost, DocumentKind, FilePath, Language};

pub struct NapiAnalysisHost {
    host: AnalysisHost,
    schema_files: Vec<PathBuf>,
    #[allow(dead_code)]
    document_files: Vec<PathBuf>,
    /// Files loaded during init — we preserve their document kind on re-add
    known_files: HashSet<String>,
    initialized: bool,
}

static HOST: OnceLock<Mutex<NapiAnalysisHost>> = OnceLock::new();

pub fn get_host() -> &'static Mutex<NapiAnalysisHost> {
    HOST.get_or_init(|| {
        Mutex::new(NapiAnalysisHost {
            host: AnalysisHost::new(),
            schema_files: Vec::new(),
            document_files: Vec::new(),
            known_files: HashSet::new(),
            initialized: false,
        })
    })
}

impl NapiAnalysisHost {
    pub fn init_from_config(&mut self, config_path: &Path) -> anyhow::Result<()> {
        let config = graphql_config::load_config(config_path)?;
        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

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
                if let Ok(extract_config) =
                    serde_json::from_value::<graphql_extract::ExtractConfig>(extract_value.clone())
                {
                    self.host.set_extract_config(extract_config);
                }
            }
        }

        let schema_result = self.host.load_schemas_from_config(project, base_dir)?;
        for path in &schema_result.loaded_paths {
            self.known_files
                .insert(path.to_string_lossy().to_string());
        }
        self.schema_files.extend(schema_result.loaded_paths);

        let extract_config = self.host.get_extract_config();
        let (loaded, _result) =
            self.host
                .load_documents_from_config(project, base_dir, &extract_config);
        for file in &loaded {
            if let Some(path) = file_path_to_pathbuf(&file.path) {
                self.known_files
                    .insert(path.to_string_lossy().to_string());
                self.document_files.push(path);
            }
        }

        self.initialized = true;
        Ok(())
    }

    pub fn lint_file(&mut self, path: &str, source: &str) -> Vec<graphql_ide::Diagnostic> {
        let file_path = FilePath::from_path(Path::new(path));

        if !self.known_files.contains(path) {
            // New file not loaded during init — add with inferred kind
            let (language, document_kind) = language_and_kind_from_path(path);
            self.host
                .add_file(&file_path, source, language, document_kind);
        }
        // For known files, content was already loaded during init.
        // ESLint sends the same content from disk, so no update needed.
        // (Live editor content updates would need a separate code path.)

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

fn file_path_to_pathbuf(fp: &FilePath) -> Option<PathBuf> {
    let s = fp.as_str();
    if let Some(rest) = s.strip_prefix("file://") {
        Some(PathBuf::from(rest))
    } else {
        Some(PathBuf::from(s))
    }
}
