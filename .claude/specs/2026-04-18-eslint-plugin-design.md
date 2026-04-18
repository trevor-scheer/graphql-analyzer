# ESLint Plugin: graphql-eslint Drop-in Replacement

**Date:** 2026-04-18
**Status:** Draft

## Context

[graphql-eslint](https://the-guild.dev/graphql/eslint/docs) (`@graphql-eslint/eslint-plugin`) is the dominant GraphQL linting solution in the JS ecosystem. Its maintainer is interested in merging projects — deprecating graphql-eslint and pointing users to graphql-analyzer.

This design covers how graphql-analyzer becomes a true drop-in replacement: an ESLint plugin backed by our Rust analyzer via napi-rs, with 1:1 rule and config compatibility.

## Goals

- **Drop-in replacement**: Users swap the package name in their ESLint config and everything works
- **1:1 rule parity**: Every graphql-eslint rule has an equivalent
- **1:1 config parity**: Rule option schemas are identical at the JSON level — no translation layer
- **1:1 preset parity**: Same preset names (`flat/schema-recommended`, `flat/operations-recommended`, etc.)
- **Native performance**: Rust analyzer does the real work; the JS layer is thin glue
- **Dual config support**: Rules configurable via ESLint config, `.graphqlrc.yaml`, or both

## Non-Goals

- Backwards compatibility with graphql-eslint internals (custom rule authoring API, `parserServices`, ESTree AST shape)
- Supporting ESLint legacy config format (`.eslintrc`) — flat config only
- Migration CLI tool (low priority — the migration is already a find-and-replace)

## Architecture

```
┌─────────────────────────────────────────────────┐
│  @graphql-analyzer/eslint-plugin  (TypeScript)  │
│  - ESLint parser (minimal Program AST)          │
│  - Rule shims (one per analyzer rule)           │
│  - Processor (GraphQL extraction from JS/TS)    │
│  - Config presets                               │
└──────────────────────┬──────────────────────────┘
                       │ napi-rs FFI
┌──────────────────────▼──────────────────────────┐
│  graphql-analyzer-napi  (Rust → .node addon)    │
│  - Salsa database singleton                     │
│  - FFI boundary types (JsDiagnostic, etc.)      │
│  - Exposes: init, lint_file, extract_graphql,   │
│    get_rules                                    │
└──────────────────────┬──────────────────────────┘
                       │ crate dependency
┌──────────────────────▼──────────────────────────┐
│  Existing crates                                │
│  (linter, analysis, hir, syntax, db, extract)   │
│  No architectural changes needed                │
└─────────────────────────────────────────────────┘
```

### Layer 1: napi-rs Binding Crate (`crates/napi`)

A new Rust crate that compiles to a native Node addon via napi-rs.

**Database lifecycle:**
- `OnceLock<Mutex<Database>>` holds a Salsa database singleton
- Initialized lazily on first `lint_file()` call with project config (schema source, document globs, rule config)
- Subsequent calls update file content in the database and query cached diagnostics
- Reinitializes if config changes (ESLint restart, config file edit)

**Source text handling:**
ESLint passes the file's current content (potentially unsaved). The binding feeds this into the Salsa database, overriding disk content. Same pattern as the LSP's `textDocument/didChange`.

**Exported API:**

```rust
#[napi(object)]
pub struct JsDiagnostic {
    pub rule: String,
    pub message: String,
    pub severity: String,       // "error" | "warning"
    pub line: u32,              // 1-based
    pub column: u32,            // 1-based
    pub end_line: u32,
    pub end_column: u32,
    pub fix: Option<JsFix>,
    pub help: Option<String>,
    pub url: Option<String>,
}

#[napi(object)]
pub struct JsFix {
    pub description: String,
    pub edits: Vec<JsTextEdit>,
}

#[napi(object)]
pub struct JsTextEdit {
    pub offset: u32,
    pub delete_len: u32,
    pub insert: String,
}

#[napi(object)]
pub struct ExtractedBlock {
    pub source: String,
    pub offset: u32,           // byte offset in host file
    pub tag: String,           // "gql", "graphql", etc.
}

#[napi(object)]
pub struct RuleMeta {
    pub name: String,
    pub description: String,
    pub default_severity: String,
    pub fixable: bool,
    pub options_schema: Option<String>,  // JSON Schema
    pub url: String,
}

/// Initialize or reinitialize the database with project config.
#[napi]
pub fn init(config: serde_json::Value) -> napi::Result<()>;

/// Lint a single file, returning all diagnostics for that file.
/// Includes both lint rule diagnostics and validation diagnostics.
#[napi]
pub fn lint_file(path: String, source: String) -> napi::Result<Vec<JsDiagnostic>>;

/// Extract GraphQL blocks from a JS/TS source file.
#[napi]
pub fn extract_graphql(source: String, language: String) -> napi::Result<Vec<ExtractedBlock>>;

/// Return metadata for all available rules.
#[napi]
pub fn get_rules() -> napi::Result<Vec<RuleMeta>>;
```

**Platform binaries:**
Published as platform-specific npm packages via `@napi-rs/cli`:
- `@graphql-analyzer/binding-darwin-arm64`
- `@graphql-analyzer/binding-darwin-x64`
- `@graphql-analyzer/binding-linux-x64-gnu`
- `@graphql-analyzer/binding-linux-arm64-gnu`
- `@graphql-analyzer/binding-win32-x64-msvc`

Installed as `optionalDependencies` — npm/pnpm/yarn auto-selects the right one. Same pattern as SWC, Turbopack, Prisma.

### Layer 2: ESLint Plugin (`@graphql-analyzer/eslint-plugin`)

A TypeScript package providing parser, processor, rules, and presets.

**Parser (`parseForESLint`):**

Returns a minimal ESTree AST. No GraphQL-to-ESTree conversion needed — our rules run in Rust, not via ESLint's visitor pattern.

```ts
export function parseForESLint(code: string, options: ParserOptions) {
  return {
    ast: {
      type: 'Program',
      sourceType: 'script',
      body: [],
      tokens: [],
      comments: [],
      loc: { start: { line: 1, column: 0 }, end: lastLineCol(code) },
      range: [0, code.length],
    },
    services: {},
  };
}
```

**Rule shims:**

One ESLint rule per analyzer rule. Each uses the `Program` selector as its hook to call into napi:

```ts
function makeRule(ruleName: string, meta: RuleMeta): ESLintRule {
  return {
    meta: {
      type: 'problem',
      docs: { description: meta.description, url: meta.url },
      schema: meta.optionsSchema ? [JSON.parse(meta.optionsSchema)] : [],
      fixable: meta.fixable ? 'code' : undefined,
    },
    create(context) {
      return {
        Program() {
          const diagnostics = binding.lintFile(
            context.filename,
            context.sourceCode.text,
          );
          for (const d of diagnostics) {
            if (d.rule !== ruleName) continue;
            context.report({
              message: d.message,
              loc: {
                start: { line: d.line, column: d.column - 1 },
                end: { line: d.endLine, column: d.endColumn - 1 },
              },
              fix: d.fix
                ? (fixer) => d.fix.edits.map((e) =>
                    fixer.replaceTextRange(
                      [e.offset, e.offset + e.deleteLen],
                      e.insert,
                    ),
                  )
                : undefined,
            });
          }
        },
      };
    },
  };
}
```

**Caching efficiency:** `lintFile()` is called once per rule per file, but the Salsa database caches results after the first call. Subsequent calls for the same file content are HashMap lookups. Optionally, the JS layer can cache results per-file to avoid even the FFI round-trip:

```ts
const cache = new Map<string, JsDiagnostic[]>();

Program() {
  const key = context.filename + '\0' + context.sourceCode.text.length;
  if (!cache.has(key)) {
    cache.set(key, binding.lintFile(context.filename, context.sourceCode.text));
  }
  const diagnostics = cache.get(key)!;
  // filter by ruleName...
}
```

**Processor (GraphQL extraction from JS/TS):**

Uses `extract_graphql` from the napi binding (backed by the `graphql-extract` crate):

```ts
export const processor = {
  preprocess(code: string, filename: string) {
    const ext = path.extname(filename);
    if (!['.js', '.jsx', '.ts', '.tsx', '.svelte', '.vue'].includes(ext)) {
      return [code];
    }
    const blocks = binding.extractGraphql(code, ext.slice(1));
    return [
      ...blocks.map((b, i) => ({
        text: b.source,
        filename: `${i}.graphql`,
      })),
      code, // original file for other ESLint rules
    ];
  },
  postprocess(messages: LintMessage[][], filename: string) {
    const blocks = binding.extractGraphql(
      fs.readFileSync(filename, 'utf8'),
      path.extname(filename).slice(1),
    );
    // Last message group is from the original file — pass through
    const originalMessages = messages.pop()!;
    // Remap GraphQL block locations back to host file offsets
    const remapped = messages.flatMap((group, i) =>
      group.map((msg) => offsetMessage(msg, blocks[i].offset)),
    );
    return [...remapped, ...originalMessages];
  },
  supportsAutofix: true,
};
```

**Config presets:**

Match graphql-eslint's preset names and composition:

```ts
export const configs = {
  'flat/schema-recommended': {
    rules: {
      '@graphql-analyzer/naming-convention': 'error',
      '@graphql-analyzer/no-typename-prefix': 'error',
      '@graphql-analyzer/no-unreachable-types': 'error',
      '@graphql-analyzer/require-description': 'error',
      // ...
    },
  },
  'flat/operations-recommended': {
    rules: {
      '@graphql-analyzer/no-anonymous-operations': 'error',
      '@graphql-analyzer/no-deprecated': 'warn',
      '@graphql-analyzer/no-duplicate-fields': 'error',
      '@graphql-analyzer/require-selections': 'error',
      // ...
    },
  },
  'flat/schema-all': { /* extends schema-recommended */ },
  'flat/operations-all': { /* extends operations-recommended */ },
  'flat/schema-relay': { /* relay rules */ },
};
```

### Config Resolution

Two config sources, merged with clear precedence:

1. **ESLint config** — rule severity and options from `rules: { '@graphql-analyzer/...': [...] }`
2. **`.graphqlrc.yaml`** — project-level config (schema source, document globs) and optionally lint rule config in `extensions.graphql-analyzer.lint`

**Merge behavior:**
- Schema/document discovery always comes from `.graphqlrc.yaml` (or `parserOptions.graphQLConfig` for programmatic config)
- If a rule is configured in both ESLint config and `.graphqlrc.yaml`, ESLint config wins (it's closer to the user)
- If a rule is only configured in `.graphqlrc.yaml`, that config is used
- This lets users who already have `.graphqlrc.yaml` lint config keep using it, while ESLint-native users configure everything in `eslint.config.js`

### Config Parity

Rule option schemas must be identical to graphql-eslint's at the JSON/serde level. This means:

- For rules both projects share: adopt graphql-eslint's option shape. If our current options differ, change ours to match (breaking change is acceptable pre-1.0).
- For rules we're adding from graphql-eslint: implement with their option schema from the start.
- For rules only we have: our own schema, no constraints.

This ensures users can copy-paste their graphql-eslint rule config verbatim.

### Validation Rules as ESLint Rules

graphql-eslint exposes the 27 graphql-js validation rules as individually configurable ESLint rules. Our analyzer runs these as always-on validation via apollo-compiler.

For the ESLint plugin, we expose them as ESLint rules too:
- The napi binding tags validation diagnostics with the equivalent graphql-eslint rule name (e.g., `known-directives`, `fields-on-correct-type`)
- Each gets a rule shim in the ESLint plugin
- Users can `"off"` them in ESLint config (the plugin filters out diagnostics for disabled rules)
- The `known-directives` rule specifically needs an `ignoreClientDirectives` option, matching graphql-eslint's behavior

This maintains the 1:1 drop-in story for users who have disabled specific validation rules.

## Rule Coverage

### Rules in both projects (22 rules matched)

| graphql-eslint | graphql-analyzer | Notes |
|---|---|---|
| alphabetize | alphabetize | |
| description-style | descriptionStyle | |
| input-name | inputName | |
| lone-executable-definition | loneExecutableDefinition | |
| naming-convention | namingConvention | Adopt graphql-eslint option schema |
| no-anonymous-operations | noAnonymousOperations | |
| no-deprecated | noDeprecated | |
| no-duplicate-fields | noDuplicateFields | |
| no-hashtag-description | noHashtagDescription | |
| no-one-place-fragments | noOnePlaceFragments | |
| no-scalar-result-type-on-mutation | noScalarResultTypeOnMutation | |
| no-typename-prefix | noTypenamePrefix | |
| no-unreachable-types | noUnreachableTypes | |
| no-unused-fields | unusedFields | |
| require-deprecation-reason | requireDeprecationReason | |
| require-description | requireDescription | |
| require-field-of-type-query-in-mutation-result | requireFieldOfTypeQueryInMutationResult | |
| require-selections | requireSelections | |
| selection-set-depth | selectionSetDepth | |
| strict-id-in-types | strictIdInTypes | |
| unique-enum-value-names | uniqueEnumValueNames | |
| unique-fragment-name / unique-operation-name | uniqueNames | Our single rule covers both |

### graphql-eslint rules to add (11)

| Rule | Category | Complexity |
|---|---|---|
| match-document-filename | Document | Medium — file path analysis |
| no-root-type | Schema | Low — check root type usage |
| require-deprecation-date | Schema | Low — parse @deprecated args |
| require-import-fragment | Document | Medium — import syntax checking |
| require-nullable-fields-with-oneof | Schema | Low — @oneOf input validation |
| require-nullable-result-in-root | Schema | Low — nullability check |
| require-type-pattern-with-oneof | Schema | Low — naming pattern check |
| relay-arguments | Schema | Medium — connection arg validation |
| relay-connection-types | Schema | Medium — connection type shape |
| relay-edge-types | Schema | Medium — edge type shape |
| relay-page-info | Schema | Low — PageInfo type check |

### Rules unique to graphql-analyzer (keep as-is)

| Rule | Notes |
|---|---|
| operationNameSuffix | Complementary to naming-convention |
| redundantFields | Novel — not in graphql-eslint |
| requireIdField | Overlaps with strict-id-in-types but document-side |
| unusedFragments | Covered by validation in graphql-eslint, explicit rule in ours |
| unusedVariables | Same — validation wrapper in graphql-eslint, explicit rule in ours |

### ESLint rule naming

graphql-eslint uses kebab-case (`no-deprecated`). Our rules use camelCase (`noDeprecated`). The ESLint plugin maps between them — ESLint users see kebab-case:

```js
rules: {
  '@graphql-analyzer/no-deprecated': 'warn',
  '@graphql-analyzer/selection-set-depth': ['error', { maxDepth: 5 }],
}
```

The shim maps `no-deprecated` → `noDeprecated` when filtering diagnostics from the binding.

## Biome / oxlint Integration

The napi binding is specific to the ESLint integration. Biome and oxlint are Rust-native and would consume the `graphql-linter` crate directly as a Rust dependency — no FFI needed.

This makes the architecture a single Rust implementation with multiple distribution channels:

| Consumer | Integration |
|---|---|
| graphql-analyzer CLI | Rust crate (direct) |
| graphql-analyzer LSP | Rust crate (direct) |
| ESLint plugin | napi-rs (.node addon) |
| Biome | Rust crate dependency |
| oxlint | Rust crate dependency |

For Biome/oxlint, the main work would be on their side: depending on the crate and mapping diagnostics to their output format. The graphql-analyzer side just needs a clean public API on the linter crate, which largely exists already.

## Test Workspace: `test-workspace/eslint-migration`

A self-contained project that demonstrates and tests the migration from graphql-eslint to graphql-analyzer's ESLint plugin.

**Structure:**

```
test-workspace/eslint-migration/
├── README.md              # Step-by-step migration walkthrough
├── package.json           # Both plugins as dependencies
├── schema.graphql         # Sample schema
├── src/
│   ├── operations.graphql # Sample operations
│   └── component.tsx      # Embedded GraphQL in TS
├── eslint.config.before.js   # graphql-eslint config
├── eslint.config.after.js    # graphql-analyzer config (the migration target)
├── eslint.config.js          # Symlink or copy of "after" for actual use
└── expected-output.json      # Expected diagnostics for CI validation
```

**README walkthrough:**
1. Show the "before" config using `@graphql-eslint/eslint-plugin`
2. Run `npx eslint .` — observe diagnostics
3. Swap to `@graphql-analyzer/eslint-plugin` (the "after" config)
4. Run `npx eslint .` — observe identical diagnostics
5. Note any differences or improvements

**CI integration:**
The workspace can be used as an integration test: run both configs, diff the output, assert parity. This catches regressions when rules change.

## Package Structure

```
packages/
├── napi/                          # Rust napi crate
│   ├── Cargo.toml
│   ├── src/lib.rs
│   ├── npm/                       # Platform packages
│   │   ├── darwin-arm64/
│   │   ├── darwin-x64/
│   │   ├── linux-x64-gnu/
│   │   ├── linux-arm64-gnu/
│   │   └── win32-x64-msvc/
│   └── package.json               # @graphql-analyzer/napi
├── eslint-plugin/                 # TypeScript ESLint plugin
│   ├── package.json               # @graphql-analyzer/eslint-plugin
│   ├── src/
│   │   ├── index.ts               # Plugin entry (parser, processor, rules, configs)
│   │   ├── parser.ts              # parseForESLint
│   │   ├── processor.ts           # GraphQL extraction from JS/TS
│   │   ├── rules/                 # Generated rule shims
│   │   └── configs/               # Preset configs
│   └── tsconfig.json
```

## Open Questions

- **Rule option schema diff**: Need a detailed audit of each shared rule's option schema to identify where our options diverge from graphql-eslint's. This determines the scope of breaking changes needed.
- **ESLint flat config only**: Is dropping legacy `.eslintrc` support acceptable? graphql-eslint supports both, but ESLint 9+ defaults to flat config and legacy is deprecated.
- **`parserServices` compatibility**: Some users may have custom ESLint rules that use graphql-eslint's `parserServices` (schema access, sibling operations). We'd need to decide whether to support this API. Likely out of scope for v1.
- **Monorepo integration**: graphql-eslint uses graphql-config's project matching to handle monorepos with multiple schemas. Our plugin should match this behavior.
