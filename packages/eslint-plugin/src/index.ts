import { parseForESLint } from "./parser";
import { processor } from "./processor";
import { buildRules } from "./rules";
import { configs } from "./configs";

export const parser = { parseForESLint };
export { processor, configs };
export const rules = buildRules();

// ESLint v9 flat config dispatches processors by name, e.g.
// `processor: "@graphql-analyzer/graphql"`. Passing the object directly
// works for invoking preprocess/postprocess but ESLint can't then route the
// virtual blocks back to a config with our rules enabled — only the named
// reference establishes that link. Mirrors @graphql-eslint's `processors`
// shape (`{ graphql: processor }`).
export const processors = {
  graphql: processor,
};

const plugin = {
  parser,
  processor,
  processors,
  rules,
  configs,
};

export default plugin;
