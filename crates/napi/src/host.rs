use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use parking_lot::Mutex;

use graphql_ide::{AnalysisHost, DocumentKind, FilePath, Language};

#[derive(Default)]
pub struct NapiAnalysisHost {
    host: AnalysisHost,
    known_files: HashMap<PathBuf, KnownFile>,
    config_path: Option<PathBuf>,
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

        let project_count = config.projects().count();
        if project_count > 1 {
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
        let resolved_schema = project
            .resolved_schema()
            .map(|path| canonicalize_or(&base_dir.join(path)));
        for path in &schema_result.loaded_paths {
            let language = language_and_kind_from_path(&path.to_string_lossy()).0;
            self.remember_file(path, language, DocumentKind::Schema);
            if let Some(file) = self.known_files.get_mut(&canonicalize_or(path)) {
                file.reload_schema |= path
                    .extension()
                    .is_some_and(|extension| extension == "json")
                    || resolved_schema.as_ref() == Some(&canonicalize_or(path));
            }
        }

        let extract_config = self.host.get_extract_config();
        let (loaded, _result) =
            self.host
                .load_documents_from_config(project, base_dir, &extract_config);
        for file in &loaded {
            let path = file_path_to_pathbuf(&file.path);
            self.remember_file(&path, file.language, file.document_kind);
        }

        self.config_path = Some(config_path.to_path_buf());
        Ok(())
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

    fn refresh_dependencies(&mut self) -> anyhow::Result<()> {
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
            if let Some(config_path) = self.config_path.clone() {
                // The schema loader owns JSON introspection and split embedded schema entries.
                let overlays: Vec<_> = self
                    .known_files
                    .iter()
                    .filter(|(path, file)| {
                        !changes.iter().any(|(changed, source, _)| {
                            changed == *path && source != &file.disk_source
                        })
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
                let mut refreshed = Self::default();
                refreshed.init_from_config(&config_path)?;
                refreshed.known_files.extend(missing);
                for (path, file, source) in overlays {
                    refreshed.known_files.entry(path.clone()).or_insert(file);
                    refreshed.update_source(&path, &source);
                }
                *self = refreshed;
                return Ok(());
            }
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

    pub fn extract_config(&self) -> graphql_extract::ExtractConfig {
        self.host.get_extract_config()
    }

    pub fn lint_file(
        &mut self,
        path: &str,
        source: &str,
    ) -> anyhow::Result<Vec<graphql_ide::Diagnostic>> {
        self.refresh_dependencies()?;
        let canonical = canonicalize_or(Path::new(path));
        let file_path = self.update_source(&canonical, source);
        let snapshot = self.host.snapshot();
        Ok(snapshot.all_diagnostics_for_file(&file_path))
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
