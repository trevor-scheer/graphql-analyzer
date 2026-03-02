# Config Crate - Claude Guide

Guidance for working with project configuration.

---

## Configuration Format

```yaml
# .graphqlrc.yaml
schema: "schema.graphql"
documents: "src/**/*.{graphql,ts,tsx}"

extensions:
  lint: recommended
```

See `README.md` in this crate for multi-project and advanced configuration.

---

## Schema Loading

| Source | Crate        |
| ------ | ------------ |
| Local  | `config`     |
| Remote | `introspect` |
