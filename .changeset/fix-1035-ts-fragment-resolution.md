---
graphql-analyzer-cli: patch
---

Fix `validate` failing to resolve fragments defined in `.ts`/`.js` document files when the `gql` tag has no matching `import { gql } from ...` declaration. The CLI now defaults `extractConfig.allowGlobalIdentifiers` to `true` for files that the user has explicitly listed via `documents:`. Set `extensions.graphql-analyzer.extractConfig.allowGlobalIdentifiers: false` to opt back into the strict behavior. ([#1035](https://github.com/trevor-scheer/graphql-analyzer/issues/1035))
