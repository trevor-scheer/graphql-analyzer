# Linter Crate - Claude Guide

Guidance for working with the lint rules system.

---

## Adding Lint Rules

Use the `/adding-lint-rules` skill for the full guided workflow.

Rules live in `src/rules/`. Each rule is a separate module.

---

## Configuration

Lint config uses `extensions.lint` in `.graphqlrc.yaml` with camelCase rule names:

```yaml
extensions:
  lint: recommended # Happy path - just use preset
```

Or configure individual rules:

```yaml
extensions:
  lint:
    noAnonymousOperations: error
    noDeprecated: warn
```
