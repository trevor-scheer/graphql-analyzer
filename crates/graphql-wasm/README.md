# graphql-wasm

WebAssembly bindings for the GraphQL language service, enabling browser-based validation and linting without a server.

## Features

- **GraphQL Validation**: Validate documents against a schema
- **Linting**: Run custom lint rules
- **Zero Server**: Runs entirely in the browser
- **TypeScript Support**: Full type definitions included

## Installation

```bash
npm install @graphql-lsp/wasm
```

## Usage

```javascript
import init, { GraphQLValidator } from '@graphql-lsp/wasm';

async function main() {
    // Initialize the WASM module
    await init();

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
        console.log('Errors:', result.errors);
    }
}

main();
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
- `errors`: Only error diagnostics
- `warnings`: Only warning diagnostics
- `count`: Total number of diagnostics
- `errorCount`: Number of errors
- `warningCount`: Number of warnings

#### Methods

- `isValid()`: Returns true if no errors
- `hasDiagnostics()`: Returns true if any diagnostics

### Quick Functions

For one-off validations:

```javascript
import { quickValidate, quickLint, quickCheck, validateSchema } from '@graphql-lsp/wasm';

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
# Install wasm-pack
cargo install wasm-pack

# Build the package
wasm-pack build --target web crates/graphql-wasm

# Or for Node.js
wasm-pack build --target nodejs crates/graphql-wasm
```

## License

MIT OR Apache-2.0
