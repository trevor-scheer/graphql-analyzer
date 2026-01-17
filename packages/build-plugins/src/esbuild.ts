import type { Plugin, OnLoadArgs, OnLoadResult, PluginBuild } from "esbuild";
import * as fs from "fs";
import * as path from "path";
import {
  type GraphQLPluginOptions,
  type ValidationContext,
  createContext,
  validateFile,
  formatResults,
  shouldIncludeFile,
} from "./shared";

export type { GraphQLPluginOptions };

/**
 * esbuild plugin for GraphQL validation and linting.
 *
 * @example
 * ```js
 * // esbuild.config.js
 * const { graphqlPlugin } = require('@graphql-lsp/build-plugins/esbuild');
 *
 * require('esbuild').build({
 *   entryPoints: ['src/index.ts'],
 *   bundle: true,
 *   plugins: [
 *     graphqlPlugin({
 *       schema: './schema.graphql',
 *       failOnError: true,
 *     }),
 *   ],
 * });
 * ```
 */
export function graphqlPlugin(options: GraphQLPluginOptions): Plugin {
  return {
    name: "graphql-lsp",

    setup(build: PluginBuild) {
      let context: ValidationContext;

      // Initialize context at build start
      build.onStart(() => {
        try {
          context = createContext(options);
          if (context.options.verbose) {
            console.log("[graphql-lsp] Plugin initialized");
            console.log(`[graphql-lsp] Schema: ${context.options.schema}`);
          }
        } catch (error) {
          return {
            errors: [
              {
                text: `Failed to initialize GraphQL plugin: ${error}`,
                pluginName: "graphql-lsp",
              },
            ],
          };
        }
      });

      // Validate .graphql and .gql files
      build.onLoad(
        { filter: /\.(graphql|gql)$/ },
        async (args: OnLoadArgs): Promise<OnLoadResult> => {
          const filePath = args.path;

          if (!shouldIncludeFile(filePath, context.options)) {
            // Return empty loader result to skip validation
            return {
              contents: fs.readFileSync(filePath, "utf-8"),
              loader: "text",
            };
          }

          const content = fs.readFileSync(filePath, "utf-8");
          const result = validateFile(context, filePath, content);

          if (context.options.verbose && result.diagnostics.length > 0) {
            console.log(`[graphql-lsp] Validated ${filePath}:`);
            for (const d of result.diagnostics) {
              console.log(`  ${d.severity}: ${d.message}`);
            }
          }

          // Convert diagnostics to esbuild format
          const errors = result.diagnostics
            .filter((d) => d.severity === "error")
            .map((d) => ({
              text: d.message,
              location: {
                file: filePath,
                line: d.startLine + 1,
                column: d.startColumn,
                length: d.endColumn - d.startColumn,
                lineText: content.split("\n")[d.startLine] || "",
              },
              pluginName: "graphql-lsp",
              notes: d.code ? [{ text: `Rule: ${d.code}` }] : [],
            }));

          const warnings = result.diagnostics
            .filter((d) => d.severity === "warning")
            .map((d) => ({
              text: d.message,
              location: {
                file: filePath,
                line: d.startLine + 1,
                column: d.startColumn,
                length: d.endColumn - d.startColumn,
                lineText: content.split("\n")[d.startLine] || "",
              },
              pluginName: "graphql-lsp",
              notes: d.code ? [{ text: `Rule: ${d.code}` }] : [],
            }));

          // Handle fail options
          if (context.options.failOnWarning) {
            errors.push(...warnings);
            warnings.length = 0;
          }

          if (!context.options.failOnError) {
            warnings.push(
              ...errors.map((e) => ({
                ...e,
                text: `[Error] ${e.text}`,
              }))
            );
            errors.length = 0;
          }

          return {
            contents: content,
            loader: "text",
            errors: errors.length > 0 ? errors : undefined,
            warnings: warnings.length > 0 ? warnings : undefined,
          };
        }
      );

      // Summary at build end
      build.onEnd((result) => {
        if (context?.options.verbose) {
          const errorCount = result.errors.length;
          const warningCount = result.warnings.length;
          console.log(
            `[graphql-lsp] Build complete: ${errorCount} error(s), ${warningCount} warning(s)`
          );
        }
      });
    },
  };
}

export default graphqlPlugin;
