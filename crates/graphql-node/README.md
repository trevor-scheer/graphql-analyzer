# @graphql-lsp/node

Native Node.js bindings for the GraphQL language service, providing high-performance validation and linting.

## Features

- **Native Performance**: Written in Rust, compiled to native Node.js addon
- **GraphQL Validation**: Validate documents against a schema
- **Linting**: Run custom lint rules
- **TypeScript Support**: Full type definitions included
- **Cross-Platform**: Pre-built binaries for major platforms

## Installation

```bash
npm install @graphql-lsp/node
```

## Usage

```javascript
const { GraphQLValidator } = require('@graphql-lsp/node');

// Create a validator
const validator = new GraphQLValidator();

// Set your schema
validator.setSchema(`
    type Query {
        hello: String
        user(id: ID!): User
    }

    type User {
        id: ID!
        name: String!
    }
`);

// Validate a document
const result = validator.validate(`
    query GetUser {
        user(id: "123") {
            id
            name
        }
    }
`);

if (result.isValid()) {
    console.log('Document is valid!');
} else {
    console.log('Errors:', result.errors());
}
```

### TypeScript

```typescript
import { GraphQLValidator, ValidationResult, Diagnostic } from '@graphql-lsp/node';

const validator = new GraphQLValidator();
validator.setSchema(schema);

const result: ValidationResult = validator.validate(document);
const errors: Diagnostic[] = result.errors();
```

## API

### GraphQLValidator

The main class for validating GraphQL documents.

#### Methods

- `setSchema(sdl: string)`: Set the GraphQL schema (SDL format)
- `validate(document: string)`: Validate a document, returns `ValidationResult`
- `lint(document: string)`: Run lint rules, returns `ValidationResult`
- `check(document: string)`: Run both validation and linting
- `configureLint(config: object)`: Configure lint rules
- `reset()`: Reset the validator

### ValidationResult

Result of a validation or lint operation.

#### Properties

- `diagnostics`: All diagnostics
- `count`: Total number of diagnostics
- `errorCount`: Number of errors
- `warningCount`: Number of warnings

#### Methods

- `errors()`: Get only error diagnostics
- `warnings()`: Get only warning diagnostics
- `isValid()`: Returns true if no errors
- `hasDiagnostics()`: Returns true if any diagnostics

### Diagnostic

A single diagnostic message.

#### Properties

- `message`: The error/warning message
- `severity`: "error", "warning", "info", or "hint"
- `startLine`: Starting line (0-based)
- `startColumn`: Starting column (0-based)
- `endLine`: Ending line (0-based)
- `endColumn`: Ending column (0-based)
- `code`: Optional rule code

### Quick Functions

For one-off validations:

```javascript
const { quickValidate, quickLint, quickCheck, validateSchema } = require('@graphql-lsp/node');

// Validate a document
const result = quickValidate(schema, document);

// Lint a document
const lintResult = quickLint(schema, document);

// Validate and lint
const checkResult = quickCheck(schema, document);

// Validate schema only
const schemaResult = validateSchema(schema);
```

## Building from Source

```bash
# Install dependencies
npm install

# Build debug version
npm run build:debug

# Build release version
npm run build
```

### Cross-compilation

Use `@napi-rs/cli` for cross-compilation:

```bash
# Build for all platforms
npx napi build --platform --release --target x86_64-apple-darwin
npx napi build --platform --release --target aarch64-apple-darwin
npx napi build --platform --release --target x86_64-unknown-linux-gnu
npx napi build --platform --release --target x86_64-pc-windows-msvc
```

## Performance

The native bindings provide significant performance improvements over pure JavaScript implementations:

- **Cold start**: ~10-50ms to parse and validate a typical schema
- **Warm validation**: ~1-5ms per document (cached schema types)
- **Incremental**: Only re-validates changed portions

## License

MIT OR Apache-2.0
