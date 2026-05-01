# Linter Crate - Claude Guide

Guidance for working with the lint rules system.

---

## Adding Lint Rules

Use the `/adding-lint-rules` skill for the full guided workflow.

Rules live in `src/rules/`. Each rule is a separate module.

---

## Configuration

Lint config lives under the `graphql-analyzer` extension namespace in
`.graphqlrc.yaml`, and uses camelCase rule names:

```yaml
extensions:
  graphql-analyzer:
    lint: recommended # Happy path - just use preset
```

Or configure individual rules:

```yaml
extensions:
  graphql-analyzer:
    lint:
      rules:
        noAnonymousOperations: error
        noDeprecated: warn
```

Putting the `lint:` block at the top of `extensions:` (without the
`graphql-analyzer:` namespace) is silently ignored by the loader — the
config validator emits a `misnamespaced-extension` warning when this happens.
