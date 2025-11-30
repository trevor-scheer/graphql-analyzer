# graphql-linter

A flexible GraphQL linting engine that provides different linting contexts for various use cases.

## Overview

`graphql-linter` is a standalone crate that implements linting rules for GraphQL documents and schemas. It's designed to be used by both the LSP server (for real-time feedback) and the CLI tool (for comprehensive project analysis).

## Architecture

The linter provides four distinct linting contexts, each with its own trait and rule set:

### 1. Standalone Document Linting

**Use case**: Quick validation of isolated queries without schema or project context.

```rust
use graphql_linter::{Linter, StandaloneDocumentContext};

let linter = Linter::new(config);
let ctx = StandaloneDocumentContext {
    document: "query { user { id } }",
    file_name: "query.graphql",
};
let diagnostics = linter.lint_standalone_document(&ctx);
```

**Current rules**: None (future rules: naming conventions, complexity limits)

### 2. Document Against Schema

**Use case**: Real-time feedback as users type in an editor.

```rust
use graphql_linter::{Linter, DocumentSchemaContext};

let linter = Linter::new(config);
let ctx = DocumentSchemaContext {
    document: "query { user { id name } }",
    file_name: "query.graphql",
    schema: &schema_index,
};
let diagnostics = linter.lint_document(&ctx);
```

**Current rules**:
- `deprecated_field`: Warns when using deprecated fields

**Performance**: Fast, runs per-document

### 3. Standalone Schema

**Use case**: Schema design validation without requiring documents.

```rust
use graphql_linter::{Linter, StandaloneSchemaContext};

let linter = Linter::new(config);
let ctx = StandaloneSchemaContext {
    schema: &schema_index,
};
let diagnostics = linter.lint_standalone_schema(&ctx);
```

**Current rules**: None (future rules: naming conventions, directive usage)

### 4. Project-Wide Analysis

**Use case**: Comprehensive analysis across all documents and schema. Typically used in CI or on-demand.

```rust
use graphql_linter::{Linter, ProjectContext};

let linter = Linter::new(config);
let ctx = ProjectContext {
    documents: &document_index,
    schema: &schema_index,
};
// Returns HashMap<file_path, Vec<Diagnostic>>
let diagnostics_by_file = linter.lint_project(&ctx);
```

**Current rules**:
- `unique_names`: Ensures operation and fragment names are unique across the project
- `unused_fields`: Detects schema fields that are never used in any operation

**Performance**: Potentially expensive on large projects

## Configuration

Linting is configured in `.graphqlrc.yaml` with tool-specific overrides:

### Basic Configuration

```yaml
# Top-level lint config (applies to all tools by default)
lint:
  recommended: error
```

### Tool-Specific Overrides

```yaml
# Top-level defaults
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

### Configuration Options

- **Severity levels**: `error`, `warn`, `off`
- **Preset**: `recommended` sets default severities for all rules
- **Per-rule config**: Override individual rules under `rules:`

## Implementing New Rules

### Document Schema Rules

Rules that check a single document against a schema:

```rust
use graphql_linter::{DocumentSchemaRule, DocumentSchemaContext, Diagnostic};

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
        // Return diagnostics

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

## Rule Registry

To enable a rule, add it to the appropriate registry in `rules/mod.rs`:

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

## Current Rules

### deprecated_field (DocumentSchemaRule)

Warns when using fields marked as deprecated in the schema.

**Severity**: `warn` (configurable)

**Example**:
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

### unique_names (ProjectRule)

Ensures operation and fragment names are unique across the entire project.

**Severity**: `error` (configurable)

**Example**:
```graphql
# file1.graphql
query GetUser { user { id } }

# file2.graphql
query GetUser { user { name } }  # ❌ Error: Duplicate operation name 'GetUser'
```

### unused_fields (ProjectRule)

Detects schema fields that are never queried in any operation or fragment.

**Severity**: `off` by default (expensive, opt-in)

**Example**:
```graphql
# Schema
type User {
  id: ID!
  email: String!  # ⚠️ Warning: Field 'email' is never used
}

# Operations only query 'id', never 'email'
query { user { id } }
```

## Performance Considerations

### LSP Usage

For real-time editor feedback, use lightweight rules:
- ✅ `lint_document()` - Fast, runs on every change
- ⚠️ `lint_project()` - Expensive, should be opt-in only

Configure LSP to disable expensive rules by default:

```yaml
extensions:
  lsp:
    lint:
      rules:
        unused_fields: off  # Too expensive for real-time
```

### CLI Usage

The CLI can run all rules including expensive project-wide analysis:

```yaml
extensions:
  cli:
    lint:
      rules:
        unused_fields: error  # Enable in CI
```

## Dependencies

This crate depends on:
- `graphql-project`: Core types (SchemaIndex, DocumentIndex, Diagnostic)
- `apollo-parser`: CST-based GraphQL parsing
- `apollo-compiler`: Validation and type information
- `serde`/`serde_json`: Configuration deserialization

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

## See Also

- [graphql-project](../graphql-project): Core project management and types
- [graphql-lsp](../graphql-lsp): LSP server integration
- [graphql-cli](../graphql-cli): CLI tool integration
