import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { directory, graphql, json, schemas, sha256 } from "./shared.mjs";

const refresh = process.argv.includes("--refresh");
const sourceList = json(join(directory, "sources.json"));
const manifestPath = join(directory, "manifest.json");
const manifest = existsSync(manifestPath) ? json(manifestPath) : {};
mkdirSync(schemas, { recursive: true });

async function download(source) {
  const options =
    source.kind === "introspection"
      ? {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "apollographql-client-name": "graphql-analyzer-benchmarks",
            "apollographql-client-version": "1.0.0",
          },
          body: JSON.stringify({
            query: graphql.getIntrospectionQuery({
              descriptions: true,
              specifiedByUrl: true,
              directiveIsRepeatable: true,
              schemaDescription: true,
              inputValueDeprecation: true,
            }),
          }),
        }
      : {};
  const response = await fetch(source.url, { ...options, signal: AbortSignal.timeout(60_000) });
  if (!response.ok) throw new Error(`${source.url}: HTTP ${response.status}`);
  if (source.kind === "sdl") return await response.text();
  const payload = await response.json();
  if (payload.errors?.length || !payload.data?.__schema) {
    throw new Error(`${source.url}: introspection failed: ${JSON.stringify(payload.errors)}`);
  }
  return graphql.printSchema(graphql.buildClientSchema(payload.data));
}

function counts(sdl) {
  const schema = graphql.buildSchema(sdl);
  const types = Object.values(schema.getTypeMap()).filter((type) => !type.name.startsWith("__"));
  return {
    types: types.length,
    fields: types.reduce(
      (total, type) => total + ("getFields" in type ? Object.keys(type.getFields()).length : 0),
      0,
    ),
  };
}

for (const [name, source] of Object.entries(sourceList)) {
  const schemaPath = join(schemas, `${name}.graphql`);
  const pinned = manifest[name];
  if (!refresh && existsSync(schemaPath) && pinned) {
    const cached = readFileSync(schemaPath);
    if (sha256(cached) === pinned.sha256) {
      console.log(`${name}: verified cached schema (${pinned.bytes} bytes)`);
      continue;
    }
  }
  const sdl = await download(source);
  const bytes = Buffer.byteLength(sdl);
  const digest = sha256(sdl);
  if (!refresh && pinned && digest !== pinned.sha256) {
    throw new Error(
      `${name}: source changed (expected ${pinned.sha256}, got ${digest}). ` +
        "Use the pinned cache or review the new schema and run fetch.mjs --refresh.",
    );
  }
  const statistics = counts(sdl);
  writeFileSync(schemaPath, sdl);
  manifest[name] = {
    ...source,
    retrievedAt: new Date().toISOString(),
    sha256: digest,
    bytes,
    ...statistics,
  };
  console.log(
    `${name}: ${bytes} bytes, ${statistics.types} types, ${statistics.fields} fields, sha256 ${digest}`,
  );
}

if (refresh || !existsSync(manifestPath)) {
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Updated ${manifestPath}`);
}
