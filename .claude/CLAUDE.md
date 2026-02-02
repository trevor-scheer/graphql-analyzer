# GraphQL Analyzer - Claude Guide

**Last Updated**: February 2026

Context and guidance for Claude when working with this codebase.

> **Note**: This project is not stable yet. Breaking changes are expected - don't hesitate to rewrite code paths that aren't working well.

---

## Quick Reference

### Critical File Locations

| Location          | Purpose                         |
| ----------------- | ------------------------------- |
| `.graphqlrc.yaml` | Project configuration           |
| `crates/*/src/`   | Crate sources                   |
| `editors/vscode/` | VS Code extension               |
| `.claude/agents/` | SME agents for consultation     |
| `.claude/skills/` | Workflow guidance               |
| `DEVELOPMENT.md`  | Build, test, and debug commands |

### Quick Answers

| Question                   | Answer                                                      |
| -------------------------- | ----------------------------------------------------------- |
| Where to add a lint rule?  | `crates/linter/src/rules/` - use `/adding-lint-rules` skill |
| Where to add validation?   | `crates/analysis/src/`                                      |
| Where's schema loading?    | `crates/introspect/` (remote), `crates/config/` (local)     |
| How does incremental work? | Salsa queries: `base-db` → `syntax` → `hir` → `analysis`    |

---

## Project Overview

A GraphQL LSP implementation in Rust providing IDE features for `.graphql` files and embedded GraphQL in TypeScript/JavaScript.

**Key characteristics:**

- Query-based architecture using [Salsa](https://github.com/salsa-rs/salsa) for incremental computation
- Project-wide analysis with proper fragment resolution across files
- Remote schema support via introspection

### Protected Core Features

These features must NOT be removed or degraded:

| Feature                       | Why Critical                      | What Enables It                                        |
| ----------------------------- | --------------------------------- | ------------------------------------------------------ |
| **Embedded GraphQL in TS/JS** | Most users write queries in TS/JS | `documentSelector` includes TS/JS in VS Code extension |
| **Real-time diagnostics**     | Users expect immediate feedback   | LSP `textDocument/didChange` notifications             |
| **Project-wide fragments**    | Fragments span many files         | `all_fragments()` indexes entire project               |

**Solve performance problems without removing features.** Use filtering, lazy evaluation, debouncing, or configuration options instead.

---

## Architecture

```
graphql-lsp / graphql-cli / graphql-mcp
    ↓
graphql-ide (Editor API, POD types)
    ↓
graphql-analysis (Validation + Linting)
    ↓
graphql-hir (Semantic layer, structure/body separation)
    ↓
graphql-syntax (Parsing, TS/JS extraction)
    ↓
graphql-db (Salsa database, FileId, memoization)
```

See `DEVELOPMENT.md` for project structure and detailed architecture.

---

## Key Concepts

### GraphQL Document Model

**Fragment scope is project-wide**, not file-scoped:

- Operations can reference fragments in other files
- Fragment spreads can reference other fragments (transitive dependencies)
- Fragment and operation names must be unique across the entire project

**When validating operations**, you MUST:

1. Include direct fragment dependencies
2. Recurse through fragment dependencies
3. Handle circular references
4. Validate against schema for all fragments in the chain

### Cache Invariants

The Salsa architecture relies on these invariants for incremental computation:

| Invariant                     | Meaning                                                       |
| ----------------------------- | ------------------------------------------------------------- |
| **Structure/Body separation** | Editing body content never invalidates structure queries      |
| **File isolation**            | Editing file A never invalidates unrelated queries for file B |
| **Index stability**           | Global indexes stay cached when edits don't change names      |
| **Lazy evaluation**           | Body queries only run when results are needed                 |

**Structure** = identity (names, types). **Body** = content (selection sets, directives).

### VS Code Extension Architecture

The extension has three separate systems - don't confuse them:

| System             | Purpose                                     | Scope                                            |
| ------------------ | ------------------------------------------- | ------------------------------------------------ |
| `documentSelector` | LSP features (diagnostics, hover, goto def) | Controls which files get IDE features            |
| Grammar injection  | Syntax highlighting only                    | Visual coloring, no semantic understanding       |
| File watcher       | Disk events only                            | File create/delete/rename, NOT real-time editing |

**Common mistake:** Thinking grammar injection provides embedded GraphQL support. It only provides colors. The `documentSelector` MUST include TS/JS for actual LSP features.

---

## Configuration

```yaml
# .graphqlrc.yaml
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"

# Lint config uses extensions.lint with camelCase rule names
extensions:
  lint: recommended # Happy path - just use preset
```

See `crates/config/README.md` for multi-project and advanced configuration.

---

## Common Tasks

| Task               | Guidance                                            |
| ------------------ | --------------------------------------------------- |
| Add a lint rule    | Use `/adding-lint-rules` skill                      |
| Add an IDE feature | Use `/add-ide-feature` skill                        |
| Fix a bug          | Use `/bug-fix-workflow` skill (test-first approach) |
| Create a PR        | Use `/create-pr` skill                              |
| Debug LSP          | Use `/debug-lsp` skill                              |

For build, test, and debug commands, see `DEVELOPMENT.md`.

---

## Troubleshooting

### "No project found" Error

Ensure `.graphqlrc.yaml` exists. For multi-project configs, use `--project` flag.

### LSP Not Responding

1. Rebuild: `cargo build`
2. Check VS Code logs: View → Output → GraphQL
3. Enable debug logging: `RUST_LOG=debug`

### Fragment Not Found Errors

- Ensure fragment file is in `document_files()`
- Check `all_fragments()` includes the file
- Verify fragment name uniqueness

---

## Instructions for Claude

### Pre-Task Skill Check (REQUIRED)

| Task Type                 | Skill to Use         |
| ------------------------- | -------------------- |
| Fixing a bug              | `/bug-fix-workflow`  |
| Adding a lint rule        | `/adding-lint-rules` |
| Adding an IDE feature     | `/add-ide-feature`   |
| Creating a PR             | `/create-pr`         |
| Feature/architecture work | `/sme-consultation`  |

Skills enforce important workflows. Skipping them leads to incomplete work.

### Creating Pull Requests

**Before opening a PR:**

1. Run checks: `cargo fmt && cargo clippy && cargo test`
2. **Create a changeset** for user-facing changes:
   ```bash
   knope document-change
   ```
3. Review changes: `git diff main...HEAD`
4. Use the `/create-pr` skill for guidance

**When to create a changeset:**

- Features, bug fixes, breaking changes → YES
- Internal refactoring, CI changes, test-only → NO

### Code Style

- No emoji in code or commits
- Follow Rust conventions: `snake_case` functions, `CamelCase` types
- Comments explain **why**, not **what**

### GitHub CLI Usage

Always use `--repo` flag (git remote uses a local proxy):

```bash
gh pr create --repo trevor-scheer/graphql-analyzer --head branch-name
gh issue view 123 --repo trevor-scheer/graphql-analyzer
```

### Things to Never Do

- Don't remove TS/JS from VS Code `documentSelector`
- Don't solve performance problems by removing features
- Don't mention CI status in PR descriptions
- Don't add features not requested
- Don't create markdown files unless asked
- Don't manually edit `.github/workflows/release.yml`

### Things to Always Do

- Read relevant READMEs before starting
- Use skills for guided workflows
- Write tests for new functionality
- Create changesets for user-facing changes
- Build and test after changes
- Ask when uncertain

---

## Expert Agents

SME agents in `.claude/agents/` provide domain guidance. Use `/sme-consultation` skill.

| Agent                 | Domain                                   |
| --------------------- | ---------------------------------------- |
| `graphql.md`          | GraphQL spec, validation rules           |
| `rust-analyzer.md`    | Query-based architecture, Salsa patterns |
| `salsa.md`            | Salsa framework, database design         |
| `lsp.md`              | LSP specification, protocol messages     |
| `apollo-rs.md`        | apollo-parser, apollo-compiler           |
| `vscode-extension.md` | Extension development                    |

---

## Skills

| Skill                | When to Use                        |
| -------------------- | ---------------------------------- |
| `/sme-consultation`  | Feature work, architecture changes |
| `/adding-lint-rules` | Implementing lint rules            |
| `/bug-fix-workflow`  | Fixing bugs (test-first)           |
| `/create-pr`         | Opening pull requests              |
| `/add-ide-feature`   | LSP features (hover, goto def)     |
| `/debug-lsp`         | Troubleshooting LSP issues         |
| `/review-pr`         | Reviewing pull requests            |

---

**End of Guide**
