# Analysis Crate - Claude Guide

Guidance for working with the validation and diagnostics engine.

---

## Validation Requirements

When validating operations, you MUST:

1. Include direct fragment dependencies
2. Recurse through fragment dependencies
3. Handle circular references
4. Validate against schema for all fragments in the chain

Fragment scope is **project-wide** - operations can reference fragments defined in other files.

---

## Adding Validation

New validation logic goes in `src/`. Follow existing patterns for producing diagnostics.

See the `graphql.md` SME agent (`.claude/agents/graphql.md`) for GraphQL spec validation rules.
