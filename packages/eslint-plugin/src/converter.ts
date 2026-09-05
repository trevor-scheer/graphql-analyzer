import { TokenKind, TypeInfo, visit, visitWithTypeInfo } from "graphql";
import { QueryDocumentKeys } from "graphql/language/ast";
import type { ASTNode, DocumentNode, GraphQLSchema, Token } from "graphql";
import type { AST } from "eslint";
import type { GraphQLESTreeNode, TypeInformation } from "./types";

export const visitorKeys = Object.fromEntries([
  ["Program", ["body"]],
  ...Object.entries(QueryDocumentKeys).map(([kind, keys]) => [
    kind,
    keys.map((key) => (key === "type" ? "gqlType" : key)),
  ]),
]);

export function convertDocument(document: DocumentNode, getSchema: () => GraphQLSchema | null) {
  const starts = [0];
  const source = document.loc!.source.body;
  for (const match of source.matchAll(/\r\n|[\n\r]/gu)) starts.push(match.index + match[0].length);
  const emptyInformation = Object.freeze({});
  let information: WeakMap<ASTNode, TypeInformation> | undefined;
  function typeInfo(node: ASTNode) {
    if (!information) {
      information = new WeakMap();
      const schema = getSchema();
      if (schema) {
        const info = new TypeInfo(schema);
        visit(
          document,
          visitWithTypeInfo(info, {
            leave(raw) {
              information!.set(raw, {
                argument: info.getArgument(),
                defaultValue: info.getDefaultValue(),
                directive: info.getDirective(),
                enumValue: info.getEnumValue(),
                fieldDef: info.getFieldDef(),
                inputType: info.getInputType(),
                parentInputType: info.getParentInputType(),
                parentType: info.getParentType(),
                gqlType: info.getType(),
              });
            },
          }),
        );
      }
    }
    return information.get(node) ?? emptyInformation;
  }
  function convert(raw: ASTNode): unknown {
    const result: Record<string, unknown> = { ...raw };
    for (const key of QueryDocumentKeys[raw.kind]) {
      const value = (raw as unknown as Record<string, unknown>)[key];
      if (key === "type") delete result.type;
      result[key === "type" ? "gqlType" : key] = Array.isArray(value)
        ? value.map(convert)
        : value
          ? convert(value as ASTNode)
          : value;
    }
    const description = "description" in raw ? raw.description : undefined;
    return Object.assign(result, {
      type: raw.kind,
      loc: { ...location(raw.loc!.start, raw.loc!.end), source },
      range: [raw.loc!.start, raw.loc!.end],
      leadingComments: description
        ? [{ type: description.block ? "Block" : "Line", value: description.value }]
        : [],
      rawNode: () => raw,
      typeInfo: () => typeInfo(raw),
    });
  }
  function location(start: number, end: number): AST.SourceLocation {
    function position(offset: number) {
      let low = 0;
      let high = starts.length;
      while (low + 1 < high) {
        const middle = (low + high) >>> 1;
        if (starts[middle] <= offset) low = middle;
        else high = middle;
      }
      return { line: low + 1, column: offset - starts[low] };
    }
    return { start: position(start), end: position(end) };
  }
  function tokenData(token: Token, type: string) {
    return {
      type,
      value: token.value,
      loc: location(token.start, token.end),
      range: [token.start, token.end] as AST.Range,
    };
  }
  const tokens = [];
  const comments = [];
  let token: Token | null = document.loc!.startToken;
  while (token) {
    if (token.kind === TokenKind.COMMENT)
      comments.push(
        tokenData(token, token.value!.trimStart().startsWith("eslint") ? "Block" : "Line"),
      );
    else if (token.kind !== TokenKind.SOF && token.kind !== TokenKind.EOF)
      tokens.push(tokenData(token, token.kind));
    token = token.next;
  }
  return {
    root: convert(document) as GraphQLESTreeNode<DocumentNode>,
    tokens,
    comments,
    visitorKeys,
  };
}
