# GraphQL LSP Project Guide

**Last Updated**: January 2026

> **Note**: This project is not stable yet. The codebase can be aggressively refactored and rearchitected as needed. Breaking changes are expected.

---

## Quick Reference

### Commands

```bash
# Build & Test
cargo build                     # Build all crates
cargo test                      # Run all tests
cargo clippy                    # Lint checks
cargo fmt                       # Format code

# CLI
target/debug/graphql validate   # Validate GraphQL files
target/debug/graphql lint       # Lint GraphQL files

# LSP
RUST_LOG=debug target/debug/graphql-lsp  # Run with logging

# VSCode Extension
cargo xtask install             # Build LSP + install extension (debug)
cargo xtask install --release   # Release build
```

### Key Locations

| What | Where |
|------|-------|
| Crate sources | `crates/*/src/` |
| Integration tests | `crates/*/tests/` |
| VSCode extension | `editors/vscode/` |
| SME agents | `.claude/agents/` |
| Design docs | `.claude/notes/active/lsp-rearchitecture/` |

### Quick Answers

- **Add lint rule?** `crates/graphql-linter/src/rules/` - use `/adding-lint-rules`
- **Add validation?** `crates/graphql-analysis/src/` - see crate README
- **Add IDE feature?** Use `/add-ide-feature` skill
- **Schema loading?** `graphql-introspect` (remote), `graphql-config` (config parsing)

---

## Project Overview

GraphQL LSP implementation in Rust with IDE features for `.graphql` files and embedded GraphQL in TypeScript/JavaScript.

### What Makes This Unique

1. **Query-Based Architecture**: [Salsa](https://github.com/salsa-rs/salsa) for automatic incremental computation
2. **Multi-Language Support**: Pure `.graphql` and embedded GraphQL in TS/JS
3. **Project-Wide Analysis**: Cross-file fragment resolution and validation
4. **Extensible Linting**: Plugin-based with tool-specific configuration

### Protected Core Features

These features must NOT be removed or degraded:

| Feature | What Enables It |
|---------|-----------------|
| Embedded GraphQL in TS/JS | `documentSelector` includes TS/JS in VSCode extension |
| Real-time diagnostics | LSP `textDocument/didChange` notifications |
| Project-wide fragment resolution | `all_fragments()` query indexes entire project |

**Performance concerns must be solved without removing features** - use filtering, lazy evaluation, or configuration options instead.

---

## Architecture

```
graphql-lsp (LSP Protocol)
    ↓
graphql-ide (Editor API, POD types, AnalysisHost)
    ↓
graphql-analysis (Validation & Linting)
    ↓
graphql-hir (High-level IR, structure/body separation)
    ↓
graphql-syntax (Parsing, LineIndex, TS/JS extraction)
    ↓
graphql-db (Salsa Database, FileId, FileContent)
```

**Supporting crates**: graphql-config, graphql-extract, graphql-introspect, graphql-linter, graphql-cli

See design docs in `.claude/notes/active/lsp-rearchitecture/` for details.

---

## Key Concepts

### GraphQL Document Model

- **Schema Document**: Type definitions, directives, schema extensions
- **Executable Document**: Operations and/or fragments (fragment-only documents are valid)

**Fragments have project-wide scope** - all fragments globally available, operations can reference fragments from other files. Validation must recursively include all transitive fragment dependencies.

### The Golden Invariant

> **"Editing a document's body never invalidates global schema knowledge"**

- **Structure** (stable): Type names, field signatures, operation/fragment names
- **Bodies** (dynamic): Selection sets, field selections, directives

### VSCode Extension Architecture (Critical)

**`documentSelector`** controls which files get LSP features (diagnostics, hover, goto def). Without TS/JS in documentSelector, those languages get NO LSP features.

**Grammar injection** only provides syntax highlighting - purely visual, no semantic understanding.

**File watcher** only fires on disk saves, not real-time edits.

**Never remove TS/JS from documentSelector** - this would break embedded GraphQL support entirely.

---

## Configuration

### .graphqlrc.yaml

```yaml
schema: schema.graphql
documents: "src/**/*.graphql"
lint: recommended  # Or: { extends: recommended, rules: { no_deprecated: warn } }

# Tool-specific overrides
extensions:
  lsp:
    lint:
      rules:
        unused_fields: off
```

### Multi-Project

```yaml
projects:
  frontend:
    schema: frontend/schema.graphql
    documents: "frontend/**/*.graphql"
```

CLI requires `--project` flag for multi-project configs.

---

## Testing

Use `/testing-patterns` skill for detailed guidance on test organization and infrastructure.

**Quick reference:**
- Unit tests: `src/*.rs` inline, ONE Salsa query, local TestDatabase
- Integration tests: `crates/*/tests/`, multiple queries, shared TestDatabase
- Caching tests: Use `TrackedDatabase` from graphql-test-utils

Use `/audit-tests` after writing tests to self-review.

---

## Common Tasks

| Task | Guidance |
|------|----------|
| Add lint rule | `/adding-lint-rules` skill |
| Add IDE feature | `/add-ide-feature` skill |
| Fix a bug | `/bug-fix-workflow` skill |
| Debug LSP | `/debug-lsp` skill |
| Create PR | `/create-pr` skill |
| Write tests | `/testing-patterns` skill |

---

## Troubleshooting

Use `/debug-lsp` skill for detailed debugging guidance including logging, OpenTelemetry, and common issues.

**Quick fixes:**
- "No project found": Check `.graphqlrc.yaml` exists, use `--project` flag for multi-project
- LSP not responding: Rebuild (`cargo build`), check VSCode Output > GraphQL
- Fragment not found: Ensure file registered in `document_files()`, check `all_fragments()`

---

## Instructions for Claude

### General Approach

1. **Read before acting**: Check relevant README.md files before starting
2. **Consult SME agents**: Use `/sme-consultation` skill for feature work
3. **Follow patterns**: Study existing code in the same layer
4. **Test incrementally**: Write tests as you go

### Code Style

- No emoji in code or commits
- Follow Rust conventions: `snake_case` functions, `CamelCase` types
- Keep lines under 100 characters

**Comments**: Only add comments explaining WHY (non-obvious behavior, edge cases, safety invariants). Don't add comments restating what code does.

### Operating Guidelines

- Branch from `main`, open PRs against `main`
- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`
- Always use `--repo trevor-scheer/graphql-lsp` with `gh` commands

### Things to Never Do

- Don't manually edit `.github/workflows/release.yml` (auto-generated)
- Don't add features not requested
- Don't mention CI results in PR descriptions
- Don't remove TS/JS from VSCode extension's `documentSelector`
- Don't solve performance problems by removing core features

### Things to Always Do

- Use skills for guided workflows
- Write tests for new functionality
- Build debug binary after changes (`cargo build`)
- Ask when uncertain

---

## Skills

| Skill | When to Use |
|-------|-------------|
| `/sme-consultation` | Feature work, bug fixes, architecture changes |
| `/adding-lint-rules` | Implementing lint rules |
| `/bug-fix-workflow` | Fixing bugs (two-commit structure) |
| `/create-pr` | Opening PRs |
| `/add-ide-feature` | LSP features (hover, goto def, etc.) |
| `/debug-lsp` | Troubleshooting LSP issues |
| `/review-pr` | Reviewing pull requests |
| `/testing-patterns` | Writing tests, choosing unit vs integration |
| `/audit-tests` | **Proactive**: Self-review after writing tests |

---

## Resources

**Crate READMEs**: Each crate in `crates/` has detailed documentation.

**SME Agents**: See `.claude/agents/` for domain experts (GraphQL spec, Salsa, rust-analyzer, LSP, etc.)

**External**:
- [Rust-Analyzer Architecture](https://rust-analyzer.github.io/book/contributing/architecture.html)
- [Salsa Documentation](https://salsa-rs.github.io/salsa/)
- [LSP Specification](https://microsoft.github.io/language-server-protocol/)
- [GraphQL Specification](https://spec.graphql.org/)
