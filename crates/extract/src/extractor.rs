use crate::{ExtractError, Language, Position, Range, Result, SourceLocation};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configuration for GraphQL extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractConfig {
    /// Magic comment to look for (default: "GraphQL")
    /// Matches comments like: /* GraphQL */ `query { ... }`
    #[serde(default = "default_magic_comment")]
    pub magic_comment: String,

    /// Tag identifiers to extract (default: `["gql", "graphql"]`)
    /// Matches: `gql`query { ... }`\` or `graphql`query { ... }`\`
    #[serde(default = "default_tag_identifiers")]
    pub tag_identifiers: Vec<String>,

    /// Module names to recognize as GraphQL sources
    /// Default includes: graphql-tag, @apollo/client, etc.
    #[serde(default = "default_modules")]
    pub modules: Vec<String>,

    /// Allow extraction without imports (global identifiers)
    #[serde(default)]
    pub allow_global_identifiers: bool,
}

fn default_magic_comment() -> String {
    "GraphQL".to_string()
}

fn default_tag_identifiers() -> Vec<String> {
    vec!["gql".to_string(), "graphql".to_string()]
}

fn default_modules() -> Vec<String> {
    vec![
        "graphql-tag".to_string(),
        "@apollo/client".to_string(),
        "apollo-server".to_string(),
        "apollo-server-express".to_string(),
        "gatsby".to_string(),
        "react-relay".to_string(),
    ]
}

impl Default for ExtractConfig {
    fn default() -> Self {
        Self {
            magic_comment: default_magic_comment(),
            tag_identifiers: default_tag_identifiers(),
            modules: default_modules(),
            allow_global_identifiers: false,
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
    /// Map of imported identifiers to their module source
    /// e.g., "gql" -> "graphql-tag"
    imports: std::collections::HashMap<String, String>,
    /// Track comments for magic comment detection
    pending_comments: Vec<(usize, String)>,
}

impl<'a> GraphQLVisitor<'a> {
    fn new(source: &'a str, config: &'a ExtractConfig) -> Self {
        Self {
            source,
            config,
            extracted: Vec::new(),
            imports: std::collections::HashMap::new(),
            pending_comments: Vec::new(),
        }
    }

    /// Check if a tag identifier is valid (imported or global allowed)
    fn is_valid_tag(&self, tag_name: &str) -> bool {
        if self.config.allow_global_identifiers {
            return true;
        }

        if let Some(module_source) = self.imports.get(tag_name) {
            return self.config.modules.contains(module_source);
        }

        false
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
            });
        }

        None
    }

    /// Check if there's a magic comment before this position
    fn check_magic_comment(&self, pos: usize) -> bool {
        // Look for a comment that precedes this position
        self.pending_comments.iter().any(|(comment_pos, content)| {
            *comment_pos < pos && content.trim() == self.config.magic_comment
        })
    }
}

impl swc_core::ecma::visit::Visit for GraphQLVisitor<'_> {
    /// Visit import declarations to track GraphQL imports
    fn visit_import_decl(&mut self, import: &swc_core::ecma::ast::ImportDecl) {
        use swc_core::ecma::visit::VisitWith;
        let module_source = String::from_utf8_lossy(import.src.value.as_bytes()).to_string();

        // Only track imports from configured modules
        if self.config.modules.contains(&module_source) {
            for specifier in &import.specifiers {
                use swc_core::ecma::ast::ImportSpecifier;
                match specifier {
                    ImportSpecifier::Named(named) => {
                        // Map local name to module source
                        let local_name =
                            String::from_utf8_lossy(named.local.sym.as_bytes()).to_string();
                        self.imports.insert(local_name, module_source.clone());
                    }
                    ImportSpecifier::Default(default) => {
                        let local_name =
                            String::from_utf8_lossy(default.local.sym.as_bytes()).to_string();
                        self.imports.insert(local_name, module_source.clone());
                    }
                    ImportSpecifier::Namespace(ns) => {
                        let local_name =
                            String::from_utf8_lossy(ns.local.sym.as_bytes()).to_string();
                        self.imports.insert(local_name, module_source.clone());
                    }
                }
            }
        }

        // Continue traversal into child nodes
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

        if !self.config.tag_identifiers.contains(&tag_name) {
            tagged.visit_children_with(self);
            return;
        }

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
                    if self.config.tag_identifiers.contains(&name) && self.is_valid_tag(&name) {
                        Some(name)
                    } else {
                        None
                    }
                }
                Expr::Member(member) => {
                    // Handle member expressions like `graphql.default`
                    if let Expr::Ident(obj) = &*member.obj {
                        let name = String::from_utf8_lossy(obj.sym.as_bytes()).to_string();
                        if self.config.tag_identifiers.contains(&name) && self.is_valid_tag(&name) {
                            Some(name)
                        } else {
                            None
                        }
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
        assert_eq!(config.magic_comment, "GraphQL");
        assert!(config.tag_identifiers.contains(&"gql".to_string()));
        assert!(config.modules.contains(&"graphql-tag".to_string()));
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
        fn test_extract_tagged_template_without_import_disallowed() {
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

            // Should not extract because gql is not imported
            assert_eq!(result.len(), 0);
        }

        #[test]
        fn test_extract_tagged_template_without_import_allowed() {
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
                allow_global_identifiers: true,
                ..Default::default()
            };
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            // Should extract because global identifiers are allowed
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
        fn test_extract_with_custom_tag() {
            let source = r"
import { customGql } from 'graphql-tag';

const query = customGql`query Custom { field }`;
";
            let mut config = ExtractConfig::default();
            config.tag_identifiers.push("customGql".to_string());
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query Custom"));
            assert_eq!(result[0].tag_name, Some("customGql".to_string()));
        }

        #[test]
        fn test_import_from_unknown_module() {
            let source = r"
import { gql } from 'unknown-module';

const query = gql`query Test { field }`;
";
            let config = ExtractConfig::default();
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            // Should not extract because module is not in the allowed list
            assert_eq!(result.len(), 0);
        }

        #[test]
        fn test_default_import() {
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
        fn test_renamed_import() {
            let source = r"
import { gql as query } from 'graphql-tag';

const q = query`query Test { field }`;
";
            let mut config = ExtractConfig::default();
            config.tag_identifiers.push("query".to_string());
            let result =
                extract_from_source(source, Language::TypeScript, &config, "test").unwrap();

            assert_eq!(result.len(), 1);
            assert!(result[0].source.contains("query Test"));
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
