---
graphql-analyzer-cli: patch
graphql-analyzer-lsp: patch
graphql-analyzer-core: patch
---

Fix `validate`, the LSP server, and the napi-based ESLint integration failing to resolve fragments defined in `.ts`/`.js` document files when the `gql` tag has no matching `import { gql } from ...` declaration. All three loading paths now default `extractConfig.allowGlobalIdentifiers` to `true` for files that the user has explicitly listed via `documents:`. Set `extensions.graphql-analyzer.extractConfig.allowGlobalIdentifiers: false` to opt back into the strict behavior. The napi loader additionally now reads `extractConfig` from the modern `extensions.graphql-analyzer.extractConfig` namespace (it was previously looking at the legacy `extensions.extractConfig`). ([#1035](https://github.com/trevor-scheer/graphql-analyzer/issues/1035))
