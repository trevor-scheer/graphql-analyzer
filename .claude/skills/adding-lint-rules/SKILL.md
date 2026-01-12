---
name: adding-lint-rules
description: Add new GraphQL lint rules following project patterns. Use when implementing a lint rule, adding validation logic, or extending the linter with new checks.
user-invocable: true
---

# Adding Lint Rules

Follow this process when implementing a new lint rule for the GraphQL linter.

## Step 1: Choose the Rule Type

Select based on the context your rule needs:

| Rule Type | Use When | Context Available |
|-----------|----------|-------------------|
| `StandaloneDocumentRule` | No schema needed | Document AST only |
| `DocumentSchemaRule` | Schema validation needed | Document + Schema |
| `ProjectRule` | Cross-file analysis needed | All project files |

## Step 2: Create the Rule File

Create `crates/graphql-linter/src/rules/your_rule.rs`:

```rust
use crate::{DocumentSchemaRule, DocumentSchemaContext, Diagnostic};

pub struct YourRule;

impl DocumentSchemaRule for YourRule {
    fn name(&self) -> &'static str {
        "your_rule"
    }

    fn description(&self) -> &'static str {
        "What this rule checks"
    }

    fn check(&self, ctx: &DocumentSchemaContext) -> Vec<Diagnostic> {
        // Implementation
        vec![]
    }
}
```

## Step 3: Register the Rule

Add the rule to `crates/graphql-linter/src/rules/mod.rs`:

1. Add `mod your_rule;`
2. Add `pub use your_rule::YourRule;`
3. Register in the appropriate rule registry

## Step 4: Write Tests

Add tests in the same file or a dedicated test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn check(schema: &str, document: &str) -> Vec<Diagnostic> {
        // Test helper
    }

    #[test]
    fn detects_violation() {
        let diagnostics = check(
            "type Query { field: String }",
            "query { field }",
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn allows_valid_usage() {
        let diagnostics = check(
            "type Query { field: String }",
            "query { field }",
        );
        assert!(diagnostics.is_empty());
    }
}
```

## Step 5: Update Documentation

Add your rule to `crates/graphql-linter/README.md` with:
- Rule name and description
- Example of violation
- Example of correct usage
- Configuration options (if any)

## Step 6: Test Incrementally

Run tests as you develop:

```bash
cargo test --package graphql-linter your_rule
cargo test --package graphql-linter
cargo clippy --package graphql-linter
```

## Checklist

Before considering the rule complete:

- [ ] Rule type matches context requirements
- [ ] Rule file created with proper trait implementation
- [ ] Rule registered in `mod.rs`
- [ ] Tests cover violation detection
- [ ] Tests cover valid usage (no false positives)
- [ ] Edge cases tested (empty input, malformed input)
- [ ] Documentation updated in linter README
- [ ] `cargo test` passes
- [ ] `cargo clippy` is clean

## Performance Considerations

- Avoid expensive operations in hot paths
- Consider caching if rule needs repeated lookups
- For `ProjectRule`, be mindful of cross-file iteration cost
- Profile with `cargo bench` if rule is complex

## Reference

See existing rules in `crates/graphql-linter/src/rules/` for patterns:
- `no_deprecated.rs` - Simple field checking
- `require_id_field.rs` - Selection set analysis
- `redundant_fields.rs` - Cross-reference checking
