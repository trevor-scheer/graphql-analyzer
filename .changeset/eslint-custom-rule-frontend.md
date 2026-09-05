---
graphql-analyzer-core: minor
graphql-analyzer-eslint-plugin: minor
---

Support GraphQL-ESLint custom rules with compatible parser services and embedded source mapping ([#1134](https://github.com/trevor-scheer/graphql-analyzer/pull/1134)).

The default parser exposes GraphQL visitors, graphql-js nodes and type information, schema and sibling services, and public rule types and helpers. The processor preserves host-language linting and maps safe custom-rule fixes and suggestions back to embedded source. Add explicit `fastParser` and `fastProcessor` entry points for native-only linting. Install the `graphql` peer dependency at `^16.5.0`.

Refresh native source overlays and known disk dependencies, recover from config changes and failed reloads, and add the core `reset()` API. Native ESLint rule-option forwarding and multi-project config support remain outside this change.
