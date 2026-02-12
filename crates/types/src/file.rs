//! File-related types: [`FileId`], [`FileUri`], [`Language`], [`DocumentKind`].

use std::path::Path;
use std::sync::Arc;

/// Input file identifier in the project.
///
/// A simple u32-based ID that uniquely identifies a file within a project.
/// `FileId`s are assigned when files are added to the project and remain stable
/// across content changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(u32);

impl FileId {
    /// Create a new `FileId` from a raw u32 value.
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw u32 value of this `FileId`.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// A URI string identifying a file.
///
/// This is typically a `file://` URI for local files or a custom scheme
/// for virtual files (e.g., `schema://` for introspected schemas).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileUri(Arc<str>);

impl FileUri {
    /// Create a new `FileUri` from a string.
    #[must_use]
    pub fn new(uri: impl Into<Arc<str>>) -> Self {
        Self(uri.into())
    }

    /// Get the URI as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FileUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Source language of a file (determines parsing strategy).
///
/// This enum represents the syntactic format of a file, which determines
/// HOW to parse it (direct GraphQL parsing vs. extraction from template literals).
///
/// This is orthogonal to [`DocumentKind`], which determines the semantic purpose
/// of the content (schema definitions vs. executable documents).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// Raw GraphQL files (.graphql, .gql, .gqls)
    GraphQL,
    /// TypeScript (.ts, .tsx)
    TypeScript,
    /// JavaScript (.js, .jsx, .mjs, .cjs)
    JavaScript,
    /// Vue Single File Components (.vue)
    Vue,
    /// Svelte components (.svelte)
    Svelte,
    /// Astro components (.astro)
    Astro,
}

impl Language {
    /// Detect language from a file path based on its extension.
    ///
    /// Returns `None` if the extension is not recognized as a GraphQL-related language.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;

        match extension {
            "graphql" | "gql" | "gqls" => Some(Self::GraphQL),
            "ts" | "tsx" => Some(Self::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "vue" => Some(Self::Vue),
            "svelte" => Some(Self::Svelte),
            "astro" => Some(Self::Astro),
            _ => None,
        }
    }

    /// Check if this language requires extraction (vs. direct GraphQL parsing).
    ///
    /// Returns `true` for languages where GraphQL is embedded in template literals
    /// (TypeScript, JavaScript, Vue, Svelte, Astro).
    /// Returns `false` for pure GraphQL files.
    #[must_use]
    pub const fn requires_extraction(&self) -> bool {
        !matches!(self, Self::GraphQL)
    }

    /// Check if this language is part of the JavaScript family.
    ///
    /// Returns `true` for TypeScript and JavaScript.
    #[must_use]
    pub const fn is_js_family(&self) -> bool {
        matches!(self, Self::TypeScript | Self::JavaScript)
    }
}

/// Document kind (determines semantic processing).
///
/// This enum represents the semantic purpose of a GraphQL document, which determines
/// WHAT to do with the content (merge into schema vs. validate as operations).
///
/// This is orthogonal to [`Language`], which determines how to parse the file.
///
/// The document kind is determined by the config (which glob pattern matched the file),
/// NOT by inspecting the content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentKind {
    /// Schema definitions (types, interfaces, enums, directives, etc.)
    ///
    /// Files with this kind contribute to the merged schema and are NOT
    /// validated as executable documents.
    Schema,

    /// Executable documents (operations, fragments)
    ///
    /// Files with this kind are validated against the schema. They should
    /// contain only operations and fragments, not type definitions.
    Executable,
}

impl DocumentKind {
    /// Returns `true` if this is a schema document.
    #[must_use]
    pub const fn is_schema(self) -> bool {
        matches!(self, Self::Schema)
    }

    /// Returns `true` if this is an executable document.
    #[must_use]
    pub const fn is_executable(self) -> bool {
        matches!(self, Self::Executable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_file_id() {
        let file_id = FileId::new(42);
        assert_eq!(file_id.as_u32(), 42);
    }

    #[test]
    fn test_file_uri() {
        let uri = FileUri::new("file:///path/to/file.graphql");
        assert_eq!(uri.as_str(), "file:///path/to/file.graphql");
        assert_eq!(uri.to_string(), "file:///path/to/file.graphql");
    }

    #[test]
    fn test_language_from_path() {
        assert_eq!(
            Language::from_path(&PathBuf::from("schema.graphql")),
            Some(Language::GraphQL)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("query.gql")),
            Some(Language::GraphQL)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("component.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("component.tsx")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("script.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("script.mjs")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("script.cjs")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("component.vue")),
            Some(Language::Vue)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("component.svelte")),
            Some(Language::Svelte)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("page.astro")),
            Some(Language::Astro)
        );
        assert_eq!(Language::from_path(&PathBuf::from("README.md")), None);
    }

    #[test]
    fn test_requires_extraction() {
        assert!(!Language::GraphQL.requires_extraction());
        assert!(Language::TypeScript.requires_extraction());
        assert!(Language::JavaScript.requires_extraction());
        assert!(Language::Vue.requires_extraction());
        assert!(Language::Svelte.requires_extraction());
        assert!(Language::Astro.requires_extraction());
    }

    #[test]
    fn test_is_js_family() {
        assert!(Language::TypeScript.is_js_family());
        assert!(Language::JavaScript.is_js_family());
        assert!(!Language::GraphQL.is_js_family());
        assert!(!Language::Vue.is_js_family());
        assert!(!Language::Svelte.is_js_family());
        assert!(!Language::Astro.is_js_family());
    }

    #[test]
    fn test_document_kind() {
        assert!(DocumentKind::Schema.is_schema());
        assert!(!DocumentKind::Schema.is_executable());
        assert!(!DocumentKind::Executable.is_schema());
        assert!(DocumentKind::Executable.is_executable());
    }
}
