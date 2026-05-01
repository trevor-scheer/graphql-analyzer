---
graphql-analyzer-cli: patch
---

Add regression coverage so `graphql validate` keeps exiting non-zero when validation errors are reported, fixing the gap that let CI integrations silently pass on errors ([#1054](https://github.com/trevor-scheer/graphql-analyzer/pull/1054))
