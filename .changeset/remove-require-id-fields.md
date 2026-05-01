---
graphql-analyzer-cli: minor
graphql-analyzer-lsp: minor
graphql-analyzer-eslint-plugin: minor
---

Remove the `require-id-field` lint rule. It was a strict subset of `require-selections` with cosmetic differences (severity, message text). Migrate via:

- `requireIdField: <severity>` → `requireSelections: [<severity>, { requireAllFields: true }]`
- `requireIdField: [<severity>, { fields: [...] }]` → `requireSelections: [<severity>, { fieldName: [...], requireAllFields: true }]`

(PR link TBD)
