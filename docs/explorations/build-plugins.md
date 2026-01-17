# Build Tool Plugins Exploration

**Issue**: #424
**Status**: Exploration
**Created**: 2026-01-17

## Overview

This document explores creating plugins for popular JavaScript build tools (Vite, Webpack, esbuild) that validate GraphQL at build time.

## Goals

1. Catch GraphQL errors at build time, not runtime
2. Integrate with existing build toolchains
3. Provide clear error messages in dev server output
4. Support incremental builds efficiently

## Dependencies

This feature depends on:
- **Node.js bindings** (#419) - for native validation performance

## Plugin Architecture

All plugins share a common core:

```
┌─────────────────────────────────┐
│  @graphql-lsp/vite              │
│  @graphql-lsp/webpack           │  Build tool adapters
│  @graphql-lsp/esbuild           │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│  @graphql-lsp/build-core        │  Shared validation logic
│  - Schema loading               │
│  - File filtering               │
│  - Caching                      │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│  @graphql-lsp/core (napi)       │  Native bindings
└─────────────────────────────────┘
```

## Shared Configuration

```typescript
// @graphql-lsp/build-core/types.ts

export interface GraphQLValidateOptions {
  /**
   * Path to schema file or URL for introspection
   */
  schema: string;

  /**
   * Glob patterns for files to validate
   * @default ['**\/*.graphql', '**\/*.{ts,tsx,js,jsx}']
   */
  include?: string[];

  /**
   * Glob patterns for files to exclude
   * @default ['node_modules/**']
   */
  exclude?: string[];

  /**
   * Whether to fail the build on errors
   * @default true in production, false in development
   */
  failOnError?: boolean;

  /**
   * Whether to fail the build on warnings
   * @default false
   */
  failOnWarn?: boolean;

  /**
   * Lint configuration
   */
  lint?: {
    preset?: 'recommended' | 'strict' | 'none';
    rules?: Record<string, 'error' | 'warn' | 'off'>;
  };

  /**
   * GraphQL config file path (alternative to schema option)
   */
  configFile?: string;
}
```

## Plugin 1: Vite

### Usage

```typescript
// vite.config.ts
import { defineConfig } from 'vite';
import { graphqlValidate } from '@graphql-lsp/vite';

export default defineConfig({
  plugins: [
    graphqlValidate({
      schema: './schema.graphql',
      include: ['src/**/*.{ts,tsx,graphql}'],
      failOnError: true,
    })
  ]
});
```

### Implementation

```typescript
// @graphql-lsp/vite/index.ts
import { Plugin } from 'vite';
import { createValidator, GraphQLValidateOptions } from '@graphql-lsp/build-core';

export function graphqlValidate(options: GraphQLValidateOptions): Plugin {
  const validator = createValidator(options);

  return {
    name: 'graphql-validate',

    // Run in both serve and build
    apply: 'serve' | 'build',

    // Build start: load schema
    async buildStart() {
      await validator.loadSchema();
    },

    // Transform: validate each file
    async transform(code, id) {
      if (!validator.shouldProcess(id)) {
        return null;
      }

      const diagnostics = validator.validate(id, code);

      if (diagnostics.errors.length > 0) {
        // In dev: show overlay
        // In build: throw or warn based on config
        this.error({
          message: formatDiagnostics(diagnostics.errors),
          id,
        });
      }

      if (diagnostics.warnings.length > 0) {
        this.warn({
          message: formatDiagnostics(diagnostics.warnings),
          id,
        });
      }

      // Don't transform, just validate
      return null;
    },

    // HMR: re-validate on change
    handleHotUpdate({ file, server }) {
      if (validator.shouldProcess(file)) {
        const diagnostics = validator.validateFile(file);
        if (diagnostics.errors.length > 0) {
          server.ws.send({
            type: 'error',
            err: {
              message: formatDiagnostics(diagnostics.errors),
              stack: '',
              plugin: 'graphql-validate',
            }
          });
        }
      }
    },

    // Watch schema file for changes
    configureServer(server) {
      server.watcher.add(options.schema);
      server.watcher.on('change', async (file) => {
        if (file === options.schema) {
          await validator.reloadSchema();
          // Re-validate all files
          server.ws.send({ type: 'full-reload' });
        }
      });
    }
  };
}
```

### Error Display

Vite shows errors in browser overlay:

```
[graphql-validate] Validation Error

src/queries/user.ts:15:5

  Unknown field "nonExistent" on type "User"

    13 | const GET_USER = gql`
    14 |   query GetUser {
  > 15 |     user { nonExistent }
       |            ^^^^^^^^^^^
    16 |   }
    17 | `;
```

## Plugin 2: Webpack

### Usage

```javascript
// webpack.config.js
const { GraphQLValidatePlugin } = require('@graphql-lsp/webpack');

module.exports = {
  plugins: [
    new GraphQLValidatePlugin({
      schema: './schema.graphql',
      include: /\.(ts|tsx|graphql)$/,
    })
  ]
};
```

### Implementation

```typescript
// @graphql-lsp/webpack/index.ts
import { Compiler, Compilation } from 'webpack';
import { createValidator, GraphQLValidateOptions } from '@graphql-lsp/build-core';

export class GraphQLValidatePlugin {
  private options: GraphQLValidateOptions;
  private validator: ReturnType<typeof createValidator>;

  constructor(options: GraphQLValidateOptions) {
    this.options = options;
    this.validator = createValidator(options);
  }

  apply(compiler: Compiler) {
    const pluginName = 'GraphQLValidatePlugin';

    // Load schema at start
    compiler.hooks.beforeCompile.tapPromise(pluginName, async () => {
      await this.validator.loadSchema();
    });

    // Validate during compilation
    compiler.hooks.thisCompilation.tap(pluginName, (compilation) => {
      compilation.hooks.processAssets.tapPromise(
        {
          name: pluginName,
          stage: Compilation.PROCESS_ASSETS_STAGE_ADDITIONS,
        },
        async () => {
          for (const [name, source] of compilation.assets) {
            if (!this.validator.shouldProcess(name)) continue;

            const code = source.source().toString();
            const diagnostics = this.validator.validate(name, code);

            for (const error of diagnostics.errors) {
              compilation.errors.push(
                new WebpackError(formatDiagnostic(error))
              );
            }

            for (const warning of diagnostics.warnings) {
              compilation.warnings.push(
                new WebpackError(formatDiagnostic(warning))
              );
            }
          }
        }
      );
    });

    // Watch mode: re-validate changed files
    compiler.hooks.watchRun.tapPromise(pluginName, async (compiler) => {
      const changedFiles = compiler.modifiedFiles || new Set();

      for (const file of changedFiles) {
        if (file === this.options.schema) {
          await this.validator.reloadSchema();
        }
      }
    });
  }
}
```

## Plugin 3: esbuild

### Usage

```typescript
// build.ts
import * as esbuild from 'esbuild';
import { graphqlValidate } from '@graphql-lsp/esbuild';

await esbuild.build({
  entryPoints: ['src/index.ts'],
  bundle: true,
  plugins: [
    graphqlValidate({
      schema: './schema.graphql',
    })
  ],
});
```

### Implementation

```typescript
// @graphql-lsp/esbuild/index.ts
import { Plugin } from 'esbuild';
import { createValidator, GraphQLValidateOptions } from '@graphql-lsp/build-core';

export function graphqlValidate(options: GraphQLValidateOptions): Plugin {
  const validator = createValidator(options);

  return {
    name: 'graphql-validate',

    async setup(build) {
      // Load schema once
      await validator.loadSchema();

      // Filter for relevant files
      const filter = /\.(graphql|ts|tsx|js|jsx)$/;

      // Validate on load
      build.onLoad({ filter }, async (args) => {
        if (!validator.shouldProcess(args.path)) {
          return null;
        }

        const source = await fs.promises.readFile(args.path, 'utf8');
        const diagnostics = validator.validate(args.path, source);

        if (diagnostics.errors.length > 0) {
          return {
            errors: diagnostics.errors.map(d => ({
              text: d.message,
              location: {
                file: args.path,
                line: d.range.start.line + 1,
                column: d.range.start.character,
              },
            })),
          };
        }

        if (diagnostics.warnings.length > 0) {
          return {
            warnings: diagnostics.warnings.map(d => ({
              text: d.message,
              location: {
                file: args.path,
                line: d.range.start.line + 1,
                column: d.range.start.character,
              },
            })),
          };
        }

        // Don't transform, just validate
        return null;
      });
    },
  };
}
```

## Shared Core

```typescript
// @graphql-lsp/build-core/index.ts
import { Analysis } from '@graphql-lsp/core';
import { minimatch } from 'minimatch';

export interface ValidatorResult {
  errors: Diagnostic[];
  warnings: Diagnostic[];
}

export function createValidator(options: GraphQLValidateOptions) {
  const analysis = new Analysis();
  let schemaLoaded = false;

  return {
    async loadSchema() {
      if (options.schema.startsWith('http')) {
        // Introspect remote schema
        const sdl = await introspect(options.schema);
        analysis.setSchema('schema.graphql', sdl);
      } else {
        const content = await fs.promises.readFile(options.schema, 'utf8');
        analysis.setSchema(options.schema, content);
      }
      schemaLoaded = true;
    },

    async reloadSchema() {
      schemaLoaded = false;
      await this.loadSchema();
    },

    shouldProcess(file: string): boolean {
      const { include = ['**/*.graphql', '**/*.{ts,tsx,js,jsx}'], exclude = ['node_modules/**'] } = options;

      // Check exclusions first
      if (exclude.some(pattern => minimatch(file, pattern))) {
        return false;
      }

      // Check inclusions
      return include.some(pattern => minimatch(file, pattern));
    },

    validate(file: string, content: string): ValidatorResult {
      if (!schemaLoaded) {
        throw new Error('Schema not loaded. Call loadSchema() first.');
      }

      analysis.setDocument(file, content);
      const diagnostics = analysis.diagnostics(file);

      return {
        errors: diagnostics.filter(d => d.severity === 'error'),
        warnings: diagnostics.filter(d => d.severity === 'warning'),
      };
    },

    validateFile(file: string): ValidatorResult {
      const content = fs.readFileSync(file, 'utf8');
      return this.validate(file, content);
    },
  };
}
```

## Caching

For incremental builds, cache validation results:

```typescript
// @graphql-lsp/build-core/cache.ts

interface CacheEntry {
  hash: string;
  diagnostics: Diagnostic[];
  timestamp: number;
}

export class ValidationCache {
  private cache = new Map<string, CacheEntry>();

  get(file: string, content: string): Diagnostic[] | null {
    const entry = this.cache.get(file);
    if (!entry) return null;

    const hash = this.hash(content);
    if (entry.hash !== hash) return null;

    return entry.diagnostics;
  }

  set(file: string, content: string, diagnostics: Diagnostic[]) {
    this.cache.set(file, {
      hash: this.hash(content),
      diagnostics,
      timestamp: Date.now(),
    });
  }

  invalidateAll() {
    this.cache.clear();
  }

  private hash(content: string): string {
    return createHash('md5').update(content).digest('hex');
  }
}
```

## Mode-Based Configuration

```typescript
// Auto-detect based on NODE_ENV or explicit mode

graphqlValidate({
  schema: './schema.graphql',

  // Explicit mode
  mode: 'production', // or 'development', 'strict'

  // Or auto-detect
  // mode: process.env.NODE_ENV
});

// Mode presets:
// development: failOnError: false, lint: 'none'
// production: failOnError: true, lint: 'recommended'
// strict: failOnError: true, failOnWarn: true, lint: 'strict'
```

## Monorepo Support

```typescript
// For monorepos with multiple schemas
graphqlValidate({
  projects: {
    frontend: {
      schema: 'packages/frontend/schema.graphql',
      include: ['packages/frontend/**/*.{ts,tsx}'],
    },
    admin: {
      schema: 'packages/admin/schema.graphql',
      include: ['packages/admin/**/*.{ts,tsx}'],
    },
  },
});
```

## Package Structure

```
@graphql-lsp/build-core/
├── package.json
├── index.ts
├── types.ts
├── cache.ts
└── utils.ts

@graphql-lsp/vite/
├── package.json
├── index.ts
└── README.md

@graphql-lsp/webpack/
├── package.json
├── index.ts
└── README.md

@graphql-lsp/esbuild/
├── package.json
├── index.ts
└── README.md
```

## Open Questions

1. **Source maps**: How to map errors back to original positions?
   - Need to track GraphQL template literal locations
   - May need parser for template literal extraction

2. **Rollup support**: Should we add a Rollup plugin too?
   - Similar to Vite plugin API
   - Lower priority given Vite's popularity

3. **Build tool version support**:
   - Vite 4+? 5+?
   - Webpack 4? 5?
   - esbuild version requirements?

4. **Performance**: Validation on every transform?
   - May need to batch or debounce
   - Cache aggressively

5. **Watch mode schema refresh**: Auto-refresh remote schemas?
   - Poll interval?
   - Manual trigger?

## Next Steps

1. [ ] Create @graphql-lsp/build-core package
2. [ ] Implement Vite plugin
3. [ ] Implement Webpack plugin
4. [ ] Implement esbuild plugin
5. [ ] Add caching layer
6. [ ] Test with real projects
7. [ ] Add documentation

## References

- [Vite Plugin API](https://vitejs.dev/guide/api-plugin.html)
- [Webpack Plugin API](https://webpack.js.org/api/plugins/)
- [esbuild Plugin API](https://esbuild.github.io/plugins/)
- [graphql-eslint](https://github.com/B2o5T/graphql-eslint) - similar validation approach
