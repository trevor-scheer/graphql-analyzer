# GraphQL Analyzer - Claude Guide

**Last Updated**: March 2026

Context and guidance for Claude when working with this codebase.

> **Note**: This project is not stable yet. Breaking changes are expected - don't hesitate to rewrite code paths that aren't working well.

---

## Quick Reference

### Critical File Locations

| Location                         | Purpose                              |
| -------------------------------- | ------------------------------------ |
| `test-workspace/.graphqlrc.yaml` | Test project configuration           |
| `crates/*/src/`                  | Crate sources                        |
| `editors/vscode/`                | VS Code extension                    |
| `.claude/agents/`                | SME agents for consultation          |
| `.claude/skills/`                | Workflow guidance                    |
| `.claude/rules/`                 | Path-scoped context rules            |
| `.claude/hooks/`                 | Session, edit, and commit automation |
| `DEVELOPMENT.md`                 | Build, test, and debug commands      |

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

## Instructions for Claude

> Path-scoped rules in `.claude/rules/` provide context-specific guidance
> (Salsa patterns, LSP conventions, testing, linter patterns, etc.) that loads
> automatically when working in relevant directories.

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

1. Run checks: `cargo fmt && cargo clippy && cargo nextest run --workspace`
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

### GitHub CLI Usage

Always use `--repo` flag (git remote uses a local proxy):

```bash
gh pr create --repo trevor-scheer/graphql-analyzer --head branch-name
gh issue view 123 --repo trevor-scheer/graphql-analyzer
```

### Things to Never Do

> Some of these are enforced by hooks (PermissionRequest guard) and deny rules.

- Don't remove TS/JS from VS Code `documentSelector`
- Don't solve performance problems by removing features
- Don't mention CI status in PR descriptions
- Don't add features not requested
- Don't create markdown files unless asked
- Don't manually edit `.github/workflows/release.yml` (enforced by deny rule + hook)

### LSP Tool

This project has **rust-analyzer** (Rust) and **tsgo** (TypeScript) LSP plugins enabled via the `LSP` tool. The LSP tool is a deferred tool — you must load it with `ToolSearch` before first use each session.

**Discovery step (do this early in each session):**

```
ToolSearch query: "select:LSP"
```

Then call `LSP` with any operation to see all available commands. The tool supports: `goToDefinition`, `findReferences`, `hover`, `documentSymbol`, `workspaceSymbol`, `goToImplementation`, `prepareCallHierarchy`, `incomingCalls`, `outgoingCalls`.

**Prefer the LSP tool over Grep/Glob for code navigation:**

- **Finding definitions** → `LSP goToDefinition` instead of grepping for `fn foo` or `function foo`
- **Finding usages** → `LSP findReferences` instead of grepping for a symbol name
- **Understanding types** → `LSP hover` instead of reading surrounding code
- **Exploring a file's structure** → `LSP documentSymbol` instead of skimming the file
- **Finding implementations** → `LSP goToImplementation` instead of grepping for `impl`
- **Understanding call graphs** → `LSP incomingCalls`/`outgoingCalls` instead of manual tracing

**When to use LSP vs other tools:**

| Task                                 | Use                  | Not                   |
| ------------------------------------ | -------------------- | --------------------- |
| "What type is this variable?"        | `LSP hover`          | Reading code context  |
| "Where is this defined?"             | `LSP goToDefinition` | `Grep` for the name   |
| "Who calls this function?"           | `LSP incomingCalls`  | `Grep` for the name   |
| "What's in this file?"               | `LSP documentSymbol` | `Read` entire file    |
| "Find all usages before refactoring" | `LSP findReferences` | `Grep` for the name   |
| "Search for a string/pattern"        | `Grep`               | LSP (not text search) |
| "Find files by name"                 | `Glob`               | LSP (not file search) |

**After editing Rust files**, check `mcp__ide__getDiagnostics` for compiler errors before running `cargo build` or `cargo check`. This is faster and catches issues immediately. Reserve `cargo build`/`cargo check`/`cargo clippy` for final validation before commits.

### Lint Rule Test Workspace

The `test-workspace/lint-examples/` project demonstrates all lint rules in action. When adding, removing, or changing lint rules:

1. **Adding a rule**: Create a new `.graphql` file in `schema/` (for schema rules) or `src/` (for document/operation rules) named after the rule (e.g., `no-typename-prefix.graphql`). The file should contain GraphQL that triggers the rule.
2. **Removing a rule**: Delete the corresponding demo file and remove the rule from the `lint-examples` project in `.graphqlrc.yaml`.
3. **Changing a rule**: Update the corresponding demo file to reflect the new behavior, and update the rule config in `.graphqlrc.yaml` if options changed.
4. **Config**: The `lint-examples` project in `.graphqlrc.yaml` must list all lint rules explicitly (no `extends: recommended`).

### Things to Always Do

- Read relevant READMEs before starting
- Use skills for guided workflows
- Load and use the `LSP` tool (via `ToolSearch`) for code navigation — prefer it over Grep/Glob for definitions, references, hover, and call hierarchy
- Write tests for new functionality
- Create changesets for user-facing changes
- Build and test after changes
- Update `test-workspace/lint-examples/` when lint rules change
- Ask when uncertain

---

## Expert Agents

SME agents in `.claude/agents/` provide domain guidance. Use `/sme-consultation` skill.
All agents have YAML frontmatter configuring model, tools, and turn limits.

| Agent                 | Domain                                        |
| --------------------- | --------------------------------------------- |
| `graphql.md`          | GraphQL spec, validation rules                |
| `rust-analyzer.md`    | Query-based architecture, Salsa patterns      |
| `salsa.md`            | Salsa framework, database design              |
| `lsp.md`              | LSP specification, protocol messages          |
| `apollo-rs.md`        | apollo-parser, apollo-compiler                |
| `rust.md`             | Rust language, ownership, idioms              |
| `apollo-client.md`    | Apollo Client patterns, fragment organization |
| `graphql-cli.md`      | CLI tools, graphql-config, ecosystem          |
| `graphiql.md`         | GraphiQL IDE, graphql-language-service        |
| `vscode-extension.md` | VS Code extension development                 |
| `playwright.md`       | E2E testing, Playwright, Electron             |

---

## Skills

Skills have YAML frontmatter with `allowed-tools` (auto-permissions) and `argument-hint` (autocomplete).

| Skill                | When to Use                        |
| -------------------- | ---------------------------------- |
| `/sme-consultation`  | Feature work, architecture changes |
| `/adding-lint-rules` | Implementing lint rules            |
| `/bug-fix-workflow`  | Fixing bugs (test-first)           |
| `/create-pr`         | Opening pull requests              |
| `/add-ide-feature`   | LSP features (hover, goto def)     |
| `/debug-lsp`         | Troubleshooting LSP issues         |
| `/review-pr`         | Reviewing pull requests            |
| `/audit-tests`       | Self-review test organization      |
| `/testing-patterns`  | Test infrastructure reference      |
