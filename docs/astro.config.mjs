import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://trevor-scheer.github.io",
  base: "/graphql-analyzer",
  integrations: [
    starlight({
      title: "GraphQL Analyzer",
      description: "Fast, Rust-powered GraphQL tooling for your editor and CI.",
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/trevor-scheer/graphql-analyzer",
        },
      ],
      editLink: {
        baseUrl: "https://github.com/trevor-scheer/graphql-analyzer/edit/main/docs/",
      },
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", slug: "getting-started/introduction" },
            { label: "Quick Start", slug: "getting-started/quick-start" },
            { label: "Installation", slug: "getting-started/installation" },
          ],
        },
        {
          label: "Editor Setup",
          items: [
            { label: "VS Code", slug: "editors/vscode" },
            { label: "Neovim", slug: "editors/neovim" },
            { label: "Other Editors", slug: "editors/other" },
          ],
        },
        {
          label: "Configuration",
          items: [
            {
              label: "Configuration File",
              slug: "configuration/config-file",
            },
            { label: "Schema Sources", slug: "configuration/schema-sources" },
            { label: "Documents", slug: "configuration/documents" },
            {
              label: "Multi-Project Workspaces",
              slug: "configuration/multi-project",
            },
            {
              label: "Tool-Specific Overrides",
              slug: "configuration/tool-overrides",
            },
          ],
        },
        {
          label: "CLI",
          items: [
            { label: "Overview", slug: "cli/overview" },
            { label: "validate", slug: "cli/validate" },
            { label: "lint", slug: "cli/lint" },
            { label: "check", slug: "cli/check" },
            { label: "Output Formats", slug: "cli/output-formats" },
            { label: "CI/CD Integration", slug: "cli/ci-cd" },
          ],
        },
        {
          label: "IDE Features",
          items: [
            { label: "Diagnostics", slug: "ide-features/diagnostics" },
            {
              label: "Go to Definition",
              slug: "ide-features/goto-definition",
            },
            {
              label: "Find References",
              slug: "ide-features/find-references",
            },
            { label: "Hover", slug: "ide-features/hover" },
            {
              label: "Embedded GraphQL",
              slug: "ide-features/embedded-graphql",
            },
          ],
        },
        {
          label: "Linting",
          items: [
            { label: "Overview", slug: "linting/overview" },
            { label: "Configuration", slug: "linting/configuration" },
          ],
        },
        {
          label: "Rules",
          items: [
            { label: "Rules Catalog", slug: "rules/catalog" },
            {
              label: "no-anonymous-operations",
              slug: "rules/no-anonymous-operations",
            },
            { label: "no-deprecated", slug: "rules/no-deprecated" },
            { label: "redundant-fields", slug: "rules/redundant-fields" },
            { label: "require-id-field", slug: "rules/require-id-field" },
            { label: "unique-names", slug: "rules/unique-names" },
            { label: "unused-fields", slug: "rules/unused-fields" },
            { label: "unused-fragments", slug: "rules/unused-fragments" },
            { label: "unused-variables", slug: "rules/unused-variables" },
            {
              label: "operation-name-suffix",
              slug: "rules/operation-name-suffix",
            },
          ],
        },
        {
          label: "AI Integration",
          items: [{ label: "MCP Server", slug: "ai-integration/mcp" }],
        },
        {
          label: "Advanced",
          items: [
            {
              label: "Remote Schema Introspection",
              slug: "advanced/remote-schemas",
            },
            {
              label: "Performance Tuning",
              slug: "advanced/performance-tuning",
            },
            { label: "Troubleshooting", slug: "advanced/troubleshooting" },
          ],
        },
      ],
    }),
  ],
});
