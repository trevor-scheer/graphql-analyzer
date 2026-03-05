---
description: Lint rule implementation patterns
paths:
  - "crates/graphql-linter/**"
---

# Lint Rule Patterns

- Each rule goes in its own file under `crates/graphql-linter/src/rules/`
- Use the `/adding-lint-rules` skill for the full implementation workflow
- Rules must have a unique name, severity, and documentation
- Always add both positive (should lint) and negative (should not lint) test cases
- Rules operate on the HIR, not raw syntax
