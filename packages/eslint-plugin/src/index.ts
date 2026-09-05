import { parseForESLint, fastParser } from "./parser";
import { processor, fastProcessor } from "./processor";
import { buildRules } from "./rules";
import { configs } from "./configs";

export const parser = { parseForESLint };
export { parseForESLint, fastParser, processor, fastProcessor, configs };
export { requireGraphQLSchema, requireGraphQLOperations } from "./helpers";
export type * from "./types";
export const processors = { graphql: processor };
export const rules = buildRules();

const plugin = {
  parser,
  fastParser,
  processor,
  fastProcessor,
  processors,
  rules,
  configs,
};

export default plugin;
