/**
 * GraphQL testing utilities for Jest and Vitest.
 *
 * @example
 * ```ts
 * import {
 *   createTestValidator,
 *   validateDocument,
 *   graphqlMatchers
 * } from '@graphql-lsp/testing';
 *
 * // Extend matchers (or use the setup files)
 * expect.extend(graphqlMatchers);
 *
 * const schema = `
 *   type Query {
 *     user(id: ID!): User
 *   }
 *   type User {
 *     id: ID!
 *     name: String!
 *   }
 * `;
 *
 * test('valid query', () => {
 *   const result = validateDocument(schema, `
 *     query GetUser {
 *       user(id: "1") { id name }
 *     }
 *   `);
 *   expect(result).toBeValidGraphQL();
 * });
 *
 * test('invalid query', () => {
 *   const result = validateDocument(schema, `
 *     query GetUser {
 *       user { nonexistent }
 *     }
 *   `);
 *   expect(result).toBeInvalidGraphQL();
 *   expect(result).toHaveGraphQLErrorMatching(/nonexistent/);
 * });
 * ```
 */

export * from "./core";
export * from "./matchers";
