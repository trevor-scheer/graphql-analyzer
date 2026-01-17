import {
  GraphQLValidator,
  type ValidationResult,
  type Diagnostic,
  validateSchema,
  quickValidate,
  quickLint,
  quickCheck,
} from "@graphql-lsp/node";

export { GraphQLValidator, ValidationResult, Diagnostic };

/**
 * Options for creating a test validator.
 */
export interface TestValidatorOptions {
  /**
   * GraphQL schema SDL.
   */
  schema: string;

  /**
   * Lint rule configuration.
   */
  lint?: Record<string, "error" | "warn" | "off">;
}

/**
 * Create a reusable test validator with a pre-loaded schema.
 *
 * @example
 * ```ts
 * const validator = createTestValidator({
 *   schema: `
 *     type Query {
 *       user(id: ID!): User
 *     }
 *     type User {
 *       id: ID!
 *       name: String!
 *     }
 *   `,
 * });
 *
 * test('valid query', () => {
 *   const result = validator.validate(`
 *     query GetUser {
 *       user(id: "1") { id name }
 *     }
 *   `);
 *   expect(result.isValid()).toBe(true);
 * });
 * ```
 */
export function createTestValidator(options: TestValidatorOptions): GraphQLValidator {
  const validator = new GraphQLValidator();
  validator.setSchema(options.schema);

  if (options.lint) {
    validator.configureLint({ rules: options.lint });
  }

  return validator;
}

/**
 * Validate a GraphQL document against a schema.
 * Convenience function for one-off tests.
 *
 * @example
 * ```ts
 * const result = validateDocument(schema, `query { user { id } }`);
 * expect(result.isValid()).toBe(true);
 * ```
 */
export function validateDocument(
  schema: string,
  document: string
): ValidationResult {
  return quickValidate(schema, document);
}

/**
 * Lint a GraphQL document against a schema.
 * Convenience function for one-off tests.
 *
 * @example
 * ```ts
 * const result = lintDocument(schema, `query { user { id } }`);
 * expect(result.warnings()).toHaveLength(0);
 * ```
 */
export function lintDocument(
  schema: string,
  document: string
): ValidationResult {
  return quickLint(schema, document);
}

/**
 * Validate and lint a GraphQL document against a schema.
 * Convenience function for one-off tests.
 *
 * @example
 * ```ts
 * const result = checkDocument(schema, `query { user { id } }`);
 * expect(result.isValid()).toBe(true);
 * expect(result.hasDiagnostics()).toBe(false);
 * ```
 */
export function checkDocument(
  schema: string,
  document: string
): ValidationResult {
  return quickCheck(schema, document);
}

/**
 * Check if a schema SDL is valid.
 *
 * @example
 * ```ts
 * const result = checkSchema(`type Query { hello: String }`);
 * expect(result.isValid()).toBe(true);
 * ```
 */
export function checkSchema(schema: string): ValidationResult {
  return validateSchema(schema);
}

/**
 * Helper to extract error messages from a validation result.
 */
export function getErrorMessages(result: ValidationResult): string[] {
  return result.errors().map((d) => d.message);
}

/**
 * Helper to extract warning messages from a validation result.
 */
export function getWarningMessages(result: ValidationResult): string[] {
  return result.warnings().map((d) => d.message);
}

/**
 * Helper to extract all diagnostic messages from a validation result.
 */
export function getAllMessages(result: ValidationResult): string[] {
  return result.diagnostics.map((d) => d.message);
}

/**
 * Helper to check if a specific error message exists.
 */
export function hasErrorMessage(
  result: ValidationResult,
  pattern: string | RegExp
): boolean {
  return result.errors().some((d) =>
    typeof pattern === "string"
      ? d.message.includes(pattern)
      : pattern.test(d.message)
  );
}

/**
 * Helper to check if a specific warning message exists.
 */
export function hasWarningMessage(
  result: ValidationResult,
  pattern: string | RegExp
): boolean {
  return result.warnings().some((d) =>
    typeof pattern === "string"
      ? d.message.includes(pattern)
      : pattern.test(d.message)
  );
}
