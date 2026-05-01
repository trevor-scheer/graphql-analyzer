---
graphql-analyzer-cli: minor
graphql-analyzer-lsp: minor
graphql-analyzer-core: minor
---

Restructure `extensions.graphql-analyzer.extractConfig` to mirror [`@graphql-tools/graphql-tag-pluck`](https://the-guild.dev/graphql/tools/docs/graphql-tag-pluck), so users coming from `@graphql-eslint` (or any pluck-based pipeline) can paste their pluck config directly. Also accepts a `pluckConfig` alias for the same block.

**Field migration (breaking):**

| Old key                  | New key                          | Notes                                                                                     |
| ------------------------ | -------------------------------- | ----------------------------------------------------------------------------------------- |
| `magicComment`           | `gqlMagicComment`                | Default changed: `"GraphQL"` → `"graphql"` (pluck convention).                            |
| `tagIdentifiers`         | _(removed)_                      | Pluck has no equivalent — bare-tag names live in `globalGqlIdentifierName`; module-bound names are derived from imports. |
| `allowGlobalIdentifiers` | _(removed)_                      | Replaced by `globalGqlIdentifierName`. `false` and `[]` both disable bare extraction.    |
| `modules: string[]`      | `modules: Array<string \| { name, identifier? }>` | Per-module `identifier` constrains which export from the module is recognized as the GraphQL tag. Strings remain accepted as shorthand for `{ name }`. |

**Behavioral changes (breaking):**

- The default `gqlMagicComment` is now `"graphql"` (lowercase) instead of `"GraphQL"`. Comments like `/* GraphQL */ \`...\`` no longer trigger extraction unless `gqlMagicComment` is explicitly set back.
- The default modules list now matches pluck's, **excluding the legacy unscoped `apollo-*` packages** (`apollo-server*`, `apollo-boost`, `apollo-angular`). Modern Apollo lives at `@apollo/client(/core)`; users on a legacy stack should add the relevant module to `modules` explicitly.
- Named imports from a module without an `identifier` constraint (e.g. `graphql-tag`) are no longer tracked — they fall through to `globalGqlIdentifierName` (matches pluck). The default global list (`["gql", "graphql"]`) covers the common case; a renamed import like `import { gql as customGql } from "graphql-tag"` only works if `customGql` is added to `globalGqlIdentifierName`.
- Setting both `extractConfig` and `pluckConfig` on the same project is now a configuration error.

**New options:**

- `gqlVueBlock` — Vue SFC block name for raw GraphQL in custom `<graphql>` blocks.
- `skipIndent` — strip common leading whitespace from extracted GraphQL.

**Pluck migration example:**

```yaml
# Paste your pluck config under `pluckConfig` (or `extractConfig` — same shape)
extensions:
  graphql-analyzer:
    pluckConfig:
      modules:
        - graphql-tag
        - { name: "@apollo/client", identifier: gql }
      globalGqlIdentifierName: ["gql", "graphql"]
```
