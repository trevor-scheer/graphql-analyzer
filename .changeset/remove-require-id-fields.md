---
graphql-analyzer-cli: minor
graphql-analyzer-lsp: minor
graphql-analyzer-eslint-plugin: minor
---

Remove the `require-id-field` lint rule — strict subset of `require-selections` with cosmetic differences. Migrate `requireIdField: <severity>` to `requireSelections: [<severity>, { requireAllFields: true }]` (or pass the same `fields:` list as `fieldName:` if you customised it) ([#1083](https://github.com/trevor-scheer/graphql-analyzer/pull/1083))
