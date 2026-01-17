# Testing Utilities Exploration

**Issue**: #422
**Status**: Exploration
**Created**: 2026-01-17

## Overview

This document explores providing testing utilities that integrate GraphQL validation into test frameworks like Jest and Vitest.

## Goals

1. Validate GraphQL during test execution
2. Provide custom matchers for GraphQL assertions
3. Enable snapshot testing for query structure
4. Generate mock data from operations

## Dependencies

This feature depends on:
- **Node.js bindings** (#419) - for native validation performance

## Feature 1: Jest/Vitest Transformer

### Overview

A transformer that validates GraphQL during test compilation, surfacing errors as test failures.

### Configuration

```javascript
// jest.config.js
module.exports = {
  transform: {
    // Transform .graphql files
    '\\.graphql$': '@graphql-lsp/jest',

    // Also extract and validate from TS/JS files
    '\\.[jt]sx?$': ['@graphql-lsp/jest', {
      extractFromSource: true,
      schema: './schema.graphql'
    }]
  }
};
```

```javascript
// vitest.config.ts
import { graphqlPlugin } from '@graphql-lsp/vitest';

export default {
  plugins: [
    graphqlPlugin({
      schema: './schema.graphql',
      validate: true,
      lint: true
    })
  ]
};
```

### Error Display

When validation fails, display as test failure:

```
FAIL src/queries/user.test.ts
  ● GraphQL Validation Error

    Unknown field "nonExistent" on type "User"

      1 | const GET_USER = gql`
      2 |   query GetUser {
    > 3 |     user { nonExistent }
        |            ^^^^^^^^^^^
      4 |   }
      5 | `;

    at Object.<anonymous> (src/queries/user.test.ts:3:12)
```

### Implementation

```typescript
// @graphql-lsp/jest/transformer.ts
import { validate } from '@graphql-lsp/core';
import { createTransformer } from '@jest/transform';

interface Options {
  schema: string;
  extractFromSource?: boolean;
  lint?: boolean;
}

export default createTransformer<Options>({
  process(sourceText, sourcePath, options) {
    const { schema, lint } = options.transformerConfig;

    // Load schema
    const schemaSource = fs.readFileSync(schema, 'utf8');

    // Validate
    const diagnostics = validate(schemaSource, sourceText);

    if (diagnostics.some(d => d.severity === 'error')) {
      // Format as Jest-friendly error
      throw new GraphQLValidationError(diagnostics, sourceText, sourcePath);
    }

    // Return compiled module
    return {
      code: `module.exports = ${JSON.stringify(sourceText)}`,
    };
  }
});
```

## Feature 2: Custom Matchers

### Overview

Jest/Vitest matchers for GraphQL assertions in tests.

### API

```typescript
import { matchers } from '@graphql-lsp/jest';

// Extend Jest/Vitest
expect.extend(matchers);

describe('GraphQL Queries', () => {
  test('query is valid against schema', () => {
    const query = gql`query { user { name } }`;
    expect(query).toBeValidGraphQL(schema);
  });

  test('query passes lint rules', () => {
    const query = gql`query GetUser { user { name } }`;
    expect(query).toPassLintRules(['require-operation-name', 'no-deprecated']);
  });

  test('query has expected complexity', () => {
    const query = gql`query { users { posts { comments } } }`;
    expect(query).toHaveComplexityBelow(100);
  });

  test('schema has no breaking changes', () => {
    expect(newSchema).toBeCompatibleWith(oldSchema);
  });
});
```

### Matcher Implementations

```typescript
// @graphql-lsp/jest/matchers.ts

export const matchers = {
  toBeValidGraphQL(received: string, schema: string) {
    const diagnostics = validate(schema, received);
    const errors = diagnostics.filter(d => d.severity === 'error');

    return {
      pass: errors.length === 0,
      message: () => errors.length > 0
        ? `Expected valid GraphQL but found errors:\n${formatDiagnostics(errors)}`
        : `Expected invalid GraphQL but document is valid`
    };
  },

  toPassLintRules(received: string, rules: string[]) {
    const diagnostics = lint(schema, received, { rules });

    return {
      pass: diagnostics.length === 0,
      message: () => diagnostics.length > 0
        ? `Expected to pass lint rules but found issues:\n${formatDiagnostics(diagnostics)}`
        : `Expected lint issues but none found`
    };
  },

  toHaveComplexityBelow(received: string, maxCost: number) {
    const result = analyzeComplexity(schema, received);

    return {
      pass: result.totalCost < maxCost,
      message: () => result.totalCost >= maxCost
        ? `Expected complexity below ${maxCost} but was ${result.totalCost}`
        : `Expected complexity above ${maxCost} but was ${result.totalCost}`
    };
  },

  toBeCompatibleWith(received: string, baseSchema: string) {
    const changes = detectBreakingChanges(baseSchema, received);
    const breaking = changes.filter(c => c.severity === 'breaking');

    return {
      pass: breaking.length === 0,
      message: () => breaking.length > 0
        ? `Expected compatible schema but found breaking changes:\n${formatChanges(breaking)}`
        : `Expected breaking changes but schemas are compatible`
    };
  }
};
```

### TypeScript Declarations

```typescript
// @graphql-lsp/jest/types.d.ts

declare global {
  namespace jest {
    interface Matchers<R> {
      toBeValidGraphQL(schema: string): R;
      toPassLintRules(rules: string[]): R;
      toHaveComplexityBelow(maxCost: number): R;
      toBeCompatibleWith(baseSchema: string): R;
    }
  }
}
```

## Feature 3: Snapshot Testing

### Overview

Capture query structure as snapshots to detect unintended changes.

### API

```typescript
import { toQuerySnapshot, toSchemaSnapshot } from '@graphql-lsp/jest';

test('GetUser query structure', () => {
  const query = gql`
    query GetUser($id: ID!) {
      user(id: $id) {
        name
        email
        posts {
          title
        }
      }
    }
  `;

  expect(toQuerySnapshot(query, schema)).toMatchSnapshot();
});

test('API schema structure', () => {
  expect(toSchemaSnapshot(schema)).toMatchSnapshot();
});
```

### Snapshot Output

```javascript
// __snapshots__/queries.test.ts.snap

exports[`GetUser query structure 1`] = `
Object {
  "name": "GetUser",
  "operation": "query",
  "variables": Array [
    Object {
      "name": "id",
      "type": "ID!",
    },
  ],
  "selections": Array [
    Object {
      "field": "user",
      "type": "User",
      "arguments": Array [
        Object { "name": "id", "value": "$id" },
      ],
      "selections": Array [
        Object { "field": "name", "type": "String" },
        Object { "field": "email", "type": "String" },
        Object {
          "field": "posts",
          "type": "[Post!]!",
          "selections": Array [
            Object { "field": "title", "type": "String" },
          ],
        },
      ],
    },
  ],
}
`;
```

### Schema Snapshot

```javascript
exports[`API schema structure 1`] = `
Object {
  "types": Object {
    "User": Object {
      "kind": "OBJECT",
      "fields": Array [
        Object { "name": "id", "type": "ID!" },
        Object { "name": "name", "type": "String" },
        Object { "name": "email", "type": "String" },
        Object { "name": "posts", "type": "[Post!]!" },
      ],
    },
    "Post": Object {
      "kind": "OBJECT",
      "fields": Array [
        Object { "name": "id", "type": "ID!" },
        Object { "name": "title", "type": "String" },
        Object { "name": "author", "type": "User!" },
      ],
    },
  },
  "queries": Array ["user", "users", "post", "posts"],
  "mutations": Array ["createUser", "updateUser", "deleteUser"],
}
`;
```

## Feature 4: Mock Data Generation

### Overview

Generate typed mock data from GraphQL operations for testing.

### API

```typescript
import { generateMock, createMockBuilder } from '@graphql-lsp/jest';

test('renders user data', () => {
  const mock = generateMock(schema, gql`
    query GetUser {
      user {
        name
        email
        posts {
          title
        }
      }
    }
  `);

  // mock = {
  //   user: {
  //     name: "mock-string-1",
  //     email: "mock-string-2",
  //     posts: [{
  //       title: "mock-string-3"
  //     }]
  //   }
  // }

  render(<UserProfile data={mock} />);
  expect(screen.getByText('mock-string-1')).toBeInTheDocument();
});
```

### Custom Mock Values

```typescript
const mockBuilder = createMockBuilder(schema)
  .withScalar('String', () => faker.lorem.word())
  .withScalar('Email', () => faker.internet.email())
  .withScalar('DateTime', () => faker.date.recent().toISOString())
  .withType('User', () => ({
    name: faker.person.fullName(),
    email: faker.internet.email(),
  }))
  .withListLength(3); // Default list length

const mock = mockBuilder.generate(gql`
  query GetUser {
    user { name email createdAt }
  }
`);
```

### Deterministic Mocks

```typescript
// Same seed = same mock data
const mock1 = generateMock(schema, query, { seed: 12345 });
const mock2 = generateMock(schema, query, { seed: 12345 });

expect(mock1).toEqual(mock2); // Deterministic
```

### Implementation

```typescript
// @graphql-lsp/jest/mock.ts

interface MockOptions {
  seed?: number;
  listLength?: number;
  maxDepth?: number;
  scalars?: Record<string, () => unknown>;
  types?: Record<string, () => unknown>;
}

export function generateMock(
  schema: string,
  document: string,
  options: MockOptions = {}
): unknown {
  const { seed, listLength = 1, maxDepth = 10 } = options;
  const rng = seed ? createSeededRandom(seed) : Math.random;

  // Parse query and schema
  const ast = parse(document);
  const schemaAst = buildSchema(schema);

  // Walk selections and generate values
  return generateSelectionSet(ast.definitions[0].selectionSet, schemaAst, {
    rng,
    listLength,
    depth: 0,
    maxDepth,
  });
}

function generateSelectionSet(
  selectionSet: SelectionSetNode,
  type: GraphQLObjectType,
  context: MockContext
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selectionSet.selections) {
    if (selection.kind === 'Field') {
      const field = type.getFields()[selection.name.value];
      result[selection.name.value] = generateValue(field.type, selection, context);
    }
  }

  return result;
}
```

## Package Structure

```
@graphql-lsp/jest/
├── package.json
├── index.ts           # Main exports
├── transformer.ts     # Jest transformer
├── matchers.ts        # Custom matchers
├── snapshot.ts        # Snapshot utilities
├── mock.ts            # Mock generation
└── types.d.ts         # TypeScript declarations

@graphql-lsp/vitest/
├── package.json
├── index.ts
├── plugin.ts          # Vitest plugin
├── matchers.ts        # Re-export from jest
├── snapshot.ts
└── mock.ts
```

## Open Questions

1. **Transformer vs Plugin**: Should validation happen at:
   - Transform time (compile error)?
   - Runtime (test failure)?
   - Both (configurable)?

2. **Schema loading**: How to handle:
   - Multiple schemas (multi-project)?
   - Remote schemas?
   - Schema generation (codegen)?

3. **Mock generation**:
   - Use faker.js or custom?
   - Handle circular references?
   - Support custom scalars automatically?

4. **Vitest compatibility**:
   - Share implementation with Jest?
   - Vitest-specific features?

## Next Steps

1. [ ] Create @graphql-lsp/jest package
2. [ ] Implement Jest transformer
3. [ ] Add custom matchers
4. [ ] Add snapshot utilities
5. [ ] Implement mock generation
6. [ ] Create Vitest variant
7. [ ] Add documentation and examples

## References

- [Jest Custom Transformers](https://jestjs.io/docs/code-transformation)
- [Jest Custom Matchers](https://jestjs.io/docs/expect#expectextendmatchers)
- [Vitest Plugins](https://vitest.dev/guide/plugins.html)
- [graphql-tools mocking](https://the-guild.dev/graphql/tools/docs/mocking)
