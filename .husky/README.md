# Git Hooks

This directory contains the git hook configuration.

## How it works

- `Cargo.toml` configures [cargo-husky](https://github.com/rhysd/cargo-husky) to automatically install git hooks during build
- Hooks are stored in `.cargo-husky/hooks/` and copied to `.git/hooks/` by cargo-husky
- **pre-commit**: Checks formatting and linting for staged files
- **pre-push**: Runs the full check suite (fmt, clippy, lint, tests) for changed files before pushing

## Setup

The hooks are automatically installed when you build the project with `cargo build`.

To manually reinstall hooks after modifying them, run:

```bash
cargo test -p husky-hooks
```

## Hooks

### pre-commit

Runs on staged files before each commit:

- **Rust formatting**: `cargo fmt --check` (for `.rs` files)
- **Rust linting**: `cargo clippy` (for `.rs`/`.toml` files)
- **TS/JS linting**: `pnpm run lint` (for `.ts`/`.tsx`/`.js`/`.jsx` files)
- **Multi-format checking**: `pnpm run fmt:check` (for `.graphql`/`.ts`/`.js`/`.md`/`.yaml`/`.json` files)

### pre-push

Runs on all changed files (vs remote) before each push:

- All the same checks as pre-commit
- **Tests**: `cargo test` (for `.rs`/`.toml` files)
