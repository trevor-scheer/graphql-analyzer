import type { ValidationResult } from "@graphql-lsp/node";

/**
 * Custom matchers for GraphQL validation testing.
 *
 * These matchers work with both Jest and Vitest.
 *
 * @example
 * ```ts
 * // vitest.setup.ts or jest.setup.ts
 * import { graphqlMatchers } from '@graphql-lsp/testing/matchers';
 * expect.extend(graphqlMatchers);
 *
 * // In tests:
 * expect(result).toBeValidGraphQL();
 * expect(result).toHaveGraphQLErrors();
 * expect(result).toHaveGraphQLErrorMatching(/field not found/);
 * ```
 */

export interface GraphQLMatchers<R = unknown> {
  /**
   * Assert that a GraphQL document is valid (no errors).
   */
  toBeValidGraphQL(): R;

  /**
   * Assert that a GraphQL document is invalid (has errors).
   */
  toBeInvalidGraphQL(): R;

  /**
   * Assert that a validation result has errors.
   */
  toHaveGraphQLErrors(): R;

  /**
   * Assert that a validation result has no errors.
   */
  toHaveNoGraphQLErrors(): R;

  /**
   * Assert that a validation result has warnings.
   */
  toHaveGraphQLWarnings(): R;

  /**
   * Assert that a validation result has no warnings.
   */
  toHaveNoGraphQLWarnings(): R;

  /**
   * Assert that a validation result has no diagnostics at all.
   */
  toHaveNoGraphQLDiagnostics(): R;

  /**
   * Assert that a validation result has an error matching the pattern.
   */
  toHaveGraphQLErrorMatching(pattern: string | RegExp): R;

  /**
   * Assert that a validation result has a warning matching the pattern.
   */
  toHaveGraphQLWarningMatching(pattern: string | RegExp): R;

  /**
   * Assert that a validation result has exactly N errors.
   */
  toHaveGraphQLErrorCount(count: number): R;

  /**
   * Assert that a validation result has exactly N warnings.
   */
  toHaveGraphQLWarningCount(count: number): R;
}

// Type augmentation for Jest
declare global {
  namespace jest {
    interface Matchers<R> extends GraphQLMatchers<R> {}
  }
}

// Type augmentation for Vitest
declare module "vitest" {
  interface Assertion<T = any> extends GraphQLMatchers<T> {}
  interface AsymmetricMatchersContaining extends GraphQLMatchers {}
}

function isValidationResult(value: unknown): value is ValidationResult {
  return (
    typeof value === "object" &&
    value !== null &&
    "diagnostics" in value &&
    "isValid" in value &&
    typeof (value as any).isValid === "function"
  );
}

function matchPattern(message: string, pattern: string | RegExp): boolean {
  return typeof pattern === "string"
    ? message.includes(pattern)
    : pattern.test(message);
}

export const graphqlMatchers = {
  toBeValidGraphQL(received: unknown) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const pass = received.isValid();
    const errors = received.errors();

    return {
      pass,
      message: () =>
        pass
          ? "Expected GraphQL document to be invalid, but it was valid"
          : `Expected GraphQL document to be valid, but got ${errors.length} error(s):\n${errors.map((e) => `  - ${e.message}`).join("\n")}`,
    };
  },

  toBeInvalidGraphQL(received: unknown) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const pass = !received.isValid();

    return {
      pass,
      message: () =>
        pass
          ? "Expected GraphQL document to be valid, but it was invalid"
          : "Expected GraphQL document to be invalid, but it was valid",
    };
  },

  toHaveGraphQLErrors(received: unknown) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const errors = received.errors();
    const pass = errors.length > 0;

    return {
      pass,
      message: () =>
        pass
          ? `Expected no GraphQL errors, but got ${errors.length}:\n${errors.map((e) => `  - ${e.message}`).join("\n")}`
          : "Expected GraphQL errors, but got none",
    };
  },

  toHaveNoGraphQLErrors(received: unknown) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const errors = received.errors();
    const pass = errors.length === 0;

    return {
      pass,
      message: () =>
        pass
          ? "Expected GraphQL errors, but got none"
          : `Expected no GraphQL errors, but got ${errors.length}:\n${errors.map((e) => `  - ${e.message}`).join("\n")}`,
    };
  },

  toHaveGraphQLWarnings(received: unknown) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const warnings = received.warnings();
    const pass = warnings.length > 0;

    return {
      pass,
      message: () =>
        pass
          ? `Expected no GraphQL warnings, but got ${warnings.length}:\n${warnings.map((w) => `  - ${w.message}`).join("\n")}`
          : "Expected GraphQL warnings, but got none",
    };
  },

  toHaveNoGraphQLWarnings(received: unknown) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const warnings = received.warnings();
    const pass = warnings.length === 0;

    return {
      pass,
      message: () =>
        pass
          ? "Expected GraphQL warnings, but got none"
          : `Expected no GraphQL warnings, but got ${warnings.length}:\n${warnings.map((w) => `  - ${w.message}`).join("\n")}`,
    };
  },

  toHaveNoGraphQLDiagnostics(received: unknown) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const diagnostics = received.diagnostics;
    const pass = diagnostics.length === 0;

    return {
      pass,
      message: () =>
        pass
          ? "Expected GraphQL diagnostics, but got none"
          : `Expected no GraphQL diagnostics, but got ${diagnostics.length}:\n${diagnostics.map((d) => `  - [${d.severity}] ${d.message}`).join("\n")}`,
    };
  },

  toHaveGraphQLErrorMatching(received: unknown, pattern: string | RegExp) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const errors = received.errors();
    const pass = errors.some((e) => matchPattern(e.message, pattern));

    return {
      pass,
      message: () =>
        pass
          ? `Expected no GraphQL error matching ${pattern}`
          : `Expected GraphQL error matching ${pattern}, but got:\n${errors.map((e) => `  - ${e.message}`).join("\n") || "  (no errors)"}`,
    };
  },

  toHaveGraphQLWarningMatching(received: unknown, pattern: string | RegExp) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const warnings = received.warnings();
    const pass = warnings.some((w) => matchPattern(w.message, pattern));

    return {
      pass,
      message: () =>
        pass
          ? `Expected no GraphQL warning matching ${pattern}`
          : `Expected GraphQL warning matching ${pattern}, but got:\n${warnings.map((w) => `  - ${w.message}`).join("\n") || "  (no warnings)"}`,
    };
  },

  toHaveGraphQLErrorCount(received: unknown, count: number) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const errors = received.errors();
    const pass = errors.length === count;

    return {
      pass,
      message: () =>
        `Expected ${count} GraphQL error(s), but got ${errors.length}`,
    };
  },

  toHaveGraphQLWarningCount(received: unknown, count: number) {
    if (!isValidationResult(received)) {
      return {
        pass: false,
        message: () => "Expected value to be a ValidationResult",
      };
    }

    const warnings = received.warnings();
    const pass = warnings.length === count;

    return {
      pass,
      message: () =>
        `Expected ${count} GraphQL warning(s), but got ${warnings.length}`,
    };
  },
};

export default graphqlMatchers;
