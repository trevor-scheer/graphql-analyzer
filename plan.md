# Docs Site Plan: graphql-analyzer

## Tooling

**Static site generator:** [Starlight](https://starlight.astro.build/) (Astro-based)

- Purpose-built for documentation sites
- Ships to GitHub Pages with zero config
- Built-in search, sidebar, dark mode, versioning support
- Markdown/MDX authoring

**Hosting:** GitHub Pages via `gh-pages` branch, deployed from GitHub Actions.

**Source location:** `docs/` at repo root.

---

## Information Architecture

### Top-level navigation (header)

| Label  | Description                          |
| ------ | ------------------------------------ |
| Docs   | Main documentation (default landing) |
| Rules  | Lint rule catalog                    |
| Blog   | Release notes, announcements         |
| GitHub | Link to repo                         |

---

### Sidebar structure (Docs section)

```
Getting Started
├── Introduction                    # What is graphql-analyzer, why use it
├── Quick Start                     # 3-minute zero-to-working setup
└── Installation                    # All install methods (CLI, LSP, VS Code, cargo)

Editor Setup
├── VS Code                         # Extension install, settings, commands
├── Neovim                           # nvim-lspconfig setup
└── Other Editors                    # Generic LSP client instructions

Configuration
├── Configuration File               # .graphqlrc.yaml format, discovery, examples
├── Schema Sources                   # Local files, globs, remote introspection
├── Documents                        # File matching, TS/JS extraction, tag config
├── Multi-Project Workspaces         # projects: key, per-project overrides
└── Tool-Specific Overrides          # extensions.lsp vs extensions.cli

CLI
├── Overview                         # validate, lint, check — when to use which
├── validate                         # Command reference, flags, exit codes
├── lint                             # Command reference, flags
├── check                            # Combined command
├── Output Formats                   # human, json, github annotations
└── CI/CD Integration                # GitHub Actions, GitLab CI, generic examples

IDE Features
├── Diagnostics                      # Real-time validation + linting
├── Go to Definition                 # What resolves where (fragments, types, fields, etc.)
├── Find References                  # Reverse lookups
├── Hover                            # Type info, deprecation, descriptions
└── Embedded GraphQL                 # TS/JS support, tagged templates, position mapping

Linting
├── Overview                         # How linting works, presets, severity levels
├── Configuration                    # recommended preset, per-rule config, rule options
└── Custom Rules (future)            # Extension point (when public API stabilizes)

AI Integration
├── MCP Server                       # Setup for Claude Desktop, available tools
└── Claude Code Plugin               # Plugin setup and usage

Advanced
├── Remote Schema Introspection      # Headers, auth, caching
├── Performance Tuning               # LSP vs CLI rule budgets, structure/body caching
└── Troubleshooting                  # Common issues, debug logging, OpenTelemetry
```

---

### Rules section (separate top-level)

A catalog page per lint rule, each with:

- Description and rationale
- Default severity
- Good/bad code examples
- Configuration options
- Which context it runs in (document, document-schema, project)

```
Rules
├── Overview / Catalog Table          # Sortable/filterable table of all rules
├── no-anonymous-operations
├── no-deprecated
├── redundant-fields
├── unused-fragments
├── unused-fields
├── unique-names
├── require-id-field
├── operation-name-suffix
└── unused-variables
```

---

### Blog section

- Release notes (auto-generated or manual per release)
- Future: architecture deep-dives, migration guides

---

## Page priorities (what to write first)

| Priority | Pages                                   | Rationale                   |
| -------- | --------------------------------------- | --------------------------- |
| P0       | Introduction, Quick Start, Installation | First-visit experience      |
| P0       | Configuration File, Schema Sources      | Required for any usage      |
| P0       | CLI Overview + check/validate/lint      | Primary CI/CD use case      |
| P0       | VS Code setup                           | Primary editor              |
| P1       | All lint rule pages                     | High reference value        |
| P1       | CI/CD Integration                       | Key adoption driver         |
| P1       | IDE Features (all)                      | Showcases value proposition |
| P1       | Multi-Project Workspaces                | Common real-world setup     |
| P2       | Neovim, Other Editors                   | Smaller audience            |
| P2       | MCP Server, Claude Code Plugin          | Emerging use case           |
| P2       | Advanced section (all)                  | Power users                 |
| P3       | Blog, Custom Rules                      | Nice to have                |

---

## Landing page structure

Hero section:

- Tagline: "Fast, Rust-powered GraphQL tooling for your editor and CI"
- Three feature cards: **IDE** / **CLI** / **AI**
- Install one-liner + "Get Started" CTA

Below the fold:

- Feature highlights with screenshots/code blocks
- "Works with" logos (VS Code, Neovim, GitHub Actions, Claude)

---

## Open questions

1. **Custom domain?** `graphql-analyzer.dev` vs `trevor-scheer.github.io/graphql-analyzer`
2. **Search:** Starlight built-in (Pagefind) should be sufficient — or Algolia?
3. **API reference?** `cargo doc` output linked/embedded, or skip for now?
4. **Versioning:** Single version for now (pre-1.0), add versioned docs later?
