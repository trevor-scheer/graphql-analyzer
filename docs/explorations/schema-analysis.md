# Schema Analysis Tools Exploration

**Issue**: #421
**Status**: Exploration
**Created**: 2026-01-17

## Overview

This document explores expanding the tooling beyond validation into schema analysis: comparing schemas, detecting breaking changes, and analyzing query complexity.

## Goals

1. Schema diff - understand what changed between versions
2. Breaking change detection - prevent accidental API breaks
3. Query complexity analysis - prevent expensive queries
4. Schema health metrics - track schema quality over time

## Feature 1: Schema Diff

### Use Cases

- Compare schema versions during PR review
- Understand changes between releases
- Document API evolution

### CLI Interface

```bash
# Compare two files
graphql schema diff --base schema-v1.graphql --head schema-v2.graphql

# Compare git refs
graphql schema diff --base main --head feature-branch

# Compare with remote schema
graphql schema diff --base https://api.example.com/graphql --head schema.graphql
```

### Output Format

```
Schema Diff: schema-v1.graphql → schema-v2.graphql

Added (3):
  + type NewType {
  +   id: ID!
  +   name: String!
  + }
  + Query.newField: NewType
  + enum Status { ACTIVE, INACTIVE }

Modified (2):
  ~ User.email: String → String! (nullability changed)
  ~ Post.status: String → Status (type changed)

Removed (1):
  - Query.deprecatedField: String
  - directive @oldDirective

Summary: 3 added, 2 modified, 1 removed
```

### JSON Output

```bash
graphql schema diff --format=json
```

```json
{
  "added": [
    { "kind": "type", "name": "NewType", "definition": "type NewType { ... }" },
    { "kind": "field", "type": "Query", "name": "newField", "fieldType": "NewType" }
  ],
  "modified": [
    {
      "kind": "field",
      "type": "User",
      "name": "email",
      "before": { "type": "String", "nullable": true },
      "after": { "type": "String!", "nullable": false }
    }
  ],
  "removed": [
    { "kind": "field", "type": "Query", "name": "deprecatedField" }
  ]
}
```

### Implementation

```rust
// crates/graphql-analysis/src/schema_diff.rs

pub struct SchemaDiff {
    pub added: Vec<SchemaChange>,
    pub modified: Vec<SchemaModification>,
    pub removed: Vec<SchemaChange>,
}

pub enum SchemaChange {
    Type { name: String, definition: String },
    Field { type_name: String, field_name: String, field_type: String },
    Directive { name: String },
    EnumValue { enum_name: String, value: String },
    InputField { type_name: String, field_name: String },
}

pub fn diff_schemas(base: &Schema, head: &Schema) -> SchemaDiff {
    // Compare type definitions
    // Compare fields within types
    // Compare directives
    // Compare enum values
}
```

## Feature 2: Breaking Change Detection

### Breaking Change Categories

#### Definite Breaking Changes (Error)

| Change | Why It Breaks |
|--------|---------------|
| Remove type | Queries using the type fail |
| Remove field | Queries selecting the field fail |
| Change field type incompatibly | Response shape changes |
| Add required argument | Existing queries missing argument fail |
| Remove enum value | Queries sending that value fail |
| Change interface/union members | Type conditions may fail |

#### Potentially Breaking Changes (Warning)

| Change | Risk |
|--------|------|
| Add nullable field | Strict clients may not handle |
| Change optional → required arg | Clients not sending it fail |
| Deprecate field/type | Works but should update |
| Change default value | May affect client behavior |

#### Safe Changes (Info)

| Change | Why It's Safe |
|--------|---------------|
| Add new type | No existing queries affected |
| Add optional field | Clients can ignore |
| Add enum value | Existing queries unaffected |
| Add optional argument | Existing queries work |

### CLI Interface

```bash
# Detect breaking changes
graphql schema breaking-changes --base main --head HEAD

# Fail CI on breaking changes
graphql schema breaking-changes --base main --head HEAD --fail-on-breaking
```

### Output

```
Breaking Change Analysis: main → HEAD

🔴 BREAKING (2):
  • Removed field Query.getUser
    Queries using this field will fail.

  • Changed User.age: Int → String
    Type changed incompatibly. Existing queries will receive wrong type.

🟡 POTENTIALLY BREAKING (1):
  • Added required argument Query.users(limit: Int!)
    Existing queries not passing 'limit' will fail.

🟢 SAFE (3):
  • Added type AdminUser
  • Added field User.avatar: String
  • Added enum value Status.ARCHIVED

Summary: 2 breaking, 1 potentially breaking, 3 safe
Exit code: 1 (breaking changes detected)
```

### Implementation

```rust
pub enum BreakingSeverity {
    Breaking,
    PotentiallyBreaking,
    Safe,
}

pub struct BreakingChange {
    pub severity: BreakingSeverity,
    pub change: SchemaChange,
    pub explanation: String,
}

pub fn detect_breaking_changes(base: &Schema, head: &Schema) -> Vec<BreakingChange> {
    let diff = diff_schemas(base, head);

    let mut changes = Vec::new();

    // Removals are breaking
    for removal in diff.removed {
        changes.push(BreakingChange {
            severity: BreakingSeverity::Breaking,
            change: removal,
            explanation: "Removed from schema".into(),
        });
    }

    // Check modifications for compatibility
    for modification in diff.modified {
        let severity = classify_modification(&modification);
        changes.push(BreakingChange {
            severity,
            change: modification.into(),
            explanation: explain_modification(&modification),
        });
    }

    changes
}
```

## Feature 3: Query Complexity Analysis

### Use Cases

- Prevent expensive queries in production
- Set query cost budgets
- Identify queries that need optimization

### Cost Model

Default cost model:
- Each field selection: 1
- List field: multiplied by expected size (or `first`/`limit` arg)
- Nested selections: multiplicative

```graphql
query GetUserPosts {
  user(id: "1") {          # cost: 1
    name                    # cost: 1
    posts(first: 10) {      # cost: 10 (list multiplier)
      title                 # cost: 10 (1 × 10)
      comments(first: 5) {  # cost: 50 (10 × 5)
        body                # cost: 50 (1 × 10 × 5)
      }
    }
  }
}
# Total: 1 + 1 + 10 + 10 + 50 + 50 = 122
```

### CLI Interface

```bash
# Analyze query complexity
graphql complexity query.graphql --schema schema.graphql

# With custom limits
graphql complexity query.graphql --schema schema.graphql --max-cost 100

# Analyze all documents
graphql complexity "src/**/*.graphql" --schema schema.graphql
```

### Output

```
Query Complexity Analysis

Query: GetUserPosts
  Total cost: 122

  Breakdown:
    user(id)           1
    ├─ name            1
    └─ posts(first:10) 10
       ├─ title        10 (1 × 10)
       └─ comments(first:5) 50 (10 × 5)
          └─ body      50 (1 × 10 × 5)

  ⚠️  Warning: Exceeds recommended cost of 100

  Suggestions:
  • Reduce posts limit from 10 to 8
  • Reduce comments limit from 5 to 3
  • Use pagination for deeply nested data
```

### Custom Cost Directives

Allow schema authors to specify costs:

```graphql
type Query {
  search(query: String!): [Result!]! @cost(weight: 10)
  user(id: ID!): User @cost(weight: 1)
}

type User {
  posts(first: Int): [Post!]! @cost(multiplier: "first", weight: 2)
}
```

### Implementation

```rust
pub struct ComplexityResult {
    pub total_cost: u64,
    pub breakdown: Vec<FieldCost>,
    pub exceeds_limit: bool,
    pub suggestions: Vec<String>,
}

pub struct FieldCost {
    pub path: Vec<String>,
    pub base_cost: u64,
    pub multiplier: u64,
    pub total: u64,
}

pub fn analyze_complexity(
    schema: &Schema,
    document: &ExecutableDocument,
    config: &ComplexityConfig,
) -> ComplexityResult {
    // Walk selection sets
    // Apply cost model
    // Track multipliers
    // Generate breakdown
}
```

## Feature 4: Schema Health Metrics

### Metrics

```bash
graphql schema stats schema.graphql
```

Output:
```
Schema Statistics

Types:
  Object types:     45
  Interface types:  8
  Union types:      3
  Enum types:       12
  Input types:      23
  Scalar types:     5
  Total:           96

Fields:
  Total fields:     312
  Deprecated:       14 (4.5%)
  Nullable:         198 (63.5%)
  Non-null:         114 (36.5%)

Relationships:
  Avg fields/type:  6.9
  Max depth:        8 (User → Posts → Comments → Author → ...)
  Circular refs:    2

Directives:
  @deprecated:      14 usages
  @custom:          5 usages

Documentation:
  Types with desc:  72/96 (75%)
  Fields with desc: 156/312 (50%)
```

## Architecture

```
┌─────────────────────────────────┐
│  CLI Commands                   │
│  - schema diff                  │
│  - schema breaking-changes      │
│  - complexity                   │
│  - schema stats                 │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│  graphql-analysis               │
│  - diff_schemas()               │
│  - detect_breaking_changes()    │
│  - analyze_complexity()         │
│  - schema_stats()               │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│  graphql-hir                    │
│  - Schema representation        │
│  - Type comparison              │
└─────────────────────────────────┘
```

## Open Questions

1. **Customizable breaking change rules?**
   - Some teams may want to allow certain "breaking" changes
   - Configuration file for rules?

2. **Complexity directive standard?**
   - Use existing `@cost` convention?
   - Support multiple cost models?

3. **Git integration depth?**
   - Just compare files?
   - Full git history analysis?
   - Schema changelog generation?

4. **Diff algorithm?**
   - Simple comparison vs semantic diff?
   - Handle renames/moves?

## Next Steps

1. [ ] Implement basic schema diff
2. [ ] Add breaking change detection
3. [ ] Implement cost model for complexity
4. [ ] Add CLI commands
5. [ ] Add schema stats command
6. [ ] Document cost configuration

## References

- [GraphQL Inspector](https://graphql-inspector.com/)
- [Apollo Schema Checks](https://www.apollographql.com/docs/studio/schema-checks/)
- [GraphQL Cost Analysis](https://github.com/pa-bru/graphql-cost-analysis)
- [Yelp's query complexity](https://engineeringblog.yelp.com/2018/05/graphql-query-cost-analysis.html)
