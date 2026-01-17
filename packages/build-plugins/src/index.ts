// Re-export all plugins
export { graphqlPlugin as vitePlugin } from "./vite";
export { GraphQLLspPlugin as WebpackPlugin } from "./webpack";
export { graphqlPlugin as esbuildPlugin } from "./esbuild";

// Re-export types
export type { GraphQLPluginOptions } from "./shared";
