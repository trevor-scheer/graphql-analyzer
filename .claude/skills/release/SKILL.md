---
name: release
description: Build and publish releases with artifacts. Use when releasing a new version, creating GitHub releases, or running cargo xtask release.
user-invocable: true
---

# Release Workflow

This skill guides you through releasing a new version of the graphql-lsp project.

## Quick Reference

```bash
# Preview what will happen
cargo xtask release --dry-run --skip-prepare

# Build artifacts only (no publish)
cargo xtask release --skip-prepare

# Build and publish to GitHub
cargo xtask release --skip-prepare --publish
```

## Release Types

### 1. Manual Release (Current Version)

Use when releasing the current version without bumping:

```bash
# 1. Build and review
cargo xtask release --skip-prepare
ls -la dist/

# 2. Commit any changes
git add -A && git commit -m "chore: release v0.1.0"

# 3. Publish to GitHub
cargo xtask release --skip-prepare --publish
```

### 2. Changeset-Based Release

Use when you have changesets that should bump the version:

```bash
# 1. Preview version bump
knope prepare-release --dry-run

# 2. Build with version bump
cargo xtask release

# 3. Review and publish
cargo xtask release --publish
```

## Command Options

```bash
cargo xtask release [OPTIONS]

  --skip-prepare  Skip knope (use current versions)
  --tag           Create git tag after building
  --publish       Create GitHub release (implies --tag)
  --dry-run       Preview without making changes
```

## What Gets Released

The release command produces:

| Artifact | Description |
|----------|-------------|
| `graphql` | CLI binary for validation and linting |
| `graphql-lsp` | Language server binary |
| `graphql-lsp-{version}.vsix` | VS Code extension |

All artifacts are collected in `dist/`.

## Pre-Release Checklist

Before releasing, verify:

- [ ] All tests pass: `cargo test`
- [ ] No clippy warnings: `cargo clippy`
- [ ] Code is formatted: `cargo fmt --check`
- [ ] CHANGELOG.md is up to date (if manual release)
- [ ] Version in Cargo.toml is correct

## Release Steps (Detailed)

### Step 1: Preview

Always start with a dry run:

```bash
cargo xtask release --dry-run --skip-prepare
```

Review the output to ensure:
- Correct version number
- Expected artifacts listed

### Step 2: Build Artifacts

```bash
cargo xtask release --skip-prepare
```

This will:
1. Sync VS Code extension version
2. Build release binaries
3. Package VS Code extension
4. Collect artifacts in `dist/`

### Step 3: Review Artifacts

```bash
ls -la dist/
# Optionally test the binaries
./dist/graphql --version
./dist/graphql-lsp --version
```

### Step 4: Commit (if needed)

If the release modified any files:

```bash
git status
git add -A
git commit -m "chore: release v{version}"
```

### Step 5: Publish

```bash
cargo xtask release --skip-prepare --publish
```

This will:
1. Create annotated git tag
2. Push commits and tags
3. Create GitHub release with artifacts

## Troubleshooting

### "knope not found"

```bash
cargo install knope
```

### "gh not found"

Install GitHub CLI and authenticate:
```bash
# Install (varies by OS)
# macOS: brew install gh
# Linux: see https://cli.github.com/

# Authenticate
gh auth login
```

### Build fails

Check that all dependencies are installed:
```bash
# Rust toolchain
rustup show

# Node.js (for VS Code extension)
node --version
npm --version

# VS Code extension dependencies
cd editors/vscode && npm install
```

### Version mismatch

The release command syncs versions automatically. If issues persist:
```bash
# Check current versions
grep version Cargo.toml | head -1
grep version editors/vscode/package.json | head -1
```

## Creating Changesets

For future releases, document changes with changesets:

```bash
# Interactive
knope document-change

# Or create .changeset/my-change.md manually:
---
default: minor
---

Add new feature X
```

See [.changeset/README.md](../../.changeset/README.md) for details.
