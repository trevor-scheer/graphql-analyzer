#!/bin/sh
#
# Custom pre-commit checks for VSCode extension and formatted files
#

set -e

# Check if VSCode extension files are staged
if git diff --cached --name-only | grep -q "^editors/vscode/"; then
    echo '+(cd editors/vscode && npm run format:check)'
    (cd editors/vscode && npm run format:check)
    echo '+(cd editors/vscode && npm run lint)'
    (cd editors/vscode && npm run lint)
fi

# Check formatting for supported file types (excluding editors/vscode which uses Prettier)
if git diff --cached --name-only | grep -v '^editors/vscode/' | grep -qE '\.(graphql|ts|tsx|js|md|yaml|yml|toml|json)$'; then
    echo '+npm run fmt:check'
    npm run fmt:check
fi
