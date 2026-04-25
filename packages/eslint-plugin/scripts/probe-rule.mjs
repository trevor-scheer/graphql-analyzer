#!/usr/bin/env node
// Probe parity for a single rule, isolated from the fixture project.
//
// Usage:
//   echo "<fixture content>" | node scripts/probe-rule.mjs <rule-name> <filename>
//
// Creates a throwaway project under /tmp with just the named rule enabled
// in `.graphqlrc.yaml`, writes the fixture content as <filename>, runs
// both plugins via ESLint.lintFiles, prints diagnostics from each as JSON.

import { ESLint } from "eslint";
import oursMod from "../dist/index.js";
import theirsMod from "@graphql-eslint/eslint-plugin";
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";

const ours = oursMod.default ?? oursMod;
const theirs = theirsMod.default ?? theirsMod;

const [, , ruleName, filename] = process.argv;
if (!ruleName || !filename) {
  console.error("usage: node probe-rule.mjs <rule-name> <filename>");
  process.exit(2);
}
const source = readFileSync(0, "utf8");

const rootDir = mkdtempSync(path.join(tmpdir(), "probe-rule-"));
const filePath = path.join(rootDir, filename);
mkdirSync(path.dirname(filePath), { recursive: true });
writeFileSync(filePath, source);

// Convert the kebab-case rule name to camelCase for the analyzer config.
const camel = ruleName.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
const isSchema = filename.endsWith(".graphql") && !filename.startsWith("src/");
// Stay close to graphql-config's defaults so graphql-eslint's parser
// happily resolves the project. Schema-side fixtures act as the schema;
// document-side fixtures act as a document with an inline schema stub.
writeFileSync(
  path.join(rootDir, ".graphqlrc.yaml"),
  isSchema
    ? `schema: "${filename}"\nextensions:\n  graphql-analyzer:\n    lint:\n      rules:\n        ${camel}: warn\n`
    : `schema: "schema.graphql"\ndocuments: "${filename}"\nextensions:\n  graphql-analyzer:\n    lint:\n      rules:\n        ${camel}: warn\n`,
);
if (!isSchema) {
  // For document-side rules, write a minimal Query so graphql-eslint can
  // parse with type info. Tests that need richer schemas can pass a
  // larger fixture as the schema file directly.
  writeFileSync(path.join(rootDir, "schema.graphql"), `type Query { _: Boolean }\n`);
}

async function run(plugin, scope) {
  const eslint = new ESLint({
    overrideConfigFile: true,
    cwd: rootDir,
    overrideConfig: [
      {
        files: ["**/*.graphql", "**/*.ts", "**/*.tsx", "**/*.js"],
        languageOptions: { parser: plugin.parser },
        plugins: { [scope]: plugin },
        rules: { [`${scope}/${ruleName}`]: 1 },
      },
    ],
  });
  const [r] = await eslint.lintFiles([filename]);
  return r.messages.filter((m) => m.ruleId === `${scope}/${ruleName}`);
}

const project = (d) => ({
  line: d.line,
  column: d.column,
  endLine: d.endLine,
  endColumn: d.endColumn,
  message: d.message,
});

try {
  const oursDiag = (await run(ours, "@graphql-analyzer")).map(project);
  const theirsDiag = (await run(theirs, "@graphql-eslint")).map(project);

  console.log(
    JSON.stringify(
      {
        rule: ruleName,
        filename,
        ours: { count: oursDiag.length, diagnostics: oursDiag },
        theirs: { count: theirsDiag.length, diagnostics: theirsDiag },
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(rootDir, { recursive: true, force: true });
}
