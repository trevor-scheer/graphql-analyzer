import { RuleTester } from "eslint";
import type { Rule } from "eslint";
import {
  parser,
  requireGraphQLSchema,
  requireGraphQLOperations,
} from "@graphql-analyzer/eslint-plugin";
import type {
  GraphQLESLintRule,
  GraphQLESTreeNode,
  TypeInformation,
  ParserServices,
} from "@graphql-analyzer/eslint-plugin";
import type { FieldNode } from "graphql";

const rule: GraphQLESLintRule<[{ enabled: boolean }], true> = {
  meta: {
    schema: [{ type: "object" }],
    docs: { description: "Check fields", category: "Operations", requiresSchema: true },
  },
  create(context) {
    const schema = requireGraphQLSchema("custom/field", context);
    const siblings = requireGraphQLOperations("custom/field", context);
    const services: ParserServices = context.sourceCode.parserServices;
    // @ts-expect-error ESLint 10 exposes parser services through sourceCode.
    void context.parserServices;
    void [schema, siblings, services, context.options[0].enabled];
    return {
      Field(node) {
        const field: GraphQLESTreeNode<FieldNode, true> = node;
        const information: TypeInformation = node.typeInfo();
        void [field.parent.selections, information.parentType, node.rawNode().selectionSet];
        context.report({ node, message: "Check field" });
      },
      FieldDefinition(node) {
        void node.parent.fields;
        void node.gqlType;
      },
      InputValueDefinition(node) {
        if (node.parent.kind === "FieldDefinition") void node.parent.arguments;
      },
    };
  },
};
new RuleTester({ languageOptions: { parser } }).run(
  "custom/field",
  rule as unknown as Rule.RuleModule,
  { valid: [], invalid: [] },
);
