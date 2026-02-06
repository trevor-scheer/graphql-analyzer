---
graphql-analyzer-lsp: minor
graphql-analyzer-vscode: minor
---

Support TypeScript schema files with correct diagnostic positions ([#558](https://github.com/trevor-scheer/graphql-analyzer/pull/558))

Schema SDL defined in TypeScript files (via `gql` template literals) now works correctly:
- Schema files in TypeScript are extracted and merged with other schema files
- Validation errors report correct line/column positions in the original TypeScript file
- Config validation ensures schema and documents patterns don't overlap
