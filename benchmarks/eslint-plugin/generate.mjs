import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { directory, graphql, json, projects, schemas, sha256 } from "./shared.mjs";

const manifestPath = join(directory, "manifest.json");
if (!existsSync(manifestPath)) throw new Error("No pinned schemas. Run fetch.mjs first.");
const manifest = json(manifestPath);
const count = Number(process.env.BENCH_FILES ?? 12);
if (!Number.isInteger(count) || count < 1)
  throw new Error("BENCH_FILES must be a positive integer");

function noRequiredArguments(field) {
  return field.args.every(
    (argument) => !graphql.isNonNullType(argument.type) || argument.defaultValue !== undefined,
  );
}

function selection(field, depth, visited) {
  const type = graphql.getNamedType(field.type);
  if (!graphql.isCompositeType(type)) return field.name;
  if (depth === 0 || visited.has(type.name) || graphql.isUnionType(type)) {
    return `${field.name} { __typename }`;
  }
  const next = new Set(visited).add(type.name);
  const children = Object.values(type.getFields())
    .filter((candidate) => !candidate.isDeprecated && noRequiredArguments(candidate))
    .sort((a, b) => a.name.localeCompare(b.name))
    .slice(0, 3);
  const body = children.length
    ? children.map((child) => selection(child, depth - 1, next)).join(" ")
    : "__typename";
  return `${field.name} { ${body} }`;
}

for (const [name, pinned] of Object.entries(manifest)) {
  const schemaSource = join(schemas, `${name}.graphql`);
  if (!existsSync(schemaSource)) throw new Error(`${name}: cached schema missing; run fetch.mjs`);
  const sdl = readFileSync(schemaSource);
  if (sha256(sdl) !== pinned.sha256)
    throw new Error(`${name}: cached schema differs from manifest`);
  const schema = graphql.buildSchema(sdl.toString("utf8"));
  const rootFields = Object.values(schema.getQueryType()?.getFields() ?? {})
    .filter((field) => !field.isDeprecated && noRequiredArguments(field))
    .sort((a, b) => a.name.localeCompare(b.name));
  if (!rootFields.length) throw new Error(`${name}: no query fields without required arguments`);
  const project = join(projects, name);
  rmSync(project, { recursive: true, force: true });
  mkdirSync(join(project, "operations"), { recursive: true });
  mkdirSync(join(project, "valid"), { recursive: true });
  copyFileSync(schemaSource, join(project, "schema.graphql"));
  writeFileSync(
    join(project, ".graphqlrc.json"),
    JSON.stringify(
      {
        schema: "schema.graphql",
        documents: "operations/*.graphql",
        extensions: {
          "graphql-analyzer": {
            lint: { rules: { noAnonymousOperations: "error", noDuplicateFields: "error" } },
          },
        },
      },
      null,
      2,
    ),
  );
  for (let index = 0; index < count; index++) {
    const chosen = [0, 1, 2]
      .map((offset) => rootFields[(index * 7 + offset) % rootFields.length])
      .filter(
        (field, position, fields) =>
          fields.findIndex((other) => other.name === field.name) === position,
      );
    const selections = chosen.map((field) => selection(field, 2, new Set())).join("\n  ");
    const operation = `Benchmark${String(index).padStart(3, "0")}`;
    const valid = `query ${operation} {\n  ${selections}\n  __typename\n}\n`;
    const duplicate = `query ${operation} {\n  ${selections}\n  __typename\n  __typename\n}\n`;
    for (const [label, source] of [
      ["valid", valid],
      ["operations", duplicate],
    ]) {
      const document = graphql.parse(source);
      const errors = graphql.validate(schema, document);
      if (errors.length) {
        throw new Error(`${name}/${label}/${operation}: ${errors[0].message}`);
      }
      writeFileSync(
        join(project, label, `operation-${String(index).padStart(3, "0")}.graphql`),
        source,
      );
    }
  }
  writeFileSync(
    join(project, "corpus.json"),
    `${JSON.stringify(
      {
        name,
        schemaSha256: pinned.sha256,
        operations: count,
        queryFieldsAvailable: rootFields.length,
        seed: "index * 7 + offset; sorted query fields",
      },
      null,
      2,
    )}\n`,
  );
  const schemaWorkload = join(project, "schema-workload");
  mkdirSync(schemaWorkload, { recursive: true });
  const sentinel = "type BenchmarkSentinel { field: String }\n";
  writeFileSync(
    join(schemaWorkload, "schema.graphql"),
    `${sdl}\n# Benchmark schema comment\n${sentinel}`,
  );
  writeFileSync(join(schemaWorkload, "valid.graphql"), `${sdl}\n${sentinel}`);
  writeFileSync(
    join(schemaWorkload, ".graphqlrc.json"),
    JSON.stringify(
      {
        schema: "schema.graphql",
        extensions: { "graphql-analyzer": { lint: { rules: { noHashtagDescription: "error" } } } },
      },
      null,
      2,
    ),
  );

  const embeddedWorkload = join(project, "embedded-workload");
  mkdirSync(join(embeddedWorkload, "embedded"), { recursive: true });
  copyFileSync(schemaSource, join(embeddedWorkload, "schema.graphql"));
  writeFileSync(
    join(embeddedWorkload, ".graphqlrc.json"),
    JSON.stringify(
      {
        schema: "schema.graphql",
        documents: "embedded/*.js",
        extensions: { "graphql-analyzer": { lint: { rules: { noDuplicateFields: "error" } } } },
      },
      null,
      2,
    ),
  );
  const embeddedDuplicate = readFileSync(join(project, "operations/operation-000.graphql"), "utf8");
  const embeddedValid = readFileSync(join(project, "valid/operation-000.graphql"), "utf8");
  writeFileSync(
    join(embeddedWorkload, "embedded/workload.js"),
    `import gql from "graphql-tag";\nconst query = gql\`${embeddedDuplicate}\`;\n`,
  );
  writeFileSync(
    join(embeddedWorkload, "embedded/valid.js"),
    `import gql from "graphql-tag";\nconst query = gql\`${embeddedValid}\`;\n`,
  );
  console.log(
    `${name}: generated and validated ${count} operations (${rootFields.length} available root fields)`,
  );
}
