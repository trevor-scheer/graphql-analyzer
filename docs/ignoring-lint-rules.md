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
# Suppress only the noDeprecated rule
# graphql-analyzer-ignore: noDeprecated
query GetUser($id: ID!) {
  user(id: $id) {
    email
  }
}
```

Multiple rules can be listed:

```graphql
# graphql-analyzer-ignore: noDeprecated, noAnonymousOperations
query {
  post(id: "1") {
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

## Partially Unused Ignore Directives

When you list multiple rules in an ignore comment and only some of them fire, the analyzer reports a separate warning for each unused rule rather than a blanket "unused ignore" message. This gives you precise feedback about which rules to remove.

For example:

```graphql
# graphql-analyzer-ignore: noDeprecated, requireIdField
views
```

If only `noDeprecated` fires (because `views` is deprecated), the analyzer reports:

> Unused rule 'requireIdField' in graphql-analyzer-ignore directive

The `noDeprecated` suppression still takes effect — only the unnecessary rule is flagged.

If **all** listed rules are unused (none of them fire on the next line), you get the standard blanket message instead:

> Unused graphql-analyzer-ignore directive

This distinction helps you tell apart stale directives (remove entirely) from over-broad ones (trim the rule list).

## Rule Names

Rule names in ignore comments use `camelCase`, matching the config file format:

| Rule Name               | Description                           |
| ----------------------- | ------------------------------------- |
| `noAnonymousOperations` | Named operations required             |
| `noDeprecated`          | Deprecated field usage                |
| `redundantFields`       | Fields duplicated by fragment spreads |
| `unusedVariables`       | Unused query variables                |
| `unusedFragments`       | Unused fragment definitions           |
| `unusedFields`          | Schema fields never queried           |
| `uniqueNames`           | Duplicate operation/fragment names    |
| `requireIdField`        | Missing id field in selections        |
| `operationNameSuffix`   | Operation name suffix conventions     |

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
# graphql-analyzer-ignore: noDeprecated
query GetLegacyUser($id: ID!) {
  user(id: $id) {
    oldEmail
  }
}
```
