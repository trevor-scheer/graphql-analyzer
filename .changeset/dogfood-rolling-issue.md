---
graphql-analyzer-cli: patch
---

Surface a `misnamespaced-extension` warning when an analyzer-specific config key (`lint`, `client`, `extractConfig`, `resolvedSchema`) appears at the top of `extensions:` rather than under `extensions.graphql-analyzer.*`. Previously the loader silently ignored these blocks, masking the misconfiguration entirely. Also flags the legacy camelCase `graphqlAnalyzer:` namespace key. The CLI prints these warnings up-front from `graphql check`, `graphql lint`, and other commands; the LSP surfaces them as config-file diagnostics.
