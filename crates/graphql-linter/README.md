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
**Current rules**: `deprecated_field`

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
use graphql_linter::LintConfig;

// Use recommended defaults
let config = LintConfig::recommended();

// Custom configuration
let mut config = LintConfig::default();
config.set_rule_severity("deprecated_field", Severity::Warn);
config.set_rule_severity("unique_names", Severity::Error);
config.set_rule_severity("unused_fields", Severity::Off);
```

### YAML Configuration

Configure in `.graphqlrc.yaml`:

```yaml
# Basic configuration
lint:
  recommended: error
```

**Tool-specific overrides:**

```yaml
# Base configuration
lint:
  recommended: error
  rules:
    deprecated_field: warn
    unique_names: error
    unused_fields: off  # Expensive, off by default

# Tool-specific overrides
extensions:
  # CLI: Enable expensive project-wide lints
  cli:
    lint:
      rules:
        unused_fields: error

  # LSP: Keep expensive lints off for performance
  lsp:
    lint:
      rules:
        unused_fields: off
        unique_names: warn
```

### Severity Levels

- `off` - Disable the rule
- `warn` - Show as warning
- `error` - Show as error

## Built-in Rules

### deprecated_field

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
    name  # ⚠️ Warning: Field 'name' is deprecated: Use fullName instead
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
query GetUser { user { id } }

# file2.graphql
query GetUser { user { name } }  # ❌ Error: Duplicate operation name 'GetUser'
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
  email: String!  # ⚠️ Warning: Field 'email' is never used
}

# Operations only query 'id', never 'email'
query { user { id } }
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
impl LintConfig {
    pub fn default() -> Self;
    pub fn recommended() -> Self;

    pub fn set_rule_severity(&mut self, rule: &str, severity: Severity);
    pub fn get_rule_severity(&self, rule: &str) -> Severity;
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
        unused_fields: off  # Too expensive for real-time
```

### For CLI Integration

Enable all rules including expensive project-wide analysis:

```yaml
extensions:
  cli:
    lint:
      rules:
        unused_fields: error  # Enable in CI
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
