---
description: Test organization and TestDatabase patterns
paths:
  - "**/tests/**"
  - "**/*_test.rs"
  - "crates/graphql-test-utils/**"
---

# Testing Conventions

- Use the `/testing-patterns` skill for detailed guidance
- Unit tests go in the same file as the code (`#[cfg(test)]` module)
- Integration tests go in `crates/<name>/tests/`
- Use `TestDatabase` from `graphql-test-utils` for tests that need Salsa queries
- Test names should describe the scenario: `test_fragment_spread_on_union_type`
- Always test both the happy path and error cases
- Run `/audit-tests` after writing new tests to self-review
