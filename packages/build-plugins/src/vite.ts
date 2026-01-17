import type { Plugin, ResolvedConfig } from "vite";
import * as fs from "fs";
import * as path from "path";
import {
  type GraphQLPluginOptions,
  type ValidationContext,
  type ValidationResult,
  createContext,
  validateFile,
  formatResults,
  shouldIncludeFile,
} from "./shared";

export type { GraphQLPluginOptions };

/**
 * Vite plugin for GraphQL validation and linting.
 *
 * @example
 * ```ts
 * // vite.config.ts
 * import { graphqlPlugin } from '@graphql-lsp/build-plugins/vite';
 *
 * export default {
 *   plugins: [
 *     graphqlPlugin({
 *       schema: './schema.graphql',
 *       failOnError: true,
 *     }),
 *   ],
 * };
 * ```
 */
export function graphqlPlugin(options: GraphQLPluginOptions): Plugin {
  let context: ValidationContext;
  let config: ResolvedConfig;
  const results: Map<string, ValidationResult> = new Map();

  return {
    name: "graphql-lsp",

    configResolved(resolvedConfig) {
      config = resolvedConfig;
    },

    buildStart() {
      context = createContext(options);

      if (context.options.verbose) {
        console.log("[graphql-lsp] Plugin initialized");
        console.log(`[graphql-lsp] Schema: ${context.options.schema}`);
      }
    },

    transform(code, id) {
      if (!shouldIncludeFile(id, context.options)) {
        return null;
      }

      const result = validateFile(context, id, code);
      results.set(id, result);

      if (result.diagnostics.length > 0 && context.options.verbose) {
        console.log(`[graphql-lsp] Validated ${id}:`);
        for (const d of result.diagnostics) {
          console.log(`  ${d.severity}: ${d.message}`);
        }
      }

      // In dev mode, emit warnings but don't fail
      if (config.command === "serve") {
        if (result.hasErrors || result.hasWarnings) {
          this.warn(formatResults([result]));
        }
        return null;
      }

      // In build mode, optionally fail on errors/warnings
      if (result.hasErrors && context.options.failOnError) {
        this.error(formatResults([result]));
      }

      if (result.hasWarnings && context.options.failOnWarning) {
        this.error(formatResults([result]));
      }

      return null;
    },

    buildEnd() {
      const allResults = Array.from(results.values());
      const hasErrors = allResults.some((r) => r.hasErrors);
      const hasWarnings = allResults.some((r) => r.hasWarnings);

      if (context.options.verbose || hasErrors || hasWarnings) {
        console.log("[graphql-lsp] Validation summary:");
        console.log(formatResults(allResults));
      }

      results.clear();
    },

    // Watch mode: validate on file change
    handleHotUpdate({ file, read }) {
      if (!shouldIncludeFile(file, context.options)) {
        return;
      }

      read().then((content) => {
        const result = validateFile(context, file, content);
        results.set(file, result);

        if (result.diagnostics.length > 0) {
          console.log(`\n[graphql-lsp] ${file}:`);
          for (const d of result.diagnostics) {
            const line = d.startLine + 1;
            const col = d.startColumn + 1;
            console.log(`  ${d.severity} (${line}:${col}): ${d.message}`);
          }
        }
      });
    },
  };
}

export default graphqlPlugin;
