import { GraphQLError, Lexer, Source, TokenKind } from "graphql";
import { Location } from "graphql/language/ast";
import { parseGraphQLSDL } from "@graphql-tools/utils";
import { convertDocument } from "./converter";
import { createServices } from "./services";
import type { GraphQLESLintParseResult, ParserOptions } from "./types";

const legacyOptions = [
  "schema",
  "documents",
  "extensions",
  "include",
  "exclude",
  "projects",
  "schemaOptions",
  "graphQLParserOptions",
  "skipGraphQLConfig",
  "operations",
];

export function parseForESLint(
  code: string,
  options: ParserOptions = {},
): GraphQLESLintParseResult {
  for (const key of legacyOptions) {
    if (key in options)
      throw new Error(
        `\`parserOptions.${key}\` was removed in graphql-eslint@4. Use physical graphql-config or \`parserOptions.graphQLConfig\`.`,
      );
  }
  try {
    let { document } = parseGraphQLSDL(options.filePath ?? "document.graphql", code, {
      noLocation: false,
    });
    if (document && !document.loc) {
      const source = new Source(code, options.filePath ?? "document.graphql");
      const lexer = new Lexer(source);
      const start = lexer.token;
      while (lexer.advance().kind !== TokenKind.EOF) {
        /* Link comment tokens for empty documents. */
      }
      document = { ...document, loc: new Location(start, lexer.token, source) };
    }
    const services = createServices(options, document!);
    void services.schema;
    void services.siblingOperations;
    const { root, tokens, comments, visitorKeys } = convertDocument(
      document!,
      () => services.schema,
    );
    return {
      services,
      visitorKeys,
      ast: {
        type: "Program",
        sourceType: "script",
        body: [root],
        loc: root.loc,
        range: root.range,
        tokens,
        comments,
      },
    } as unknown as GraphQLESLintParseResult;
  } catch (error) {
    if (error instanceof GraphQLError) {
      throw Object.assign(new SyntaxError(`[graphql-analyzer] ${error.message}`), {
        index: error.positions?.[0],
        lineNumber: error.locations?.[0]?.line,
        column: error.locations?.[0]?.column,
      });
    }
    throw error;
  }
}
