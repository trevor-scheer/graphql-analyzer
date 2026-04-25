# ESLint Migration Demo

This workspace demonstrates migrating from `@graphql-eslint/eslint-plugin`
to `@graphql-analyzer/eslint-plugin`.

## Setup

```bash
npm install
```

## The Migration

The migration is a find-and-replace in your ESLint config:

1. Replace `@graphql-eslint/eslint-plugin` with `@graphql-analyzer/eslint-plugin`
2. Replace `@graphql-eslint` with `@graphql-analyzer` in plugin names and rule prefixes

That's it. Rule names and options are identical.

## Compare

Run linting with the old plugin:

```bash
npm run lint:before
```

Run linting with the new plugin:

```bash
npm run lint:after
```

Both should produce a similar set of diagnostics. `packages/eslint-plugin/test/parity.test.mjs`
enforces the exact parity guarantees (shared rule names fire on the same fixture files) and
documents intentional gaps.

## Files

| File                       | Purpose                                    |
| -------------------------- | ------------------------------------------ |
| `eslint.config.before.mjs` | Original graphql-eslint config             |
| `eslint.config.after.mjs`  | Migrated graphql-analyzer config           |
| `eslint.config.mjs`        | Active config (uses the "after" config)    |
| `schema.graphql`           | Sample schema with intentional lint issues |
| `src/operations.graphql`   | Sample operations with lint issues         |
| `src/component.tsx`        | Embedded GraphQL in TypeScript             |
