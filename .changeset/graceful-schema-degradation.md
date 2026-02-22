---
graphql-analyzer-cli: minor
graphql-analyzer-lsp: patch
---

Strict validation mode and schema pattern diagnostics ([#620](https://github.com/trevor-scheer/graphql-analyzer/pull/620))

**CLI Changes:**

- `validate` now fails by default if no schema files are found (exit code 2)
- Added `--syntax-only` flag to skip schema validation and only check document syntax
- Fails if no document files are found matching configured patterns

**LSP Changes:**

- Added per-pattern diagnostics: each schema pattern that matches no files now shows a warning on the specific pattern in the config
- Added summary diagnostic on `schema` key when ALL patterns fail to match
- Diagnostics now underline just the pattern text instead of the entire `schema:` line
