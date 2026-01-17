import type { Compiler, Compilation } from "webpack";
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

const PLUGIN_NAME = "GraphQLLspPlugin";

/**
 * Webpack plugin for GraphQL validation and linting.
 *
 * @example
 * ```js
 * // webpack.config.js
 * const { GraphQLLspPlugin } = require('@graphql-lsp/build-plugins/webpack');
 *
 * module.exports = {
 *   plugins: [
 *     new GraphQLLspPlugin({
 *       schema: './schema.graphql',
 *       failOnError: true,
 *     }),
 *   ],
 * };
 * ```
 */
export class GraphQLLspPlugin {
  private options: GraphQLPluginOptions;
  private context: ValidationContext | null = null;

  constructor(options: GraphQLPluginOptions) {
    this.options = options;
  }

  apply(compiler: Compiler): void {
    const logger = compiler.getInfrastructureLogger(PLUGIN_NAME);

    // Initialize context at compile start
    compiler.hooks.beforeCompile.tapAsync(
      PLUGIN_NAME,
      (_params, callback) => {
        try {
          this.context = createContext(this.options);
          if (this.context.options.verbose) {
            logger.info("Plugin initialized");
            logger.info(`Schema: ${this.context.options.schema}`);
          }
          callback();
        } catch (error) {
          callback(error as Error);
        }
      }
    );

    // Validate files during compilation
    compiler.hooks.thisCompilation.tap(PLUGIN_NAME, (compilation) => {
      compilation.hooks.processAssets.tap(
        {
          name: PLUGIN_NAME,
          stage: Compilation.PROCESS_ASSETS_STAGE_ADDITIONAL,
        },
        () => {
          this.validateAllFiles(compilation, logger);
        }
      );
    });

    // Watch mode: validate changed files
    compiler.hooks.watchRun.tapAsync(PLUGIN_NAME, (compiler, callback) => {
      const changedFiles = compiler.modifiedFiles || new Set();
      const removedFiles = compiler.removedFiles || new Set();

      if (this.context?.options.verbose && changedFiles.size > 0) {
        logger.info(`Changed files: ${Array.from(changedFiles).join(", ")}`);
      }

      callback();
    });
  }

  private validateAllFiles(
    compilation: Compilation,
    logger: ReturnType<Compiler["getInfrastructureLogger"]>
  ): void {
    if (!this.context) {
      logger.error("Context not initialized");
      return;
    }

    const results: ValidationResult[] = [];
    const rootDir = compilation.compiler.context;

    // Find all GraphQL files in the compilation
    const graphqlFiles = this.findGraphQLFiles(rootDir);

    for (const filePath of graphqlFiles) {
      if (!shouldIncludeFile(filePath, this.context.options)) {
        continue;
      }

      try {
        const content = fs.readFileSync(filePath, "utf-8");
        const result = validateFile(this.context, filePath, content);
        results.push(result);

        if (result.diagnostics.length > 0 && this.context.options.verbose) {
          logger.info(`Validated ${filePath}:`);
          for (const d of result.diagnostics) {
            logger.info(`  ${d.severity}: ${d.message}`);
          }
        }
      } catch (error) {
        logger.warn(`Failed to read ${filePath}: ${error}`);
      }
    }

    // Report results
    const hasErrors = results.some((r) => r.hasErrors);
    const hasWarnings = results.some((r) => r.hasWarnings);

    if (hasErrors || hasWarnings) {
      const report = formatResults(results);

      if (hasErrors && this.context.options.failOnError) {
        compilation.errors.push(new Error(`GraphQL validation failed:\n${report}`));
      } else if (hasErrors) {
        compilation.warnings.push(new Error(`GraphQL validation errors:\n${report}`));
      }

      if (hasWarnings && this.context.options.failOnWarning) {
        compilation.errors.push(new Error(`GraphQL lint warnings:\n${report}`));
      } else if (hasWarnings && !hasErrors) {
        compilation.warnings.push(new Error(`GraphQL lint warnings:\n${report}`));
      }
    }

    if (this.context.options.verbose) {
      logger.info(`Validated ${results.length} GraphQL files`);
    }
  }

  private findGraphQLFiles(dir: string): string[] {
    const files: string[] = [];

    const walk = (currentDir: string) => {
      try {
        const entries = fs.readdirSync(currentDir, { withFileTypes: true });

        for (const entry of entries) {
          const fullPath = path.join(currentDir, entry.name);

          if (entry.isDirectory()) {
            if (entry.name !== "node_modules" && !entry.name.startsWith(".")) {
              walk(fullPath);
            }
          } else if (entry.isFile()) {
            const ext = path.extname(entry.name);
            if (ext === ".graphql" || ext === ".gql") {
              files.push(fullPath);
            }
          }
        }
      } catch {
        // Skip directories we can't read
      }
    };

    walk(dir);
    return files;
  }
}

export default GraphQLLspPlugin;
