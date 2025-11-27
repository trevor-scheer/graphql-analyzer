# GraphQL LSP Test Workspace - Pokemon Edition

A comprehensive test workspace for the GraphQL LSP implementation featuring a Pokemon-themed schema with realistic project structure.

## Project Structure

```
test-workspace/
├── schema.graphql              # Main Pokemon GraphQL schema
├── graphql.config.yaml         # GraphQL configuration
└── src/
    ├── fragments/              # Reusable GraphQL fragments
    │   ├── pokemon.graphql     # Pokemon-related fragments
    │   ├── trainer.graphql     # Trainer-related fragments
    │   └── battle.graphql      # Battle-related fragments
    ├── queries/                # Query operations
    │   ├── pokemon-queries.graphql
    │   ├── trainer-queries.graphql
    │   ├── battle-queries.graphql
    │   └── misc-queries.graphql
    ├── mutations/              # Mutation operations
    │   ├── trainer-mutations.graphql
    │   └── battle-mutations.graphql
    ├── services/               # TypeScript services with embedded GraphQL
    │   ├── pokemon-service.ts
    │   ├── trainer-service.ts
    │   └── battle-service.ts
    ├── components/             # React components with GraphQL queries
    │   ├── PokemonCard.tsx
    │   ├── TrainerProfile.tsx
    │   └── BattleViewer.tsx
    └── types/
        └── graphql-types.ts    # TypeScript type definitions
```

## Schema Overview

The Pokemon schema includes:

- **Types**: Pokemon, Trainer, Battle, Item, Move, Ability
- **Enums**: PokemonType, Region, TrainerClass, BattleStatus
- **Unions**: EvolutionRequirement, BattleAction
- **Operations**: Queries, Mutations, and Subscriptions
- **Complexity**: Nested types, fragments, unions, and relationships

## Testing the Extension

### Option 1: From VS Code (Recommended)

1. Open the `editors/vscode` directory in VS Code:
   ```bash
   cd /Users/trevor/Repositories/graphql-lsp/editors/vscode
   code .
   ```

2. Press `F5` to launch the Extension Development Host

3. In the new VS Code window that opens, open this test workspace:
   ```
   File > Open Folder > Select test-workspace
   ```

4. Test various LSP features:
   - Open any `.graphql` file to see syntax highlighting
   - Open `.ts` or `.tsx` files to test embedded GraphQL support
   - Test fragment references (e.g., `...PokemonBasic` in queries)
   - Test goto definition from fragment spreads
   - Hover over types to see documentation
   - Check diagnostics for validation errors

### Option 2: Test the LSP Server Directly

Run the validation CLI:

```bash
cd /Users/trevor/Repositories/graphql-lsp
cargo build
target/debug/graphql validate test-workspace/
```

## What to Test

### Fragment References
- Navigate from `...PokemonBasic` to its definition in [src/fragments/pokemon.graphql](src/fragments/pokemon.graphql)
- Test cross-file fragment usage in queries and components

### Embedded GraphQL
- Open [src/services/pokemon-service.ts](src/services/pokemon-service.ts) to test TypeScript support
- Open [src/components/PokemonCard.tsx](src/components/PokemonCard.tsx) to test React components
- Verify that GraphQL inside `gql` template literals is recognized

### Validation
- Schema validation across all files
- Operation name uniqueness
- Fragment name uniqueness
- Type checking for fields and arguments

### Hover & Documentation
- Hover over Pokemon types to see descriptions
- Hover over fields to see documentation strings
- Check enum value descriptions

## Key Features to Validate

1. **Cross-file References**: Fragment usage across multiple files
2. **Nested Fragments**: Fragments using other fragments
3. **Union Types**: Battle actions with different types
4. **Complex Queries**: Multi-level nested queries with variables
5. **Embedded GraphQL**: GraphQL in TypeScript/JavaScript files
6. **Subscriptions**: Real-time battle updates

## Schema Highlights

- **18 Pokemon Types**: Fire, Water, Electric, etc.
- **9 Regions**: Kanto through Paldea
- **Complex Evolution System**: Level, Item, Trade, and Friendship requirements
- **Battle System**: Turn-based with multiple action types
- **Trainer Management**: Teams, badges, and statistics

## Adding New Test Cases

To add new test cases:

1. Create new `.graphql` files in appropriate directories
2. Add TypeScript files with embedded GraphQL in `src/services/` or `src/components/`
3. Reference existing fragments using `#import` or `...FragmentName`
4. Run validation to ensure no errors

## Common Test Scenarios

1. **Fragment Spread Navigation**:
   - Open [src/queries/pokemon-queries.graphql](src/queries/pokemon-queries.graphql:5)
   - Click on `...PokemonWithEvolution` to jump to definition

2. **Embedded GraphQL Validation**:
   - Open [src/services/trainer-service.ts](src/services/trainer-service.ts)
   - Verify no validation errors in `gql` blocks

3. **Complex Nested Queries**:
   - Open [src/components/BattleViewer.tsx](src/components/BattleViewer.tsx)
   - Check inline fragments and nested selections

4. **Subscription Support**:
   - View [src/components/BattleViewer.tsx](src/components/BattleViewer.tsx:57) for subscription example
