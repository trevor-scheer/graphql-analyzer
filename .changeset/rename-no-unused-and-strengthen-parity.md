---
graphql-analyzer-cli: minor
graphql-analyzer-lsp: minor
graphql-analyzer-mcp: minor
graphql-analyzer-core: minor
graphql-analyzer-eslint-plugin: minor
---

**Drop-in name parity** with `@graphql-eslint/eslint-plugin`: the three remaining mismatched rule names were renamed so all 31 shared rules now line up 1:1.

- `unused-fields` → `no-unused-fields` (config key: `unusedFields` → `noUnusedFields`)
- `unused-fragments` → `no-unused-fragments` (config key: `unusedFragments` → `noUnusedFragments`)
- `unused-variables` → `no-unused-variables` (config key: `unusedVariables` → `noUnusedVariables`)

This is a breaking change for users who configured these rules under their old names; update `.graphqlrc.yaml` lint config keys accordingly. Migration guide added at `linting/migrating-from-graphql-eslint`.

The ESLint shim now propagates `messageId` and `fix` from the analyzer through to `LintMessage`. The parity test compares `(line, column, endLine, endColumn, message, messageId, fix)` together per diagnostic so any drift across rules surfaces as a clean diff. graphql-eslint emits stable `messageId` values for ~22 shared rules; those are now matched verbatim — both kebab-case ids that mirror the rule name (e.g. `"no-anonymous-operations"`, `"alphabetize"`) and the SHOUTY_SNAKE constants graphql-eslint uses for richer per-site distinctions (e.g. `"HASHTAG_COMMENT"`, `"MISSING_ARGUMENTS"`, `"MESSAGE_REQUIRE_DATE"`, `"MUST_HAVE_CONNECTION_SUFFIX"`).

Behavioral parity tightened on the three newly-aligned rules:

- **`no-unused-fields`** message now reads `Field "X" is unused` (matching graphql-eslint), with the diagnostic still anchored at the field name token.
- **`no-unused-fragments`** message reads `Fragment "X" is never used.` and the diagnostic anchors on the `fragment` keyword token (graphql-js's NoUnusedFragmentsRule range, post graphql-eslint adapter).
- **`no-unused-variables`** message reads `Variable "$name" is never used in operation "Op".` (or `… is never used.` for anonymous ops) and anchors on the `$` sigil — matching graphql-js verbatim.

The `alphabetize` rule now emits a `LintMessage.fix` matching graphql-eslint's swap edit. Other rules that ship internal autofixes (`require-selections`, `no-unused-fragments`, `no-unused-variables`) continue to expose those fixes to LSP/CLI consumers but suppress them in the ESLint shim, since graphql-eslint either ships them as `suggest` or doesn't autofix them at all.
