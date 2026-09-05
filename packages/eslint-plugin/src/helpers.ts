import type { GraphQLESLintRuleContext, ParserServices } from "./types";

function services(context: GraphQLESLintRuleContext<any>): ParserServices {
  return context.sourceCode?.parserServices ?? context.parserServices;
}

export function requireGraphQLSchema(ruleId: string, context: GraphQLESLintRuleContext<any>) {
  const schema = services(context)?.schema;
  if (!schema)
    throw new Error(
      `Rule \`${ruleId}\` requires graphql-config \`schema\` field to be set and loaded.`,
    );
  return schema;
}

export function requireGraphQLOperations(ruleId: string, context: GraphQLESLintRuleContext<any>) {
  const siblings = services(context)?.siblingOperations;
  if (!siblings?.available)
    throw new Error(
      `Rule \`${ruleId}\` requires graphql-config \`documents\` field to be set and loaded.`,
    );
  return siblings;
}
