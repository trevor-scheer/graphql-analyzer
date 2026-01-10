# LSP and CLI Feature Proposals

This document outlines potential features for both the GraphQL LSP and CLI, informed by our Subject Matter Expert (SME) agents. Each feature includes context, implementation proposals, alternatives with tradeoffs, and the agents consulted.

---

## Table of Contents

### LSP Features
1. [Autocompletion](#1-autocompletion)
2. [Document Symbols](#2-document-symbols)
3. [Workspace Symbols](#3-workspace-symbols)
4. [Rename Symbol](#4-rename-symbol)
5. [Code Actions / Quick Fixes](#5-code-actions--quick-fixes)
6. [Signature Help](#6-signature-help)
7. [Semantic Tokens](#7-semantic-tokens)
8. [Code Lens](#8-code-lens)
9. [Inlay Hints](#9-inlay-hints)
10. [Selection Range](#10-selection-range)
11. [Folding Ranges](#11-folding-ranges)
12. [Document Formatting](#12-document-formatting)

### CLI Features
13. [Schema Diff / Breaking Changes Detection](#13-schema-diff--breaking-changes-detection)
14. [Coverage Report](#14-coverage-report)
15. [Init Command](#15-init-command)
16. [Schema Download Command](#16-schema-download-command)
17. [Check Command (Combined Validate + Lint)](#17-check-command-combined-validate--lint)
18. [Stats Command](#18-stats-command)
19. [Fix Command (Auto-fix Lint Issues)](#19-fix-command-auto-fix-lint-issues)
20. [Codegen Integration](#20-codegen-integration)

### Shared Features
21. [Fragment Usage Analysis](#21-fragment-usage-analysis)
22. [Field Usage Analysis](#22-field-usage-analysis)
23. [Deprecation Reporting](#23-deprecation-reporting)
24. [Complexity Analysis](#24-complexity-analysis)

---

## LSP Features

### 1. Autocompletion

**Context**: Autocompletion is a foundational IDE feature that GraphQL developers expect. GraphiQL provides instant field, type, and argument completion. Users typing queries need suggestions for available fields, arguments, directives, and fragment spreads.

**Agents Consulted**: GraphiQL, LSP, GraphQL, Apollo Client

**Implementation Proposal**:

Implement `textDocument/completion` with context-aware suggestions:

```rust
// In graphql-ide/src/completion.rs
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub deprecated: bool,
    pub insert_text: Option<String>,
}

pub enum CompletionKind {
    Field,
    Type,
    Fragment,
    Variable,
    Directive,
    Argument,
    EnumValue,
}

impl Analysis {
    pub fn completions(&self, file: &FilePath, position: Position) -> Vec<CompletionItem> {
        // 1. Determine completion context (field, argument, type, etc.)
        // 2. Query HIR for available options
        // 3. Filter and rank results
    }
}
```

**Completion Contexts**:

| Context | Trigger | Suggestions |
|---------|---------|-------------|
| After `{` or field | Start typing | Fields on parent type |
| After `(` or argument | Start typing | Arguments for field/directive |
| After `...` | Fragment name | Available fragments for type |
| After `$` | Variable name | Defined variables |
| After `@` | Directive name | Available directives |
| After `:` in variable | Type | Schema types |
| In enum value position | Value | Enum values |

**Alternative Approaches**:

| Approach | Pros | Cons |
|----------|------|------|
| **Eager indexing** | Instant completions | Higher memory usage |
| **On-demand computation** | Lower memory | Potential latency on large schemas |
| **Hybrid (index hot paths)** | Balance of speed/memory | More complex implementation |

**Recommendation**: Hybrid approach - index field names and types eagerly (they're accessed frequently), compute argument details on-demand.

**Performance Requirements** (per LSP agent):
- Must complete in <100ms or users perceive lag
- Support cancellation for typing-ahead scenarios
- Use incremental completion for refining results

---

### 2. Document Symbols

**Context**: Document symbols provide an outline of operations and fragments in a file. Enables quick navigation via editor outline views (Cmd+Shift+O in VSCode).

**Agents Consulted**: LSP, rust-analyzer, VSCode Extension

**Implementation Proposal**:

```rust
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbol>,
}

pub enum SymbolKind {
    Operation,   // query, mutation, subscription
    Fragment,
    Field,
    Variable,
    Argument,
}

impl Analysis {
    pub fn document_symbols(&self, file: &FilePath) -> Vec<DocumentSymbol> {
        // Parse file, extract definition hierarchy
    }
}
```

**Symbol Hierarchy**:
```
query GetUser
├── $id: ID!
├── user
│   ├── id
│   ├── name
│   └── ...UserFields
fragment UserFields
├── email
└── createdAt
```

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Flat list** | Simple implementation | Loses structure context |
| **Hierarchical (selection sets)** | Full structure visibility | Can be overwhelming for large operations |
| **Two-level (operations + top fields)** | Good balance | Loses nested structure |

**Recommendation**: Hierarchical with collapsible nodes - matches user expectations from GraphiQL explorer.

---

### 3. Workspace Symbols

**Context**: Workspace symbols enable searching across all files (Cmd+T in VSCode). Critical for finding fragments and operations by name in large projects.

**Agents Consulted**: LSP, rust-analyzer, GraphiQL

**Implementation Proposal**:

```rust
impl Analysis {
    pub fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbol> {
        // Use fuzzy matching on indexed names
        // Return operations, fragments, schema types
    }
}
```

**Index Strategy** (per rust-analyzer patterns):
- Maintain incremental symbol index via Salsa
- Index updates when files change
- Fuzzy match with case-insensitive prefix matching

**What to Index**:
- Operation names (query/mutation/subscription)
- Fragment names
- Schema type names (if schema files are in workspace)
- Directive names

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Full symbol index** | Fast search | Memory overhead |
| **On-demand search** | Low memory | Slow on large projects |
| **Trigram index** | Fast fuzzy search | Complex implementation |

**Recommendation**: Full symbol index with Salsa - memory is acceptable for typical GraphQL projects, and speed is critical per LSP agent guidelines.

---

### 4. Rename Symbol

**Context**: Rename refactoring for fragments, operations, and variables. Must update all references across the project atomically.

**Agents Consulted**: LSP, GraphQL, rust-analyzer

**Implementation Proposal**:

```rust
pub struct RenameResult {
    pub changes: HashMap<FilePath, Vec<TextEdit>>,
}

impl Analysis {
    pub fn prepare_rename(&self, file: &FilePath, position: Position) -> Option<Range> {
        // Validate rename is allowed at this position
        // Return the range of the symbol being renamed
    }

    pub fn rename(&self, file: &FilePath, position: Position, new_name: &str) -> Option<RenameResult> {
        // 1. Identify symbol at position
        // 2. Find all references (reuse find_references)
        // 3. Generate text edits for each location
    }
}
```

**Renameable Symbols**:

| Symbol | Scope | Complexity |
|--------|-------|------------|
| Fragment name | Project-wide | Medium - uses find_references |
| Operation name | Project-wide | Medium |
| Variable name | Single operation | Low - file-local |
| Argument name | Single field invocation | Low |
| Field alias | Single selection | Low |

**What NOT to rename**:
- Schema types (would require schema modification)
- Schema fields (same reason)
- Built-in directives

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Exact rename only** | Safe, predictable | Limited utility |
| **With validation** | Catches conflicts | More complex |
| **Preview mode** | User can review | Extra round-trip |

**Recommendation**: Implement with validation (check for name conflicts) and use `prepareRename` to provide feedback before committing.

---

### 5. Code Actions / Quick Fixes

**Context**: Code actions provide contextual fixes and refactorings. Essential for addressing diagnostics with automated fixes.

**Agents Consulted**: LSP, GraphiQL, GraphQL, rust-analyzer

**Implementation Proposal**:

```rust
pub struct CodeAction {
    pub title: String,
    pub kind: CodeActionKind,
    pub diagnostics: Vec<Diagnostic>,
    pub edit: Option<WorkspaceEdit>,
    pub command: Option<Command>,
}

pub enum CodeActionKind {
    QuickFix,
    Refactor,
    RefactorExtract,
    RefactorInline,
    Source,
}
```

**Proposed Code Actions**:

| Diagnostic | Quick Fix |
|------------|-----------|
| Unknown field | Suggest similar field names |
| Unknown fragment | Create fragment definition |
| Unknown variable | Add variable definition |
| Deprecated field | Show deprecation reason, suggest replacement |
| Missing required argument | Add argument with placeholder |
| Unused variable | Remove variable definition |
| Unused fragment | Remove fragment definition |

**Refactoring Actions**:

| Action | Description |
|--------|-------------|
| Extract fragment | Extract selection into new fragment |
| Inline fragment | Replace fragment spread with its content |
| Add field alias | Add alias to disambiguate |
| Wrap in named operation | Convert anonymous to named |
| Add `__typename` | Add typename for union/interface discrimination |

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Eager computation** | Actions available immediately | Expensive for large files |
| **Lazy with resolve** | Fast initial response | Extra round-trip |
| **Context-limited** | Focused suggestions | May miss useful actions |

**Recommendation**: Use lazy computation with `codeAction/resolve` - compute expensive fixes only when user selects them (per LSP agent's advice on responsiveness).

---

### 6. Signature Help

**Context**: Show argument information when typing field arguments or directive arguments. Similar to function signature help in programming languages.

**Agents Consulted**: LSP, GraphiQL, GraphQL

**Implementation Proposal**:

```rust
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: u32,
    pub active_parameter: Option<u32>,
}

pub struct SignatureInformation {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<ParameterInformation>,
}

impl Analysis {
    pub fn signature_help(&self, file: &FilePath, position: Position) -> Option<SignatureHelp> {
        // Triggered after '(' in field arguments
        // Show all arguments with types and descriptions
    }
}
```

**Trigger Characters**: `(`, `,`

**Display Example**:
```
user(id: ID!, includeDeleted: Boolean = false): User
      ^^ active parameter
```

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Full signature** | Complete information | Can be verbose |
| **Active param only** | Focused | Loses context |
| **Progressive disclosure** | Best of both | More complex UI |

**Recommendation**: Full signature with active parameter highlighting - matches GraphiQL's approach.

---

### 7. Semantic Tokens

**Context**: Semantic tokens provide rich syntax highlighting based on semantic analysis, beyond what TextMate grammars can achieve. Enables distinguishing types, fields, deprecations by color.

**Agents Consulted**: LSP, VSCode Extension, GraphQL

**Implementation Proposal**:

```rust
pub struct SemanticToken {
    pub delta_line: u32,
    pub delta_start: u32,
    pub length: u32,
    pub token_type: SemanticTokenType,
    pub token_modifiers: SemanticTokenModifiers,
}

pub enum SemanticTokenType {
    Type,           // User, Post, etc.
    Field,          // id, name, etc.
    Variable,       // $id, $limit
    Fragment,       // ...UserFields
    Operation,      // query, mutation
    Directive,      // @deprecated, @skip
    Argument,       // id:, limit:
    EnumMember,     // ACTIVE, PENDING
    Comment,
    String,
    Number,
    Keyword,        // query, fragment, on
}

bitflags! {
    pub struct SemanticTokenModifiers: u32 {
        const DEPRECATED = 0b0001;
        const DEFINITION = 0b0010;
        const READONLY = 0b0100;
    }
}
```

**Benefits**:
- Deprecated fields shown in strikethrough
- Types vs fields distinguished by color
- Variables highlighted consistently
- Schema-aware highlighting (valid vs invalid fields)

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Full semantic tokens** | Rich highlighting | Requires schema |
| **Syntactic only** | Works without schema | Less informative |
| **Hybrid** | Graceful degradation | Two code paths |

**Recommendation**: Hybrid approach - provide syntactic tokens always, enhance with semantic tokens when schema is available.

---

### 8. Code Lens

**Context**: Code lenses display actionable information above operations/fragments. Useful for showing reference counts, execution buttons, or type information.

**Agents Consulted**: LSP, VSCode Extension, GraphiQL

**Implementation Proposal**:

```rust
pub struct CodeLens {
    pub range: Range,
    pub command: Option<Command>,
    pub data: Option<Value>,
}

// Example code lenses:
// "5 references" above fragment definition
// "Run query" above operation
// "Copy as cURL" above operation
```

**Proposed Code Lenses**:

| Location | Lens | Action |
|----------|------|--------|
| Fragment definition | "N references" | Go to references |
| Operation definition | "Run" | Execute query (if configured) |
| Operation definition | "Copy as cURL" | Copy to clipboard |
| Type reference | "Go to schema" | Navigate to type definition |

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Always show** | Discoverable | Visual clutter |
| **On hover only** | Clean UI | Less discoverable |
| **Configurable** | User choice | Settings complexity |

**Recommendation**: Configurable with sensible defaults (reference count on, execute off by default).

---

### 9. Inlay Hints

**Context**: Inlay hints show inline type information without modifying the source. Useful for showing inferred variable types or field return types.

**Agents Consulted**: LSP, rust-analyzer, VSCode Extension

**Implementation Proposal**:

```rust
pub struct InlayHint {
    pub position: Position,
    pub label: String,
    pub kind: InlayHintKind,
    pub padding_left: bool,
    pub padding_right: bool,
}

pub enum InlayHintKind {
    Type,
    Parameter,
}
```

**Proposed Inlay Hints**:

```graphql
query GetUser($id: ID!) {
  user(id: $id) {
    name: String     # <- inlay hint showing return type
    posts: [Post!]!  # <- inlay hint
  }
}
```

| Location | Hint |
|----------|------|
| After field selection | Return type |
| After argument | Parameter name (for positional clarity) |
| After variable reference | Variable type |

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Show all types** | Full visibility | Visual noise |
| **Show non-obvious only** | Cleaner | Complex heuristics |
| **User toggle** | User control | Extra configuration |

**Recommendation**: Off by default, user-enabled - GraphQL types are usually clear from context, unlike Rust where type inference benefits from hints.

---

### 10. Selection Range

**Context**: Selection range enables smart expand/shrink selection (Shift+Alt+Right/Left). Selects progressively larger syntactic units.

**Agents Consulted**: LSP, rust-analyzer

**Implementation Proposal**:

```rust
impl Analysis {
    pub fn selection_ranges(&self, file: &FilePath, positions: Vec<Position>) -> Vec<SelectionRange> {
        // For each position, return nested ranges:
        // field -> selection set -> operation -> document
    }
}
```

**Selection Hierarchy**:
```
field_name        <- initial selection
field with args   <- expand
selection set     <- expand
operation body    <- expand
operation         <- expand
document          <- expand
```

---

### 11. Folding Ranges

**Context**: Folding ranges enable collapsing code blocks. Useful for large operations with many fields.

**Agents Consulted**: LSP, VSCode Extension

**Implementation Proposal**:

```rust
pub struct FoldingRange {
    pub start_line: u32,
    pub end_line: u32,
    pub kind: FoldingRangeKind,
}

pub enum FoldingRangeKind {
    Region,   // selection sets
    Comment,  // multi-line comments
}
```

**Foldable Regions**:
- Selection sets `{ ... }`
- Operation definitions
- Fragment definitions
- Multi-line string arguments
- Block comments

---

### 12. Document Formatting

**Context**: Format GraphQL documents consistently. Important for team consistency and pre-commit hooks.

**Agents Consulted**: GraphQL CLI, GraphQL, rust-analyzer

**Implementation Proposal**:

```rust
impl Analysis {
    pub fn format(&self, file: &FilePath, options: FormattingOptions) -> Vec<TextEdit> {
        // Use apollo-compiler's built-in formatting or custom formatter
    }
}
```

**Formatting Options**:
- Indentation (spaces vs tabs, width)
- Max line width
- Argument wrapping threshold
- Selection set style

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Built-in formatter** | No dependencies | Must implement/maintain |
| **External tool (prettier)** | Battle-tested | External dependency |
| **Configurable style** | Flexibility | More complexity |

**Recommendation**: Built-in formatter using apollo-compiler's serialization with configurable options. External prettier integration as optional alternative.

---

## CLI Features

### 13. Schema Diff / Breaking Changes Detection

**Context**: Detect breaking changes between schema versions. Critical for API evolution and CI/CD pipelines.

**Agents Consulted**: GraphQL CLI, GraphQL, GraphQL Inspector reference

**Implementation Proposal**:

```bash
# Compare two schema files
graphql schema diff old.graphql new.graphql

# Compare against remote endpoint
graphql schema diff --base https://api.example.com/graphql new.graphql

# Output formats
graphql schema diff --format json old.graphql new.graphql
```

**Breaking Changes to Detect**:

| Change | Severity |
|--------|----------|
| Type removed | Breaking |
| Field removed | Breaking |
| Argument removed | Breaking |
| Required argument added | Breaking |
| Type changed (incompatible) | Breaking |
| Nullable → Non-null | Breaking |
| Field added | Non-breaking |
| Type added | Non-breaking |
| Optional argument added | Non-breaking |
| Deprecation added | Non-breaking |

**Implementation**:

```rust
pub struct SchemaDiff {
    pub breaking: Vec<BreakingChange>,
    pub non_breaking: Vec<Change>,
    pub dangerous: Vec<DangerousChange>,
}

pub fn diff_schemas(old: &Schema, new: &Schema) -> SchemaDiff {
    // Compare type by type, field by field
}
```

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Strict breaking detection** | Safe | May flag safe changes |
| **Usage-aware** | Smarter | Requires usage data |
| **Customizable rules** | Flexible | Configuration overhead |

**Recommendation**: Start with strict detection, add usage-aware mode later (requires field usage tracking).

---

### 14. Coverage Report

**Context**: Show which schema fields are used by operations. Helps identify dead schema code and missing tests.

**Agents Consulted**: GraphQL CLI, GraphQL Inspector reference

**Implementation Proposal**:

```bash
# Generate coverage report
graphql coverage

# Output formats
graphql coverage --format json
graphql coverage --format html

# Filter by type
graphql coverage --type Query
```

**Report Contents**:

```
Schema Coverage Report
======================

Overall: 68% (245/360 fields)

Type Coverage:
  Query:      85% (17/20 fields)
  Mutation:   72% (13/18 fields)
  User:       90% (9/10 fields)
  Post:       45% (9/20 fields)

Unused Fields:
  Post.legacyId (deprecated)
  Post.internalMetadata
  Query.debugInfo
```

**Implementation**:

```rust
pub struct CoverageReport {
    pub total_fields: usize,
    pub used_fields: usize,
    pub by_type: HashMap<String, TypeCoverage>,
    pub unused: Vec<UnusedField>,
}

pub fn calculate_coverage(schema: &Schema, documents: &[Document]) -> CoverageReport {
    // Walk all documents, track field usage
    // Compare against all schema fields
}
```

---

### 15. Init Command

**Context**: Initialize a new GraphQL project with configuration file. Improves onboarding experience.

**Agents Consulted**: GraphQL CLI

**Implementation Proposal**:

```bash
# Interactive initialization
graphql init

# Non-interactive with defaults
graphql init --yes

# Specify schema source
graphql init --schema https://api.example.com/graphql
```

**Interactive Prompts**:
1. Schema source (file path, URL, or glob pattern)
2. Documents location (glob pattern)
3. Output format (YAML or JSON)
4. Enable recommended lint rules?

**Generated Config**:

```yaml
# .graphqlrc.yml
schema: schema.graphql
documents: "src/**/*.{graphql,ts,tsx}"
lint:
  recommended: error
```

---

### 16. Schema Download Command

**Context**: Download schema from remote endpoint via introspection. Essential for projects using remote schemas.

**Agents Consulted**: GraphQL CLI, GraphQL

**Implementation Proposal**:

```bash
# Download schema to file
graphql schema download https://api.example.com/graphql -o schema.graphql

# With headers (for authentication)
graphql schema download https://api.example.com/graphql \
  --header "Authorization: Bearer token" \
  -o schema.graphql

# Output formats
graphql schema download https://api.example.com/graphql --format sdl
graphql schema download https://api.example.com/graphql --format json
```

**Features**:
- HTTP headers support (authentication)
- Output to file or stdout
- SDL or JSON introspection format
- Timeout configuration
- Retry on failure

---

### 17. Check Command (Combined Validate + Lint)

**Context**: Single command for CI that runs both validation and linting. Simplifies CI configuration.

**Agents Consulted**: GraphQL CLI

**Implementation Proposal**:

```bash
# Run all checks
graphql check

# Equivalent to
graphql validate && graphql lint
```

**Benefits**:
- Single command for CI
- Unified exit codes
- Combined output
- Parallel execution internally

---

### 18. Stats Command

**Context**: Display statistics about the GraphQL project. Useful for understanding project size and complexity.

**Agents Consulted**: GraphQL CLI

**Implementation Proposal**:

```bash
graphql stats

# Output:
# GraphQL Project Statistics
# ==========================
# Schema:
#   Types: 45 (15 objects, 8 inputs, 10 enums, 12 interfaces)
#   Fields: 312
#   Directives: 8
#
# Documents:
#   Files: 24
#   Operations: 18 (12 queries, 5 mutations, 1 subscription)
#   Fragments: 32
#
# Complexity:
#   Avg operation depth: 4.2
#   Max operation depth: 8
#   Avg fragment spreads per operation: 2.3
```

---

### 19. Fix Command (Auto-fix Lint Issues)

**Context**: Automatically fix lint issues that have safe fixes. Saves developer time.

**Agents Consulted**: GraphQL CLI, rust-analyzer

**Implementation Proposal**:

```bash
# Fix all auto-fixable issues
graphql fix

# Dry run (show what would be fixed)
graphql fix --dry-run

# Fix specific rule
graphql fix --rule unused-fragments
```

**Auto-fixable Issues**:

| Rule | Fix |
|------|-----|
| Unused fragment | Remove fragment definition |
| Unused variable | Remove variable definition |
| Redundant type condition | Remove unnecessary type condition |
| Trailing whitespace | Remove whitespace |
| Missing `__typename` on union | Add `__typename` |

---

### 20. Codegen Integration

**Context**: Generate TypeScript types from GraphQL operations. Reduces need for separate codegen tool.

**Agents Consulted**: GraphQL CLI, Apollo Client

**Implementation Proposal**:

```bash
# Generate types
graphql codegen

# Watch mode
graphql codegen --watch

# Output configuration via .graphqlrc.yml
```

**Config**:

```yaml
schema: schema.graphql
documents: "src/**/*.graphql"
extensions:
  codegen:
    generates:
      src/generated/types.ts:
        plugins:
          - typescript
          - typescript-operations
```

**Alternatives**:

| Approach | Pros | Cons |
|----------|------|------|
| **Built-in codegen** | Unified tooling | Significant implementation effort |
| **graphql-codegen wrapper** | Leverage existing | External dependency |
| **Plugin system** | Extensible | Architecture complexity |

**Recommendation**: Start with graphql-codegen wrapper integration, consider built-in implementation later based on user demand.

---

## Shared Features

### 21. Fragment Usage Analysis

**Context**: Analyze how fragments are used across the project. Identify unused or inefficient fragment patterns.

**Agents Consulted**: GraphQL, Apollo Client, GraphiQL

**Implementation**:

Both LSP (as diagnostic/code lens) and CLI (as report):

```rust
pub struct FragmentUsage {
    pub name: String,
    pub definition_file: FilePath,
    pub usage_count: usize,
    pub usages: Vec<FragmentReference>,
    pub transitive_dependencies: Vec<String>,
}

pub fn analyze_fragment_usage(db: &dyn Database) -> Vec<FragmentUsage> {
    // Query all_fragments()
    // Cross-reference with all operations
}
```

**LSP Display**: Code lens showing "N references" on fragment definitions

**CLI Display**:
```
Fragment Usage Report
=====================
UserFields: 12 usages in 8 files
PostFields: 3 usages in 2 files
⚠ UnusedFragment: 0 usages (consider removing)
```

---

### 22. Field Usage Analysis

**Context**: Track which schema fields are used in operations. Critical for schema evolution decisions.

**Agents Consulted**: GraphQL, GraphQL CLI

**Implementation**:

```rust
pub struct FieldUsage {
    pub type_name: String,
    pub field_name: String,
    pub usage_count: usize,
    pub operations: Vec<String>,
}

pub fn analyze_field_usage(db: &dyn Database) -> HashMap<(TypeName, FieldName), FieldUsage> {
    // Walk all operations
    // Track every field selection
}
```

**LSP**: Hover shows "Used in N operations"
**CLI**: Coverage report (feature #14)

---

### 23. Deprecation Reporting

**Context**: Surface deprecated field usage prominently. Help teams migrate away from deprecated fields.

**Agents Consulted**: GraphQL, GraphiQL, LSP

**Implementation**:

**LSP Features**:
- Diagnostic with warning severity
- Strikethrough via semantic tokens
- Hover shows deprecation reason and suggested replacement
- Code action to replace with suggested field

**CLI Features**:
```bash
graphql deprecations

# Output:
# Deprecated Field Usage
# ======================
# User.legacyId (deprecated: "Use id instead")
#   - src/queries.graphql:15
#   - src/mutations.graphql:8
#
# Post.oldTitle (deprecated: "Use title instead")
#   - src/components/Post.tsx:23
```

---

### 24. Complexity Analysis

**Context**: Analyze query complexity to prevent expensive operations. Important for API rate limiting awareness.

**Agents Consulted**: GraphQL, GraphQL CLI

**Implementation Proposal**:

```rust
pub struct ComplexityAnalysis {
    pub operation_name: String,
    pub total_complexity: u32,
    pub depth: u32,
    pub breadth: u32,
    pub breakdown: Vec<FieldComplexity>,
}

pub fn analyze_complexity(operation: &Operation, schema: &Schema) -> ComplexityAnalysis {
    // Calculate based on:
    // - Selection depth
    // - List field multipliers
    // - Connection pattern detection
}
```

**CLI**:
```bash
graphql complexity

# GetAllPosts: complexity=450, depth=5
#   posts (x100)
#     author (x1)
#       posts (x10)  <- warning: nested pagination
```

**LSP**: Show complexity in code lens, warn if exceeds threshold

---

## Implementation Priority

Based on user impact and implementation complexity:

### High Priority (Foundation)
1. **Autocompletion** - Most requested IDE feature
2. **Document Symbols** - Essential navigation
3. **Code Actions** - Actionable diagnostics
4. **Schema Diff** - Critical for CI/CD

### Medium Priority (Enhancement)
5. **Workspace Symbols** - Project-wide navigation
6. **Rename Symbol** - Safe refactoring
7. **Signature Help** - Better argument experience
8. **Check Command** - CI convenience
9. **Init Command** - Onboarding improvement

### Lower Priority (Nice to Have)
10. **Semantic Tokens** - Enhanced highlighting
11. **Coverage Report** - Analytics
12. **Codegen Integration** - Tool unification
13. **Code Lens** - Visual information
14. **Inlay Hints** - Type visibility

---

## Architecture Considerations

Per rust-analyzer and LSP agent guidance:

1. **All features should be Salsa queries** - Enables caching and incrementality
2. **IDE layer (graphql-ide) owns the API** - LSP and CLI both consume it
3. **Keep LSP thin** - Only protocol translation, no business logic
4. **Support cancellation** - Long operations must check cancellation token
5. **Sub-100ms response** - Interactive features must be fast

---

## References

- [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [rust-analyzer Architecture](https://rust-analyzer.github.io/book/contributing/architecture.html)
- [GraphQL Specification](https://spec.graphql.org/)
- [graphql-language-service](https://github.com/graphql/graphiql/tree/main/packages/graphql-language-service)
- [graphql-inspector](https://graphql-inspector.com/)
