use crate::{ExtractError, Language, Position, Range, Result, SourceLocation};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

/// Configuration for extracting GraphQL from TypeScript/JavaScript.
///
/// Schema mirrors `@graphql-tools/graphql-tag-pluck` so that a user migrating
/// from `@graphql-eslint` (or any pluck-based pipeline) can paste their pluck
/// config directly into `extensions.graphql-analyzer.extractConfig` (or its
/// `pluckConfig` alias) and have it work.
///
/// We deliberately omit pluck's legacy `apollo-*` (unscoped) modules from the
/// defaults — modern Apollo lives at `@apollo/client` and `@apollo/client/core`,
/// and the unscoped `apollo-server*` packages no longer re-export `gql`. Users
/// still on a legacy stack can list those modules explicitly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractConfig {
    /// Modules whose imports of GraphQL tags are recognized.
    /// JSON entries may be either a string (shorthand for `{ "name": <string> }`)
    /// or `{ "name": ..., "identifier"?: ... }`.
    #[serde(default = "default_modules")]
    pub modules: Vec<ModuleConfig>,

    /// Magic comment recognized for ``/* graphql */ `...` `` extraction.
    /// Default: `"graphql"` (matches pluck).
    #[serde(default = "default_gql_magic_comment")]
    pub gql_magic_comment: String,

    /// Names of identifiers recognized as GraphQL tags without an import.
    /// JSON accepts a string, an array of strings, or `false` (disable bare
    /// extraction entirely). Default: `["gql", "graphql"]`.
    #[serde(
        default = "default_global_gql_identifier_name",
        deserialize_with = "deserialize_global_gql_identifier_name"
    )]
    pub global_gql_identifier_name: Vec<String>,

    /// Optional Vue SFC block name (e.g., `"graphql"` for `<graphql>` blocks
    /// containing raw GraphQL source). Blocks with this name are extracted
    /// directly without going through tagged-template logic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gql_vue_block: Option<String>,

    /// If true, normalize indentation in extracted GraphQL by stripping the
    /// minimum common leading whitespace from each line.
    #[serde(default)]
    pub skip_indent: bool,
}

/// One entry in `modules`. JSON accepts either a bare string (shorthand for
/// `{ "name": <string> }`) or `{ "name": ..., "identifier"?: ... }`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModuleConfig {
    /// Module specifier (e.g., `"graphql-tag"`, `"@apollo/client"`).
    pub name: String,
    /// When set, only the export with this name is recognized as the GraphQL tag.
    /// When unset, any default import from this module is recognized; named
    /// imports fall through to `globalGqlIdentifierName` (matches pluck).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
}

impl<'de> Deserialize<'de> for ModuleConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ModuleConfigVisitor;

        impl<'de> Visitor<'de> for ModuleConfigVisitor {
            type Value = ModuleConfig;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a module name string or `{ name, identifier? }` object")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<ModuleConfig, E> {
                Ok(ModuleConfig {
                    name: value.to_string(),
                    identifier: None,
                })
            }

            fn visit_string<E: de::Error>(
                self,
                value: String,
            ) -> std::result::Result<ModuleConfig, E> {
                Ok(ModuleConfig {
                    name: value,
                    identifier: None,
                })
            }

            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<ModuleConfig, M::Error> {
                let mut name: Option<String> = None;
                let mut identifier: Option<String> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => name = Some(map.next_value()?),
                        "identifier" => identifier = Some(map.next_value()?),
                        other => {
                            return Err(de::Error::unknown_field(other, &["name", "identifier"]))
                        }
                    }
                }
                Ok(ModuleConfig {
                    name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                    identifier,
                })
            }
        }

        deserializer.deserialize_any(ModuleConfigVisitor)
    }
}

fn deserialize_global_gql_identifier_name<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct GlobalIdVisitor;

    impl<'de> Visitor<'de> for GlobalIdVisitor {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("`false`, a string, or an array of strings")
        }

        fn visit_bool<E: de::Error>(self, b: bool) -> std::result::Result<Vec<String>, E> {
            if b {
                Err(de::Error::custom(
                    "globalGqlIdentifierName: `true` is not valid; use a string, an array of strings, or `false` to disable",
                ))
            } else {
                Ok(Vec::new())
            }
        }

        fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<Vec<String>, E> {
            Ok(vec![s.to_string()])
        }

        fn visit_string<E: de::Error>(self, s: String) -> std::result::Result<Vec<String>, E> {
            Ok(vec![s])
        }

        fn visit_seq<S: SeqAccess<'de>>(
            self,
            mut seq: S,
        ) -> std::result::Result<Vec<String>, S::Error> {
            let mut v = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                v.push(item);
            }
            Ok(v)
        }
    }

    deserializer.deserialize_any(GlobalIdVisitor)
}

fn default_gql_magic_comment() -> String {
    "graphql".to_string()
}

fn default_global_gql_identifier_name() -> Vec<String> {
    vec!["gql".to_string(), "graphql".to_string()]
}

fn default_modules() -> Vec<ModuleConfig> {
    // Mirrors `@graphql-tools/graphql-tag-pluck`'s defaults minus the legacy
    // unscoped `apollo-*` packages (apollo-server*, apollo-boost, apollo-angular)
    // and `' apollo-server-lambda'` (a longstanding upstream typo with a leading
    // space). Modern Apollo lives at `@apollo/client(/core)`. Users on a legacy
    // Apollo stack can list those modules explicitly via `extractConfig.modules`.
    let with_id = |name: &str, id: &str| ModuleConfig {
        name: name.to_string(),
        identifier: Some(id.to_string()),
    };
    let no_id = |name: &str| ModuleConfig {
        name: name.to_string(),
        identifier: None,
    };
    vec![
        no_id("graphql-tag"),
        no_id("graphql-tag.macro"),
        with_id("@apollo/client", "gql"),
        with_id("@apollo/client/core", "gql"),
        with_id("gatsby", "graphql"),
        with_id("react-relay", "graphql"),
        with_id("react-relay/hooks", "graphql"),
        with_id("relay-runtime", "graphql"),
        with_id("babel-plugin-relay/macro", "graphql"),
        with_id("graphql.macro", "gql"),
        with_id("urql", "gql"),
        with_id("@urql/core", "gql"),
        with_id("@urql/preact", "gql"),
        with_id("@urql/svelte", "gql"),
        with_id("@urql/vue", "gql"),
    ]
}

impl Default for ExtractConfig {
    fn default() -> Self {
        Self {
            modules: default_modules(),
            gql_magic_comment: default_gql_magic_comment(),
            global_gql_identifier_name: default_global_gql_identifier_name(),
            gql_vue_block: None,
            skip_indent: false,
        }
    }
}

/// Resolve the effective `ExtractConfig` for files the user has explicitly
/// declared as GraphQL document sources (e.g., matched by a project's
/// `documents:` glob).
///
/// Pass the JSON value at `extensions.graphql-analyzer.extractConfig`
/// (or its `pluckConfig` alias). Pass `None` if the user provided neither.
///
/// Unset fields fall back to the pluck-aligned defaults (permissive — bare
/// `gql`/`graphql` tags are recognized without an import; matches pluck and
/// `@graphql-eslint` behavior — see issue #1035).
#[must_use]
pub fn resolve_for_documents(user_override: Option<&serde_json::Value>) -> ExtractConfig {
    let Some(value) = user_override else {
        return ExtractConfig::default();
    };
    match serde_json::from_value::<ExtractConfig>(value.clone()) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!("Failed to parse extractConfig: {e}; falling back to defaults");
            ExtractConfig::default()
        }
    }
}

/// Extracted GraphQL content with source location
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedGraphQL {
    /// The extracted GraphQL source code
    pub source: String,

    /// Source location in the original file
    pub location: SourceLocation,

    /// The tag name used (e.g., "gql", "graphql"), if any
    pub tag_name: Option<String>,

    /// File-level byte range of the enclosing TS/JS declaration when this
    /// block is the sole GraphQL content in that declaration.
    /// `None` for pure GraphQL files or when the declaration has multiple declarators.
    pub declaration_range: Option<(usize, usize)>,
}

/// Extract GraphQL from a file
#[tracing::instrument(skip(config), fields(path = %path.display()))]
pub fn extract_from_file(path: &Path, config: &ExtractConfig) -> Result<Vec<ExtractedGraphQL>> {
    let language = Language::from_path(path)
        .ok_or_else(|| ExtractError::UnsupportedFileType(path.to_path_buf()))?;
    tracing::trace!(language = ?language, "Detected language");

    let source = fs::read_to_string(path)?;
    let result = extract_from_source(&source, language, config, &path.display().to_string())?;
    tracing::debug!(blocks_extracted = result.len(), "Extraction complete");
    Ok(result)
}

/// Extract GraphQL from source code string
#[tracing::instrument(skip(source, config), fields(language = ?language, source_len = source.len()))]
pub fn extract_from_source(
    source: &str,
    language: Language,
    config: &ExtractConfig,
    path: &str,
) -> Result<Vec<ExtractedGraphQL>> {
    match language {
        Language::GraphQL => {
            // Raw GraphQL file - return entire content
            tracing::trace!("Returning full GraphQL file content");
            Ok(vec![ExtractedGraphQL {
                source: source.to_string(),
                location: SourceLocation::new(
                    0,
                    source.len(),
                    Range::new(
                        Position::new(0, 0),
                        position_from_offset(source, source.len()),
                    ),
                ),
                tag_name: None,
                declaration_range: None,
            }])
        }
        Language::TypeScript | Language::JavaScript => {
            extract_from_js_family(source, language, config, path)
        }
        Language::Vue | Language::Svelte => extract_from_sfc(source, config, path),
        Language::Astro => extract_from_astro(source, config, path),
    }
}

#[tracing::instrument(skip(source, config), fields(language = ?language, source_len = source.len()), level = "debug")]
fn extract_from_js_family(
    source: &str,
    language: Language,
    config: &ExtractConfig,
    path: &str,
) -> Result<Vec<ExtractedGraphQL>> {
    use swc_common::sync::Lrc;
    use swc_common::{FileName, SourceMap};
    use swc_core::ecma::ast::EsVersion;
    use swc_core::ecma::parser::{parse_file_as_module, Syntax};
    use swc_core::ecma::visit::VisitWith;

    let source_map = Lrc::new(SourceMap::default());
    let source_file = source_map.new_source_file(
        Lrc::new(FileName::Custom(path.to_string())),
        source.to_string(),
    );

    // Enable JSX/TSX only for extensions that support it (.tsx, .jsx).
    // Plain .ts files don't allow JSX, and enabling tsx mode causes SWC
    // to misparse generic arrow functions like `<T>(arg: T) => ...` as JSX.
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let syntax = match language {
        Language::TypeScript => Syntax::Typescript(swc_core::ecma::parser::TsSyntax {
            tsx: ext == "tsx",
            decorators: true,
            ..Default::default()
        }),
        Language::JavaScript => Syntax::Es(swc_core::ecma::parser::EsSyntax {
            jsx: ext == "jsx",
            ..Default::default()
        }),
        _ => unreachable!("extract_from_js_family only handles JS/TS"),
    };

    let module = parse_file_as_module(&source_file, syntax, EsVersion::EsNext, None, &mut vec![])
        .map_err(|e| ExtractError::Parse {
        path: std::path::PathBuf::from(path),
        message: format!("SWC parse error: {e:?}"),
    })?;

    let mut visitor = GraphQLVisitor::new(source, config);
    module.visit_with(&mut visitor);

    Ok(visitor.extracted)
}

/// A script block extracted from a Vue/Svelte SFC or Astro frontmatter.
struct ScriptBlock<'a> {
    content: &'a str,
    /// Byte offset of the script content within the original file
    offset: usize,
    /// Whether the script content is TypeScript
    is_typescript: bool,
}

/// Extract GraphQL from a Vue or Svelte single-file component.
///
/// Finds `<script>` blocks (including `<script setup>` in Vue), determines
/// the language from the `lang` attribute, extracts the inner content, and
/// passes it through the JS/TS extraction pipeline.
fn extract_from_sfc(
    source: &str,
    config: &ExtractConfig,
    path: &str,
) -> Result<Vec<ExtractedGraphQL>> {
    let blocks = find_script_blocks(source);
    if blocks.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for block in &blocks {
        let script_lang = if block.is_typescript {
            Language::TypeScript
        } else {
            Language::JavaScript
        };
        let extracted = extract_from_js_family(block.content, script_lang, config, path)?;
        for mut item in extracted {
            item.location.offset += block.offset;
            item.location.range = Range::new(
                position_from_offset(source, item.location.offset),
                position_from_offset(source, item.location.offset + item.location.length),
            );
            results.push(item);
        }
    }

    Ok(results)
}

/// Extract GraphQL from an Astro file's frontmatter (between `---` fences).
///
/// Astro frontmatter is always TypeScript.
fn extract_from_astro(
    source: &str,
    config: &ExtractConfig,
    path: &str,
) -> Result<Vec<ExtractedGraphQL>> {
    let Some(block) = find_astro_frontmatter(source) else {
        return Ok(Vec::new());
    };

    let extracted = extract_from_js_family(block.content, Language::TypeScript, config, path)?;
    let mut results = Vec::new();
    for mut item in extracted {
        item.location.offset += block.offset;
        item.location.range = Range::new(
            position_from_offset(source, item.location.offset),
            position_from_offset(source, item.location.offset + item.location.length),
        );
        results.push(item);
    }

    Ok(results)
}

/// Find the Astro frontmatter block (content between the first `---` pair).
fn find_astro_frontmatter(source: &str) -> Option<ScriptBlock<'_>> {
    // Astro frontmatter starts at the very beginning with `---`
    let trimmed = source.trim_start();
    let leading_whitespace = source.len() - trimmed.len();

    if !trimmed.starts_with("---") {
        return None;
    }

    let fence_start = leading_whitespace;
    let content_start = fence_start + 3;
    // Skip the newline after the opening fence
    let content_start = source[content_start..]
        .find('\n')
        .map_or(content_start, |i| content_start + i + 1);

    // Find the closing fence
    let rest = &source[content_start..];
    // Look for `---` on its own line
    for (i, line) in rest.lines().enumerate() {
        if line.trim() == "---" {
            let line_offset: usize = rest.lines().take(i).map(|l| l.len() + 1).sum();
            let content = &source[content_start..content_start + line_offset];
            return Some(ScriptBlock {
                content,
                offset: content_start,
                is_typescript: true,
            });
        }
    }

    None
}

/// Find `<script>` blocks in a Vue or Svelte SFC.
fn find_script_blocks(source: &str) -> Vec<ScriptBlock<'_>> {
    let mut blocks = Vec::new();
    let mut search_from = 0;

    while search_from < source.len() {
        let rest = &source[search_from..];

        // Find opening <script tag (case-insensitive)
        let open_tag_start = match rest
            .as_bytes()
            .windows(7)
            .position(|w| w.eq_ignore_ascii_case(b"<script"))
        {
            Some(pos) => search_from + pos,
            None => break,
        };

        // Find the end of the opening tag `>`
        let tag_content_start = open_tag_start + 7;
        let tag_rest = &source[tag_content_start..];
        let tag_end = match tag_rest.find('>') {
            Some(pos) => tag_content_start + pos,
            None => break,
        };

        // Parse attributes from the opening tag
        let attrs = &source[tag_content_start..tag_end];

        // In Vue, skip <script> blocks that don't have setup or lang attrs
        // that indicate it's a plain script (both <script> and <script setup> are valid)
        let is_typescript = detect_script_lang_typescript(attrs);

        let content_start = tag_end + 1;

        // Find closing </script> tag
        let close_rest = &source[content_start..];
        let close_pos = match close_rest
            .as_bytes()
            .windows(9)
            .position(|w| w.eq_ignore_ascii_case(b"</script>"))
        {
            Some(pos) => content_start + pos,
            None => break,
        };

        let content = &source[content_start..close_pos];
        blocks.push(ScriptBlock {
            content,
            offset: content_start,
            is_typescript,
        });

        search_from = close_pos + 9;
    }

    blocks
}

/// Detect if a script tag's attributes indicate TypeScript.
///
/// Returns true for `lang="ts"`, `lang="typescript"`, or when the attribute
/// is absent (defaulting to TS since most modern projects use TypeScript).
fn detect_script_lang_typescript(attrs: &str) -> bool {
    // Extract the lang attribute value
    if let Some(lang_pos) = attrs.find("lang") {
        let after_lang = &attrs[lang_pos + 4..];
        let after_eq = after_lang.trim_start().strip_prefix('=');
        if let Some(after_eq) = after_eq {
            let after_eq = after_eq.trim_start();
            let lang_value = if let Some(stripped) = after_eq.strip_prefix('"') {
                stripped.split('"').next()
            } else if let Some(stripped) = after_eq.strip_prefix('\'') {
                stripped.split('\'').next()
            } else {
                after_eq.split_whitespace().next()
            };
            return matches!(lang_value, Some("ts" | "typescript" | "tsx"));
        }
    }

    // No lang attribute: Vue and Svelte default to JS per framework specs
    false
}

/// Visitor to extract GraphQL from JavaScript/TypeScript AST
struct GraphQLVisitor<'a> {
    source: &'a str,
    config: &'a ExtractConfig,
    extracted: Vec<ExtractedGraphQL>,
    /// Local binding names imported from a recognized module that satisfy
    /// the module's identifier rule. Pluck-aligned: only entries here plus
    /// `globalGqlIdentifierName` are accepted as GraphQL tags.
    defined_identifiers: std::collections::HashSet<String>,
    /// Track comments for magic comment detection
    pending_comments: Vec<(usize, String)>,
    /// Declaration range set by `visit_var_decl`/`visit_export_decl` for single-declarator statements
    current_declaration_range: Option<(usize, usize)>,
}

impl<'a> GraphQLVisitor<'a> {
    fn new(source: &'a str, config: &'a ExtractConfig) -> Self {
        Self {
            source,
            config,
            extracted: Vec::new(),
            defined_identifiers: std::collections::HashSet::new(),
            pending_comments: Vec::new(),
            current_declaration_range: None,
        }
    }

    /// Check if a local binding is recognized as a GraphQL tag.
    ///
    /// Pluck rule: accept if the name is either (a) a tracked import binding
    /// from `modules` that satisfied that module's identifier rule, or (b)
    /// listed in `globalGqlIdentifierName`.
    fn is_valid_tag(&self, tag_name: &str) -> bool {
        self.defined_identifiers.contains(tag_name)
            || self
                .config
                .global_gql_identifier_name
                .iter()
                .any(|s| s == tag_name)
    }

    /// Extract string content from a template literal
    fn extract_template_literal(
        &self,
        tpl: &swc_core::ecma::ast::Tpl,
        tag_name: Option<String>,
    ) -> Option<ExtractedGraphQL> {
        if tpl.quasis.is_empty() {
            return None;
        }

        // For now, only support templates without expressions
        if tpl.exprs.is_empty() && tpl.quasis.len() == 1 {
            let quasi = &tpl.quasis[0];
            let raw_str = String::from_utf8_lossy(quasi.raw.as_bytes());

            // Calculate positions
            let start_offset = quasi.span.lo.0 as usize - 1; // -1 to account for SWC byte offset
            let length = raw_str.len();

            let start_pos = position_from_offset(self.source, start_offset);
            let end_pos = position_from_offset(self.source, start_offset + length);

            return Some(ExtractedGraphQL {
                source: raw_str.to_string(),
                location: SourceLocation::new(start_offset, length, Range::new(start_pos, end_pos)),
                tag_name,
                declaration_range: self.current_declaration_range,
            });
        }

        None
    }

    /// Check if there's a magic comment before this position
    fn check_magic_comment(&self, pos: usize) -> bool {
        // Look for a comment that precedes this position
        self.pending_comments.iter().any(|(comment_pos, content)| {
            *comment_pos < pos && content.trim() == self.config.gql_magic_comment
        })
    }
}

/// Extend a byte range to cover the full line(s), including leading whitespace,
/// trailing semicolon, and trailing newline.
fn extend_to_line_bounds(source: &str, start: usize, end: usize) -> (usize, usize) {
    // Extend backward to line start
    let line_start = source[..start].rfind('\n').map_or(0, |pos| pos + 1);

    // Extend forward past `;` and trailing `\n`
    let mut line_end = end;
    let rest = &source[end..];
    let trimmed = rest.trim_start_matches([' ', '\t']);
    if let Some(after_semi) = trimmed.strip_prefix(';') {
        line_end = source.len() - after_semi.len();
        if after_semi.starts_with('\n') {
            line_end += 1;
        } else if after_semi.starts_with("\r\n") {
            line_end += 2;
        }
    } else if rest.starts_with('\n') {
        line_end += 1;
    } else if rest.starts_with("\r\n") {
        line_end += 2;
    }

    (line_start, line_end)
}

impl swc_core::ecma::visit::Visit for GraphQLVisitor<'_> {
    /// Track single-declarator variable declarations so we can capture
    /// the full declaration range for single-definition GraphQL blocks.
    fn visit_var_decl(&mut self, decl: &swc_core::ecma::ast::VarDecl) {
        use swc_core::ecma::visit::VisitWith;

        if decl.decls.len() == 1 {
            let start = decl.span.lo.0 as usize - 1;
            let end = decl.span.hi.0 as usize - 1;
            self.current_declaration_range = Some(extend_to_line_bounds(self.source, start, end));
        }

        decl.visit_children_with(self);
        self.current_declaration_range = None;
    }

    /// Track exported variable declarations to capture the wider span
    /// including the `export` keyword.
    fn visit_export_decl(&mut self, export: &swc_core::ecma::ast::ExportDecl) {
        use swc_core::ecma::ast::Decl;
        use swc_core::ecma::visit::VisitWith;

        if let Decl::Var(var_decl) = &export.decl {
            if var_decl.decls.len() == 1 {
                let start = export.span.lo.0 as usize - 1;
                let end = export.span.hi.0 as usize - 1;
                self.current_declaration_range =
                    Some(extend_to_line_bounds(self.source, start, end));
            }
        }

        export.visit_children_with(self);
        self.current_declaration_range = None;
    }

    /// Visit import declarations to track which local bindings refer to a
    /// GraphQL tag from a configured module.
    ///
    /// Pluck-aligned matching:
    /// - Module has `identifier`: only the named import whose *imported* name
    ///   matches (post-aliasing) is tracked; the local binding is what we record.
    /// - Module has no `identifier`: only the default import is tracked
    ///   (the binding can have any local name); named imports from such modules
    ///   fall through to `globalGqlIdentifierName`.
    /// - Namespace imports (`import * as X from 'mod'`) are tracked
    ///   unconditionally so member calls like ``X.gql`...` `` keep working —
    ///   pluck doesn't track these, but our existing tests rely on this and
    ///   dropping it would be a silent regression.
    fn visit_import_decl(&mut self, import: &swc_core::ecma::ast::ImportDecl) {
        use swc_core::ecma::ast::{ImportSpecifier, ModuleExportName};
        use swc_core::ecma::visit::VisitWith;
        let module_source = String::from_utf8_lossy(import.src.value.as_bytes()).to_string();

        let Some(module_config) = self.config.modules.iter().find(|m| m.name == module_source)
        else {
            import.visit_children_with(self);
            return;
        };

        for specifier in &import.specifiers {
            match specifier {
                ImportSpecifier::Named(named) => {
                    if let Some(expected) = module_config.identifier.as_deref() {
                        let imported_name = match &named.imported {
                            Some(ModuleExportName::Ident(i)) => {
                                String::from_utf8_lossy(i.sym.as_bytes()).to_string()
                            }
                            Some(ModuleExportName::Str(s)) => {
                                String::from_utf8_lossy(s.value.as_bytes()).to_string()
                            }
                            None => String::from_utf8_lossy(named.local.sym.as_bytes()).to_string(),
                        };
                        if imported_name == expected {
                            let local_name =
                                String::from_utf8_lossy(named.local.sym.as_bytes()).to_string();
                            self.defined_identifiers.insert(local_name);
                        }
                    }
                }
                ImportSpecifier::Default(default) => {
                    if module_config.identifier.is_none() {
                        let local_name =
                            String::from_utf8_lossy(default.local.sym.as_bytes()).to_string();
                        self.defined_identifiers.insert(local_name);
                    }
                }
                ImportSpecifier::Namespace(ns) => {
                    let local_name = String::from_utf8_lossy(ns.local.sym.as_bytes()).to_string();
                    self.defined_identifiers.insert(local_name);
                }
            }
        }

        import.visit_children_with(self);
    }

    /// Visit tagged template expressions (e.g., gql`query { ... }`)
    fn visit_tagged_tpl(&mut self, tagged: &swc_core::ecma::ast::TaggedTpl) {
        use swc_core::ecma::ast::Expr;
        use swc_core::ecma::visit::VisitWith;
        let tag_name = match &*tagged.tag {
            Expr::Ident(ident) => String::from_utf8_lossy(ident.sym.as_bytes()).to_string(),
            Expr::Member(member) => {
                // Handle member expressions like `graphql.default`
                if let Expr::Ident(obj) = &*member.obj {
                    String::from_utf8_lossy(obj.sym.as_bytes()).to_string()
                } else {
                    tagged.visit_children_with(self);
                    return;
                }
            }
            _ => {
                tagged.visit_children_with(self);
                return;
            }
        };

        if !self.is_valid_tag(&tag_name) {
            tagged.visit_children_with(self);
            return;
        }

        if let Some(extracted) = self.extract_template_literal(&tagged.tpl, Some(tag_name)) {
            self.extracted.push(extracted);
        }

        // Continue traversal into child nodes
        tagged.visit_children_with(self);
    }

    /// Visit call expressions to handle cases like:
    /// - gql(/* GraphQL */ "query")
    /// - graphql(`query { ... }`, [fragment1, fragment2])
    fn visit_call_expr(&mut self, call: &swc_core::ecma::ast::CallExpr) {
        use swc_core::ecma::ast::{Callee, Expr, Lit};
        use swc_core::ecma::visit::VisitWith;

        let tag_name = match &call.callee {
            Callee::Expr(expr) => match &**expr {
                Expr::Ident(ident) => {
                    let name = String::from_utf8_lossy(ident.sym.as_bytes()).to_string();
                    self.is_valid_tag(&name).then_some(name)
                }
                Expr::Member(member) => {
                    // Handle member expressions like `graphql.default`
                    if let Expr::Ident(obj) = &*member.obj {
                        let name = String::from_utf8_lossy(obj.sym.as_bytes()).to_string();
                        self.is_valid_tag(&name).then_some(name)
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        };

        // If this is a valid GraphQL tag function call, check the first argument
        if let Some(tag) = tag_name {
            if let Some(first_arg) = call.args.first() {
                match &*first_arg.expr {
                    // Handle template literal: graphql(`query { ... }`)
                    Expr::Tpl(tpl) => {
                        if let Some(extracted) = self.extract_template_literal(tpl, Some(tag)) {
                            self.extracted.push(extracted);
                        }
                    }
                    // Handle string literal with magic comment: gql(/* GraphQL */ "query")
                    Expr::Lit(Lit::Str(str_lit)) => {
                        let pos = str_lit.span.lo.0 as usize;
                        if self.check_magic_comment(pos) {
                            let start_offset = str_lit.span.lo.0 as usize - 1;
                            let content =
                                String::from_utf8_lossy(str_lit.value.as_bytes()).to_string();
                            let length = content.len();

                            let start_pos = position_from_offset(self.source, start_offset);
                            let end_pos = position_from_offset(self.source, start_offset + length);

                            self.extracted.push(ExtractedGraphQL {
                                source: content,
                                location: SourceLocation::new(
                                    start_offset,
                                    length,
                                    Range::new(start_pos, end_pos),
                                ),
                                tag_name: None,
                                declaration_range: self.current_declaration_range,
                            });
                        }
                    }
                    _ => {}
                }
            }
        } else {
            // Not a GraphQL tag function, check for magic comments in string arguments
            for arg in &call.args {
                if let Expr::Lit(Lit::Str(str_lit)) = &*arg.expr {
                    let pos = str_lit.span.lo.0 as usize;
                    if self.check_magic_comment(pos) {
                        let start_offset = str_lit.span.lo.0 as usize - 1;
                        let content = String::from_utf8_lossy(str_lit.value.as_bytes()).to_string();
                        let length = content.len();

                        let start_pos = position_from_offset(self.source, start_offset);
                        let end_pos = position_from_offset(self.source, start_offset + length);

                        self.extracted.push(ExtractedGraphQL {
                            source: content,
                            location: SourceLocation::new(
                                start_offset,
                                length,
                                Range::new(start_pos, end_pos),
                            ),
                            tag_name: None,
                            declaration_range: self.current_declaration_range,
                        });
                    }
                }
            }
        }

        // Continue traversal into child nodes
        call.visit_children_with(self);
    }

    /// Visit variable declarations to handle magic comments
    fn visit_var_declarator(&mut self, decl: &swc_core::ecma::ast::VarDeclarator) {
        use swc_core::ecma::ast::{Expr, Lit};
        use swc_core::ecma::visit::VisitWith;
        if let Some(init) = &decl.init {
            match &**init {
                Expr::Lit(Lit::Str(str_lit)) => {
                    let pos = str_lit.span.lo.0 as usize;
                    if self.check_magic_comment(pos) {
                        let start_offset = str_lit.span.lo.0 as usize - 1;
                        let content = String::from_utf8_lossy(str_lit.value.as_bytes()).to_string();
                        let length = content.len();

                        let start_pos = position_from_offset(self.source, start_offset);
                        let end_pos = position_from_offset(self.source, start_offset + length);

                        self.extracted.push(ExtractedGraphQL {
                            source: content,
                            location: SourceLocation::new(
                                start_offset,
                                length,
                                Range::new(start_pos, end_pos),
                            ),
                            tag_name: None,
                            declaration_range: self.current_declaration_range,
                        });
                    }
                }
                Expr::Tpl(tpl) => {
                    let pos = tpl.span.lo.0 as usize;
                    if self.check_magic_comment(pos) {
                        if let Some(extracted) = self.extract_template_literal(tpl, None) {
                            self.extracted.push(extracted);
                        }
                    }
                }
                _ => {}
            }
        }

        // Continue traversal into child nodes
        decl.visit_children_with(self);
    }
}

/// Calculate position from byte offset
fn position_from_offset(source: &str, offset: usize) -> Position {
    let mut line: u32 = 0;
    let mut column: u32 = 0;

    for (idx, ch) in source.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += ch.len_utf16() as u32;
        }
    }

    Position::new(line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ExtractConfig::default();
        assert_eq!(config.gql_magic_comment, "graphql");
        assert_eq!(
            config.global_gql_identifier_name,
            vec!["gql".to_string(), "graphql".to_string()]
        );
        assert!(
            config.modules.iter().any(|m| m.name == "graphql-tag"),
            "default modules should include graphql-tag"
        );
        assert!(
            config
                .modules
                .iter()
                .any(|m| m.name == "@apollo/client" && m.identifier.as_deref() == Some("gql")),
            "default modules should include @apollo/client with identifier gql"
        );
        assert!(
            !config.modules.iter().any(|m| m.name.starts_with("apollo-")),
            "default modules should not include any unscoped apollo-* legacy packages"
        );
        assert!(!config.skip_indent);
        assert!(config.gql_vue_block.is_none());
    }

    #[test]
    fn test_resolve_for_documents_uses_default_when_user_override_is_none() {
        let cfg = resolve_for_documents(None);
        assert_eq!(
            cfg.global_gql_identifier_name,
            vec!["gql".to_string(), "graphql".to_string()]
        );
        assert_eq!(cfg.gql_magic_comment, "graphql");
    }

    #[test]
    fn test_resolve_for_documents_honors_explicit_strict_mode() {
        // `globalGqlIdentifierName: false` disables bare/global tag extraction.
        let user = serde_json::json!({ "globalGqlIdentifierName": false });
        let cfg = resolve_for_documents(Some(&user));
        assert!(cfg.global_gql_identifier_name.is_empty());
    }

    #[test]
    fn test_resolve_for_documents_merges_partial_user_config() {
        // User overrides only modules; other fields keep defaults.
        let user = serde_json::json!({ "modules": ["my-tag-lib"] });
        let cfg = resolve_for_documents(Some(&user));
        assert_eq!(cfg.modules.len(), 1);
        assert_eq!(cfg.modules[0].name, "my-tag-lib");
        assert!(cfg.modules[0].identifier.is_none());
        assert_eq!(cfg.gql_magic_comment, "graphql");
        assert_eq!(
            cfg.global_gql_identifier_name,
            vec!["gql".to_string(), "graphql".to_string()]
        );
    }

    #[test]
    fn test_resolve_for_documents_accepts_module_object_form() {
        let user = serde_json::json!({
            "modules": [
                "graphql-tag",
                { "name": "my-tag-lib", "identifier": "tag" },
            ],
            "gqlMagicComment": "GQL",
        });
        let cfg = resolve_for_documents(Some(&user));
        assert_eq!(cfg.modules.len(), 2);
        assert_eq!(cfg.modules[0].name, "graphql-tag");
        assert!(cfg.modules[0].identifier.is_none());
        assert_eq!(cfg.modules[1].name, "my-tag-lib");
        assert_eq!(cfg.modules[1].identifier.as_deref(), Some("tag"));
        assert_eq!(cfg.gql_magic_comment, "GQL");
    }

    #[test]
    fn test_resolve_for_documents_accepts_global_identifier_string_form() {
        let user = serde_json::json!({ "globalGqlIdentifierName": "myTag" });
        let cfg = resolve_for_documents(Some(&user));
        assert_eq!(cfg.global_gql_identifier_name, vec!["myTag".to_string()]);
    }

    #[test]
    fn test_resolve_for_documents_falls_back_when_value_is_not_an_object() {
        let user = serde_json::json!("not-an-object");
        let cfg = resolve_for_documents(Some(&user));
        // Falls back to defaults on parse error rather than erroring.
        assert_eq!(
            cfg.global_gql_identifier_name,
            vec!["gql".to_string(), "graphql".to_string()]
        );
    }

    #[test]
    fn test_module_config_rejects_global_true() {
        let user = serde_json::json!({ "globalGqlIdentifierName": true });
        let result = serde_json::from_value::<ExtractConfig>(user);
        assert!(
            result.is_err(),
            "globalGqlIdentifierName: true should be rejected (only false, string, or array are valid)"
        );
    }

    #[test]
    fn test_extract_config_rejects_unknown_field() {
        // Catch typos like `magicComment` (old name) vs `gqlMagicComment` (new).
        let user = serde_json::json!({ "magicComment": "GraphQL" });
        let result = serde_json::from_value::<ExtractConfig>(user);
        assert!(
            result.is_err(),
            "unknown field `magicComment` should be rejected to surface schema migration issues"
        );
    }

    #[test]
    fn test_extract_raw_graphql() {
        let source = r"
query GetUser {
  user {
    id
    name
  }
}
";
        let config = ExtractConfig::default();
        let result = extract_from_source(source, Language::GraphQL, &config, "test").unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, source);
        assert_eq!(result[0].tag_name, None);
    }

    #[test]
    fn test_position_from_offset() {
        let source = "line 0\nline 1\nline 2";
        let pos = position_from_offset(source, 0);
        assert_eq!(pos, Position::new(0, 0));

        let pos = position_from_offset(source, 7); // Start of "line 1"
        assert_eq!(pos, Position::new(1, 0));

        let pos = position_from_offset(source, 14); // Start of "line 2"
        assert_eq!(pos, Position::new(2, 0));
    }

    mod typescript_tests {
        use super::*;

        #[test]
        fn test_extract_tagged_template_with_import() {
            let source = r"
import { gql } from 'graphql-tag';

const query = gql`
  query GetUser {
    user {
      id
      name
    }
  }
`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
            assert_eq!(result[0].tag_name, Some("gql".to_string()));
        }

        #[test]
        fn test_extract_tagged_template_without_import_strict_mode() {
            // Setting `globalGqlIdentifierName` to an empty list disables
            // bare/global tag extraction (pluck's `globalGqlIdentifierName: false`).
            let source = r"
const query = gql`
  query GetUser {
    user {
      id
    }
  }
`;
";
            let config = ExtractConfig {
                global_gql_identifier_name: Vec::new(),
                ..Default::default()
            };
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 0);
        }

        #[test]
        fn test_extract_tagged_template_without_import_default_extracts() {
            // Pluck-aligned default: bare `gql`/`graphql` tags extract without
            // requiring an import (issue #1035).
            let source = r"
const query = gql`
  query GetUser {
    user {
      id
    }
  }
`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
            assert_eq!(result[0].tag_name, Some("gql".to_string()));
        }

        #[test]
        fn test_extract_from_apollo_client() {
            let source = r"
import { gql } from '@apollo/client';

const QUERY = gql`
  query GetPosts {
    posts {
      id
      title
    }
  }
`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetPosts"));
            assert_eq!(result[0].tag_name, Some("gql".to_string()));
        }

        #[test]
        fn test_extract_multiple_queries() {
            let source = r"
import { gql } from 'graphql-tag';

const query1 = gql`query Q1 { field1 }`;
const query2 = gql`query Q2 { field2 }`;
const query3 = gql`mutation M1 { updateField }`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 3);
            assert!(result[0].source.contains("query Q1"));
            assert!(result[1].source.contains("query Q2"));
            assert!(result[2].source.contains("mutation M1"));
        }

        #[test]
        fn test_extract_graphql_tag_identifier() {
            let source = r"
import { graphql } from 'graphql-tag';

const query = graphql`
  query GetData {
    data {
      id
    }
  }
`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetData"));
            assert_eq!(result[0].tag_name, Some("graphql".to_string()));
        }

        #[test]
        fn test_extract_from_javascript() {
            let source = r"
import { gql } from 'graphql-tag';

const query = gql`
  query GetUser {
    user {
      id
    }
  }
`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::JavaScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
        }

        #[test]
        fn test_extract_with_jsx() {
            let source = r"
import { gql } from '@apollo/client';
import { useQuery } from '@apollo/client';

const GET_USER = gql`
  query GetUser($id: ID!) {
    user(id: $id) {
      id
      name
    }
  }
`;

function UserComponent({ userId }) {
  const { data } = useQuery(GET_USER, { variables: { id: userId } });
  return <div>{data?.user?.name}</div>;
}
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test.tsx").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
        }

        #[test]
        fn test_extract_with_custom_global_identifier() {
            // Adding a custom name to `globalGqlIdentifierName` makes it
            // recognized as a bare GraphQL tag.
            let source = r"
const query = customGql`query Custom { field }`;
";
            let mut config = ExtractConfig::default();
            config
                .global_gql_identifier_name
                .push("customGql".to_string());
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query Custom"));
            assert_eq!(result[0].tag_name, Some("customGql".to_string()));
        }

        #[test]
        fn test_extract_with_custom_module_and_identifier() {
            // Per-module `identifier` lets users scope which export from a
            // module is recognized as the GraphQL tag.
            let source = r"
import { tag } from 'my-tag-lib';

const query = tag`query Custom { field }`;
";
            let mut config = ExtractConfig {
                global_gql_identifier_name: Vec::new(),
                ..Default::default()
            };
            config.modules.push(ModuleConfig {
                name: "my-tag-lib".to_string(),
                identifier: Some("tag".to_string()),
            });
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query Custom"));
            assert_eq!(result[0].tag_name, Some("tag".to_string()));
        }

        #[test]
        fn test_import_from_unknown_module_strict_mode() {
            // With global fallback disabled, an import from an unrecognized
            // module gives no path to extraction.
            let source = r"
import { gql } from 'unknown-module';

const query = gql`query Test { field }`;
";
            let config = ExtractConfig {
                global_gql_identifier_name: Vec::new(),
                ..Default::default()
            };
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 0);
        }

        #[test]
        fn test_default_import() {
            // graphql-tag has no `identifier` constraint in defaults, so any
            // default-imported binding from it is treated as a GraphQL tag
            // (matches pluck).
            let source = r"
import gql from 'graphql-tag';

const query = gql`query Test { field }`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query Test"));
        }

        #[test]
        fn test_renamed_named_import_from_module_with_identifier() {
            // `@apollo/client` has `identifier: 'gql'`. Pluck-aligned: the
            // *imported* name must match `gql`, while the *local* binding
            // can be anything.
            let source = r"
import { gql as query } from '@apollo/client';

const q = query`query Test { field }`;
";
            // Disable global fallback so we exercise the import-tracking path.
            let config = ExtractConfig {
                global_gql_identifier_name: Vec::new(),
                ..Default::default()
            };
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query Test"));
            assert_eq!(result[0].tag_name, Some("query".to_string()));
        }

        #[test]
        fn test_named_import_from_no_identifier_module_falls_through_to_global() {
            // Pluck rule: named imports from a module without `identifier`
            // are NOT tracked — they fall through to `globalGqlIdentifierName`.
            // `gql` is in the global list by default, so this still works:
            let source = r"
import { gql } from 'graphql-tag';

const q = gql`query Test { field }`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();
            assert_eq!(result.len(), 1);

            // But aliased to a name not in globalGqlIdentifierName, it should not match.
            let source_aliased = r"
import { gql as customGql } from 'graphql-tag';

const q = customGql`query Test { field }`;
";
            let strict_config = ExtractConfig {
                global_gql_identifier_name: Vec::new(),
                ..Default::default()
            };
            let result =
                extract_from_source(source_aliased, Language::TypeScript, &strict_config, "test")
                    .unwrap();
            assert_eq!(
                result.len(),
                0,
                "aliased named import from no-identifier module should not match without global fallback"
            );
        }

        #[test]
        fn test_typescript_decorators() {
            let source = r"
import { gql } from 'graphql-tag';

@Component
class UserQuery {
  query = gql`query GetUser { user { id } }`;
}
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
        }

        #[test]
        fn test_parse_error_handling() {
            let source = "import { gql } from 'graphql-tag'; const x = %%%invalid%%%";

            let config = ExtractConfig::default();
            let result = extract_from_source(source, Language::TypeScript, &config, "test");

            assert!(result.is_err());
            if let Err(ExtractError::Parse { message, .. }) = result {
                assert!(message.contains("SWC parse error"));
            } else {
                panic!("Expected parse error");
            }
        }

        #[test]
        fn test_multiline_query_formatting() {
            let source = r"
import { gql } from 'graphql-tag';

const query = gql`
  query GetUserWithPosts {
    user {
      id
      name
      posts {
        id
        title
        content
      }
    }
  }
`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUserWithPosts"));
            assert!(result[0].source.contains("posts"));
            assert!(result[0].source.contains("title"));
            assert!(result[0].source.contains("content"));
        }

        #[test]
        fn test_fragment_extraction() {
            let source = r"
import { gql } from '@apollo/client';

const USER_FRAGMENT = gql`
  fragment UserFields on User {
    id
    name
    email
  }
`;

const GET_USER = gql`
  query GetUser($id: ID!) {
    user(id: $id) {
      ...UserFields
    }
  }
  ${USER_FRAGMENT}
`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            // Should extract both fragments (the second has an expression, but we can count them)
            assert!(!result.is_empty());
            assert!(result[0].source.contains("fragment UserFields"));
        }

        #[test]
        fn test_location_tracking() {
            let source = r"import { gql } from 'graphql-tag';
const q = gql`query Test { field }`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            let location = &result[0].location;

            // Verify we have location information
            assert!(location.offset > 0);
            assert!(location.length > 0);
            // Line numbers are usize, so they're always >= 0
            assert!(location.range.end.line >= location.range.start.line);
        }

        #[test]
        fn test_all_javascript_extensions() {
            let test_cases = vec![
                (Language::JavaScript, "script.js"),
                (Language::JavaScript, "script.jsx"),
                (Language::JavaScript, "script.mjs"),
                (Language::JavaScript, "script.cjs"),
            ];

            let source = r"
import { gql } from 'graphql-tag';
const query = gql`query Test { field }`;
";

            for (lang, _filename) in test_cases {
                let config = ExtractConfig::default();
                let result = extract_from_source(source, lang, &config, "test").unwrap();
                assert_eq!(result.len(), 1, "Failed for {lang:?}");
            }
        }

        #[test]
        fn test_extract_call_expression_with_second_argument() {
            let source = r"
import { graphql } from 'graphql-tag';

const fragment1 = graphql`fragment F1 on User { id }`;
const fragment2 = graphql`fragment F2 on User { name }`;

const document = graphql(`
  query GetUser {
    user {
      ...F1
      ...F2
    }
  }
`, [fragment1, fragment2]);
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            // Should extract all three: both fragments and the query
            assert_eq!(result.len(), 3);
            assert!(result[0].source.contains("fragment F1"));
            assert!(result[1].source.contains("fragment F2"));
            assert!(result[2].source.contains("query GetUser"));
            assert_eq!(result[2].tag_name, Some("graphql".to_string()));
        }

        #[test]
        fn test_extract_call_expression_without_second_argument() {
            let source = r"
import { graphql } from 'graphql-tag';

const document = graphql(`
  query GetUser {
    user {
      id
    }
  }
`);
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            // Should extract the query
            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
            assert_eq!(result[0].tag_name, Some("graphql".to_string()));
        }

        #[test]
        fn test_extract_call_expression_gql_variant() {
            let source = r"
import { gql } from 'graphql-tag';

const document = gql(`
  query GetPosts {
    posts {
      id
      title
    }
  }
`, []);
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            // Should extract the query
            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetPosts"));
            assert_eq!(result[0].tag_name, Some("gql".to_string()));
        }

        #[test]
        fn test_generic_arrow_function_in_ts_file() {
            // Issue #755: SWC parse error on .ts files with generic arrow functions.
            // `<T>` is parsed as JSX when tsx mode is enabled, but .ts files
            // don't support JSX — only .tsx files do.
            let source = r"
import { gql } from 'graphql-tag';

const genericArrowFunction = <T>(arg: T): T => {
  return arg;
};

const query = gql`query GetUser { user { id } }`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test.ts").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
        }
    }

    mod vue_tests {
        use super::*;

        #[test]
        fn test_extract_from_vue_sfc_with_ts() {
            let source = r#"<template>
  <div>{{ data?.user?.name }}</div>
</template>

<script lang="ts">
import { gql } from 'graphql-tag';

const GET_USER = gql`
  query GetUser($id: ID!) {
    user(id: $id) {
      id
      name
    }
  }
`;

export default {
  data() { return {}; }
};
</script>
"#;
            let config = ExtractConfig::default();
            let result = extract_from_source(source, Language::Vue, &config, "test.vue").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
            assert_eq!(result[0].tag_name, Some("gql".to_string()));
            // Offset should point into the original file, past the <template> and <script> tag
            assert!(result[0].location.offset > 50);
        }

        #[test]
        fn test_extract_from_vue_sfc_with_js() {
            let source = r"<template>
  <div>Hello</div>
</template>

<script>
import { gql } from '@apollo/client';

const QUERY = gql`query GetPosts { posts { id } }`;
</script>
";
            let config = ExtractConfig::default();
            let result = extract_from_source(source, Language::Vue, &config, "test.vue").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetPosts"));
        }

        #[test]
        fn test_extract_from_vue_script_setup() {
            let source = r#"<script setup lang="ts">
import { gql } from 'graphql-tag';

const GET_DATA = gql`
  query GetData {
    items { id name }
  }
`;
</script>

<template>
  <div>Hello</div>
</template>
"#;
            let config = ExtractConfig::default();
            let result = extract_from_source(source, Language::Vue, &config, "test.vue").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetData"));
        }

        #[test]
        fn test_extract_from_vue_both_script_blocks() {
            let source = r#"<script lang="ts">
import { gql } from 'graphql-tag';
const FRAGMENT = gql`fragment UserFields on User { id name }`;
</script>

<script setup lang="ts">
import { gql } from 'graphql-tag';
const QUERY = gql`query GetUser { user { ...UserFields } }`;
</script>

<template><div /></template>
"#;
            let config = ExtractConfig::default();
            let result = extract_from_source(source, Language::Vue, &config, "test.vue").unwrap();

            assert_eq!(result.len(), 2);
            assert!(result[0].source.contains("fragment UserFields"));
            assert!(result[1].source.contains("query GetUser"));
        }

        #[test]
        fn test_vue_no_script_block() {
            let source = r"<template>
  <div>Template only component</div>
</template>

<style scoped>
div { color: red; }
</style>
";
            let config = ExtractConfig::default();
            let result = extract_from_source(source, Language::Vue, &config, "test.vue").unwrap();

            assert!(result.is_empty());
        }

        #[test]
        fn test_vue_offset_maps_to_original_file() {
            let source = r#"<template><div /></template>

<script lang="ts">
import { gql } from 'graphql-tag';
const Q = gql`query Test { field }`;
</script>
"#;
            let config = ExtractConfig::default();
            let result = extract_from_source(source, Language::Vue, &config, "test.vue").unwrap();

            assert_eq!(result.len(), 1);
            let loc = &result[0].location;
            // The offset should point to the GraphQL content in the original file
            let extracted_from_original = &source[loc.offset..loc.offset + loc.length];
            assert_eq!(extracted_from_original, result[0].source);
        }
    }

    mod svelte_tests {
        use super::*;

        #[test]
        fn test_extract_from_svelte_with_ts() {
            let source = r#"<script lang="ts">
  import { gql } from 'graphql-tag';

  const GET_USER = gql`
    query GetUser {
      user { id name }
    }
  `;
</script>

<main>
  <h1>Hello</h1>
</main>
"#;
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Svelte, &config, "test.svelte").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetUser"));
        }

        #[test]
        fn test_extract_from_svelte_with_js() {
            let source = r"<script>
  import { gql } from '@apollo/client';

  const QUERY = gql`query GetPosts { posts { id } }`;
</script>

<p>Content</p>
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Svelte, &config, "test.svelte").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetPosts"));
        }

        #[test]
        fn test_svelte_context_module_script() {
            // Svelte allows <script context="module"> for module-level code
            let source = r#"<script context="module" lang="ts">
  import { gql } from 'graphql-tag';
  export const SHARED = gql`query Shared { config { version } }`;
</script>

<script lang="ts">
  import { gql } from 'graphql-tag';
  const LOCAL = gql`query Local { user { id } }`;
</script>

<p>Content</p>
"#;
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Svelte, &config, "test.svelte").unwrap();

            assert_eq!(result.len(), 2);
            assert!(result[0].source.contains("query Shared"));
            assert!(result[1].source.contains("query Local"));
        }

        #[test]
        fn test_svelte_no_script_block() {
            let source = r"<p>Just markup</p>";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Svelte, &config, "test.svelte").unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn test_svelte_offset_maps_to_original_file() {
            let source = r#"<script lang="ts">
import { gql } from 'graphql-tag';
const Q = gql`query Test { field }`;
</script>

<p>Content</p>
"#;
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Svelte, &config, "test.svelte").unwrap();

            assert_eq!(result.len(), 1);
            let loc = &result[0].location;
            let extracted_from_original = &source[loc.offset..loc.offset + loc.length];
            assert_eq!(extracted_from_original, result[0].source);
        }
    }

    mod astro_tests {
        use super::*;

        #[test]
        fn test_extract_from_astro_frontmatter() {
            let source = r"---
import { gql } from 'graphql-tag';

const GET_DATA = gql`
  query GetData {
    posts { id title }
  }
`;
---

<html>
  <body>
    <h1>Hello</h1>
  </body>
</html>
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Astro, &config, "test.astro").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query GetData"));
        }

        #[test]
        fn test_astro_no_frontmatter() {
            let source = r"<html><body><h1>Hello</h1></body></html>";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Astro, &config, "test.astro").unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn test_astro_multiple_queries() {
            let source = r"---
import { gql } from '@apollo/client';

const QUERY1 = gql`query Q1 { users { id } }`;
const QUERY2 = gql`query Q2 { posts { id } }`;
---

<html><body /></html>
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Astro, &config, "test.astro").unwrap();

            assert_eq!(result.len(), 2);
            assert!(result[0].source.contains("query Q1"));
            assert!(result[1].source.contains("query Q2"));
        }

        #[test]
        fn test_astro_offset_maps_to_original_file() {
            let source = r"---
import { gql } from 'graphql-tag';
const Q = gql`query Test { field }`;
---

<html><body /></html>
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Astro, &config, "test.astro").unwrap();

            assert_eq!(result.len(), 1);
            let loc = &result[0].location;
            let extracted_from_original = &source[loc.offset..loc.offset + loc.length];
            assert_eq!(extracted_from_original, result[0].source);
        }

        #[test]
        fn test_astro_empty_frontmatter() {
            let source = r"---
---

<html><body /></html>
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::Astro, &config, "test.astro").unwrap();
            assert!(result.is_empty());
        }
    }

    mod script_block_tests {
        use super::*;

        #[test]
        fn test_find_script_blocks_basic() {
            let source = r#"<script lang="ts">
const x = 1;
</script>"#;
            let blocks = find_script_blocks(source);
            assert_eq!(blocks.len(), 1);
            assert!(blocks[0].is_typescript);
            assert!(blocks[0].content.contains("const x = 1;"));
        }

        #[test]
        fn test_find_script_blocks_no_lang() {
            let source = r"<script>
const x = 1;
</script>";
            let blocks = find_script_blocks(source);
            assert_eq!(blocks.len(), 1);
            assert!(!blocks[0].is_typescript);
        }

        #[test]
        fn test_detect_lang_variants() {
            assert!(detect_script_lang_typescript(r#" lang="ts""#));
            assert!(detect_script_lang_typescript(r#" lang="typescript""#));
            assert!(detect_script_lang_typescript(r#" lang="tsx""#));
            assert!(detect_script_lang_typescript(r" lang='ts'"));
            assert!(!detect_script_lang_typescript(r#" lang="js""#));
            assert!(!detect_script_lang_typescript(""));
        }

        #[test]
        fn test_find_astro_frontmatter() {
            let source = "---\nconst x = 1;\n---\n<html />";
            let block = find_astro_frontmatter(source).unwrap();
            assert!(block.is_typescript);
            assert!(block.content.contains("const x = 1;"));
            assert_eq!(block.offset, 4); // after "---\n"
        }

        #[test]
        fn test_no_astro_frontmatter() {
            let source = "<html><body /></html>";
            assert!(find_astro_frontmatter(source).is_none());
        }
    }
}
