---
graphql-analyzer-cli: minor
graphql-analyzer-lsp: patch
---

Strict validation mode and pattern diagnostics ([#620](https://github.com/trevor-scheer/graphql-analyzer/pull/620))

**CLI Changes:**

- `validate` now fails by default if no schema files are found (exit code 2)
- Added `--syntax-only` flag to skip schema validation and only check document syntax
- Fails if no document files are found matching configured patterns

**LSP Changes:**

- Added per-pattern error diagnostics for both `schema` and `documents`: each pattern that matches no files shows an error on the specific pattern in the config
- Added summary error diagnostic on `schema`/`documents` key when ALL patterns fail to match
- Diagnostics now underline just the key name (not the colon)
