# GraphQL LSP Project Guide

**Last Updated**: January 2026

This document provides context and guidance for working with the GraphQL LSP codebase. It's designed to help future iterations of Claude understand the project quickly and work effectively.

> **Note**: This project is not stable yet. The codebase can be aggressively refactored and rearchitected as needed while it's still early. Breaking changes are expected - don't hesitate to rewrite code paths that aren't working well.

---

## Table of Contents

- [Quick Reference](#quick-reference)
- [Project Overview](#project-overview)
  - [Protected Core Features](#protected-core-features)
- [Architecture](#architecture)
- [Development Workflow](#development-workflow)
- [Key Concepts](#key-concepts)
  - [VSCode Extension Architecture (Critical)](#vscode-extension-architecture-critical)
- [Configuration](#configuration)
- [Testing](#testing)
- [Common Tasks](#common-tasks)
- [Troubleshooting](#troubleshooting)
- [Instructions for Claude](#instructions-for-claude)
  - [Operating Guidelines](#operating-guidelines)
- [Expert Agents](#expert-agents)
- [Skills](#skills)

---

## Quick Reference

### Most Common Commands

```bash
# Build & Test
cargo build                     # Build all crates
cargo test                      # Run all tests
cargo clippy                    # Lint checks
cargo fmt                       # Format code

# Benchmarking
cargo bench                     # Run all benchmarks
cargo bench parse_cold          # Run specific benchmark
cargo bench -- --save-baseline main  # Save baseline for comparison

# CLI Tools
target/debug/graphql validate   # Validate GraphQL files
target/debug/graphql lint       # Lint GraphQL files

# LSP Development
RUST_LOG=debug target/debug/graphql-lsp  # Run LSP with logging

# VSCode Extension
cd editors/vscode
npm run compile                 # Build extension
npm run format                  # Format TypeScript
npm run lint                    # Lint TypeScript
```

### Critical File Locations

- **Project structure**: `.graphqlrc.yaml`
- **Crate sources**: `crates/*/src/`
- **Integration tests**: `tests/`
- **Benchmarks**: `benches/`
- **VSCode extension**: `editors/vscode/`
- **Design docs**: `.claude/notes/active/lsp-rearchitecture/`

### Quick Answers

**Q: Where do I add a new lint rule?**
A: `crates/graphql-linter/src/rules/` - See [Linter README](../crates/graphql-linter/README.md)

**Q: How do I add validation logic?**
A: Add queries in `crates/graphql-analysis/src/` - See [Analysis README](../crates/graphql-analysis/README.md)

**Q: Where's the schema loading code?**
A: `crates/graphql-introspect/` for remote introspection, `crates/graphql-config/` for config parsing

**Q: How does incremental computation work?**
A: Via Salsa queries in `graphql-db` → `graphql-syntax` → `graphql-hir` → `graphql-analysis`

---

## Project Overview

This is a **GraphQL Language Server Protocol (LSP)** implementation written in Rust, providing IDE features for GraphQL files including validation, diagnostics, goto definition, find references, and more.

### What Makes This Project Unique

1. **Query-Based Architecture**: Uses [Salsa](https://github.com/salsa-rs/salsa) for automatic incremental computation
2. **Multi-Language Support**: Works with pure `.graphql` files and embedded GraphQL in TypeScript/JavaScript
3. **Project-Wide Analysis**: Validates across all files with proper fragment resolution
4. **Extensible Linting**: Plugin-based linting system with tool-specific configuration
5. **Remote Schema Support**: Introspects remote GraphQL endpoints and converts to SDL

### Protected Core Features

These features are fundamental to the project's value proposition and must NOT be removed or degraded:

| Feature                              | Why It's Critical                                                     | What Enables It                                                 |
| ------------------------------------ | --------------------------------------------------------------------- | --------------------------------------------------------------- |
| **Embedded GraphQL in TS/JS**        | Most GraphQL users write queries in TS/JS files, not `.graphql` files | `documentSelector` includes TS/JS languages in VSCode extension |
| **Real-time diagnostics**            | Users expect immediate feedback while typing                          | LSP `textDocument/didChange` notifications                      |
| **Project-wide fragment resolution** | Fragments are defined across many files                               | `all_fragments()` query indexes entire project                  |
| **Schema validation**                | Invalid schemas break everything downstream                           | `graphql-analysis` validates before use                         |

**Performance concerns are valid but must be solved without removing features.** Acceptable solutions:

- Server-side filtering (only process files containing GraphQL)
- Lazy/deferred processing
- Debouncing rapid changes
- User configuration to opt-out

### Key Technologies

- **Language**: Rust (see `rust-toolchain.toml` for version)
- **LSP Framework**: [tower-lsp](https://github.com/ebkalderon/tower-lsp)
- **GraphQL Parsing**: [apollo-compiler](https://github.com/apollographql/apollo-rs) and [graphql-parser](https://github.com/graphql-rust/graphql-parser)
- **Incremental Computation**: [Salsa](https://github.com/salsa-rs/salsa)
- **Build System**: Cargo

---

## Architecture

### Current Architecture (Rearchitecture in Progress)

The codebase is transitioning to a query-based, incremental architecture inspired by [rust-analyzer](https://rust-analyzer.github.io/book/contributing/architecture.html).

```
┌─────────────────────────────────────────────────┐
│  graphql-lsp (LSP Protocol Adapter)             │
│  - tower-lsp integration                        │
│  - JSON-RPC handling                            │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│  graphql-ide (Editor API)                       │
│  - POD types (Position, Range, Location)        │
│  - AnalysisHost & Analysis snapshots            │
│  - Thread-safe, lock-free queries               │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│  graphql-analysis (Validation & Linting)        │
│  - file_diagnostics() query                     │
│  - Schema validation                            │
│  - Document validation                          │
│  - Lint integration                             │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│  graphql-hir (High-level IR)                    │
│  - Separates structure from bodies              │
│  - schema_types(), all_fragments() queries      │
│  - Fine-grained invalidation                    │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│  graphql-syntax (Parsing)                       │
│  - parse() query (file-local)                   │
│  - LineIndex for position conversion            │
│  - TypeScript/JavaScript extraction             │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│  graphql-db (Salsa Database)                    │
│  - FileId, FileContent, FileMetadata            │
│  - RootDatabase                                 │
│  - Automatic memoization & invalidation         │
└─────────────────────────────────────────────────┘
```

### Supporting Crates

- **graphql-config**: Parses `.graphqlrc.yaml` configuration files
- **graphql-extract**: Extracts GraphQL from TypeScript/JavaScript template literals
- **graphql-introspect**: Introspects remote GraphQL endpoints and converts to SDL
- **graphql-linter**: Pluggable linting engine with document and project-wide rules
- **graphql-cli**: CLI tool for validation and linting

### Directory Structure

```
crates/
├── graphql-analysis/    # Validation layer (Salsa queries)
├── graphql-cli/         # CLI tool
├── graphql-config/      # Configuration parser
├── graphql-db/          # Salsa database foundation
├── graphql-extract/     # Extract GraphQL from TS/JS
├── graphql-hir/         # Semantic layer (structure/body separation)
├── graphql-ide/         # Editor API (POD types)
├── graphql-introspect/  # Remote schema introspection
├── graphql-linter/      # Linting engine
├── graphql-lsp/         # LSP server implementation
└── graphql-syntax/      # Parsing layer

benches/                 # Performance benchmarks
editors/
└── vscode/              # VSCode extension

tests/                   # Integration tests
```

For detailed architecture documentation, see:

- [Foundation Phase](../.claude/notes/active/lsp-rearchitecture/01-FOUNDATION.md)
- [Semantics Phase](../.claude/notes/active/lsp-rearchitecture/02-SEMANTICS.md)
- [Analysis Phase](../.claude/notes/active/lsp-rearchitecture/03-ANALYSIS.md)

---

## Development Workflow

### Setup

1. Clone the repository
2. Ensure Rust toolchain matches `rust-toolchain.toml`
3. Run `cargo build` to build all crates
4. Run `cargo test` to verify everything works

### Before Committing

Pre-commit hooks are configured via [cargo-husky](https://github.com/rhysd/cargo-husky):

```bash
cargo fmt                # Format Rust code
cargo clippy             # Lint Rust code
cargo test               # Run tests

# For VSCode extension changes
cd editors/vscode
npm run format:check     # Check formatting
npm run lint             # Lint TypeScript
```

### Creating Pull Requests

1. Create a feature branch from `main` (or target branch)
2. Make your changes following code quality standards
3. Commit with conventional commit messages (e.g., `feat:`, `fix:`, `refactor:`)
4. Use `gh pr create` to open a PR
5. Follow PR guidelines (see below)

### PR Guidelines

Use the `/create-pr` skill for detailed guidance. Key points:

- Write clear, descriptive PR titles (no emoji)
- Explain what changed and why
- Include tests for new functionality
- Document consulted SME agents
- **Never mention CI status** (tests passing, clippy clean) - CI enforces these

For bug fixes, use the `/bug-fix-workflow` skill which enforces the two-commit structure (failing test first, then fix).

---

## Key Concepts

### GraphQL Document Model

Understanding the GraphQL document model is critical for implementing features correctly.

#### Document Types

- **Schema Document**: Type definitions, directives, schema extensions
- **Executable Document**: Operations (query/mutation/subscription) and/or fragments

**Important**: An executable document can contain ONLY fragments with no operations. This is valid GraphQL.

#### Fragment Scope

Fragments have **project-wide scope**, not file scope:

- All fragments are globally available across the entire project
- Operations can reference fragments defined in other files
- Fragment spreads can reference other fragments (transitive dependencies)

#### Validation Implications

When validating operations, you MUST:

1. Include **direct fragment dependencies** (fragments referenced by operation)
2. **Recurse through fragment dependencies** (fragments referenced by fragments)
3. **Handle circular references** (fragment A → fragment B → fragment A)
4. **Validate against schema** for all fragments in the dependency chain

**Failing to include transitive fragment dependencies will cause incorrect validation errors.**

#### Naming and Uniqueness

- **Operation Names**: Must be unique across the entire project (when named)
- **Fragment Names**: Must be unique across the entire project
- **Type Names**: Must be unique within the schema
- **Anonymous Operations**: Only allowed when a document contains a single operation

### The Golden Invariant

> **"Editing a document's body never invalidates global schema knowledge"**

This principle drives the architecture:

- **Structure** (stable): Type names, field signatures, operation names, fragment names
- **Bodies** (dynamic): Selection sets, field selections, directives

By separating structure from bodies, we achieve fine-grained incremental recomputation.

### Salsa Query System

All derived data is computed via Salsa queries:

- **Automatic memoization**: Results cached by inputs
- **Dependency tracking**: Salsa knows what depends on what
- **Incremental invalidation**: Only affected queries re-run
- **Lazy evaluation**: Queries only run when results are needed

Example flow:

```rust
// User types in a file
file_content.set_text(db, new_content);

// Salsa automatically invalidates:
parse(db, file_content)           // File changed
operation_body(db, operation_id)  // Body changed
file_diagnostics(db, file_id)     // Needs revalidation

// Salsa keeps cached:
schema_types(db)                  // Schema unchanged ✅
all_fragments(db)                 // No fragment changes ✅
```

### LSP Feature Support

Current LSP features:

- **Diagnostics**: Real-time validation and linting with accurate positions
- **Goto Definition**: Navigate to type/field/fragment/variable definitions
- **Find References**: Find all usages of types/fields/fragments
- **Hover**: Type information and descriptions
- **Schema Introspection**: Load schemas from remote URLs

All features work in:

- Pure GraphQL files (`.graphql`, `.gql`)
- Embedded GraphQL in TypeScript/JavaScript

### VSCode Extension Architecture (Critical)

The VSCode extension has two separate systems that are often confused:

#### Document Selector (LSP Features)

```typescript
documentSelector: [
  { scheme: "file", language: "graphql" },
  { scheme: "file", language: "typescript" },
  { scheme: "file", language: "typescriptreact" },
  // etc.
];
```

The `documentSelector` controls which files receive **LSP features**:

- `textDocument/didOpen` and `textDocument/didChange` notifications
- Diagnostics, hover, goto definition, find references, completion
- Real-time feedback as the user types

**Without a language in `documentSelector`, that language gets NO LSP features.**

#### Grammar Injection (Syntax Highlighting Only)

Grammar injection (via TextMate grammars) provides **syntax highlighting only**:

- Colorizes GraphQL inside template literals
- Purely visual - no semantic understanding
- Completely separate from LSP

#### File Watcher (Disk Events Only)

```typescript
fileEvents: workspace.createFileSystemWatcher("**/*.{graphql,gql,ts,tsx}");
```

The file watcher fires `workspace/didChangeWatchedFiles` **only on disk saves**:

- Does NOT provide real-time editing feedback
- Does NOT send document content to the server
- Only useful for detecting file creation/deletion/rename

#### Common Misconception

**WRONG**: "Grammar injection provides embedded GraphQL support, so we can remove TS/JS from documentSelector"

**RIGHT**: Grammar injection only provides colors. The documentSelector is required for all actual LSP features (diagnostics, hover, goto def, etc.) in TS/JS files.

---

## Configuration

### GraphQL Configuration (`.graphqlrc.yaml`)

```yaml
# Single-project configuration
schema: schema.graphql
documents: "src/**/*.graphql"

# Happy path - use recommended preset
lint: recommended

# Or: Preset with overrides (ESLint-style)
lint:
  extends: recommended
  rules:
    no_deprecated: warn
    require_id_field: error

# Or: Fine-grained rules only
lint:
  rules:
    unique_names: error
    no_deprecated: warn

# Tool-specific overrides
extensions:
  lsp:
    lint:
      rules:
        unused_fields: off
  cli:
    lint:
      rules:
        unused_fields: error
```

### Multi-Project Configuration

```yaml
projects:
  frontend:
    schema: frontend/schema.graphql
    documents: "frontend/**/*.graphql"
  backend:
    schema: backend/schema.graphql
    documents: "backend/**/*.graphql"
```

CLI commands require `--project` flag for multi-project configs (unless a "default" project exists):

```bash
graphql validate --project frontend
graphql lint --project backend
```

### Remote Schema Loading

Schemas can be loaded from URLs via introspection:

```yaml
schema: https://api.example.com/graphql
```

The introspection flow:

1. `graphql-config` detects URL
2. `graphql-introspect` executes introspection query
3. JSON response converted to SDL
4. SDL used for validation and IDE features

See [graphql-introspect README](../crates/graphql-introspect/README.md) for details.

---

## Testing

### Running Tests

```bash
# All tests
cargo test

# Specific crate
cargo test --package graphql-linter

# Specific test
cargo test --package graphql-linter redundant_fields

# With output
cargo test -- --nocapture

# Integration tests only
cargo test --test '*'
```

### Test Organization

- **Unit tests**: Located alongside source files (`mod tests { ... }`)
- **Integration tests**: In `tests/` directory
- **Snapshot tests**: Using `cargo-insta` (if applicable)

### Writing Tests

Tests should prioritize **human readability**. A test that's easy to understand is easy to maintain and debug.

#### Test Readability Guidelines

- **Use helper functions** to reduce boilerplate and make test intent clear
- **Use fixtures** for common test data (schemas, documents, configs)
- **Use snapshot tests** (`cargo-insta`) for complex output validation
- **Name tests descriptively** - the name should explain what's being tested and expected behavior
- **Keep tests focused** - one logical assertion per test when possible

#### Example Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Helper function reduces boilerplate and clarifies intent
    fn validate(schema: &str, document: &str) -> Vec<Diagnostic> {
        let db = TestDatabase::new();
        db.set_schema(schema);
        db.set_document(document);
        db.diagnostics()
    }

    #[test]
    fn fragment_spread_on_wrong_type_reports_error() {
        let diagnostics = validate(
            "type Query { user: User } type User { name: String }",
            "query { user { ...AdminFields } } fragment AdminFields on Admin { role }",
        );

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Admin"));
    }
}
```

#### When to Use Snapshots

Use `cargo-insta` snapshots when:

- Output is complex or multi-line (diagnostic messages, formatted output)
- You want to catch unintended changes in output format
- Manual assertion would be verbose and hard to read

```rust
#[test]
fn lint_report_format() {
    let report = run_linter(FIXTURE_DOCUMENT);
    insta::assert_snapshot!(report);
}
```

### Performance Benchmarks

The project includes comprehensive benchmarks to validate the Salsa-based incremental computation architecture. See [benches/README.md](../benches/README.md) for complete documentation.

#### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench parse_cold

# Save baseline for comparison
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main
```

#### What the Benchmarks Validate

1. **Salsa Caching**: Warm queries should be 100-1000x faster than cold queries
2. **Golden Invariant**: Editing operation bodies doesn't invalidate schema cache (< 100ns)
3. **Fragment Resolution**: Cross-file fragment resolution benefits from caching
4. **AnalysisHost Performance**: High-level IDE API performance

#### Interpreting Results

Criterion generates HTML reports in `target/criterion/`. Open `target/criterion/report/index.html` to view:

- Performance distributions
- Regression detection
- Comparison with previous runs

**Expected results if architecture is working correctly:**

- Warm vs Cold: 100-1000x speedup
- Golden Invariant: < 100 nanoseconds
- Fragment Resolution: ~10x speedup with caching

---

## Common Tasks

### Adding a New Lint Rule

Use the `/adding-lint-rules` skill for step-by-step guidance. Quick reference:

- Location: `crates/graphql-linter/src/rules/`
- See [graphql-linter README](../crates/graphql-linter/README.md) for complete guide

### Adding Schema Validation

Add validation queries in `crates/graphql-analysis/src/`:

```rust
// In schema_validation.rs
pub fn validate_schema_file(
    db: &dyn GraphQLAnalysisDatabase,
    file_id: FileId,
) -> Vec<Diagnostic> {
    // Implementation using HIR queries
}
```

See [graphql-analysis README](../crates/graphql-analysis/README.md) for architecture.

### Adding an IDE Feature

1. **Add the feature type** in `crates/graphql-ide/src/types.rs` (POD struct)
2. **Implement the query** in `crates/graphql-ide/src/lib.rs` on `Analysis`
3. **Add tests** in `crates/graphql-ide/src/lib.rs`
4. **Integrate in LSP** in `crates/graphql-lsp/src/`

Example:

```rust
// In graphql-ide
impl Analysis {
    pub fn your_feature(&self, file: &FilePath, pos: Position) -> Option<YourResult> {
        // Query HIR and analysis layers
        Some(YourResult { /* ... */ })
    }
}
```

### Debugging the LSP

```bash
# Run with debug logging
RUST_LOG=debug target/debug/graphql-lsp

# Module-specific logging
RUST_LOG=graphql_lsp=debug,graphql_analysis=info target/debug/graphql-lsp

# With OpenTelemetry (requires otel feature)
cargo build --features otel
OTEL_TRACES_ENABLED=1 target/debug/graphql-lsp

# View traces in Jaeger
docker run -d --name jaeger -p 4317:4317 -p 16686:16686 jaegertracing/all-in-one:latest
# Open http://localhost:16686
```

See [Logging Strategy](#logging-strategy) for details.

### Building VSCode Extension

```bash
cd editors/vscode
npm install
npm run compile

# Package extension
npm run package

# Install locally
code --install-extension graphql-lsp-*.vsix
```

---

## Troubleshooting

### Common Issues

#### "No project found" Error in CLI

**Problem**: Running `graphql validate` or `graphql lint` shows "No project found"

**Solution**:

- Ensure `.graphqlrc.yaml` exists in current directory or parent
- For multi-project configs without "default", use `--project` flag:
  ```bash
  graphql validate --project frontend
  ```

#### LSP Not Responding in Editor

**Problem**: LSP features not working in VSCode

**Diagnosis**:

```bash
# Check if LSP binary exists
ls -la target/debug/graphql-lsp

# Test LSP directly
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | target/debug/graphql-lsp
```

**Solution**:

- Rebuild: `cargo build`
- Check VSCode extension logs: View → Output → GraphQL
- Increase logging: Set `RUST_LOG=debug` in VSCode settings

#### Tests Failing After Schema Changes

**Problem**: Tests fail after modifying schema types

**Cause**: Salsa cache persists across test runs in same process

**Solution**:

- Tests should create fresh database instances
- Use `cargo test -- --test-threads=1` if needed
- Clear `target/` and rebuild: `cargo clean && cargo build`

#### Fragment Not Found Errors

**Problem**: Validation shows "Unknown fragment" but fragment exists

**Cause**: Missing fragment index or stale cache

**Solution**:

- Ensure fragment file is registered in `document_files()`
- Check that `all_fragments()` query includes the file
- Verify fragment name uniqueness (duplicates may be filtered)

#### Slow LSP Performance

**Problem**: Editor feels sluggish with LSP enabled

**Diagnosis**:

```bash
# Run with OpenTelemetry to identify hot spots
cargo build --features otel
OTEL_TRACES_ENABLED=1 target/debug/graphql-lsp
# View traces at http://localhost:16686
```

**Solution**:

- Disable expensive lints in LSP config:
  ```yaml
  extensions:
    lsp:
      lint:
        rules:
          unused_fields: off
  ```
- Check for large files causing re-parsing
- Profile with `cargo flamegraph` if needed

---

## Logging Strategy

### Log Levels

- **ERROR**: Critical failures (schema load errors, processing failures)
- **WARN**: Non-fatal issues (missing config, stale data)
- **INFO**: High-level operations (document open/save, validation complete)
- **DEBUG**: Detailed operations (cache hits, timing)
- **TRACE**: Deep debugging (not currently used)

### Configuration

Set `RUST_LOG` environment variable:

```bash
RUST_LOG=debug                                      # All debug logs
RUST_LOG=graphql_lsp=debug,graphql_analysis=info   # Module-specific
RUST_LOG=off                                        # Disable logging
```

### Guidelines

- Log user-facing operations at **INFO**
- Log performance metrics at **DEBUG**
- Include context (file paths, positions) in messages
- Use structured fields: `tracing::info!(uri = ?doc_uri, "message")`
- Log errors immediately before propagating
- Avoid logging sensitive data (API keys, credentials)

### OpenTelemetry Integration

For performance analysis:

1. Build with `otel` feature: `cargo build --features otel`
2. Start Jaeger: `docker run -d --name jaeger -p 4317:4317 -p 16686:16686 jaegertracing/all-in-one:latest`
3. Run with tracing enabled: `OTEL_TRACES_ENABLED=1 target/debug/graphql-lsp`
4. View traces: http://localhost:16686

Overhead: ~1-2% CPU when enabled, zero when disabled.

---

## Instructions for Claude

### Pre-Task Skill Check (REQUIRED)

**Before starting any implementation, check if a skill applies:**

| If the task involves...        | Use this skill FIRST        |
| ------------------------------ | --------------------------- |
| Fixing a bug or issue          | `/bug-fix-workflow`         |
| Adding a lint rule             | `/adding-lint-rules`        |
| Adding an IDE/LSP feature      | `/add-ide-feature`          |
| Creating a pull request        | `/create-pr`                |
| Reviewing a pull request       | `/review-pr`                |
| Feature/bug/architecture work  | `/sme-consultation`         |

**This is not optional.** Skills enforce important workflows (e.g., bug fixes require a failing test first). Skipping them leads to incomplete work that must be redone.

### General Approach

1. **Check for applicable skills**: See the table above - invoke the skill BEFORE starting work
2. **Read before acting**: Always check relevant README.md files and this document before starting work
3. **Understand the architecture**: Know which layer you're working in (db → syntax → hir → analysis → ide → lsp)
4. **Consult expert agents (REQUIRED)**: Use the `/sme-consultation` skill which guides consultation of SME agents in `.claude/agents/`
5. **Follow the patterns**: Study existing code in the same layer before adding new features
6. **Test incrementally**: Write tests as you go, don't batch at the end
7. **Keep it simple**: Don't over-engineer or add unnecessary abstractions

### Code Style

- **No emoji** in code or commits (unless user explicitly requests)
- Follow Rust conventions: `snake_case` for functions, `CamelCase` for types
- Keep lines under 100 characters where reasonable

#### Comment Guidelines

Code should be self-documenting. Avoid comments that merely restate what the code does.

**DO NOT add comments that:**

- Describe what the next line of code does (e.g., `// Parse the file`, `// Return the result`)
- Repeat information obvious from variable/function names (e.g., `// Create source map` before `SourceMap::new()`)
- Mark sections with obvious purpose (e.g., `// Phase 1: Load files`)
- Explain standard operations (e.g., `// Collect into vec`, `// Handle error case`)
- Describe test structure (e.g., `// Test database`, `// First line`)

**DO add comments for:**

- **Why** something non-obvious is done (e.g., `// Use offset 0 because apollo-compiler errors lack precise positions`)
- Subtle behavior or edge cases (e.g., `// Handles cycles in fragment references`)
- Safety invariants (e.g., `// SAFETY: storage is owned and outlives references`)
- Temporary workarounds or known limitations
- Architecture decisions that aren't evident from the code
- References to external specifications or issues

**Examples:**

```rust
// BAD - restates what the code does
// Parse the file content
let parse = parse(db, content, metadata);

// BAD - obvious from function name
// Find the operation at the given index
let mut op_count = 0;

// GOOD - explains non-obvious behavior
// apollo-compiler errors don't have precise positions, so we use offset 0
errors.extend(with_errors.errors.iter().map(|e| ParseError {
    message: e.to_string(),
    offset: 0,
}));

// GOOD - documents a design decision
// Use empty tree/ast for main since callers use blocks via documents() iterator
let main_tree = apollo_parser::Parser::new("").parse();
```

### Working with This Project

**When adding validation:**

1. Check if it belongs in schema or document validation
2. Add as a Salsa query in `graphql-analysis`
3. Write tests showing the validation working
4. Update documentation if adding new concepts

**When adding lint rules:**

1. Determine the rule type (standalone/document/project)
2. Implement in `graphql-linter/src/rules/`
3. Add comprehensive tests
4. Document in linter README
5. Consider performance implications

**When fixing bugs:**

Use the `/bug-fix-workflow` skill which guides the two-commit structure (failing test first, then fix).

**After making changes:**

1. Build debug binary: `cargo build`
2. Run tests: `cargo test`
3. If LSP changes: Test with VSCode extension
4. If extension changes: Rebuild with `npm run compile`

### Operating Guidelines

Claude must follow these guidelines when working on this project:

1. **Always branch from `main`**: Unless explicitly told otherwise, create new branches from `main`
2. **Always open PRs against `main`**: Unless a different target branch is specified
3. **Always use the PR template**: When creating PRs with `gh pr create`, the template at `.github/PULL_REQUEST_TEMPLATE.md` will be used automatically
4. **Use descriptive branch names**: `feat/goto-definition`, `fix/fragment-resolution`, `docs/update-readme`
5. **Use conventional commits**: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`

### GitHub CLI (gh) Usage

**When running as a Claude web instance, always use the `gh` CLI instead of MCP GitHub tools.** MCP tools require permission approval dialogs that cannot be accepted in the web client, causing them to timeout.

The git remote also uses a local proxy that `gh` doesn't recognize as a GitHub host. **Always use the `--repo` flag** with all `gh` commands:

```bash
# Correct - always specify the repo explicitly
gh issue list --repo trevor-scheer/graphql-analyzer
gh issue view 123 --repo trevor-scheer/graphql-analyzer
gh pr view 123 --repo trevor-scheer/graphql-analyzer

# For pr create, also include --head to specify the branch
gh pr create --repo trevor-scheer/graphql-analyzer --head your-branch-name

# Incorrect - will fail with "none of the git remotes configured for this repository point to a known GitHub host"
gh issue list
gh pr create
```

### Branching and PRs

- **Default approach**: Create new branch from `main`, make changes, open PR against `main`
- Use descriptive branch names: `feat/goto-definition`, `fix/fragment-resolution`
- Target `main` unless specifically told otherwise (e.g., "open PR against lsp-rearchitecture branch")
- Follow PR guidelines above
- PRs will automatically use the template in `.github/PULL_REQUEST_TEMPLATE.md`

### Working with Git Worktrees

When starting work in a new git worktree:

```bash
cp -r /path/to/main/worktree/.claude /path/to/new/worktree/.claude
```

This preserves notes and local settings.

### Handling Uncertainty

**If unclear about approach:**

- Don't guess - ask the user for clarification
- Use the AskUserQuestion pattern if multiple approaches are valid
- Reference existing patterns in the codebase

**If code is ambiguous:**

- Read the README for that crate
- Look for tests showing intended usage
- Check design docs in `.claude/notes/active/`

### Updating This Document

- Suggest updates as the project evolves
- Call out when sections become outdated
- Add new sections for new patterns or concepts
- Keep the Quick Reference up to date

### Things to Never Do

- ❌ Don't manually edit `.github/workflows/release.yml` (auto-generated by cargo-dist)
- ❌ Don't add features not requested by the user
- ❌ Don't create markdown files unless explicitly asked
- ❌ Don't mention CI results in PR descriptions (tests passing, clippy clean, etc.) - CI enforces these, mentioning them is noise
- ❌ Don't use excessive emoji
- ❌ Don't commit without running `cargo fmt` and `cargo clippy`
- ❌ Don't remove TS/JS from VSCode extension's `documentSelector` - this breaks embedded GraphQL support (see [VSCode Extension Architecture](#vscode-extension-architecture-critical))
- ❌ Don't solve performance problems by removing core features - find smarter solutions (filtering, lazy evaluation, configuration options)

### Things to Always Do

- ✅ Read this file and relevant READMEs before starting
- ✅ Use skills for guided workflows (`/sme-consultation`, `/bug-fix-workflow`, `/create-pr`, `/adding-lint-rules`)
- ✅ Write tests for new functionality
- ✅ Update documentation when changing behavior
- ✅ Follow the existing code style and patterns
- ✅ Build the debug binary after changes
- ✅ Ask when uncertain about the correct approach

---

## Additional Resources

### Crate Documentation

Each crate has a detailed README:

- [graphql-db](../crates/graphql-db/README.md) - Salsa database layer
- [graphql-syntax](../crates/graphql-syntax/README.md) - Parsing layer
- [graphql-hir](../crates/graphql-hir/README.md) - Semantic layer
- [graphql-analysis](../crates/graphql-analysis/README.md) - Validation layer
- [graphql-ide](../crates/graphql-ide/README.md) - Editor API
- [graphql-linter](../crates/graphql-linter/README.md) - Linting engine
- [graphql-introspect](../crates/graphql-introspect/README.md) - Schema introspection

### Design Documents

- [Rearchitecture Overview](../.claude/notes/active/lsp-rearchitecture/README.md)
- [Phase 1: Foundation](../.claude/notes/active/lsp-rearchitecture/01-FOUNDATION.md)
- [Phase 2: Semantics](../.claude/notes/active/lsp-rearchitecture/02-SEMANTICS.md)
- [Phase 3: Analysis](../.claude/notes/active/lsp-rearchitecture/03-ANALYSIS.md)

### External Resources

- [Rust-Analyzer Architecture](https://rust-analyzer.github.io/book/contributing/architecture.html) - Inspiration for this project
- [Salsa Documentation](https://salsa-rs.github.io/salsa/) - Incremental computation framework
- [LSP Specification](https://microsoft.github.io/language-server-protocol/) - Protocol reference
- [GraphQL Specification](https://spec.graphql.org/) - Language reference

---

## Expert Agents

This project includes Subject Matter Expert (SME) agents in `.claude/agents/` that provide opinionated guidance on specific domains. These agents encourage proper API usage, enforce best practices, and propose solutions with tradeoffs.

### Available Agents

| Agent                        | File                  | Domain                                                            |
| ---------------------------- | --------------------- | ----------------------------------------------------------------- |
| **GraphQL Specification**    | `graphql.md`          | GraphQL spec compliance, validation rules, type system            |
| **Apollo Client**            | `apollo-client.md`    | Apollo Client patterns, caching, fragment colocation              |
| **rust-analyzer**            | `rust-analyzer.md`    | Query-based architecture, Salsa, incremental computation          |
| **Salsa**                    | `salsa.md`            | Salsa framework, database design, snapshot isolation, concurrency |
| **Rust**                     | `rust.md`             | Idiomatic Rust, ownership, error handling, API design             |
| **Language Server Protocol** | `lsp.md`              | LSP specification, protocol messages, client compatibility        |
| **GraphiQL**                 | `graphiql.md`         | IDE features, graphql-language-service, UX patterns               |
| **GraphQL CLI**              | `graphql-cli.md`      | CLI design, graphql-config, ecosystem tooling                     |
| **VSCode Extension**         | `vscode-extension.md` | Extension development, activation, language client                |
| **Apollo-rs**                | `apollo-rs.md`        | apollo-parser, apollo-compiler, error-tolerant parsing            |

### SME Consultation

Use the `/sme-consultation` skill when implementing features, fixing bugs, or making architecture changes. The skill provides:

- Work-type to agent mapping (which agents to consult for what)
- Documentation format for PRs and issue comments
- Guidance on how to document consulted agents

---

## Skills

Skills provide contextual guidance for common workflows. They activate automatically based on task description or can be invoked manually.

| Skill             | Slash Command        | When It Activates                                 |
| ----------------- | -------------------- | ------------------------------------------------- |
| SME Consultation  | `/sme-consultation`  | Feature work, bug fixes, architecture changes     |
| Adding Lint Rules | `/adding-lint-rules` | Implementing lint rules, adding validation        |
| Bug Fix Workflow  | `/bug-fix-workflow`  | Fixing bugs, addressing issues                    |
| Create PR         | `/create-pr`         | Opening PRs, preparing for review                 |
| Add IDE Feature   | `/add-ide-feature`   | Implementing LSP features (hover, goto def, etc.) |
| Debug LSP         | `/debug-lsp`         | Troubleshooting LSP server issues                 |
| Review PR         | `/review-pr`         | Reviewing pull requests                           |

Skills are located in `.claude/skills/` and are loaded into context when relevant.

---

**End of Guide**

This document is a living resource. Suggest improvements as you discover gaps or outdated information.
