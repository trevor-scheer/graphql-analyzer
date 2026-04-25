import { parseForESLint } from "./parser";
import { processor } from "./processor";
import { buildRules } from "./rules";
import { configs } from "./configs";

export const parser = { parseForESLint };
export { processor, configs };
export const rules = buildRules();

const plugin = {
  parser,
  processor,
  rules,
  configs,
};

export default plugin;
