# Ignoring Lint Rules

You can suppress lint diagnostics on a per-case basis using inline comments, similar to ESLint's `// eslint-disable-next-line`.

## Syntax

Place a `# graphql-analyzer-ignore` comment on the line immediately before the code you want to suppress:

```graphql
# Suppress all lint rules on the next line
# graphql-analyzer-ignore
query {
  users {
    id
  }
}
```

To suppress specific rules only, add a colon followed by a comma-separated list of rule names:

```graphql
# Suppress only the no_deprecated rule
# graphql-analyzer-ignore: no_deprecated
query GetUser($id: ID!) {
  user(id: $id) {
    email
  }
}
```

Multiple rules can be listed:

```graphql
# graphql-analyzer-ignore: no_deprecated, require_id_field
query GetPost($id: ID!) {
  post(id: $id) {
    views
  }
}
```

## Behavior

- The comment **only affects the immediately following line**. A blank line between the comment and the code means the comment has no effect.
- Without rule names, **all** lint rules are suppressed for that line.
- With rule names, only the listed rules are suppressed; other rules still apply.

### Correct

```graphql
# graphql-analyzer-ignore
query {
  users {
    id
  }
}
```

### Incorrect (blank line gap)

```graphql
# graphql-analyzer-ignore

query {
  users {
    id
  }
} # NOT suppressed - blank line breaks the connection
```

## Unused Ignore Warnings

If an ignore comment doesn't actually suppress any diagnostic, the analyzer reports an `unused_ignore` warning. This helps keep your codebase clean by catching stale or unnecessary ignore comments.

Common causes:

- The code on the next line doesn't trigger any lint rules
- The specified rule name doesn't match any diagnostic on the next line
- There's a blank line between the comment and the target code

```graphql
# graphql-analyzer-ignore          # warning: Unused graphql-analyzer-ignore directive
query GetUser {
  user {
    id
    name
  }
}
```

## Rule Names

Rule names in ignore comments use `snake_case`, matching the internal rule identifiers:

| Rule Name                 | Description                           |
| ------------------------- | ------------------------------------- |
| `no_anonymous_operations` | Named operations required             |
| `no_deprecated`           | Deprecated field usage                |
| `redundant_fields`        | Fields duplicated by fragment spreads |
| `unused_variables`        | Unused query variables                |
| `unused_fragments`        | Unused fragment definitions           |
| `unused_fields`           | Schema fields never queried           |
| `unique_names`            | Duplicate operation/fragment names    |
| `require_id_field`        | Missing id field in selections        |
| `operation_name_suffix`   | Operation name suffix conventions     |

## Configuration vs. Ignore Comments

Use **configuration** (`.graphqlrc.yaml`) to disable rules project-wide. Use **ignore comments** for specific exceptions where a rule generally applies but a particular case is intentional.

```yaml
# .graphqlrc.yaml - disable a rule for the whole project
extensions:
  lint:
    extends: recommended
    rules:
      noDeprecated: off
```

```graphql
# Inline - suppress for one specific usage
# graphql-analyzer-ignore: no_deprecated
query GetLegacyUser($id: ID!) {
  user(id: $id) {
    oldEmail
  }
}
```
