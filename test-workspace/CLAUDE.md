# GraphQL Analyzer - Test Workspace

This is the test workspace for the GraphQL Analyzer project. It contains sample GraphQL projects used for testing the LSP server, CLI, and IDE features.

## Project Structure

Each subdirectory is a separate GraphQL project configured in `.graphqlrc.yaml`:

| Project                    | Purpose                                     |
| -------------------------- | ------------------------------------------- |
| `pokemon/`                 | Basic schema + operations with Apollo       |
| `starwars/`                | Basic schema + operations with Apollo       |
| `github/`                  | Multi-file schema with TS documents         |
| `countries/`               | Remote schema via introspection             |
| `apollo-app/`              | Client schema extensions                    |
| `relay-app/`               | Relay-style project                         |
| `schema-extensions/`       | Schema with extension files                 |
| `misconfigured-schema/`    | Intentionally broken config (missing files) |
| `misconfigured-documents/` | Intentionally broken config (missing files) |

## LSP Plugin

This workspace has the `graphql-lsp` plugin enabled, pointing to the locally built binary from the parent project (`../target/debug/graphql-lsp`). Build it with:

```bash
cargo build --package graphql-lsp
```

## Usage

Run Claude Code from this directory to test LSP features against the sample projects.
