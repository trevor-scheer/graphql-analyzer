---
description: GraphQL document model - fragment scope and validation rules
globs:
  - "crates/graphql-hir/**"
  - "crates/graphql-analysis/**"
  - "crates/graphql-ide/**"
---

# GraphQL Document Model

Fragment scope is **project-wide**, not file-scoped:

- Operations can reference fragments in other files
- Fragment spreads can reference other fragments (transitive dependencies)
- Fragment and operation names must be unique across the entire project

When validating operations, you MUST:

1. Include direct fragment dependencies
2. Recurse through fragment dependencies
3. Handle circular references
4. Validate against schema for all fragments in the chain

## Common Pitfall: Fragment Not Found

- Ensure fragment file is in `document_files()`
- Check `all_fragments()` includes the file
- Verify fragment name uniqueness
