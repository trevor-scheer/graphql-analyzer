import { GraphQLValidator, type Diagnostic } from "@graphql-lsp/node";
import * as fs from "fs";
import * as path from "path";

export interface GraphQLPluginOptions {
  /**
   * Path to the GraphQL schema file or SDL string.
   * Can be a file path or inline SDL.
   */
  schema: string;

  /**
   * Glob patterns for GraphQL files to validate.
   * @default ["**\/*.graphql", "**\/*.gql"]
   */
  include?: string[];

  /**
   * Glob patterns for files to exclude.
   * @default ["node_modules/**"]
   */
  exclude?: string[];

  /**
   * Whether to fail the build on validation errors.
   * @default true
   */
  failOnError?: boolean;

  /**
   * Whether to fail the build on lint warnings.
   * @default false
   */
  failOnWarning?: boolean;

  /**
   * Lint configuration object.
   */
  lint?: Record<string, "error" | "warn" | "off">;

  /**
   * Enable verbose logging.
   * @default false
   */
  verbose?: boolean;
}

export interface ValidationContext {
  validator: GraphQLValidator;
  options: Required<GraphQLPluginOptions>;
}

export function createDefaultOptions(
  options: GraphQLPluginOptions
): Required<GraphQLPluginOptions> {
  return {
    schema: options.schema,
    include: options.include ?? ["**/*.graphql", "**/*.gql"],
    exclude: options.exclude ?? ["node_modules/**"],
    failOnError: options.failOnError ?? true,
    failOnWarning: options.failOnWarning ?? false,
    lint: options.lint ?? {},
    verbose: options.verbose ?? false,
  };
}

export function createContext(options: GraphQLPluginOptions): ValidationContext {
  const resolvedOptions = createDefaultOptions(options);
  const validator = new GraphQLValidator();

  // Load schema
  let schemaSdl: string;
  if (
    resolvedOptions.schema.includes("{") ||
    resolvedOptions.schema.includes("type ")
  ) {
    // Inline SDL
    schemaSdl = resolvedOptions.schema;
  } else {
    // File path
    const schemaPath = path.resolve(resolvedOptions.schema);
    if (!fs.existsSync(schemaPath)) {
      throw new Error(`Schema file not found: ${schemaPath}`);
    }
    schemaSdl = fs.readFileSync(schemaPath, "utf-8");
  }

  validator.setSchema(schemaSdl);

  // Configure lint rules if provided
  if (Object.keys(resolvedOptions.lint).length > 0) {
    validator.configureLint({ rules: resolvedOptions.lint });
  }

  return {
    validator,
    options: resolvedOptions,
  };
}

export interface ValidationResult {
  file: string;
  diagnostics: Diagnostic[];
  hasErrors: boolean;
  hasWarnings: boolean;
}

export function validateFile(
  context: ValidationContext,
  filePath: string,
  content: string
): ValidationResult {
  const result = context.validator.check(content);

  const diagnostics = result.diagnostics;
  const hasErrors = result.errorCount > 0;
  const hasWarnings = result.warningCount > 0;

  return {
    file: filePath,
    diagnostics,
    hasErrors,
    hasWarnings,
  };
}

export function formatDiagnostic(file: string, diagnostic: Diagnostic): string {
  const severity = diagnostic.severity.toUpperCase();
  const location = `${file}:${diagnostic.startLine + 1}:${diagnostic.startColumn + 1}`;
  const code = diagnostic.code ? ` [${diagnostic.code}]` : "";
  return `${severity}${code}: ${diagnostic.message}\n  at ${location}`;
}

export function formatResults(results: ValidationResult[]): string {
  const lines: string[] = [];

  for (const result of results) {
    if (result.diagnostics.length === 0) continue;

    lines.push(`\n${result.file}:`);
    for (const diagnostic of result.diagnostics) {
      lines.push(`  ${formatDiagnostic(result.file, diagnostic)}`);
    }
  }

  const errorCount = results.reduce(
    (sum, r) => sum + r.diagnostics.filter((d) => d.severity === "error").length,
    0
  );
  const warningCount = results.reduce(
    (sum, r) => sum + r.diagnostics.filter((d) => d.severity === "warning").length,
    0
  );

  if (errorCount > 0 || warningCount > 0) {
    lines.push(`\n${errorCount} error(s), ${warningCount} warning(s)`);
  }

  return lines.join("\n");
}

export function shouldIncludeFile(
  filePath: string,
  options: Required<GraphQLPluginOptions>
): boolean {
  const ext = path.extname(filePath);
  if (ext !== ".graphql" && ext !== ".gql") {
    return false;
  }

  // Check exclude patterns
  for (const pattern of options.exclude) {
    if (matchGlob(filePath, pattern)) {
      return false;
    }
  }

  // Check include patterns
  for (const pattern of options.include) {
    if (matchGlob(filePath, pattern)) {
      return true;
    }
  }

  return false;
}

function matchGlob(filePath: string, pattern: string): boolean {
  // Simple glob matching - convert ** and * to regex
  const regex = new RegExp(
    "^" +
      pattern
        .replace(/\*\*/g, "<<<GLOBSTAR>>>")
        .replace(/\*/g, "[^/]*")
        .replace(/<<<GLOBSTAR>>>/g, ".*")
        .replace(/\//g, "\\/") +
      "$"
  );
  return regex.test(filePath);
}
