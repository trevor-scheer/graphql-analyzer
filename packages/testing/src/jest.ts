/**
 * Jest setup for GraphQL testing.
 *
 * Add this to your Jest configuration:
 *
 * ```js
 * // jest.config.js
 * module.exports = {
 *   setupFilesAfterEnv: ['@graphql-lsp/testing/jest'],
 * };
 * ```
 *
 * Or import in your setup file:
 *
 * ```ts
 * // jest.setup.ts
 * import '@graphql-lsp/testing/jest';
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

import { graphqlMatchers } from "./matchers";

// Extend Jest matchers
expect.extend(graphqlMatchers);

// Re-export everything for convenience
export * from "./core";
export * from "./matchers";
