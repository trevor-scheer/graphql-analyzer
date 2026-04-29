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
            { label: "completions", slug: "cli/completions" },
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
            { label: "ESLint Plugin", slug: "linting/eslint-plugin" },
            {
              label: "Migrating from graphql-eslint",
              slug: "linting/migrating-from-graphql-eslint",
            },
          ],
        },
        {
          label: "Rules",
          items: [
            { label: "Rules Catalog", slug: "rules/catalog" },
            { label: "alphabetize", slug: "rules/alphabetize" },
            { label: "descriptionStyle", slug: "rules/descriptionStyle" },
            { label: "inputName", slug: "rules/inputName" },
            {
              label: "loneExecutableDefinition",
              slug: "rules/loneExecutableDefinition",
            },
            { label: "matchDocumentFilename", slug: "rules/matchDocumentFilename" },
            { label: "namingConvention", slug: "rules/namingConvention" },
            {
              label: "noAnonymousOperations",
              slug: "rules/noAnonymousOperations",
            },
            { label: "noDeprecated", slug: "rules/noDeprecated" },
            { label: "noDuplicateFields", slug: "rules/noDuplicateFields" },
            {
              label: "noHashtagDescription",
              slug: "rules/noHashtagDescription",
            },
            { label: "noOnePlaceFragments", slug: "rules/noOnePlaceFragments" },
            { label: "noRootType", slug: "rules/noRootType" },
            {
              label: "noScalarResultTypeOnMutation",
              slug: "rules/noScalarResultTypeOnMutation",
            },
            { label: "noTypenamePrefix", slug: "rules/noTypenamePrefix" },
            { label: "noUnreachableTypes", slug: "rules/noUnreachableTypes" },
            { label: "noUnusedFields", slug: "rules/noUnusedFields" },
            { label: "noUnusedFragments", slug: "rules/noUnusedFragments" },
            { label: "noUnusedVariables", slug: "rules/noUnusedVariables" },
            { label: "operationNameSuffix", slug: "rules/operationNameSuffix" },
            { label: "redundantFields", slug: "rules/redundantFields" },
            { label: "relayArguments", slug: "rules/relayArguments" },
            { label: "relayConnectionTypes", slug: "rules/relayConnectionTypes" },
            { label: "relayEdgeTypes", slug: "rules/relayEdgeTypes" },
            { label: "relayPageInfo", slug: "rules/relayPageInfo" },
            { label: "requireDeprecationDate", slug: "rules/requireDeprecationDate" },
            {
              label: "requireDeprecationReason",
              slug: "rules/requireDeprecationReason",
            },
            { label: "requireDescription", slug: "rules/requireDescription" },
            {
              label: "requireFieldOfTypeQueryInMutationResult",
              slug: "rules/requireFieldOfTypeQueryInMutationResult",
            },
            { label: "requireIdField", slug: "rules/requireIdField" },
            { label: "requireImportFragment", slug: "rules/requireImportFragment" },
            {
              label: "requireNullableFieldsWithOneof",
              slug: "rules/requireNullableFieldsWithOneof",
            },
            { label: "requireNullableResultInRoot", slug: "rules/requireNullableResultInRoot" },
            { label: "requireSelections", slug: "rules/requireSelections" },
            { label: "requireTypePatternWithOneof", slug: "rules/requireTypePatternWithOneof" },
            { label: "restyFieldNames", slug: "rules/restyFieldNames" },
            { label: "selectionSetDepth", slug: "rules/selectionSetDepth" },
            { label: "strictIdInTypes", slug: "rules/strictIdInTypes" },
            {
              label: "uniqueEnumValueNames",
              slug: "rules/uniqueEnumValueNames",
            },
            { label: "uniqueNames", slug: "rules/uniqueNames" },
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
