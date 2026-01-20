# graphql-apollo-ext

Extensions for `apollo-parser`: visitor pattern, name extraction, and collection utilities.

**Note**: This crate is specifically tied to `apollo-parser`'s CST types. For parser-agnostic
utilities, see `graphql-utils`.

## Features

- **Visitor pattern** (`CstVisitor`) - Traverse GraphQL CST nodes with custom logic
- **Name extraction** (`NameExt`, `TypeConditionExt`, `BaseTypeExt`) - Extract names without option chains
- **Definition iterators** (`DocumentExt`) - Filter operations, fragments, and type definitions
- **Collection utilities** - Collect variables, fragments, fields, directives from documents

## Usage

```rust
use graphql_apollo_ext::{CstVisitor, walk_document, collect_fragment_spreads};
use apollo_parser::Parser;

// Using the visitor pattern
struct FieldCounter(usize);

impl CstVisitor for FieldCounter {
    fn visit_field(&mut self, _field: &apollo_parser::cst::Field) {
        self.0 += 1;
    }
}

let tree = Parser::new("query { user { name } }").parse();
let mut counter = FieldCounter(0);
walk_document(&mut counter, &tree);
assert_eq!(counter.0, 2);

// Using collection utilities
let fragments = collect_fragment_spreads(&tree);
```

## Modules

- `visitor` - The `CstVisitor` trait and `walk_*` functions
- `names` - Extension traits for name extraction (`NameExt`, `RangeExt`, etc.)
- `definitions` - `DocumentExt` trait for filtering definitions
- `collectors` - Pre-built collectors for common patterns
