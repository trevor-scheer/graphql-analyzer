# @graphql-lsp/testing

GraphQL testing utilities with custom matchers for Jest and Vitest.

## Features

- Custom matchers for GraphQL validation results
- Helper functions for common testing patterns
- Works with both Jest and Vitest
- Native performance via Rust-based validation

## Installation

```bash
npm install @graphql-lsp/testing @graphql-lsp/node
```

## Quick Setup

### Jest

```js
// jest.config.js
module.exports = {
  setupFilesAfterEnv: ['@graphql-lsp/testing/jest'],
};
```

### Vitest

```ts
// vitest.config.ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    setupFiles: ['@graphql-lsp/testing/vitest'],
  },
});
```

## Usage

### Basic Validation

```ts
import { validateDocument, checkSchema } from '@graphql-lsp/testing';

const schema = `
  type Query {
    user(id: ID!): User
  }
  type User {
    id: ID!
    name: String!
  }
`;

test('valid query', () => {
  const result = validateDocument(schema, `
    query GetUser {
      user(id: "1") { id name }
    }
  `);
  expect(result).toBeValidGraphQL();
});

test('invalid query - missing argument', () => {
  const result = validateDocument(schema, `
    query GetUser {
      user { id }
    }
  `);
  expect(result).toBeInvalidGraphQL();
  expect(result).toHaveGraphQLErrorMatching(/argument.*required/i);
});

test('schema is valid', () => {
  const result = checkSchema(schema);
  expect(result).toBeValidGraphQL();
});
```

### Reusable Test Validator

```ts
import { createTestValidator } from '@graphql-lsp/testing';

const validator = createTestValidator({
  schema: `
    type Query {
      users: [User!]!
    }
    type User {
      id: ID!
      name: String!
      email: String @deprecated(reason: "Use contactEmail instead")
    }
  `,
  lint: {
    'no_deprecated': 'warn',
  },
});

test('valid query', () => {
  const result = validator.validate(`query { users { id name } }`);
  expect(result).toBeValidGraphQL();
});

test('deprecated field warning', () => {
  const result = validator.check(`query { users { email } }`);
  expect(result).toBeValidGraphQL(); // Still valid, just has warnings
  expect(result).toHaveGraphQLWarnings();
  expect(result).toHaveGraphQLWarningMatching(/deprecated/);
});
```

## Custom Matchers

All matchers work with `ValidationResult` objects:

| Matcher | Description |
|---------|-------------|
| `toBeValidGraphQL()` | Document has no errors |
| `toBeInvalidGraphQL()` | Document has errors |
| `toHaveGraphQLErrors()` | Result contains errors |
| `toHaveNoGraphQLErrors()` | Result contains no errors |
| `toHaveGraphQLWarnings()` | Result contains warnings |
| `toHaveNoGraphQLWarnings()` | Result contains no warnings |
| `toHaveNoGraphQLDiagnostics()` | Result has no diagnostics |
| `toHaveGraphQLErrorMatching(pattern)` | Has error matching string/regex |
| `toHaveGraphQLWarningMatching(pattern)` | Has warning matching string/regex |
| `toHaveGraphQLErrorCount(n)` | Has exactly n errors |
| `toHaveGraphQLWarningCount(n)` | Has exactly n warnings |

### Examples

```ts
// Check document is valid
expect(result).toBeValidGraphQL();

// Check for specific error
expect(result).toHaveGraphQLErrorMatching(/field.*not found/i);

// Check error count
expect(result).toHaveGraphQLErrorCount(2);

// Combine matchers
expect(result).toBeInvalidGraphQL();
expect(result).toHaveGraphQLErrorMatching(/Unknown field/);
expect(result).toHaveNoGraphQLWarnings();
```

## Helper Functions

### Core Helpers

```ts
import {
  validateDocument,  // Validate against schema
  lintDocument,      // Lint only
  checkDocument,     // Validate + lint
  checkSchema,       // Validate schema SDL
} from '@graphql-lsp/testing';
```

### Result Helpers

```ts
import {
  getErrorMessages,    // Get all error messages
  getWarningMessages,  // Get all warning messages
  getAllMessages,      // Get all diagnostic messages
  hasErrorMessage,     // Check for error pattern
  hasWarningMessage,   // Check for warning pattern
} from '@graphql-lsp/testing';

const result = validateDocument(schema, document);

// Get messages for custom assertions
const errors = getErrorMessages(result);
expect(errors).toContain('Expected message');

// Pattern matching
expect(hasErrorMessage(result, /Unknown field/)).toBe(true);
```

## Manual Matcher Setup

If you prefer not to use the setup files:

```ts
import { graphqlMatchers } from '@graphql-lsp/testing/matchers';

// In your setup file
expect.extend(graphqlMatchers);
```

## TypeScript Support

Full TypeScript support with type augmentation for matchers:

```ts
// Types are automatically available after setup
const result = validateDocument(schema, document);
expect(result).toBeValidGraphQL(); // ✅ Type-safe
```

## License

MIT OR Apache-2.0
