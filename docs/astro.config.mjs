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
            { label: "alphabetize", slug: "rules/alphabetize" },
            { label: "description-style", slug: "rules/description-style" },
            { label: "input-name", slug: "rules/input-name" },
            {
              label: "lone-executable-definition",
              slug: "rules/lone-executable-definition",
            },
            { label: "naming-convention", slug: "rules/naming-convention" },
            {
              label: "no-anonymous-operations",
              slug: "rules/no-anonymous-operations",
            },
            { label: "no-deprecated", slug: "rules/no-deprecated" },
            { label: "no-duplicate-fields", slug: "rules/no-duplicate-fields" },
            {
              label: "no-hashtag-description",
              slug: "rules/no-hashtag-description",
            },
            {
              label: "no-one-place-fragments",
              slug: "rules/no-one-place-fragments",
            },
            {
              label: "no-scalar-result-type-on-mutation",
              slug: "rules/no-scalar-result-type-on-mutation",
            },
            { label: "no-typename-prefix", slug: "rules/no-typename-prefix" },
            {
              label: "no-unreachable-types",
              slug: "rules/no-unreachable-types",
            },
            {
              label: "operation-name-suffix",
              slug: "rules/operation-name-suffix",
            },
            { label: "redundant-fields", slug: "rules/redundant-fields" },
            {
              label: "require-deprecation-reason",
              slug: "rules/require-deprecation-reason",
            },
            { label: "require-description", slug: "rules/require-description" },
            {
              label: "require-field-of-type-query-in-mutation-result",
              slug: "rules/require-field-of-type-query-in-mutation-result",
            },
            { label: "require-id-field", slug: "rules/require-id-field" },
            { label: "require-selections", slug: "rules/require-selections" },
            { label: "selection-set-depth", slug: "rules/selection-set-depth" },
            { label: "strict-id-in-types", slug: "rules/strict-id-in-types" },
            {
              label: "unique-enum-value-names",
              slug: "rules/unique-enum-value-names",
            },
            { label: "unique-names", slug: "rules/unique-names" },
            { label: "unused-fields", slug: "rules/unused-fields" },
            { label: "unused-fragments", slug: "rules/unused-fragments" },
            { label: "unused-variables", slug: "rules/unused-variables" },
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
