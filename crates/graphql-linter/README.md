# graphql-linter

A flexible GraphQL linting engine with support for document-level and project-wide analysis.

## Features

- **Multiple Linting Contexts**: Document-level, schema-only, and project-wide analysis
- **Configurable Rules**: Enable/disable rules with custom severity levels
- **Tool-Specific Config**: Different rule sets for LSP vs CLI
- **Extensible**: Easy to add new custom rules
- **Performance-Aware**: Separate fast and expensive rule categories

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
graphql-linter = { path = "../graphql-linter" }
```

## Getting Started

### Document Against Schema Linting

Fast, real-time linting for single documents:

```rust
use graphql_linter::{Linter, DocumentSchemaContext, LintConfig};

let config = LintConfig::recommended();
let linter = Linter::new(config);

let ctx = DocumentSchemaContext {
    document: "query { user { oldField } }",
    file_name: "query.graphql",
    schema: &schema_index,
};

let diagnostics = linter.lint_document(&ctx);

for diagnostic in diagnostics {
    println!("{}: {}", diagnostic.severity, diagnostic.message);
}
```

### Project-Wide Linting

Comprehensive analysis across all documents:

```rust
use graphql_linter::{Linter, ProjectContext, LintConfig};

let config = LintConfig::recommended();
let linter = Linter::new(config);

let ctx = ProjectContext {
    documents: &document_index,
    schema: &schema_index,
};

// Returns HashMap<file_path, Vec<Diagnostic>>
let diagnostics_by_file = linter.lint_project(&ctx);

for (file, diagnostics) in diagnostics_by_file {
    println!("{}:", file);
    for diagnostic in diagnostics {
        println!("  {}", diagnostic.message);
    }
}
```

## Linting Contexts

The linter provides four distinct contexts:

### 1. Standalone Document

Quick validation without schema or project context.

```rust
use graphql_linter::{Linter, StandaloneDocumentContext};

let ctx = StandaloneDocumentContext {
    document: "query GetUser { user { id } }",
    file_name: "query.graphql",
};

let diagnostics = linter.lint_standalone_document(&ctx);
```

**Use case**: Basic syntax and naming checks without schema
**Performance**: Very fast
**Current rules**: None (reserved for future naming/complexity rules)

### 2. Document Against Schema

Validate a single document against a schema.

```rust
use graphql_linter::{Linter, DocumentSchemaContext};

let ctx = DocumentSchemaContext {
    document: "query { user { id name } }",
    file_name: "query.graphql",
    schema: &schema_index,
};

let diagnostics = linter.lint_document(&ctx);
```

**Use case**: Real-time editor feedback
**Performance**: Fast, runs per-document
**Current rules**: `no_deprecated`

### 3. Standalone Schema

Schema design validation without documents.

```rust
use graphql_linter::{Linter, StandaloneSchemaContext};

let ctx = StandaloneSchemaContext {
    schema: &schema_index,
};

let diagnostics = linter.lint_standalone_schema(&ctx);
```

**Use case**: Schema-only validation
**Performance**: Fast
**Current rules**: None (reserved for future schema design rules)

### 4. Project-Wide Analysis

Comprehensive analysis across all documents and schema.

```rust
use graphql_linter::{Linter, ProjectContext};

let ctx = ProjectContext {
    documents: &document_index,
    schema: &schema_index,
};

let diagnostics_by_file = linter.lint_project(&ctx);
```

**Use case**: CI/CD, comprehensive project analysis
**Performance**: Potentially expensive on large projects
**Current rules**: `unique_names`, `unused_fields`

## Configuration

### Basic Configuration

```rust
use graphql_linter::{LintConfig, FullLintConfig, ExtendsConfig, LintRuleConfig, LintSeverity};
use std::collections::HashMap;

// Use recommended defaults
let config = LintConfig::recommended();

// Preset with rules override
let config = LintConfig::Full(FullLintConfig {
    extends: Some(ExtendsConfig::Single("recommended".to_string())),
    rules: HashMap::from([
        ("no_deprecated".to_string(), LintRuleConfig::Severity(LintSeverity::Warn)),
    ]),
});
```

### YAML Configuration

Configure in `.graphqlrc.yaml`:

```yaml
# Happy path - just use recommended preset
lint: recommended

# Fine-grained rules only (no presets)
lint:
  rules:
    unique_names: error
    no_deprecated: warn

# Preset with overrides (ESLint-style)
lint:
  extends: recommended
  rules:
    no_deprecated: off
    require_id_field: error

# Multiple presets (later overrides earlier)
lint:
  extends: [recommended]
  rules:
    unused_fields: warn
```

**Tool-specific overrides:**

```yaml
# Base configuration
lint:
  extends: recommended
  rules:
    no_deprecated: warn

# Tool-specific overrides
extensions:
  cli:
    lint:
      rules:
        unused_fields: error  # Enable expensive rule in CI

  lsp:
    lint:
      rules:
        unused_fields: off  # Disable expensive rule in editor
```

### Legacy Configuration (Deprecated)

The following format is still supported for backwards compatibility but is deprecated:

```yaml
# DEPRECATED: Use `lint: recommended` or `lint: { extends: recommended }` instead
lint:
  recommended: error
  rules:
    no_deprecated: warn
```

### Severity Levels

- `off` - Disable the rule
- `warn` - Show as warning
- `error` - Show as error

## Built-in Rules

### redundant_fields

**Type**: StandaloneDocumentRule
**Default**: `off` (opt-in)
**Performance**: Fast

Detects fields in a selection set that are redundant because they are already included in a sibling fragment spread. This helps keep queries clean and maintainable by avoiding duplication.

**Project-wide fragment resolution**: The rule has access to all fragments across the entire project, so it works correctly even when fragments are defined in different files. This is consistent with GraphQL's global fragment scope.

The rule is alias-aware - it only considers a field redundant if it has the same alias (or no alias) as in the fragment. Differently aliased versions of the same field are not considered redundant.

```graphql
# Fragment definition
fragment UserFields on User {
  id
  name
}

# Query with redundant fields
query GetUser {
  user {
    ...UserFields
    id # ⚠️ Warning: Redundant - already in UserFields
    name # ⚠️ Warning: Redundant - already in UserFields
    userId: id # ✅ OK - Different alias
  }
}
```

The rule handles:

- Direct redundancy (field in same selection set as fragment spread)
- Transitive redundancy (field in fragment that includes other fragments)
- Circular fragment references (prevents infinite loops)
- Aliased fields (only same alias is considered redundant)

### no_deprecated

**Type**: DocumentSchemaRule
**Default**: `warn`
**Performance**: Fast

Warns when using fields marked as deprecated in the schema.

```graphql
# Schema
type User {
  id: ID!
  name: String! @deprecated(reason: "Use fullName instead")
  fullName: String!
}

# Query
query {
  user {
    name # ⚠️ Warning: Field 'name' is deprecated: Use fullName instead
  }
}
```

### unique_names

**Type**: ProjectRule
**Default**: `error`
**Performance**: Fast (project-wide but efficient)

Ensures operation and fragment names are unique across the project.

```graphql
# file1.graphql
query GetUser {
  user {
    id
  }
}

# file2.graphql
query GetUser {
  user {
    name
  }
} # ❌ Error: Duplicate operation name 'GetUser'
```

### unused_fields

**Type**: ProjectRule
**Default**: `off` (opt-in)
**Performance**: Expensive on large schemas

Detects schema fields that are never queried in any operation or fragment.

```graphql
# Schema
type User {
  id: ID!
  email: String! # ⚠️ Warning: Field 'email' is never used
}

# Operations only query 'id', never 'email'
query {
  user {
    id
  }
}
```

## Creating Custom Rules

### Document Schema Rules

Rules that check a single document against a schema:

```rust
use graphql_linter::{DocumentSchemaRule, DocumentSchemaContext, Diagnostic, Severity};

pub struct MyRule;

impl DocumentSchemaRule for MyRule {
    fn name(&self) -> &'static str {
        "my_rule"
    }

    fn description(&self) -> &'static str {
        "Description of what this rule checks"
    }

    fn check(&self, ctx: &DocumentSchemaContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Parse and analyze the document
        // Check against schema in ctx.schema
        // Add diagnostics as needed

        diagnostics
    }
}
```

### Project Rules

Rules that analyze the entire project:

```rust
use graphql_linter::{ProjectRule, ProjectContext, Diagnostic};
use std::collections::HashMap;

pub struct MyProjectRule;

impl ProjectRule for MyProjectRule {
    fn name(&self) -> &'static str {
        "my_project_rule"
    }

    fn description(&self) -> &'static str {
        "Description of project-wide check"
    }

    fn check(&self, ctx: &ProjectContext) -> HashMap<String, Vec<Diagnostic>> {
        let mut diagnostics_by_file = HashMap::new();

        // Analyze all documents in ctx.documents
        // Check against schema in ctx.schema
        // Group diagnostics by file path

        diagnostics_by_file
    }
}
```

### Registering Rules

Add rules to the registries in `rules/mod.rs`:

```rust
pub fn all_document_schema_rules() -> Vec<Box<dyn DocumentSchemaRule>> {
    vec![
        Box::new(DeprecatedFieldRule),
        Box::new(MyRule),  // Add your rule here
    ]
}

pub fn all_project_rules() -> Vec<Box<dyn ProjectRule>> {
    vec![
        Box::new(UniqueNamesRule),
        Box::new(UnusedFieldsRule),
        Box::new(MyProjectRule),  // Add your rule here
    ]
}
```

## API Reference

### Linter

```rust
impl Linter {
    pub fn new(config: LintConfig) -> Self;

    pub fn lint_standalone_document(
        &self,
        ctx: &StandaloneDocumentContext
    ) -> Vec<Diagnostic>;

    pub fn lint_document(
        &self,
        ctx: &DocumentSchemaContext
    ) -> Vec<Diagnostic>;

    pub fn lint_standalone_schema(
        &self,
        ctx: &StandaloneSchemaContext
    ) -> Vec<Diagnostic>;

    pub fn lint_project(
        &self,
        ctx: &ProjectContext
    ) -> HashMap<String, Vec<Diagnostic>>;
}
```

### LintConfig

```rust
/// Lint configuration - supports multiple formats
pub enum LintConfig {
    /// Simple preset: `lint: recommended`
    Preset(String),

    /// Full config with extends and rules
    Full(FullLintConfig),

    /// Legacy format (deprecated)
    LegacyRules { rules: HashMap<String, LintRuleConfig> },
}

pub struct FullLintConfig {
    pub extends: Option<ExtendsConfig>,
    pub rules: HashMap<String, LintRuleConfig>,
}

pub enum ExtendsConfig {
    Single(String),          // extends: recommended
    Multiple(Vec<String>),   // extends: [recommended, strict]
}

impl LintConfig {
    pub fn default() -> Self;           // Empty config (no rules)
    pub fn recommended() -> Self;       // Preset("recommended")

    pub fn get_severity(&self, rule: &str) -> Option<LintSeverity>;
    pub fn is_enabled(&self, rule: &str) -> bool;
    pub fn validate(&self) -> Result<(), String>;
    pub fn merge(&self, override_config: &Self) -> Self;
}
```

### Diagnostic

```rust
pub struct Diagnostic {
    pub message: String,
    pub severity: Severity,
    pub location: Option<Location>,
    pub rule: String,
}

pub struct Location {
    pub line: usize,
    pub column: usize,
}

pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}
```

## Performance Considerations

### For LSP Integration

Use lightweight rules for real-time feedback:

```rust
// Good: Fast document-level linting
let diagnostics = linter.lint_document(&ctx);

// Avoid: Expensive project-wide linting in real-time
// let diagnostics = linter.lint_project(&ctx);  // Too slow!
```

Configure LSP to disable expensive rules:

```yaml
extensions:
  lsp:
    lint:
      rules:
        unused_fields: off
```

### For CLI Integration

Enable all rules including expensive project-wide analysis:

```yaml
extensions:
  cli:
    lint:
      rules:
        unused_fields: error # Enable in CI
```

## Examples

### Custom Rule for Query Complexity

```rust
use graphql_linter::{DocumentSchemaRule, DocumentSchemaContext, Diagnostic, Severity};
use apollo_parser::Parser;

pub struct QueryComplexityRule {
    max_depth: usize,
}

impl DocumentSchemaRule for QueryComplexityRule {
    fn name(&self) -> &'static str {
        "query_complexity"
    }

    fn description(&self) -> &'static str {
        "Limits query nesting depth"
    }

    fn check(&self, ctx: &DocumentSchemaContext) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let parser = Parser::new(ctx.document);
        let tree = parser.parse();

        // Analyze depth and add diagnostics if too deep

        diagnostics
    }
}
```

### Filtering Diagnostics by Severity

```rust
use graphql_linter::{Linter, ProjectContext, Severity};

let diagnostics_by_file = linter.lint_project(&ctx);

// Only show errors
for (file, diagnostics) in diagnostics_by_file {
    let errors: Vec<_> = diagnostics.into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();

    if !errors.is_empty() {
        println!("{}:", file);
        for error in errors {
            println!("  {}", error.message);
        }
    }
}
```

## Testing

Run tests:

```bash
cargo test -p graphql-linter
```

Test individual rules:

```bash
cargo test -p graphql-linter deprecated
cargo test -p graphql-linter unique_names
cargo test -p graphql-linter unused_fields
```

## License

MIT OR Apache-2.0
