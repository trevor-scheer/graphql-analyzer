# Changesets & Release Workflow

This directory contains changeset files that document changes for the changelog.

## Quick Start

```bash
# After making changes, create a changeset
knope document-change

# Or create manually (see format below)

# When ready to release
cargo xtask release --dry-run --skip-prepare  # Preview
cargo xtask release --skip-prepare --publish  # Build + GitHub release
```

## What is a Changeset?

A changeset is a Markdown file describing a change. Each changeset specifies:
- The type of version bump (major, minor, patch)
- A summary for the changelog

### Changeset Format

Create a file in `.changeset/` with any name ending in `.md`:

```markdown
---
default: minor
---

Add support for feature X
```

The frontmatter specifies the version bump type. The body becomes the changelog entry.

### Version Bump Types

- **major**: Breaking changes (API changes, removed features)
- **minor**: New features, significant improvements
- **patch**: Bug fixes, documentation updates

### When to Create a Changeset

**DO create changesets for:**
- New features
- Bug fixes
- Breaking changes
- Significant improvements

**DON'T create changesets for:**
- Internal refactoring (no behavior change)
- CI/CD changes
- Test-only changes
- Typo fixes

## Release Workflow

### Option 1: Full Manual Release (Recommended)

```bash
# 1. Preview what will happen
cargo xtask release --dry-run --skip-prepare

# 2. Build release artifacts
cargo xtask release --skip-prepare

# 3. Review artifacts
ls -la dist/

# 4. Commit and create GitHub release
git add -A && git commit -m "chore: release v0.1.0"
cargo xtask release --skip-prepare --publish
```

### Option 2: One-Step Release

```bash
# Build and publish in one step
cargo xtask release --skip-prepare --publish
```

### Option 3: With Version Bump (via Knope)

If you have changesets and want Knope to bump versions:

```bash
# Let knope process changesets and bump versions
cargo xtask release --publish
```

## Release Command Reference

```bash
cargo xtask release [OPTIONS]

Options:
  --skip-prepare  Skip knope prepare-release (use current versions)
  --tag           Create git tag after building
  --publish       Create GitHub release with artifacts (implies --tag)
  --dry-run       Show what would happen without doing it
```

### What the Release Command Does

1. **Prepare** (unless `--skip-prepare`): Runs `knope prepare-release` to bump versions from changesets
2. **Sync versions**: Updates VS Code extension version to match workspace
3. **Build binaries**: Builds `graphql` and `graphql-lsp` in release mode
4. **Package extension**: Compiles and packages VS Code extension (.vsix)
5. **Collect artifacts**: Copies everything to `dist/`
6. **Tag** (if `--tag` or `--publish`): Creates annotated git tag
7. **Publish** (if `--publish`): Pushes to remote and creates GitHub release

### Release Artifacts

After running the release command, `dist/` contains:

```
dist/
├── graphql                      # CLI binary
├── graphql-lsp                  # LSP server binary
└── graphql-lsp-{version}.vsix   # VS Code extension
```

## Versioning Strategy

This project uses **unified versioning**:

- All crates share the same version (workspace version in `Cargo.toml`)
- VS Code extension version is synced automatically
- Single `CHANGELOG.md` tracks all changes
- Git tags use format `v{version}` (e.g., `v0.1.0`)

## Tools

### Knope

[Knope](https://knope.tech) handles version management and changelog generation.

```bash
# Install
cargo install knope

# Create a changeset interactively
knope document-change

# Preview release (version bump + changelog)
knope prepare-release --dry-run
```

### GitHub CLI

The `gh` CLI is required for `--publish` to create GitHub releases.

```bash
# Install (if not already)
# macOS: brew install gh
# Linux: see https://cli.github.com/

# Authenticate
gh auth login
```

## Troubleshooting

### "knope not found"

Install Knope: `cargo install knope`

### "gh not found" or authentication errors

Install and authenticate the GitHub CLI:
```bash
gh auth login
```

### Version mismatch between Cargo.toml and package.json

The release command syncs these automatically. If manually fixing:
```bash
# Check versions
grep version Cargo.toml | head -1
grep version editors/vscode/package.json | head -1
```
