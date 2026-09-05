import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createRequire } from "node:module";
import { test } from "node:test";
import { ESLint } from "eslint";
const require = createRequire(import.meta.url);
const plugin = require("../dist");
const { version } = require("../package.json");

for (const name of ["parser", "fastParser", "processor", "fastProcessor"]) {
  test(`${name} supports ESLint's persistent result cache`, async (t) => {
    const directory = mkdtempSync(join(tmpdir(), "graphql-eslint-cache-"));
    t.after(() => rmSync(directory, { recursive: true, force: true }));
    const isParser = name.toLowerCase().includes("parser");
    const filename = isParser ? "query.graphql" : "source.js";
    writeFileSync(
      join(directory, filename),
      isParser ? "query Named { field }" : "const value = 1;",
    );
    let visits = 0;
    const config = {
      files: [isParser ? "**/*.graphql" : "**/*.js"],
      ...(isParser
        ? {
            languageOptions: {
              parser: plugin[name],
              parserOptions: { graphQLConfig: { schema: "" } },
            },
          }
        : { processor: plugin[name] }),
      plugins: {
        probe: {
          rules: {
            count: {
              meta: { schema: [] },
              create() {
                return {
                  Program() {
                    visits++;
                  },
                };
              },
            },
          },
        },
      },
      rules: { "probe/count": "error" },
    };
    const options = {
      cwd: directory,
      cache: true,
      cacheLocation: join(directory, ".eslintcache"),
      overrideConfigFile: true,
      overrideConfig: [config],
    };
    const first = await new ESLint(options).lintFiles([filename]);
    assert.equal(first[0].errorCount, 0);
    assert.equal(visits, 1);
    await new ESLint(options).lintFiles([filename]);
    assert.equal(visits, 1, "the second ESLint instance should read the persistent cache");
    assert.equal(plugin[name].meta.version, version);
    assert.equal(typeof plugin[name].meta.name, "string");
  });
}
