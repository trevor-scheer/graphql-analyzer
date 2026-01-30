# Contributing to graphql-analyzer

Thank you for your interest in contributing! This guide will help you get started with development and explain our processes.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Code Quality](#code-quality)
- [Submitting Changes](#submitting-changes)
- [Project Structure](#project-structure)
- [Communication](#communication)

---

## Getting Started

### Prerequisites

- **Rust**: Install via [rustup](https://rustup.rs/). The project uses the toolchain specified in `rust-toolchain.toml`.
- **Node.js & npm**: Required for VSCode extension development
- **Git**: For version control
- **VSCode**: Recommended for extension development

### Fork and Clone

1. Fork the repository on GitHub
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/graphql-analyzer.git
   cd graphql-analyzer
   ```
3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/trevor-scheer/graphql-analyzer.git
   ```

---

## Development Setup

### Build the Project

```bash
# Build all crates
cargo build --workspace

# Build specific crate
cargo build --package graphql-lsp
```

### Run Tests

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test --package graphql-linter

# Run specific test
cargo test --package graphql-linter test_redundant_fields

# Run with output
cargo test -- --nocapture
```

### Quick Install for LSP and extension (xtask)

The project includes an `xtask` command that builds the LSP server and installs the VSCode extension in one step:

```bash
# Build LSP server (debug) and install extension
cargo xtask install

# Build LSP server (release) and install extension
cargo xtask install --release
```

This is the fastest way to test changes in your local VSCode instance.

### Run the LSP Server standalone

```bash
# Development build
cargo run --package graphql-cli -- lsp

# With debug logging
RUST_LOG=debug cargo run --package graphql-cli -- lsp

# Release build (for performance testing)
cargo build --release --package graphql-cli
RUST_LOG=info ./target/release/graphql lsp
```

### Run the CLI

```bash
# Run from source
cargo run --package graphql-cli -- validate
cargo run --package graphql-cli -- lint

# Build and run
cargo build --package graphql-cli
./target/debug/graphql validate --help
```

### VSCode Extension Development

```bash
# Install dependencies
cd editors/vscode
npm install

# Compile TypeScript
npm run compile

# Watch mode (auto-recompile on changes)
npm run watch

# Open in VSCode and press F5 to launch Extension Development Host
code .
```

The extension will automatically use `target/debug/graphql lsp` when running from the repository.

---

## Testing

### Writing Tests

**Unit Tests** (alongside source code):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        // Arrange
        let input = "...";

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

**Integration Tests** (`tests/` directory):

```rust
// tests/validation_test.rs
use graphql_lsp::*;

#[test]
fn test_project_wide_validation() {
    // Test cross-file validation
}
```

### Test Coverage

- Write tests for new features
- Add regression tests for bug fixes
- Test edge cases and error conditions
- Aim for high coverage of critical paths

### Running Specific Tests

```bash
# Run all tests in a crate
cargo test --package graphql-linter

# Run tests matching a pattern
cargo test fragment

# Run with output
cargo test -- --nocapture --test-threads=1
```

---

## Code Quality

### Pre-Commit Checks

The project uses [cargo-husky](https://github.com/rhysd/cargo-husky) for pre-commit hooks. Before each commit, the following run automatically:

- `cargo fmt --all --check` - Format checking
- `cargo clippy --workspace --all-targets --all-features` - Linting

### Manual Checks

Run these before pushing:

```bash
# Format code
cargo fmt --all

# Check for linting issues
cargo clippy --workspace --all-targets --all-features

# Run all tests
cargo test --workspace

# Format TS code (VSCode extension)
npm run fmt
```

### Addressing Clippy Warnings

Fix all Clippy warnings before submitting:

```bash
cargo clippy --workspace --all-targets --all-features --fix
```

For warnings you believe are false positives, use `#[allow(clippy::specific_lint)]` with a comment explaining why.

---

## Submitting Changes

### Pull Request Process

1. **Sync with upstream:**

   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Ensure quality:**
   - All tests pass: `cargo test --workspace`
   - Code is formatted: `cargo fmt --all`
   - No Clippy warnings: `cargo clippy --workspace`

3. **Push to your fork:**

   ```bash
   git push origin your-branch-name
   ```

4. **Create Pull Request:**
   - Use `gh pr create` or GitHub web interface
   - Target the `main` branch (unless working on a specific feature branch)
   - Fill out the PR template

### Pull Request Guidelines

**Title:**

- Clear and descriptive
- Follow conventional commit format
- No excessive emoji

**Description:**

- Explain what changed and why
- Reference related issues (e.g., "Fixes #123")
- Call out new and updated tests
- Include examples if adding user-facing features

**Don't:**

- Mention that tests or linting passed (this is expected)
- Include unrelated changes
- Add features not requested or discussed

**Example PR Description:**

```markdown
## Summary

Adds completion support for field selections in queries.

## Changes

- Implemented `completion` LSP method in graphql-lsp
- Added schema-based field suggestions in graphql-ide
- Includes type information in completion items

## Testing

- Added unit tests for completion logic
- Added integration test with multi-file schema
- Manually tested in VSCode extension

Fixes #45
```

### Review Process

- Maintainers will review your PR
- Address feedback and push updates to your branch
- Engage in discussion constructively
- Be patient - reviews may take a few days

---

## Project Structure

### Crate Organization

```
crates/
├── graphql-db/          # Salsa database layer
├── graphql-syntax/      # Parsing
├── graphql-hir/         # Semantic representation
├── graphql-analysis/    # Validation & linting
├── graphql-ide/         # Editor API
├── graphql-lsp/         # LSP server
├── graphql-cli/         # CLI tool
├── graphql-mcp/         # MCP server
├── graphql-config/      # Configuration
├── graphql-extract/     # GraphQL document extraction
├── graphql-introspect/  # Schema introspection
└── graphql-linter/      # Linting engine
```

### Architecture Layers

```
LSP/CLI/MCP (user-facing)
    ↓
IDE API (POD types, snapshots)
    ↓
Analysis (validation, linting)
    ↓
HIR (semantic queries)
    ↓
Syntax (parsing)
    ↓
Database (Salsa, incremental)
```

See [.claude/CLAUDE.md](.claude/CLAUDE.md) for detailed architecture documentation.

### Adding Features

**New Lint Rule:**

1. Add rule in `crates/graphql-linter/src/rules/your_rule.rs`
2. Register in `crates/graphql-linter/src/rules/mod.rs`
3. Add tests
4. Update linter README

**New Validation:**

1. Add query in `crates/graphql-analysis/src/`
2. Update `file_diagnostics()` or relevant query
3. Add tests
4. Update analysis README

**New IDE Feature:**

1. Add POD type in `crates/graphql-ide/src/types.rs`
2. Implement query in `crates/graphql-ide/src/lib.rs`
3. Integrate in LSP (`crates/graphql-lsp/src/`)
4. Add tests
5. Update documentation

See [.claude/CLAUDE.md#common-tasks](.claude/CLAUDE.md#common-tasks) for detailed guides.

---

## Communication

### Questions & Discussions

- **General questions**: [GitHub Discussions](https://github.com/trevor-scheer/graphql-analyzer/discussions)
- **Bug reports**: [GitHub Issues](https://github.com/trevor-scheer/graphql-analyzer/issues)
- **Feature requests**: [GitHub Issues](https://github.com/trevor-scheer/graphql-analyzer/issues) with "enhancement" label

### Reporting Bugs

Include:

- GraphQL LSP version
- Operating system and version
- Rust version (`rustc --version`)
- Steps to reproduce
- Expected vs actual behavior
- Relevant logs (set `RUST_LOG=debug`)
- Minimal reproduction case if possible

### Feature Requests

Before opening an issue:

- Check if the feature already exists
- Search existing issues for similar requests
- Explain the use case and motivation
- Provide examples of how it would work

### Code of Conduct

- Be respectful and inclusive
- Assume good intent
- Provide constructive feedback
- Focus on the code, not the person
- Help make the community welcoming

---

## Additional Resources

### Documentation

- [Project Guide](.claude/CLAUDE.md) - Comprehensive guide for contributors and Claude
- [Architecture Design Docs](.claude/notes/active/lsp-rearchitecture/) - Detailed architecture documentation
- Crate READMEs - Each crate has a detailed README

### External Resources

- [Rust-Analyzer Architecture](https://rust-analyzer.github.io/book/contributing/architecture.html) - Inspiration for this project
- [Salsa Book](https://salsa-rs.github.io/salsa/) - Incremental computation framework
- [LSP Specification](https://microsoft.github.io/language-server-protocol/) - Protocol reference
- [GraphQL Specification](https://spec.graphql.org/) - Language reference

---

## License

By contributing, you agree that your contributions will be licensed under the same terms as the project: MIT OR Apache-2.0.

---

Thank you for contributing to graphql-analyzer!
