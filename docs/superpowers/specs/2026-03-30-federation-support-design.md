# Federation Support Design

**Date:** 2026-03-30
**Status:** Draft
**Author:** Trevor + Claude

---

## Context

GraphQL federation has become the standard approach for building large-scale GraphQL APIs. Apollo Federation v2 directives are the de facto standard, supported not just by Apollo but also by WunderGraph Cosmo, Grafbase, and others. Despite this adoption, IDE tooling for federation development is severely lacking. Standard GraphQL validators produce false positives on federation schemas, FieldSet strings are opaque to tooling, and cross-subgraph navigation doesn't exist outside vendor-locked solutions.

This design adds first-class federation support to graphql-analyzer, targeting Apollo Federation v2 directives as the universal directive set while supporting multiple vendor config formats.

### Prior Art: apollo-language-server

The `apollo-language-server` Rust crate (crates.io) was an earlier attempt at federation-aware IDE tooling. It provides `SchemaSource` abstractions for registry/file/introspection, config management, and basic LSP features. While it validates the problem space, it is incomplete and its patterns are not a template for this work. We take inspiration from the problems it identified, not its solutions.

---

## Decisions

### What we chose: Approach C — "Federation as a Mode"

A dedicated `federation` crate owns federation domain logic. The LSP operates in explicit modes that determine the feature set. Existing crates gain minimal federation hooks.

### Alternatives considered

**Approach A: "Federation as a Layer"** — A new crate inserted between `analysis` and `ide` in the dependency chain. Clean separation but creates an awkward interface boundary: features like hover and completions live in `ide` but need federation data, leading to either leaky abstractions or duplicated feature implementations. Rejected because federation concerns don't cleanly layer — they cross-cut existing features.

**Approach B: "Federation Woven In"** — Distribute federation awareness across all existing crates. No new crate boundary, features naturally integrated. Rejected because it spreads federation concerns everywhere, makes non-federation codepaths harder to reason about, and makes federation hard to test in isolation.

### Why Approach C

The mode concept directly maps to the user experience we want: transparency about what features are available based on current configuration. Each mode has a well-defined capability matrix. Graceful degradation is natural — the mode determines the feature set. The dedicated crate keeps federation logic testable in isolation while thin integration points in existing crates keep the architecture clean.

### Target: Apollo Federation v2 directives

Apollo Federation v2 directives are the implementation target because they are the de facto standard across vendors. WunderGraph Cosmo, Grafbase, and others all support the same directive set. Federation v1 schemas (detected by absence of `@link`) will receive best-effort support with guidance to migrate to v2.

### Config strategy: auto-detect vendor configs, .graphqlrc.yaml remains relevant

We auto-detect supergraph config files from multiple vendors rather than requiring configuration in `.graphqlrc.yaml`. This meets users where they are — they already have a `supergraph.yaml` or `graph.yaml`. The `.graphqlrc.yaml` continues to serve its existing purpose (lint rules, document patterns, client config) and gains a new optional `federation` extension for cases where auto-detection needs help.

### Phase 1 scope: local supergraph config

The initial implementation reads local config files and local schema files only. Registry integration (Apollo GraphOS, Cosmo Cloud) and introspection-based schema fetching are future work. This keeps the first milestone achievable without vendor-specific auth flows.

---

## Architecture

### Crate Structure

```
graphql-lsp / graphql-cli / graphql-mcp    (entrypoints)
    |
graphql-ide          (Editor API, federation-enriched features)
    |
graphql-federation   (NEW: federation domain logic)
    |
graphql-analysis     (Validation + Linting, delegates to federation rules)
    |
graphql-hir          (Semantic layer)
    |
graphql-syntax       (Parsing, TS/JS extraction)
    |
graphql-db           (Salsa database)
```

The `federation` crate depends on `hir` and `base-db` (for type/field definitions and Salsa queries). The `ide` crate depends on `federation` for enriched features. The `analysis` crate depends on `federation` for validation rules.

### The `graphql-federation` Crate

Owns all federation-specific domain logic:

**Config parsing:**
- Apollo `supergraph.yaml` / `supergraph.yml` parser
- WunderGraph Cosmo `graph.yaml` / `graph.yml` parser
- Unified `SupergraphConfig` type that normalizes across formats
- Config file watching and change detection

**Federation directive definitions:**
- Complete directive definitions for Federation v2.0 through v2.7
- Version-aware directive availability (e.g., `@interfaceObject` requires v2.3+)
- `@link` import validation

**FieldSet parser and validator:**
- Parses the `FieldSet` scalar strings used in `@key`, `@requires`, `@provides`
- Validates field paths exist on the target type
- Validates FieldSet constraints (no aliases, no arguments, no fragments)
- Provides completions and go-to-definition within FieldSet strings

**Subgraph validation rules:**
- `@key` fields exist and are valid types (not lists, interfaces, unions)
- `@external` fields are referenced by `@key`, `@requires`, or `@provides`
- `@requires` fields are marked `@external`
- `@provides` targets object types and references `@external` fields
- `@override` constraints (not from self, not on interfaces, not with `@external`)
- `@link` import completeness (used directives are imported)
- Version compatibility (directives match declared federation version)

**Cross-subgraph schema model (Supergraph mode):**
- Merges type definitions across subgraphs
- Tracks field ownership (which subgraph defines/resolves each field)
- Entity resolution map (which subgraphs can resolve which entities via which keys)
- Detects composition errors locally (field sharing without `@shareable`, type mismatches)

**Mode detection and capability reporting:**
- Determines current mode from available configuration
- Reports capabilities and limitations to the LSP layer
- Provides structured status for UI display

### Modes

The LSP operates in one of four modes, auto-detected from workspace configuration:

#### Detection Logic (priority order)

1. Scan workspace root and parent directories for supergraph config files:
   - `supergraph.yaml` / `supergraph.yml` (Apollo Rover format)
   - `graph.yaml` / `graph.yml` (WunderGraph Cosmo format)
2. If supergraph config found, determine sub-mode:
   - Does `.graphqlrc.yaml` have a project whose schema matches a subgraph entry? → **Subgraph mode**
   - Are all subgraph schemas resolvable (local files exist)? → **Supergraph mode**
   - Does `.graphqlrc.yaml` have `documents` pointing at the composed supergraph schema? → **Client mode**
   - Fallback: supergraph config found but can't resolve all schemas → **Subgraph mode** (degraded)
3. No supergraph config → **Standard mode** (current behavior unchanged)

#### Mode Descriptions

**Standard:** No federation awareness. Current behavior, zero changes. This is the default.

**Subgraph:** The workspace contains a single subgraph's schema. Federation directives are validated. FieldSet strings are parsed and checked. Federation-specific completions and hover are available. Sibling subgraph schemas may or may not be available.

**Supergraph:** All subgraph schemas in the topology are available locally. Full cross-subgraph features: entity navigation, composition preview, field ownership tracking, unused entity detection.

**Client:** The workspace consumes a supergraph API. Operations are validated against the composed API schema. Hover shows which subgraph owns each field. `@inaccessible` fields are hidden from completions.

#### Capability Matrix

| Feature | Standard | Subgraph | Supergraph | Client |
|---|---|---|---|---|
| Syntax/parse errors | Yes | Yes | Yes | Yes |
| Standard GraphQL validation | Yes | Yes | Yes | Yes |
| Lint rules | Yes | Yes | Yes | Yes |
| Federation directive validation | - | Yes | Yes | - |
| FieldSet parsing & validation | - | Yes | Yes | - |
| Federation-aware completions | - | Yes | Yes | - |
| Cross-subgraph go-to-definition | - | - | Yes | - |
| Composition error preview | - | - | Yes | - |
| Field ownership in hover | - | - | Yes | Yes |
| `@inaccessible` awareness | - | - | Yes | Yes |
| Unused entity/field detection | - | - | Yes | - |
| Suppress false positive errors | - | Yes | Yes | - |

#### Surfacing Mode to Users

- **LSP notification:** Custom `graphql/status` notification with mode, capabilities, and warnings
- **VS Code status bar:** `GraphQL: Subgraph (products)` / `GraphQL: Supergraph (5 subgraphs)` / `GraphQL: Client` / `GraphQL: Standard`
- **Degraded mode warnings:** When in Subgraph mode because sibling schemas are unavailable, show: "3 of 5 sibling subgraphs unavailable — cross-subgraph features limited. Run all subgraphs locally or provide schema files to enable full Supergraph mode."
- **CLI output:** `graphql check` prints mode and capability summary at startup

### Config Formats

#### Apollo Rover (`supergraph.yaml`)

```yaml
federation_version: =2.7.0
subgraphs:
  products:
    routing_url: https://products.example.com/graphql
    schema:
      file: ./subgraphs/products/schema.graphql
  reviews:
    routing_url: https://reviews.example.com/graphql
    schema:
      file: ./subgraphs/reviews/schema.graphql
  users:
    routing_url: https://users.example.com/graphql
    schema:
      subgraph_url: http://localhost:4003/graphql
      introspection_headers:
        Authorization: Bearer ${env.USERS_TOKEN}
  inventory:
    routing_url: https://inventory.example.com/graphql
    schema:
      graphref: my-graph@production
      subgraph: inventory
```

**Schema sources:** `file` (local path), `subgraph_url` (introspection), `graphref` + `subgraph` (Apollo GraphOS registry). Phase 1 supports `file` only.

#### WunderGraph Cosmo (`graph.yaml`)

```yaml
version: 1
subgraphs:
  - name: products
    routing_url: http://localhost:4001/graphql
    schema:
      file: ./subgraphs/products/schema.graphql
  - name: reviews
    routing_url: http://localhost:4002/graphql
    schema:
      file: ./subgraphs/reviews/schema.graphql
  - name: users
    routing_url: http://localhost:4003/graphql
    introspection:
      url: http://localhost:4003/graphql
      headers:
        Authorization: Bearer ${env.TOKEN}
```

**Differences from Apollo:** Array instead of map, `introspection` block instead of `subgraph_url`, `version` field instead of `federation_version`. Phase 1 supports `schema.file` only.

#### Unified Internal Model

Both formats normalize to:

```rust
pub struct SupergraphConfig {
    pub source_format: ConfigFormat,         // Apollo | Cosmo
    pub federation_version: Option<String>,
    pub subgraphs: IndexMap<String, SubgraphConfig>,
}

pub struct SubgraphConfig {
    pub name: String,
    pub routing_url: Option<String>,
    pub schema_source: SchemaSource,
}

pub enum SchemaSource {
    File(PathBuf),
    Introspection { url: String, headers: HashMap<String, String> },
    Registry { graph_ref: String, subgraph: String },
}

pub enum ConfigFormat {
    Apollo,
    Cosmo,
}
```

#### .graphqlrc.yaml Integration

The `.graphqlrc.yaml` remains the primary project config. An optional `federation` extension provides overrides when auto-detection needs help:

```yaml
projects:
  products:
    schema: src/schema/**/*.graphql
    documents: src/**/*.{graphql,ts}
    extensions:
      federation:
        supergraph: ./supergraph.yaml   # explicit path (overrides auto-detect)
        subgraph: products              # which subgraph this project represents
      lint:
        extends: recommended
```

This is optional — auto-detection handles most cases. The extension exists for:
- Repos where the supergraph config is in a non-standard location
- Disambiguating which subgraph a project represents when auto-detection is ambiguous
- Monorepos with multiple subgraphs in different directories

### Integration Points with Existing Crates

#### `config` crate

- New: `SupergraphConfigLoader` that finds and parses supergraph config files
- New: `FederationExtension` parsed from `.graphqlrc.yaml` `extensions.federation`
- Existing config loading unchanged

#### `base-db` crate

- `DocumentKind` may gain awareness of federation context, or federation metadata may be stored separately in the federation crate's own Salsa queries. Design TBD during implementation — the key constraint is that non-federation codepaths must not pay for federation.

#### `analysis` crate

- New: when federation mode is active, federation validation rules run in addition to standard validation
- New: standard validation rules that would produce false positives on federation schemas are suppressed (unknown directives, undefined runtime types like `_Entity`, `_Any`, `_Service`)
- Existing validation unchanged when not in federation mode

#### `ide` crate

- New: federation-enriched hover (field ownership, entity keys, federation directive docs)
- New: federation-aware completions (federation directives, FieldSet field suggestions)
- New: go-to-definition within FieldSet strings
- New: cross-subgraph go-to-definition for entity types (Supergraph mode only)
- Existing features unchanged when not in federation mode

#### `lsp` crate

- New: mode detection on workspace initialization
- New: custom `graphql/status` notification for mode/capability reporting
- New: supergraph config file watching
- Existing LSP handlers check federation mode and delegate to enriched implementations when active

#### `linter` crate

- New: federation-specific lint rules (naming conventions for entities, `@key` best practices, etc.)
- These are opt-in lint rules, not validation — they follow existing lint rule patterns

### Federation Directive Validation (Detail)

These are the subgraph-level validation rules implemented in Phase 1. They require only the local subgraph schema.

**`@key` validation:**
- `fields` argument is a syntactically valid FieldSet
- Referenced fields exist on the annotated type
- Referenced fields are not list types, interface types, or union types
- Referenced fields do not have required arguments
- FieldSet does not contain aliases, arguments, or fragment spreads
- At least one `@key` exists on types marked with `@interfaceObject`

**`@external` validation:**
- Not applied to interface fields
- In v2: every `@external` field is referenced by at least one `@key`, `@requires`, or `@provides`

**`@requires` validation:**
- `fields` argument is a syntactically valid FieldSet
- Referenced fields exist on the same type
- Referenced leaf fields (or a parent on the path) are marked `@external`

**`@provides` validation:**
- Annotated field returns an object type
- `fields` argument references fields on the return type
- Referenced fields are `@external`

**`@override` validation:**
- `from` argument does not equal the current subgraph name (requires knowing own subgraph name from config)
- Not combined with `@external`, `@requires`, or `@provides`
- Not applied to interface fields

**`@link` validation:**
- URL is well-formed with a valid federation version
- All directives used by short name are listed in `import`
- Directives used are available in the declared federation version

**False positive suppression:**
- `_Entity`, `_Service`, `_Any`, `_FieldSet` types are recognized as federation built-ins
- `Query._entities` and `Query._service` fields are recognized as federation built-ins
- Federation directives are recognized without explicit definitions in the schema

### FieldSet Parser

The FieldSet scalar (`@key(fields: "...")`, `@requires(fields: "...")`, `@provides(fields: "...")`) contains a mini-language that standard GraphQL tooling treats as an opaque string. We parse and validate it.

**Grammar:**
```
FieldSet     = Selection+
Selection    = Field
Field        = Name SelectionSet?
SelectionSet = '{' Selection+ '}'
Name         = [_A-Za-z][_0-9A-Za-z]*
```

Notably absent vs full GraphQL: no aliases, no arguments, no directives, no fragment spreads, no inline fragments.

**IDE features within FieldSet strings:**
- Syntax highlighting (via semantic tokens)
- Completions (suggest fields on the target type)
- Go-to-definition (navigate to the field definition)
- Hover (show field type information)
- Diagnostics (field doesn't exist, field is a list type, etc.)

This requires detecting cursor position within a string literal argument of a federation directive and switching to FieldSet-aware behavior.

---

## Phased Implementation

### Phase 1: Foundation (MVP)

**Goal:** Federation-aware validation for a single subgraph with local config.

- `graphql-federation` crate: config parsing, mode detection, directive definitions, FieldSet parser
- Supergraph config parsing: Apollo `supergraph.yaml` and Cosmo `graph.yaml` (local `file` sources only)
- Mode detection and capability reporting via LSP notification
- VS Code status bar integration showing current mode
- Federation directive validation (all subgraph-level rules listed above)
- FieldSet parsing and validation with diagnostics
- False positive suppression for federation built-in types
- Federation directive completions (`@key`, `@external`, `@requires`, `@provides`, `@shareable`, etc.)

**Not included:** Cross-subgraph features, composition preview, remote schema fetching, registry integration.

### Phase 2: FieldSet IDE Features

**Goal:** Rich editing experience inside FieldSet strings.

- Completions within FieldSet strings (suggest fields on the target type)
- Go-to-definition from FieldSet field references to their definitions
- Hover within FieldSet strings showing field type info
- Semantic tokens for FieldSet syntax highlighting

### Phase 3: Cross-Subgraph Features (Supergraph Mode)

**Goal:** Full supergraph awareness when all subgraph schemas are available locally.

- Load and merge all subgraph schemas from supergraph config
- Cross-subgraph go-to-definition for entity types
- Field ownership tracking (which subgraph defines/resolves each field)
- Hover enrichment showing subgraph origin
- Composition error preview (detect `INVALID_FIELD_SHARING`, type mismatches, etc.)
- Entity resolution map visualization
- Unused entity detection

### Phase 4: Client Mode

**Goal:** Enhanced experience for client repos consuming a supergraph.

- Detect client mode from config
- Compose or load the supergraph API schema
- Hide `@inaccessible` fields from completions
- Show subgraph origin in hover for operation fields
- Surface contract/tag information when available

### Phase 5: Remote Schema Sources

**Goal:** Support subgraph schemas that don't exist locally.

- Introspection-based schema fetching (`subgraph_url` / `introspection`)
- Apollo GraphOS registry integration (`graphref` + `subgraph`)
- WunderGraph Cosmo Cloud integration
- GraphQL Hive registry integration
- Auth token management (env vars, credential helpers)
- Schema caching and staleness detection
- Background refresh with change notifications

### Phase 6: Federation Lint Rules

**Goal:** Best-practice lint rules for federation schemas.

- Entity naming conventions
- `@key` field selection best practices (prefer `id` fields, avoid deep nesting)
- `@shareable` usage patterns
- `@override` migration guidance
- Unused `@external` fields (already covered by validation, but lint can suggest removal)
- `@inaccessible` audit (fields that could be made inaccessible)

### Phase 7: Advanced Composition

**Goal:** Deep composition analysis and migration tooling.

- Full local composition (potentially via `apollo-rs` composition or `rover` integration)
- Composition diff preview (what changes when you modify your subgraph)
- Federation v1 → v2 migration assistant
- Breaking change detection across subgraph versions
- Entity usage analysis across the supergraph

---

## Open Questions

1. **Composition engine:** Should we implement composition logic ourselves, shell out to `rover supergraph compose`, or use an existing Rust crate? `apollo-federation-types` exists but may not include the full composition algorithm. This affects Phase 3 and Phase 7.

2. **Subgraph identity:** In Subgraph mode, how does the LSP know "I am the products subgraph"? Options: infer from supergraph config (match local schema paths), explicit in `.graphqlrc.yaml` federation extension, or prompt the user. Needed for `@override` validation and mode display.

3. **Federation v1 support depth:** Federation v1 schemas (no `@link`) are still in production. How much effort do we put into v1-specific behavior vs encouraging migration to v2?

4. **Supergraph config location:** Auto-detection scans upward from workspace root. Should we also check common monorepo patterns (e.g., `gateway/supergraph.yaml`, `infra/supergraph.yaml`)?

5. **Multiple supergraph configs:** A monorepo might contain multiple supergraph configs for different environments. How do we handle this? Pick the first? Let the user choose? Support all simultaneously?

---

## Appendix A: Apollo Federation v2 Directives Reference

### Directives Present in Both v1 and v2

**`@key(fields: FieldSet!, resolvable: Boolean = true)`**
- Locations: `OBJECT`, `INTERFACE` (interface support v2.3+)
- Repeatable: Yes
- Designates a type as a federated entity. `fields` identifies the unique key. `resolvable: false` (v2 only) means this subgraph cannot look up the entity, only reference it.

**`@external`**
- Locations: `FIELD_DEFINITION`
- Declares a field is defined in another subgraph. Used with `@key`, `@requires`, `@provides`.

**`@requires(fields: FieldSet!)`**
- Locations: `FIELD_DEFINITION`
- Declares fields that must be fetched from other subgraphs before this field resolves.

**`@provides(fields: FieldSet!)`**
- Locations: `FIELD_DEFINITION`
- Declares nested fields this resolver co-returns, avoiding extra subgraph fetches.

**`@extends`**
- Locations: `OBJECT`, `INTERFACE`
- Alternative to `extend type` keyword. Optional in v2, retained for backward compatibility.

### Directives New in v2

**`@shareable`**
- Locations: `FIELD_DEFINITION`, `OBJECT`
- Repeatable: Yes (v2.2+)
- Declares a field may be resolved by multiple subgraphs. Required on all subgraphs that share the field.

**`@inaccessible`**
- Locations: All type system locations
- Hides an element from the public API schema while keeping it available internally for query planning.

**`@override(from: String!, label: String)`**
- Locations: `FIELD_DEFINITION`
- Migrates field resolution from one subgraph to another. `label` enables progressive override (v2.7+).

**`@tag(name: String!)`**
- Locations: All type system locations
- Repeatable: Yes
- Attaches metadata for contract schema filtering.

**`@composeDirective(name: String!)`**
- Locations: `SCHEMA`
- Repeatable: Yes
- Preserves a custom directive through composition.

**`@interfaceObject`**
- Locations: `OBJECT`
- Minimum version: v2.3
- Lets a subgraph contribute fields to an interface entity without knowing implementations.

**`@link(url: String!, import: [String], as: String, for: link__Purpose)`**
- Locations: `SCHEMA`
- Repeatable: Yes
- Opts into Federation v2 and imports directives. Presence of `@link` with a federation v2 URL is the definitive signal for "this is a v2 subgraph."

### Access Control Directives (v2.1+)

- `@authenticated` — Restricts to authenticated requests
- `@requiresScopes(scopes: [[Scope!]!]!)` — Requires OAuth scopes
- `@policy(policies: [[Policy!]!]!)` — Custom policy evaluation

### Demand Control Directives (v2.2+)

- `@cost(weight: Int)` — Cost units for demand control
- `@listSize(assumedSize: Int, slicingArguments: [String!], sizedFields: [String!], requireOneSlicingArgument: Boolean)` — Cardinality hints

### Built-in Runtime Types

These are injected by subgraph libraries at runtime, not written by developers:

- `scalar _Any` — Arbitrary JSON for entity representations
- `scalar FieldSet` (or `_FieldSet`) — String-serialized selection set
- `type _Service { sdl: String! }` — Subgraph SDL introspection
- `union _Entity = ...` — Union of all entity types with `resolvable: true`
- `Query._entities(representations: [_Any!]!): [_Entity]!` — Entity lookup
- `Query._service: _Service!` — SDL retrieval

## Appendix B: Vendor Config Format Comparison

| Aspect | Apollo Rover | WunderGraph Cosmo |
|---|---|---|
| File | `supergraph.yaml` | `graph.yaml` |
| Subgraph structure | Named map (`subgraphs.<name>`) | Array with `name` field |
| Local schema | `schema.file` | `schema.file` |
| Introspection | `schema.subgraph_url` + `introspection_headers` | `introspection.url` + `introspection.headers` |
| Registry | `schema.graphref` + `schema.subgraph` | N/A (uses Cosmo Cloud CLI) |
| Version field | `federation_version` | `version` (config format version) |
| Env var syntax | `${env.VAR_NAME}` | `${env.VAR_NAME}` |
| Subscriptions | N/A | `subscription.url` + `subscription.protocol` |
| Feature flags | N/A | `feature_flags[]` |

## Appendix C: Key Composition Errors

Errors that require multiple subgraph schemas to detect (Supergraph mode only):

| Error Code | Description |
|---|---|
| `INVALID_FIELD_SHARING` | Field in multiple subgraphs without `@shareable` |
| `EXTERNAL_TYPE_MISMATCH` | `@external` field type differs from owning subgraph |
| `EXTERNAL_MISSING_ON_BASE` | `@external` field not defined in any other subgraph |
| `FIELD_TYPE_MISMATCH` | Shared field has incompatible types across subgraphs |
| `EMPTY_MERGED_ENUM_TYPE` | Enum has no common values across subgraphs |
| `INTERFACE_FIELD_NO_IMPLEM` | Merged interface field missing implementation |
| `EXTENSION_WITH_NO_BASE` | Type extended but never defined as base |
| `NO_QUERIES` | Composed supergraph has no query fields |
| `REFERENCED_INACCESSIBLE` | Accessible type references `@inaccessible` type |
| `ONLY_INACCESSIBLE_CHILDREN` | Type has only `@inaccessible` members |
