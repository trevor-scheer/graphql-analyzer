import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { buildASTSchema, buildSchema, Kind, print } from "graphql";
import type { DocumentNode } from "graphql";
import { GraphQLConfig, loadConfigSync } from "graphql-config";
import type {
  GraphQLExtensionDeclaration,
  GraphQLProjectConfig,
  IGraphQLConfig,
} from "graphql-config";
import { CodeFileLoader } from "@graphql-tools/code-file-loader";
import type { GraphQLTagPluckOptions } from "@graphql-tools/graphql-tag-pluck";
import { mergeTypeDefs } from "@graphql-tools/merge";
import { parseGraphQLSDL } from "@graphql-tools/utils";
import { findEmbeddedBlock } from "./embedded";
import { getSiblings } from "./siblings";
import { loadSources } from "./source-cache";
import type { ParserOptions, ParserServices } from "./types";

const codeFileLoader: GraphQLExtensionDeclaration = (api) => {
  api.loaders.schema.register(new CodeFileLoader());
  api.loaders.documents.register(new CodeFileLoader());
  return { name: "code-file-loaders" };
};
const projectConfigs = new WeakMap<GraphQLProjectConfig, GraphQLConfig>();
const schemas = new Map<string, NonNullable<ParserServices["schema"]>>();
function cachedSchema(key: string, build: () => NonNullable<ParserServices["schema"]>) {
  let schema = schemas.get(key);
  if (schema) schemas.delete(key);
  else schema = build();
  schemas.set(key, schema);
  if (schemas.size > 16) schemas.delete(schemas.keys().next().value!);
  return schema;
}

function existingPath(filePath: string): string {
  let path = resolve(filePath);
  while (!existsSync(path) && dirname(path) !== path) path = dirname(path);
  return path;
}

export function resolveProject(options: ParserOptions = {}): GraphQLProjectConfig {
  const filePath = options.filePath ?? resolve("document.graphql");
  const inline = options.graphQLConfig;
  const config = inline
    ? new GraphQLConfig(
        {
          config: ("projects" in inline ? inline : { schema: "", ...inline }) as IGraphQLConfig,
          filepath: resolve("graphql.config.yml"),
        },
        [codeFileLoader],
      )
    : (loadConfigSync({
        rootDir: existingPath(dirname(filePath)),
        throwOnMissing: false,
        extensions: [codeFileLoader],
      }) ??
      new GraphQLConfig({ config: { schema: "" }, filepath: resolve("graphql.config.yml") }, [
        codeFileLoader,
      ]));
  const project = config.getProjectForFile(
    findEmbeddedBlock(filePath)?.record.filename ?? filePath,
  );
  projectConfigs.set(project, config);
  return project;
}

export function getPluckConfig(filePath: string): GraphQLTagPluckOptions | undefined {
  return resolveProject({ filePath }).extensions.pluckConfig as GraphQLTagPluckOptions | undefined;
}

export function createServices(options: ParserOptions, document: DocumentNode): ParserServices {
  const filePath = resolve(options.filePath ?? "document.graphql");
  const embedded = findEmbeddedBlock(filePath);
  const overlays = embedded
    ? embedded.record.blocks.map((block, index) => {
        const location = resolve(embedded.record.filename, `${index}_document.graphql`);
        return {
          location,
          document:
            location === filePath ? document : parseGraphQLSDL(location, block.text).document!,
        };
      })
    : [{ location: filePath, document }];
  let project: GraphQLProjectConfig | undefined;
  const getProject = () => (project ??= resolveProject(options));
  let schema: ParserServices["schema"] | undefined;
  let siblingOperations: ParserServices["siblingOperations"] | undefined;
  return {
    get schema() {
      if (schema !== undefined) return schema;
      if (options.schemaSdl !== undefined)
        return (schema = cachedSchema(`inline:${options.schemaSdl}`, () =>
          buildSchema(options.schemaSdl!),
        ));
      const config = getProject();
      if (!config.schema || (Array.isArray(config.schema) && config.schema.length === 0))
        return (schema = null);
      try {
        const loaded = loadSources(config, "schema", () =>
          projectConfigs.get(config)!.extensions.loaders.schema.loadTypeDefsSync(config.schema, {
            pluckConfig: config.extensions.pluckConfig,
          }),
        );
        const replacements: typeof overlays = [];
        const definitions = loaded.sources.flatMap((source) => {
          const location = source.location && resolve(source.location);
          const replacement =
            location === embedded?.record.filename
              ? overlays
              : overlays.filter((overlay) => overlay.location === location);
          replacements.push(...replacement);
          return replacement.length
            ? replacement.map((overlay) => overlay.document)
            : source.document
              ? [source.document]
              : [];
        });
        const key = loaded.key
          ? JSON.stringify([
              loaded.key,
              replacements.map((source) => [source.location, source.document.loc?.source.body]),
            ])
          : JSON.stringify([
              config.filepath,
              config.name,
              definitions.map((definition) => [
                definition.loc?.source.name,
                definition.loc?.source.body,
                print(definition),
              ]),
            ]);
        return (schema = cachedSchema(key, () => {
          const typeDefs = mergeTypeDefs(definitions);
          const federation = typeDefs.definitions.some(
            (node) =>
              (node.kind === Kind.SCHEMA_EXTENSION || node.kind === Kind.SCHEMA_DEFINITION) &&
              node.directives?.some(
                (directive) =>
                  directive.name.value === "link" &&
                  directive.arguments?.some(
                    (argument) =>
                      argument.name.value === "url" &&
                      argument.value.kind === Kind.STRING &&
                      argument.value.value.includes("specs.apollo.dev/federation/"),
                  ),
              ),
          );
          if (federation) {
            const { buildSubgraphSchema } =
              require("@apollo/subgraph") as typeof import("@apollo/subgraph");
            return buildSubgraphSchema(typeDefs);
          }
          return buildASTSchema(typeDefs);
        }));
      } catch (error) {
        throw new Error(
          `Error while loading schema: ${error instanceof Error ? error.message : String(error)}`,
          { cause: error },
        );
      }
    },
    get siblingOperations() {
      if (siblingOperations) return siblingOperations;
      if (options.schemaSdl !== undefined) return (siblingOperations = getSiblings([]));
      const config = getProject();
      if (!config.documents) return (siblingOperations = getSiblings([]));
      const counts = new Map<string, number>();
      const sources = loadSources(config, "documents", () =>
        config.loadDocumentsSync(config.documents!, {
          skipGraphQLImport: true,
          pluckConfig: config.extensions.pluckConfig,
        }),
      ).sources.flatMap((source) => {
        if (!source.document || !source.location) return [];
        let location = resolve(source.location);
        if (!/\.(?:graphql|gql)$/u.test(location)) {
          const index = counts.get(location) ?? 0;
          counts.set(location, index + 1);
          location = resolve(location, `${index}_document.graphql`);
        }
        return [
          {
            location,
            document:
              overlays.find((overlay) => overlay.location === location)?.document ??
              source.document,
          },
        ];
      });
      if (embedded) {
        for (let index = sources.length - 1; index >= 0; index--) {
          if (dirname(sources[index].location) === embedded.record.filename)
            sources.splice(index, 1);
        }
      }
      for (const overlay of overlays) {
        if (
          !sources.some((source) => source.location === overlay.location) &&
          overlay.document.definitions.some(
            (node) =>
              node.kind === Kind.OPERATION_DEFINITION || node.kind === Kind.FRAGMENT_DEFINITION,
          )
        )
          sources.push(overlay);
      }
      return (siblingOperations = getSiblings(sources));
    },
  };
}
