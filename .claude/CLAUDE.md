# GraphQL Analyzer - Claude Guide

**Last Updated**: March 2026

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

### Architecture

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
See `crates/CLAUDE.md` for crate architecture details, key concepts, and cache invariants.

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

**Changeset format:** Always include a PR link at the end of the first line:

```markdown
---
graphql-analyzer-cli: patch
---

Fix argument parsing bug ([#123](https://github.com/trevor-scheer/graphql-analyzer/pull/123))
```

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

### rust-analyzer LSP

The rust-analyzer LSP plugin is enabled for this project. Use it proactively:

- **After editing Rust files**, check `mcp__ide__getDiagnostics` for compiler errors before running `cargo build` or `cargo check`. This is faster and catches issues immediately.
- **When exploring unfamiliar code**, use `mcp__ide__getHoverInfo` to inspect types and signatures, and `mcp__ide__getDefinition` to jump to definitions.
- **When refactoring**, use `mcp__ide__getReferences` to find all usages before renaming or modifying code.
- **Prefer LSP tools over cargo commands** for quick feedback during iterative editing. Reserve `cargo build`/`cargo check`/`cargo clippy` for final validation before commits.

### Things to Always Do

- Read relevant READMEs before starting
- Use skills for guided workflows
- Use rust-analyzer LSP tools for fast Rust feedback during editing
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
