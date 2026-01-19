---
name: sme-consultation
description: Consult SME agents before implementing features, fixing bugs, or making architecture changes. Use when working on LSP features, GraphQL validation, lint rules, VSCode extension, Salsa queries, CLI changes, or Rust API design.
user-invocable: true
---

# SME Agent Consultation

Before implementing features, fixing bugs, or making architecture changes, you MUST consult the relevant Subject Matter Expert agents in `.claude/agents/`.

## Work Type to Agent Mapping

| Work Type                                 | Required Agents                         |
| ----------------------------------------- | --------------------------------------- |
| **New LSP features**                      | `lsp.md`, `rust-analyzer.md`, `rust.md` |
| **GraphQL validation/linting**            | `graphql.md`, `apollo-rs.md`            |
| **VSCode extension changes**              | `vscode-extension.md`                   |
| **CLI tool changes**                      | `graphql-cli.md`                        |
| **Salsa/incremental computation**         | `salsa.md`, `rust-analyzer.md`          |
| **Salsa debugging (hangs, cache issues)** | `salsa.md`                              |
| **IDE UX features**                       | `graphiql.md`, `lsp.md`                 |
| **Apollo-specific patterns**              | `apollo-client.md`, `apollo-rs.md`      |
| **Rust API design**                       | `rust.md`                               |

## How to Consult

1. **Identify the work type** from the table above
2. **Read the relevant agent files** in `.claude/agents/`
3. **Apply the guidance** to your implementation
4. **Document what you learned** (see below)

Use the Task tool with `subagent_type=general-purpose` for deep consultation when needed.

## Documentation Requirements

### In User Communications

When proposing or explaining a solution, mention which agents were consulted:

> "I consulted the `lsp.md` and `rust-analyzer.md` agents for guidance on this feature. The LSP agent confirmed this follows the specification, and the rust-analyzer agent recommended using a Salsa query for incremental computation."

### In PR Descriptions

Include a "Consulted SME Agents" section:

```markdown
## Consulted SME Agents

- **lsp.md**: Confirmed `textDocument/definition` response format
- **rust-analyzer.md**: Recommended query-based architecture for goto definition
- **rust.md**: Advised on error handling patterns using `Result<Option<T>>`
```

### In Issue Comments

Note agent consultations when providing analysis:

> "After consulting the `graphql.md` agent, I can confirm this is expected behavior per section 5.8.3 of the GraphQL specification regarding fragment spread validation."

## Why This Matters

- **Traceability**: Users can understand reasoning behind decisions
- **Review Quality**: Reviewers know which domain expertise was applied
- **Knowledge Transfer**: Future sessions can see what guidance was relevant
- **Accountability**: Ensures agents are actually being consulted

## Available Agents

| Agent                 | Domain                                                            |
| --------------------- | ----------------------------------------------------------------- |
| `graphql.md`          | GraphQL spec compliance, validation rules, type system            |
| `apollo-client.md`    | Apollo Client patterns, caching, fragment colocation              |
| `rust-analyzer.md`    | Query-based architecture, Salsa, incremental computation          |
| `salsa.md`            | Salsa framework, database design, snapshot isolation, concurrency |
| `rust.md`             | Idiomatic Rust, ownership, error handling, API design             |
| `lsp.md`              | LSP specification, protocol messages, client compatibility        |
| `graphiql.md`         | IDE features, graphql-language-service, UX patterns               |
| `graphql-cli.md`      | CLI design, graphql-config, ecosystem tooling                     |
| `vscode-extension.md` | Extension development, activation, language client                |
| `apollo-rs.md`        | apollo-parser, apollo-compiler, error-tolerant parsing            |
