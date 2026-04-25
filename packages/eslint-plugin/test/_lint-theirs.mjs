#!/usr/bin/env node
// Child-process runner: lint a file with @graphql-eslint/eslint-plugin and
// print the resulting diagnostics as JSON.
//
// graphql-eslint caches its parsed `graphQLConfig` at module scope; running
// each parity probe in a fresh process bypasses that and prevents
// rule-N's state from leaking into rule-(N+1)'s diagnostics.
//
// Args (positional, JSON-encoded for transparency):
//   1: cwd (the throwaway project root)
//   2: rule (kebab-case)
//   3: target file (relative to cwd)
//   4: severity (1 or 2)
//   5: optional rule options as JSON string ("undefined" for none)

process.env.NODE_ENV = "test";
import { ESLint } from "eslint";
import theirsMod from "@graphql-eslint/eslint-plugin";
const theirs = theirsMod.default ?? theirsMod;

const [, , cwd, rule, target, severity, optionsRaw] = process.argv;
const options = optionsRaw && optionsRaw !== "undefined" ? JSON.parse(optionsRaw) : undefined;
const ruleEntry = options !== undefined ? [Number(severity), options] : Number(severity);

const eslint = new ESLint({
  overrideConfigFile: true,
  cwd,
  overrideConfig: [
    {
      files: ["**/*.graphql"],
      languageOptions: { parser: theirs.parser },
      plugins: { "@graphql-eslint": theirs },
      rules: { [`@graphql-eslint/${rule}`]: ruleEntry },
    },
  ],
});

const [r] = await eslint.lintFiles([target]);
const messages = r.messages.filter((m) => m.ruleId === `@graphql-eslint/${rule}`);
process.stdout.write(JSON.stringify(messages));
