# graphql-types

Foundation types for the GraphQL LSP stack.

This crate provides shared types with zero external dependencies, making it suitable as a foundation layer that all other crates can depend on.

## Type Categories

- **File types**: `FileId`, `FileUri`, `Language`, `DocumentKind`
- **Position types**: `Position`, `Range`, `OffsetRange`
- **Severity types**: `DiagnosticSeverity`, `RuleSeverity`
- **Edit types**: `TextEdit`, `CodeFix`

## Key Concepts

### Language vs DocumentKind

These two enums represent orthogonal dimensions:

- **Language**: Determines HOW to parse a file (direct GraphQL vs. extraction from template literals)
- **DocumentKind**: Determines WHAT to do with the content (schema definitions vs. executable documents)

A TypeScript file can contain either schema definitions or executable documents, depending on how it's configured in `.graphqlrc.yaml`.
