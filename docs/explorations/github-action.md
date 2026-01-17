# GitHub Action Exploration

**Issue**: #420
**Status**: Exploration
**Created**: 2026-01-17

## Overview

This document explores creating a GitHub Action for GraphQL validation with rich PR integration.

## Goals

1. Easy CI validation for GraphQL projects
2. Inline PR annotations for errors
3. SARIF output for GitHub security tab integration
4. Schema change detection and breaking change warnings

## Technical Analysis

### Action Types

GitHub Actions can be:
1. **JavaScript/TypeScript**: Runs in Node.js, easiest integration
2. **Docker**: Packages entire environment, larger but isolated
3. **Composite**: Combines multiple steps, uses existing tools

**Recommendation**: Docker action wrapping the Rust CLI
- Pre-built binary avoids compilation time
- Consistent environment across runs
- Can include all dependencies

### Action Structure

```
.github/actions/graphql-validate/
├── action.yml           # Action metadata
├── Dockerfile           # Build environment
├── entrypoint.sh        # Main script
└── README.md
```

Or as a separate repository:
```
graphql-lsp/validate-action/
├── action.yml
├── Dockerfile
└── ...
```

## API Design

### Inputs

```yaml
inputs:
  config:
    description: 'Path to .graphqlrc.yaml'
    required: false
    default: '.graphqlrc.yaml'

  schema:
    description: 'Path to schema file (overrides config)'
    required: false

  documents:
    description: 'Glob pattern for documents (overrides config)'
    required: false

  fail-on:
    description: 'Fail on error severity: error, warning, or info'
    required: false
    default: 'error'

  annotate:
    description: 'Add inline PR annotations'
    required: false
    default: 'true'

  sarif:
    description: 'Generate SARIF output for security tab'
    required: false
    default: 'false'

  lint:
    description: 'Enable linting (uses config rules)'
    required: false
    default: 'true'
```

### Outputs

```yaml
outputs:
  errors:
    description: 'Number of errors found'

  warnings:
    description: 'Number of warnings found'

  sarif-file:
    description: 'Path to SARIF output file (if sarif: true)'
```

## Usage Examples

### Basic Usage

```yaml
name: GraphQL Validation
on: [pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: graphql-lsp/validate@v1
```

### With Configuration

```yaml
- uses: graphql-lsp/validate@v1
  with:
    config: .graphqlrc.yaml
    fail-on: error
    annotate: true
```

### Schema Override

```yaml
- uses: graphql-lsp/validate@v1
  with:
    schema: schema.graphql
    documents: 'src/**/*.graphql'
```

### With SARIF Upload

```yaml
- uses: graphql-lsp/validate@v1
  id: validate
  with:
    sarif: true

- uses: github/codeql-action/upload-sarif@v3
  if: always()
  with:
    sarif_file: ${{ steps.validate.outputs.sarif-file }}
```

### Multi-Project

```yaml
- uses: graphql-lsp/validate@v1
  with:
    config: .graphqlrc.yaml
    project: frontend
```

## Implementation Details

### action.yml

```yaml
name: 'GraphQL Validate'
description: 'Validate GraphQL schemas and operations'
author: 'graphql-lsp'
branding:
  icon: 'check-circle'
  color: 'purple'

inputs:
  config:
    description: 'Path to .graphqlrc.yaml'
    default: '.graphqlrc.yaml'
  fail-on:
    description: 'Severity level to fail on'
    default: 'error'
  annotate:
    description: 'Add inline PR annotations'
    default: 'true'
  sarif:
    description: 'Generate SARIF output'
    default: 'false'
  lint:
    description: 'Run linting'
    default: 'true'

outputs:
  errors:
    description: 'Number of errors'
  warnings:
    description: 'Number of warnings'
  sarif-file:
    description: 'Path to SARIF file'

runs:
  using: 'docker'
  image: 'Dockerfile'
  args:
    - ${{ inputs.config }}
    - ${{ inputs.fail-on }}
    - ${{ inputs.annotate }}
    - ${{ inputs.sarif }}
    - ${{ inputs.lint }}
```

### Dockerfile

```dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release --package graphql-cli

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/graphql /usr/local/bin/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
```

### Inline Annotations

GitHub Actions supports workflow commands for annotations:

```bash
# Error annotation
echo "::error file=src/query.graphql,line=5,col=10::Unknown field 'foo'"

# Warning annotation
echo "::warning file=src/query.graphql,line=8::Field 'name' is deprecated"
```

CLI output format for GitHub:

```bash
graphql validate --format=github
```

Output:
```
::error file=src/queries/user.graphql,line=5,col=3,endLine=5,endColumn=15::Unknown field "nonExistent" on type "User"
::warning file=src/queries/user.graphql,line=8,col=3::Field "email" is deprecated: Use "contactEmail" instead
```

### SARIF Output

SARIF (Static Analysis Results Interchange Format) structure:

```json
{
  "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
  "version": "2.1.0",
  "runs": [{
    "tool": {
      "driver": {
        "name": "graphql-lsp",
        "version": "0.1.0",
        "informationUri": "https://github.com/trevor-scheer/graphql-lsp",
        "rules": [
          {
            "id": "graphql/unknown-field",
            "name": "UnknownField",
            "shortDescription": { "text": "Field does not exist on type" },
            "defaultConfiguration": { "level": "error" }
          }
        ]
      }
    },
    "results": [
      {
        "ruleId": "graphql/unknown-field",
        "level": "error",
        "message": { "text": "Unknown field \"foo\" on type \"User\"" },
        "locations": [{
          "physicalLocation": {
            "artifactLocation": { "uri": "src/queries/user.graphql" },
            "region": {
              "startLine": 5,
              "startColumn": 3,
              "endLine": 5,
              "endColumn": 6
            }
          }
        }]
      }
    ]
  }]
}
```

CLI command:
```bash
graphql validate --format=sarif > results.sarif
```

## PR Comment Summary

Optional feature: Post a summary comment on PRs

```markdown
## GraphQL Validation Results

✅ **Schema**: Valid
❌ **Documents**: 3 errors, 2 warnings

### Errors

| File | Line | Message |
|------|------|---------|
| `src/queries/user.graphql` | 5 | Unknown field "foo" |
| `src/queries/user.graphql` | 12 | Fragment "Bar" not found |
| `src/queries/post.graphql` | 3 | Type "Poast" not found |

### Warnings

| File | Line | Message |
|------|------|---------|
| `src/queries/user.graphql` | 8 | Field "email" is deprecated |
| `src/queries/post.graphql` | 15 | Unused fragment "PostFields" |
```

## CLI Enhancements Needed

The CLI needs new output formats:

```bash
# GitHub Actions annotations
graphql validate --format=github

# SARIF for security tab
graphql validate --format=sarif

# JSON for programmatic use
graphql validate --format=json
```

## Distribution Options

### Option A: Separate Repository

```
graphql-lsp/validate-action/
├── action.yml
├── Dockerfile
└── README.md
```

Pros:
- Clear separation
- Independent versioning
- Marketplace listing

Cons:
- Separate maintenance
- Version coordination

### Option B: Monorepo Directory

```
graphql-lsp/graphql-lsp/
├── .github/
│   └── actions/
│       └── validate/
│           ├── action.yml
│           └── Dockerfile
```

Pros:
- Single repository
- Shared CI
- Always in sync

Cons:
- More complex release process

**Recommendation**: Start with monorepo directory, extract if needed.

## Open Questions

1. **Pre-built vs build-from-source**:
   - Pre-built: faster, ~5 seconds
   - Build: slower (~2 min), but always current

2. **Remote schemas**: How to handle introspection?
   - Secrets for auth tokens?
   - Cache schema between runs?

3. **Incremental validation**: Only validate changed files?
   - Would need to track dependencies
   - May miss cross-file issues

4. **Monorepo support**: Multiple projects?
   - Matrix strategy per project?
   - Single run with multiple configs?

## Next Steps

1. [ ] Add `--format=github` to CLI
2. [ ] Add `--format=sarif` to CLI
3. [ ] Create action.yml
4. [ ] Create Dockerfile with pre-built binary
5. [ ] Test with sample repository
6. [ ] Document usage patterns
7. [ ] Publish to GitHub Marketplace

## References

- [GitHub Actions documentation](https://docs.github.com/en/actions)
- [Creating Docker actions](https://docs.github.com/en/actions/creating-actions/creating-a-docker-container-action)
- [SARIF specification](https://sarifweb.azurewebsites.net/)
- [GitHub Code Scanning](https://docs.github.com/en/code-security/code-scanning)
