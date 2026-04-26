# Release Process

This document describes how graphql-analyzer's components are versioned,
released, and distributed. Automation is driven by [Knope](https://knope.tech)
and the `.github/workflows/release.yml` workflow.

## Released components

| Package                          | Artifact kind                             | Starting version |
| -------------------------------- | ----------------------------------------- | ---------------- |
| `graphql-analyzer-cli`           | Rust binary (GH Release)                  | independent      |
| `graphql-analyzer-lsp`           | Rust binary + VS Code extension (coupled) | independent      |
| `graphql-analyzer-mcp`           | Rust binary (GH Release)                  | independent      |
| `graphql-analyzer-core`          | npm packages (6) — native addon           | 0.1.0-alpha.x    |
| `graphql-analyzer-eslint-plugin` | npm package                               | 0.1.0-alpha.x    |

Each package is listed in `knope.toml` with its `versioned_files` and
changelog location. Changesets target these package names.

### Version coupling

Two packages bundle multiple files under one version:

- **`graphql-analyzer-lsp`** versions `crates/lsp/Cargo.toml` and
  `editors/vscode/package.json` together. The VS Code extension ships with
  the LSP binary bundled, so they're one user-facing unit.
- **`graphql-analyzer-core`** versions the Rust crate, the npm dispatcher
  (`@graphql-analyzer/core`), and five platform stubs
  (`@graphql-analyzer/core-<triple>`) together. npm's `optionalDependencies`
  resolution requires exact-version pins between them, so they move in lockstep.

## Alpha phase

`knope.toml` sets `prerelease_label = "alpha"`. All npm releases are tagged
`alpha` on the registry; consumers opt in via `npm install @graphql-analyzer/foo@alpha`.

Graduating out of alpha:

1. Remove `prerelease_label = "alpha"` from `knope.toml`.
2. Drop `--tag alpha` from the npm publish steps in `release.yml`.

## Supported platforms

Rust binaries (CLI, LSP, MCP) and the `@graphql-analyzer/core` native addon build for:

- macOS — `aarch64-apple-darwin`, `x86_64-apple-darwin`
- Linux — `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`
- Windows — `x86_64-pc-windows-msvc`

The VS Code extension is packaged once per target (`darwin-arm64`, `darwin-x64`,
`linux-arm64`, `linux-x64`, `win32-x64`), bundling the matching LSP binary.

## Release flow

```
  PR authors                         CI                                 npm / GH
  ───────────                        ──                                 ────────
  knope document-change ──►  .changeset/*.md
                                      │
                                      ▼
                              merge to main
                                      │
                                      ▼
                    prepare-release.yml (knope)
                     • consume changesets
                     • bump versioned_files
                     • write CHANGELOG.md
                     • open "release/next" PR
                                      │
                                      ▼
                              merge release PR
                                      │
                                      ▼
                        release.yml (on main)
                     • build-binaries (Rust, 5 targets)
                     • build-vscode (per-platform .vsix)
                     • build-core (native addon, 5 targets)
                     • knope release
                       • create GH releases with CHANGELOGs
                       • upload binary + .vsix artifacts ───► GitHub Releases
                     • publish-vscode ───────────────────────► VS Code Marketplace
                     • publish-openvsx ──────────────────────► Open VSX Registry
                     • publish-npm
                       • platform stubs (5) ─────────────────► npm
                       • @graphql-analyzer/core ─────────────► npm
                       • @graphql-analyzer/eslint-plugin ────► npm
                     • update-homebrew-formula ──────────────► homebrew-tap
```

## Authoring changesets

```sh
knope document-change
```

Or create the file manually in `.changeset/`. Each changeset targets one or
more of the knope package names:

```markdown
---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: minor
---

Short description of the user-facing change. ([#123](https://github.com/trevor-scheer/graphql-analyzer/pull/123))
```

### Which package to target

| Change                                      | Target package                   |
| ------------------------------------------- | -------------------------------- |
| CLI feature or bug fix                      | `graphql-analyzer-cli`           |
| LSP or VS Code extension change             | `graphql-analyzer-lsp`           |
| MCP server change                           | `graphql-analyzer-mcp`           |
| Native addon — Rust side or any npm package | `graphql-analyzer-core`          |
| ESLint plugin (JS-only change)              | `graphql-analyzer-eslint-plugin` |
| New lint rule                               | whichever consumers are affected |

Shared crate changes (e.g., `graphql-linter`, `graphql-analysis`) don't have
their own changeset package — they're internal. Target whichever released
component(s) the change reaches users through.

## npm publishing auth

The workflow uses npm [Trusted Publishing](https://docs.npmjs.com/trusted-publishers)
(OIDC) exclusively — no `NPM_TOKEN` secret lives in CI. Each of the seven
`@graphql-analyzer/*` package names must be bound on npmjs.com to:

- Repository: `trevor-scheer/graphql-analyzer`
- Workflow: `.github/workflows/release.yml`
- Environment: _none_
- Job: `publish-npm`

The `publish-npm` job sets `permissions: id-token: write` and runs
`npm publish --provenance --access public --tag alpha` for every package. npm
verifies the OIDC token against the registered binding before accepting the
publish.

### Publish ordering and idempotency

The `publish-npm` job runs publishes in a fixed order: 5 platform stubs →
`@graphql-analyzer/core` dispatcher → `@graphql-analyzer/eslint-plugin`. The
order is mandatory — the dispatcher pins each platform stub via exact-version
`optionalDependencies`, and the eslint-plugin pins the dispatcher via a normal
exact-version dependency.

Each step short-circuits via `npm view <name>@<version>` if that version is
already on the registry, so a re-run of a partially-completed job picks up where
the failure left off.

## Testing a release locally

```sh
# Dry-run knope's prepare-release (shows what would be bumped and written)
knope --dry-run prepare-release

# Validate knope.toml syntax
knope --validate

# Build the native addon locally (debug)
npm run build:debug --workspace=@graphql-analyzer/core

# Build the ESLint plugin
npm run build --workspace=@graphql-analyzer/eslint-plugin
```

## Installation methods

### CLI

**macOS / Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.sh | sh
```

**Windows:**

```powershell
irm https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.ps1 | iex
```

**Homebrew:**

```sh
brew install trevor-scheer/tap/graphql-cli
```

**From source:**

```sh
cargo install --git https://github.com/trevor-scheer/graphql-analyzer graphql-cli
```

### LSP (standalone, for non-VS Code editors)

```sh
curl -fsSL https://raw.githubusercontent.com/trevor-scheer/graphql-analyzer/main/scripts/install.sh | sh -s -- lsp
```

### VS Code extension

Install from the Marketplace (`graphql-analyzer.graphql-analyzer`) or Open VSX.

### ESLint plugin

```sh
npm install --save-dev @graphql-analyzer/eslint-plugin@alpha
```

## Troubleshooting

### knope `--validate` fails with "inconsistent versions"

All files listed under a single `versioned_files` must already share a
version before the next release. If they drift, hand-align them in a
preparatory commit.

### npm publish fails partway through

Re-run the failed `publish-npm` job from the GitHub Actions UI. Each publish
step checks `npm view <name>@<version>` first and skips packages already on the
registry, so a re-run finishes whatever didn't complete the first time.

If a stub fails mid-stream and the dispatcher publishes anyway (it shouldn't —
each step is sequential and `set -euo pipefail`), users on the affected
platform will see `Cannot find module @graphql-analyzer/core-<triple>` at
install time. Recover by re-running the workflow once the underlying issue is
fixed.

### optionalDependencies resolve incorrectly

All six `@graphql-analyzer/core*` packages must share an exact version, and the
platform stubs must be published _before_ the dispatcher. The `publish-npm` job
enforces this ordering, and `scripts/sync-workspace-deps.mjs` (run by knope
during prepare-release) keeps the version pins in lockstep.
