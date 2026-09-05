const assert = require("node:assert/strict");
const { createRequire } = require("node:module");
const { resolve } = require("node:path");
const localRequire = createRequire(resolve("package.json"));
const plugin = localRequire("@graphql-analyzer/eslint-plugin");
const typescriptVersion = localRequire("typescript/package.json").version;
assert.throws(() => localRequire.resolve("@graphql-eslint/eslint-plugin"), /Cannot find module/);
for (const extension of ["vue", "svelte"]) {
  const filename = resolve(`component.${extension}`);
  const source =
    extension === "vue"
      ? '<script setup lang="ts">\nconst query = gql`query Named { name }`;\n</script>'
      : '<script lang="ts">\nconst query = gql`query Named { name }`;\n</script>';
  const blocks = plugin.processor.preprocess(source, filename);
  assert.equal(blocks.length, 2, extension);
  assert.match(blocks[0].text, /query Named \{ name \}/);
  const parsed = plugin.parseForESLint(blocks[0].text, {
    filePath: resolve(filename, `0_${blocks[0].filename}`),
    schemaSdl: "type Query { name: String }",
  });
  assert.equal(parsed.services.schema.getQueryType().name, "Query");
  assert.equal(parsed.ast.body[0].rawNode().definitions[0].name.value, "Named");
  plugin.processor.postprocess(
    blocks.map(() => []),
    filename,
  );
}
console.log(
  `Packed Vue/Svelte consumer passed: TypeScript ${typescriptVersion}, Node ${process.version}`,
);
