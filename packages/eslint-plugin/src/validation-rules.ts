import { relative, resolve } from "node:path";
import type { Rule, SourceCode } from "eslint";
import {
  ExecutableDefinitionsRule,
  FieldsOnCorrectTypeRule,
  FragmentsOnCompositeTypesRule,
  getLocation,
  GraphQLError,
  Kind,
  KnownArgumentNamesRule,
  KnownDirectivesRule,
  KnownFragmentNamesRule,
  KnownTypeNamesRule,
  LoneAnonymousOperationRule,
  LoneSchemaDefinitionRule,
  Lexer,
  NoFragmentCyclesRule,
  NoUndefinedVariablesRule,
  OverlappingFieldsCanBeMergedRule,
  parse,
  PossibleFragmentSpreadsRule,
  PossibleTypeExtensionsRule,
  ProvidedRequiredArgumentsRule,
  ScalarLeafsRule,
  SingleFieldSubscriptionsRule,
  Source,
  TokenKind,
  UniqueArgumentNamesRule,
  UniqueDirectiveNamesRule,
  UniqueDirectivesPerLocationRule,
  UniqueFieldDefinitionNamesRule,
  UniqueInputFieldNamesRule,
  UniqueOperationTypesRule,
  UniqueTypeNamesRule,
  UniqueVariableNamesRule,
  validate,
  ValuesOfCorrectTypeRule,
  VariablesAreInputTypesRule,
  VariablesInAllowedPositionRule,
  visit,
} from "graphql";
import type { DocumentNode, NameNode, ValidationRule } from "graphql";
import { validateSDL } from "graphql/validation/validate.js";
import type { SDLValidationRule } from "graphql/validation/ValidationContext.js";
import { createServices } from "./services";
import type { ParserOptions, ParserServices, SiblingOperations } from "./types";

const graphqlRules = {
  "executable-definitions": { rule: ExecutableDefinitionsRule, schema: true },
  "fields-on-correct-type": { rule: FieldsOnCorrectTypeRule, schema: true, suggestions: true },
  "fragments-on-composite-type": { rule: FragmentsOnCompositeTypesRule, schema: true },
  "known-argument-names": { rule: KnownArgumentNamesRule, schema: true, suggestions: true },
  "known-directives": { rule: KnownDirectivesRule, schema: true },
  "known-fragment-names": { rule: KnownFragmentNamesRule, schema: true, fragments: true },
  "known-type-names": { rule: KnownTypeNamesRule, schema: true, suggestions: true },
  "lone-anonymous-operation": { rule: LoneAnonymousOperationRule, schema: true },
  "lone-schema-definition": { rule: LoneSchemaDefinitionRule, schema: false },
  "no-fragment-cycles": { rule: NoFragmentCyclesRule, schema: true },
  "no-undefined-variables": { rule: NoUndefinedVariablesRule, schema: true, fragments: true },
  "one-field-subscriptions": { rule: SingleFieldSubscriptionsRule, schema: true },
  "overlapping-fields-can-be-merged": { rule: OverlappingFieldsCanBeMergedRule, schema: true },
  "possible-fragment-spread": { rule: PossibleFragmentSpreadsRule, schema: true },
  "possible-type-extension": { rule: PossibleTypeExtensionsRule, schema: true, suggestions: true },
  "provided-required-arguments": { rule: ProvidedRequiredArgumentsRule, schema: true },
  "scalar-leafs": { rule: ScalarLeafsRule, schema: true, suggestions: true },
  "unique-argument-names": { rule: UniqueArgumentNamesRule, schema: true },
  "unique-directive-names": { rule: UniqueDirectiveNamesRule, schema: false },
  "unique-directive-names-per-location": { rule: UniqueDirectivesPerLocationRule, schema: true },
  "unique-field-definition-names": { rule: UniqueFieldDefinitionNamesRule, schema: false },
  "unique-input-field-names": { rule: UniqueInputFieldNamesRule, schema: true },
  "unique-operation-types": { rule: UniqueOperationTypesRule, schema: false },
  "unique-type-names": { rule: UniqueTypeNamesRule, schema: false },
  "unique-variable-names": { rule: UniqueVariableNamesRule, schema: true },
  "value-literals-of-correct-type": {
    rule: ValuesOfCorrectTypeRule,
    schema: true,
    suggestions: true,
  },
  "variables-are-input-types": { rule: VariablesAreInputTypesRule, schema: true },
  "variables-in-allowed-position": { rule: VariablesInAllowedPositionRule, schema: true },
} as const;

interface ParsedState {
  document: DocumentNode;
  services: ParserServices;
}
interface FailedState {
  document: null;
  error: GraphQLError;
  reported: boolean;
}
type ValidationState = ParsedState | FailedState;

const states = new WeakMap<SourceCode, ValidationState>();
type ValidationToken = {
  type: string;
  range: [number, number];
  loc: { start: { line: number; column: number }; end: { line: number; column: number } };
};
const tokenCache = new WeakMap<SourceCode, ValidationToken[]>();

function stateFor(context: Rule.RuleContext): ValidationState {
  const { sourceCode } = context;
  let state = states.get(sourceCode);
  if (state) return state;

  const program = sourceCode.ast as unknown as {
    body?: { rawNode?: () => DocumentNode }[];
  };
  let document: DocumentNode;
  try {
    document = program.body?.[0]?.rawNode?.() ?? parse(sourceCode.text);
  } catch (error) {
    if (!(error instanceof GraphQLError)) throw error;
    state = { document: null, error, reported: false };
    states.set(sourceCode, state);
    return state;
  }
  const parserServices = (sourceCode as SourceCode & { parserServices?: Partial<ParserServices> })
    .parserServices;
  const services =
    parserServices?.schema !== undefined && parserServices?.siblingOperations
      ? (parserServices as ParserServices)
      : undefined;
  state = {
    document,
    services:
      services ??
      createServices(
        {
          ...(context.languageOptions.parserOptions as ParserOptions | undefined),
          filePath: context.filename,
        },
        document,
      ),
  };
  states.set(sourceCode, state);
  return state;
}

function requireSchema(ruleName: string, services: ParserServices) {
  if (!services.schema) {
    throw new Error(
      `Rule \`${ruleName}\` requires graphql-config \`schema\` field to be set and loaded. See https://the-guild.dev/graphql/eslint/docs/usage#providing-schema for more info`,
    );
  }
  return services.schema;
}

function requireSiblings(ruleName: string, services: ParserServices): SiblingOperations {
  if (!services.siblingOperations.available) {
    throw new Error(
      `Rule \`${ruleName}\` requires graphql-config \`documents\` field to be set and loaded. See https://the-guild.dev/graphql/eslint/docs/usage#providing-operations for more info`,
    );
  }
  return services.siblingOperations;
}

function withSiblingFragments(
  document: DocumentNode,
  services: ParserServices,
  ruleName: string,
): DocumentNode {
  const definitions = [...document.definitions];
  const known = new Set(
    definitions
      .filter((node) => node.kind === Kind.FRAGMENT_DEFINITION)
      .map((node) => node.name.value),
  );
  while (true) {
    const missing = new Set<string>();
    visit(
      { kind: Kind.DOCUMENT, definitions },
      {
        FragmentSpread(node) {
          if (!known.has(node.name.value)) missing.add(node.name.value);
        },
      },
    );
    if (missing.size === 0) break;
    const siblings = requireSiblings(ruleName, services);
    let added = false;
    for (const name of missing) {
      const fragment = siblings.getFragment(name)[0]?.document;
      if (!fragment) continue;
      definitions.push(fragment);
      known.add(name);
      added = true;
    }
    if (!added) break;
  }
  return { ...document, definitions };
}

function withoutClientDirectives(document: DocumentNode, options: unknown): DocumentNode {
  const ignored = (options as { ignoreClientDirectives?: string[] } | undefined)
    ?.ignoreClientDirectives;
  if (!ignored?.length) return document;
  const filter = <T extends { directives?: readonly { name: { value: string } }[] }>(
    node: T,
  ): T => ({
    ...node,
    directives: node.directives?.filter((directive) => !ignored.includes(directive.name.value)),
  });
  return visit(document, { Field: filter, OperationDefinition: filter });
}

function tokensFor(sourceCode: SourceCode): ValidationToken[] {
  const parserTokens = sourceCode.ast.tokens as ValidationToken[] | undefined;
  if (parserTokens?.length) return parserTokens;
  const cached = tokenCache.get(sourceCode);
  if (cached) return cached;
  const lexer = new Lexer(new Source(sourceCode.text));
  const tokens: ValidationToken[] = [];
  for (let token = lexer.advance(); token.kind !== TokenKind.EOF; token = lexer.advance()) {
    const start = getLocation(lexer.source, token.start);
    const end = getLocation(lexer.source, token.end);
    tokens.push({
      type: token.kind,
      range: [token.start, token.end],
      loc: {
        start: { line: start.line, column: start.column - 1 },
        end: { line: end.line, column: end.column - 1 },
      },
    });
  }
  tokenCache.set(sourceCode, tokens);
  return tokens;
}

function tokenFor(error: GraphQLError, sourceCode: SourceCode) {
  const location = error.locations?.[0];
  if (!location) return undefined;
  return tokensFor(sourceCode).find(
    (token) =>
      token.loc?.start.line === location.line && token.loc.start.column === location.column - 1,
  );
}

function reportValidationError(
  context: Rule.RuleContext,
  error: GraphQLError,
  suggestions: boolean,
) {
  const location = error.locations?.[0];
  const token = tokenFor(error, context.sourceCode);
  const afterAt =
    token?.type === "@"
      ? context.sourceCode.ast.tokens?.length
        ? context.sourceCode.getNodeByRangeIndex(token.range[1] + 1)
        : tokensFor(context.sourceCode).find((candidate) => candidate.range[0] >= token.range[1])
      : null;
  const loc = afterAt?.loc ??
    token?.loc ?? {
      line: location?.line ?? 1,
      column: (location?.column ?? 1) - 1,
    };
  const names = error.message.match(/Did you mean (?<content>.*)\?$/u)?.groups?.content;
  const suggest =
    suggestions && token && names
      ? [...names.matchAll(/"(?<name>[^"]*)"/gu)].map((match) => ({
          desc: `Rename to \`${match.groups!.name}\``,
          fix: (fixer: Rule.RuleFixer) => fixer.replaceTextRange(token.range, match.groups!.name),
        }))
      : [];
  context.report({ loc, message: error.message, ...(suggest.length ? { suggest } : {}) });
}

function reportUniqueNames(ruleName: string, context: Rule.RuleContext, state: ParsedState) {
  const kind =
    ruleName === "unique-fragment-name" ? Kind.FRAGMENT_DEFINITION : Kind.OPERATION_DEFINITION;
  for (const definition of state.document.definitions) {
    if (definition.kind !== kind || !definition.name?.loc) continue;
    const siblings = requireSiblings(ruleName, state.services);
    const documentName = definition.name.value;
    const matches =
      kind === Kind.FRAGMENT_DEFINITION
        ? siblings.getFragment(documentName)
        : siblings.getOperation(documentName);
    const conflicts = matches.filter(
      (match) =>
        resolve(match.filePath).replaceAll("\\", "/") !==
        resolve(context.filename).replaceAll("\\", "/"),
    );
    if (!conflicts.length) continue;
    const name = definition.name as NameNode;
    const start = getLocation(name.loc!.source, name.loc!.start);
    const end = getLocation(name.loc!.source, name.loc!.end);
    const summary = conflicts
      .map(
        (match) =>
          `\t${relative(process.cwd(), match.filePath.replace(/[/\\]\d+_document.graphql$/u, ""))}`,
      )
      .join("\n");
    context.report({
      loc: {
        start: { line: start.line, column: start.column - 1 },
        end: { line: end.line, column: end.column - 1 },
      },
      messageId: ruleName,
      data: { documentName, summary },
    });
  }
}

export function createValidationListener(
  ruleName: string,
  context: Rule.RuleContext,
): Rule.RuleListener {
  return {
    Program() {
      const state = stateFor(context);
      if (state.document === null) {
        if (!state.reported) {
          state.reported = true;
          const location = state.error.locations?.[0];
          context.report({
            loc: { line: location?.line ?? 1, column: (location?.column ?? 1) - 1 },
            message: state.error.message,
          });
        }
        return;
      }
      if (ruleName === "unique-fragment-name" || ruleName === "unique-operation-name") {
        reportUniqueNames(ruleName, context, state);
        return;
      }
      const definition = graphqlRules[ruleName as keyof typeof graphqlRules];
      if (!definition) throw new Error(`Unknown GraphQL validation rule: ${ruleName}`);
      let document = state.document;
      if (!document.definitions.length) return;
      if ("fragments" in definition) {
        document = withSiblingFragments(document, state.services, ruleName);
      }
      if (ruleName === "known-directives") {
        document = withoutClientDirectives(document, context.options[0]);
      }
      const schema = definition.schema ? requireSchema(ruleName, state.services) : null;
      try {
        const errors = schema
          ? validate(schema, document, [definition.rule as ValidationRule])
          : validateSDL(document, null, [definition.rule as SDLValidationRule]);
        for (const error of errors) {
          reportValidationError(context, error, "suggestions" in definition);
        }
      } catch (error) {
        context.report({
          loc: { line: 1, column: 0 },
          message: error instanceof Error ? error.message : String(error),
        });
      }
    },
  };
}
