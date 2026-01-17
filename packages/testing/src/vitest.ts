/**
 * Vitest setup for GraphQL testing.
 *
 * Add this to your Vitest configuration:
 *
 * ```ts
 * // vitest.config.ts
 * import { defineConfig } from 'vitest/config';
 *
 * export default defineConfig({
 *   test: {
 *     setupFiles: ['@graphql-lsp/testing/vitest'],
 *   },
 * });
 * ```
 *
 * Or import in your setup file:
 *
 * ```ts
 * // vitest.setup.ts
 * import '@graphql-lsp/testing/vitest';
 * ```
 *
 * Then use the matchers in your tests:
 *
 * ```ts
 * import { validateDocument } from '@graphql-lsp/testing';
 *
 * test('valid query', () => {
 *   const result = validateDocument(schema, 'query { user { id } }');
 *   expect(result).toBeValidGraphQL();
 * });
 * ```
 */

import { expect } from "vitest";
import { graphqlMatchers } from "./matchers";

// Extend Vitest matchers
expect.extend(graphqlMatchers);

// Re-export everything for convenience
export * from "./core";
export * from "./matchers";
