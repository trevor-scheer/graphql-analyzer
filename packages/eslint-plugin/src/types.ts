import type { AST, Linter, Rule, SourceCode } from "eslint";
import type { ASTKindToNode, ASTNode, DocumentNode, FragmentDefinitionNode, GraphQLSchema, OperationDefinitionNode, OperationTypeNode, SelectionSetNode, TypeInfo } from "graphql";
import type { IGraphQLConfig } from "graphql-config";
import type * as G from "graphql";

export type TypeInformation = {
  argument: ReturnType<TypeInfo["getArgument"]>;
  defaultValue: ReturnType<TypeInfo["getDefaultValue"]>;
  directive: ReturnType<TypeInfo["getDirective"]>;
  enumValue: ReturnType<TypeInfo["getEnumValue"]>;
  fieldDef: ReturnType<TypeInfo["getFieldDef"]>;
  inputType: ReturnType<TypeInfo["getInputType"]>;
  parentInputType: ReturnType<TypeInfo["getParentInputType"]>;
  parentType: ReturnType<TypeInfo["getParentType"]>;
  gqlType: ReturnType<TypeInfo["getType"]>;
};

type Converted<T, W extends boolean> = T extends ASTNode ? GraphQLESTreeNode<T, W> : T extends ReadonlyArray<infer U> ? Converted<U, W>[] : T;
type NodeWithType = G.FieldDefinitionNode | G.InputValueDefinitionNode | G.ListTypeNode | G.NonNullTypeNode | G.OperationTypeDefinitionNode | G.VariableDefinitionNode;
type NodeWithName = G.ArgumentNode | G.DirectiveDefinitionNode | G.EnumValueDefinitionNode | G.ExecutableDefinitionNode | G.FieldDefinitionNode | G.FieldNode | G.FragmentSpreadNode | G.NamedTypeNode | G.TypeDefinitionNode | G.TypeExtensionNode | G.VariableNode;
type ParentNode<T> = T extends DocumentNode ? AST.Program
  : T extends G.DefinitionNode ? DocumentNode
  : T extends G.EnumValueDefinitionNode ? G.EnumTypeDefinitionNode | G.EnumTypeExtensionNode
  : T extends G.InputValueDefinitionNode ? G.DirectiveDefinitionNode | G.FieldDefinitionNode | G.InputObjectTypeDefinitionNode | G.InputObjectTypeExtensionNode
  : T extends G.FieldDefinitionNode ? G.InterfaceTypeDefinitionNode | G.InterfaceTypeExtensionNode | G.ObjectTypeDefinitionNode | G.ObjectTypeExtensionNode
  : T extends SelectionSetNode ? G.ExecutableDefinitionNode | G.FieldNode | G.InlineFragmentNode
  : T extends G.SelectionNode ? SelectionSetNode
  : T extends G.TypeNode ? NodeWithType
  : T extends G.NameNode ? NodeWithName
  : T extends G.DirectiveNode ? G.InputObjectTypeDefinitionNode | G.ObjectTypeDefinitionNode
  : T extends G.VariableNode ? G.VariableDefinitionNode : unknown;
export type GraphQLESTreeNode<T, W extends boolean = false> = T extends ASTNode ? {
  -readonly [K in keyof T as K extends "loc" ? never : K extends "type" ? "gqlType" : K]: Converted<T[K], W>;
} & {
  type: T["kind"];
  loc: AST.SourceLocation;
  range: AST.Range;
  leadingComments: { type: "Line" | "Block"; value: string }[];
  rawNode(): T;
  typeInfo(): W extends true ? TypeInformation : Record<string, never>;
  parent: GraphQLESTreeNode<ParentNode<T>, W>;
} : T;

export type FragmentSource = { filePath: string; document: FragmentDefinitionNode };
export type OperationSource = { filePath: string; document: OperationDefinitionNode };
export type SiblingOperations = {
  available: boolean;
  getFragment(name: string): FragmentSource[];
  getFragments(): FragmentSource[];
  getFragmentByType(typeName: string): FragmentSource[];
  getFragmentsInUse(node: FragmentDefinitionNode | OperationDefinitionNode | SelectionSetNode, recursive?: boolean): FragmentDefinitionNode[];
  getOperation(name: string): OperationSource[];
  getOperations(): OperationSource[];
  getOperationByType(type: OperationTypeNode): OperationSource[];
};
export type Schema = GraphQLSchema | null;
export type ParserOptions = Linter.ParserOptions & { filePath?: string; graphQLConfig?: IGraphQLConfig; schemaSdl?: string };
export type ParserServices = { schema: Schema; siblingOperations: SiblingOperations };
export type GraphQLESLintParseResult = Linter.ESLintParseResult & { services: ParserServices };
export type ReportDescriptor = ({ loc: AST.SourceLocation | { line: number; column: number } } | { node: { loc: AST.SourceLocation } }) & Rule.ReportDescriptorMessage & Rule.ReportDescriptorOptions;
export type GraphQLESLintRuleContext<Options = any[]> = Omit<Rule.RuleContext, "options" | "parserServices" | "report" | "sourceCode"> & {
  options: Options;
  parserServices: ParserServices;
  sourceCode: SourceCode & { parserServices: ParserServices };
  report(descriptor: ReportDescriptor): void;
};
export type GraphQLESLintRuleListener<W extends boolean = false> = Record<string, any> & {
  [K in keyof ASTKindToNode]?: (node: GraphQLESTreeNode<ASTKindToNode[K], W>) => void;
};
export type RuleDocsInfo<Options> = Omit<NonNullable<Rule.RuleMetaData["docs"]>, "category"> & {
  category: "Operations" | "Schema" | ("Operations" | "Schema")[];
  requiresSchema?: true;
  requiresSiblings?: true;
  examples?: { title: string; code: string; usage?: Options }[];
  configOptions?: Options | { schema?: Options; operations?: Options };
  graphQLJSRuleName?: string;
  isDisabledForAllConfig?: true;
  whenNotToUseIt?: string;
};
export type GraphQLESLintRule<Options = [], W extends boolean = false> = {
  meta: Omit<Rule.RuleMetaData, "docs"> & { docs?: RuleDocsInfo<Options> };
  create(context: GraphQLESLintRuleContext<Options>): GraphQLESLintRuleListener<W>;
};
