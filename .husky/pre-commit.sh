#!/bin/sh
#
# Custom pre-commit checks for VSCode extension and formatted files
#

set -e

# Check Rust formatting if any Rust files are staged
if git diff --cached --name-only | grep -qE '\.rs$'; then
    echo '+cargo fmt --check'
    cargo fmt --check
fi

# Run clippy if any Rust files are staged
if git diff --cached --name-only | grep -qE '\.(rs|toml)$'; then
    echo '+cargo clippy --workspace --all-targets --all-features'
    cargo clippy --workspace --all-targets --all-features
fi

# Lint VSCode extension if its files are staged
if git diff --cached --name-only | grep -q "^editors/vscode/"; then
    echo '+(cd editors/vscode && npm run lint)'
    (cd editors/vscode && npm run lint)
fi

# Check formatting with oxfmt for supported file types
if git diff --cached --name-only | grep -qE '\.(graphql|ts|tsx|js|md|yaml|yml|toml|json)$'; then
    echo '+npm run fmt:check'
    npm run fmt:check
fi
