use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use parking_lot::Mutex;

use graphql_ide::{AnalysisHost, DocumentKind, FilePath, Language};

struct ProjectState {
    config: graphql_config::ProjectConfig,
    host: AnalysisHost,
    known_files: HashMap<PathBuf, KnownFile>,
}

#[derive(Default)]
pub struct NapiAnalysisHost {
    projects: Vec<ProjectState>,
    workspace_root: PathBuf,
}

#[derive(Clone)]
struct KnownFile {
    path: FilePath,
    language: Language,
    document_kind: DocumentKind,
    disk_source: Option<String>,
    disk_fingerprint: Option<FileFingerprint>,
    overlay: Option<String>,
    transformed: bool,
    reload_schema: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct FileFingerprint {
    modified: Option<SystemTime>,
    len: u64,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FileFingerprint {
    fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok(),
            len: metadata.len(),
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (
                    metadata.dev(),
                    metadata.ino(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                )
            },
        })
    }
}

static HOST: OnceLock<Mutex<NapiAnalysisHost>> = OnceLock::new();

pub fn get_host() -> &'static Mutex<NapiAnalysisHost> {
    HOST.get_or_init(|| Mutex::new(NapiAnalysisHost::default()))
}

impl NapiAnalysisHost {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn init_from_config(&mut self, config_path: &Path) -> anyhow::Result<()> {
        self.reset();
        let config = graphql_config::load_config(config_path)?;
        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        let projects = config
            .projects()
            .map(|(name, project)| {
                ProjectState::load(project, base_dir)
                    .map_err(|e| anyhow::anyhow!("Invalid project '{name}': {e}"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        if projects.is_empty() {
            anyhow::bail!("No projects found in config");
        }
        self.projects = projects;
        self.workspace_root = base_dir.to_path_buf();
        Ok(())
    }

    pub fn extract_config(&self) -> graphql_extract::ExtractConfig {
        // Extraction has no file path for project routing, so it uses the first project.
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
    ) -> anyhow::Result<Vec<graphql_ide::Diagnostic>> {
        let canonical = canonicalize_or(Path::new(path));

        let Some(project_idx) = self.project_for_file(&canonical) else {
            return Ok(Vec::new());
        };
        let project = &mut self.projects[project_idx];

        project.refresh_dependencies(&self.workspace_root)?;
        let file_path = project.update_source(&canonical, source);

        // ESLint options apply only to this call; later files keep the project config.
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

        Ok(diagnostics)
    }

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
        // A single project also owns files outside its discovery patterns.
        if self.projects.len() == 1 {
            return Some(0);
        }
        None
    }
}

impl ProjectState {
    fn load(project: &graphql_config::ProjectConfig, base_dir: &Path) -> anyhow::Result<Self> {
        let mut state = Self {
            config: project.clone(),
            host: AnalysisHost::new(),
            known_files: HashMap::new(),
        };
        if let Some(lint_value) = project.lint() {
            let lint_config = serde_json::from_value::<graphql_linter::LintConfig>(lint_value)?;
            if let Err(e) = lint_config.validate() {
                return Err(anyhow::anyhow!("Invalid lint configuration:\n\n{e}"));
            }
            state.host.set_lint_config(lint_config);
        }

        let extract_value = project.extract_config()?;
        let extract_config = graphql_extract::resolve_for_documents(extract_value.as_ref());
        state.host.set_extract_config(extract_config);

        let schema_result = state.host.load_schemas_from_config(project, base_dir)?;
        let resolved_schema = project
            .resolved_schema()
            .map(|path| canonicalize_or(&base_dir.join(path)));
        for path in &schema_result.loaded_paths {
            let language = language_and_kind_from_path(&path.to_string_lossy()).0;
            state.remember_file(path, language, DocumentKind::Schema);
            if let Some(file) = state.known_files.get_mut(&canonicalize_or(path)) {
                file.reload_schema |= path
                    .extension()
                    .is_some_and(|extension| extension == "json")
                    || resolved_schema.as_ref() == Some(&canonicalize_or(path));
            }
        }

        let extract_config = state.host.get_extract_config();
        let (loaded, _result) =
            state
                .host
                .load_documents_from_config(project, base_dir, &extract_config);
        for file in &loaded {
            let path = file_path_to_pathbuf(&file.path);
            state.remember_file(&path, file.language, file.document_kind);
        }

        Ok(state)
    }

    fn remember_file(&mut self, path: &Path, language: Language, document_kind: DocumentKind) {
        let file_path = FilePath::from_path(path);
        let disk_source = std::fs::read_to_string(path).ok();
        let registered_source = self.host.snapshot().file_content(&file_path);
        let transformed = registered_source != disk_source;
        self.known_files.insert(
            canonicalize_or(path),
            KnownFile {
                path: file_path,
                language,
                document_kind,
                disk_source,
                disk_fingerprint: FileFingerprint::read(path),
                overlay: None,
                transformed,
                reload_schema: transformed && document_kind.is_schema(),
            },
        );
    }

    fn update_source(&mut self, path: &Path, source: &str) -> FilePath {
        let file = self
            .known_files
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let (language, document_kind) =
                    language_and_kind_from_path(&path.to_string_lossy());
                KnownFile {
                    path: FilePath::from_path(path),
                    language,
                    document_kind,
                    disk_source: std::fs::read_to_string(path).ok(),
                    disk_fingerprint: FileFingerprint::read(path),
                    overlay: None,
                    transformed: false,
                    reload_schema: false,
                }
            });
        if file.transformed {
            let prefix = format!("{}#L", file.path.as_str());
            for registered_path in self.host.files() {
                if registered_path.as_str().starts_with(&prefix) {
                    self.host.remove_file(&registered_path);
                }
            }
            file.transformed = false;
        }
        self.host
            .add_file(&file.path, source, file.language, file.document_kind);
        file.overlay = (file.disk_source.as_deref() != Some(source)).then(|| source.to_string());
        file.path.clone()
    }

    fn refresh_dependencies(&mut self, base_dir: &Path) -> anyhow::Result<()> {
        let changes: Vec<_> = self
            .known_files
            .iter()
            .filter_map(|(path, file)| {
                let fingerprint = FileFingerprint::read(path);
                if fingerprint == file.disk_fingerprint {
                    return None;
                }
                let source = std::fs::read_to_string(path).ok();
                Some((path.clone(), source, fingerprint))
            })
            .collect();

        if changes.iter().any(|(path, source, _)| {
            let file = &self.known_files[path];
            file.reload_schema && source != &file.disk_source
        }) {
            // The schema loader owns JSON introspection and split embedded schema entries.
            let overlays: Vec<_> = self
                .known_files
                .iter()
                .filter(|(path, file)| {
                    !changes
                        .iter()
                        .any(|(changed, source, _)| changed == *path && source != &file.disk_source)
                })
                .filter_map(|(path, file)| {
                    file.overlay
                        .as_ref()
                        .map(|source| (path.clone(), file.clone(), source.clone()))
                })
                .collect();
            let missing: Vec<_> = changes
                .iter()
                .filter(|(_, source, _)| source.is_none())
                .map(|(path, _, fingerprint)| {
                    let mut file = self.known_files[path].clone();
                    file.disk_source = None;
                    file.disk_fingerprint = fingerprint.clone();
                    file.overlay = None;
                    (path.clone(), file)
                })
                .collect();
            let mut refreshed = Self::load(&self.config, base_dir)?;
            refreshed.known_files.extend(missing);
            for (path, file, source) in overlays {
                refreshed.known_files.entry(path.clone()).or_insert(file);
                refreshed.update_source(&path, &source);
            }
            *self = refreshed;
            return Ok(());
        }

        for (path, source, fingerprint) in changes {
            let Some(file) = self.known_files.get_mut(&path) else {
                continue;
            };
            file.disk_fingerprint = fingerprint;
            if source == file.disk_source {
                continue;
            }
            if let Some(source) = &source {
                self.host
                    .add_file(&file.path, source, file.language, file.document_kind);
            } else {
                self.host.remove_file(&file.path);
            }
            file.disk_source = source;
            file.overlay = None;
        }
        Ok(())
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

fn file_path_to_pathbuf(fp: &FilePath) -> PathBuf {
    let s = fp.as_str();
    let rest = s.strip_prefix("file://").unwrap_or(s);
    // On Windows, `file:///C:/foo` leaves `/C:/foo` after the strip — drop the
    // extra leading slash so `C:/foo` round-trips as a valid path.
    if cfg!(windows) && rest.starts_with('/') && rest.len() > 3 && rest.as_bytes()[2] == b':' {
        PathBuf::from(&rest[1..])
    } else {
        PathBuf::from(rest)
    }
}
