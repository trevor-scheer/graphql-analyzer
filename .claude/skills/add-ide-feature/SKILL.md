---
name: add-ide-feature
description: Add new IDE/LSP features like hover, goto definition, find references, or completion. Use when implementing editor features, LSP handlers, or IDE functionality.
user-invocable: true
---

# Adding IDE Features

Follow this workflow when implementing new LSP/IDE features.

## Architecture Overview

```
User Action → LSP Handler → graphql-ide → graphql-analysis → graphql-hir → graphql-syntax
                  ↓
            Response to Editor
```

## Step-by-Step Process

### 1. Understand the LSP Method

First, check the LSP specification for the method you're implementing:

| Feature          | LSP Method                    | Response Type                  |
| ---------------- | ----------------------------- | ------------------------------ |
| Hover            | `textDocument/hover`          | `Hover`                        |
| Goto Definition  | `textDocument/definition`     | `Location` or `LocationLink[]` |
| Find References  | `textDocument/references`     | `Location[]`                   |
| Completion       | `textDocument/completion`     | `CompletionItem[]`             |
| Document Symbols | `textDocument/documentSymbol` | `DocumentSymbol[]`             |

### 2. Define the POD Type

Add your result type in `crates/graphql-ide/src/types.rs`:

```rust
// Use plain-old-data types - no Salsa dependencies
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YourFeatureResult {
    pub location: Location,
    pub info: String,
}
```

### 3. Implement the Query

Add the feature method in `crates/graphql-ide/src/lib.rs` on `Analysis`:

```rust
impl Analysis {
    pub fn your_feature(
        &self,
        file: &FilePath,
        position: Position,
    ) -> Option<YourFeatureResult> {
        let file_id = self.file_id(file)?;
        let db = &*self.db;

        // Use HIR/analysis queries to compute result
        // ...

        Some(YourFeatureResult { /* ... */ })
    }
}
```

### 4. Add Tests

Test in `crates/graphql-ide/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn your_feature_works() {
        let host = test_host_with_files(&[
            ("schema.graphql", "type Query { user: User }"),
            ("query.graphql", "query { user { id } }"),
        ]);
        let analysis = host.analysis();

        let result = analysis.your_feature(
            &FilePath::new("query.graphql"),
            Position { line: 0, column: 10 },
        );

        assert!(result.is_some());
    }
}
```

### 5. Wire Up the LSP Handler

In `crates/graphql-lsp/src/handlers/`:

```rust
pub async fn handle_your_feature(
    state: &ServerState,
    params: YourFeatureParams,
) -> Result<Option<YourFeatureResponse>> {
    let analysis = state.analysis();
    let file = uri_to_file_path(&params.text_document.uri)?;
    let position = lsp_position_to_ide(params.position);

    let result = analysis.your_feature(&file, position);

    Ok(result.map(|r| convert_to_lsp_response(r)))
}
```

### 6. Register in Server

In `crates/graphql-lsp/src/server.rs`, add to capabilities and router.

## SME Agents to Consult

Use `/sme-consultation` and consult:

- **lsp.md**: Protocol correctness, response format
- **rust-analyzer.md**: Query architecture, caching patterns
- **graphiql.md**: UX expectations, feature parity
- **rust.md**: Idiomatic implementation

## Checklist

- [ ] LSP spec reviewed for method
- [ ] POD type defined in graphql-ide
- [ ] Query implemented on Analysis
- [ ] Tests written and passing
- [ ] LSP handler wired up
- [ ] Server capabilities updated
- [ ] Works in both .graphql and embedded GraphQL
- [ ] SME agents consulted and documented
