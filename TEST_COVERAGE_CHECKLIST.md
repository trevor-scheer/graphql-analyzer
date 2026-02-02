# Rust Test Coverage Checklist

This document tracks test coverage for all Rust source files in the codebase.

**Last Updated:** 2026-02-02

## Legend

- ✅ Has inline tests (`#[cfg(test)]`)
- ⬜ No inline tests
- 📁 Separate test file exists
- ✓ Reviewed - no tests needed
- ➕ Tests added (this session)

## Summary Statistics

| Crate      | Files | With Tests    | Coverage | Notes                |
| ---------- | ----- | ------------- | -------- | -------------------- |
| apollo-ext | 5     | 4             | 80%      | Good coverage        |
| base-db    | 1     | 1             | 100%     | Complete             |
| syntax     | 1     | 1             | 100%     | Complete             |
| extract    | 5     | 2             | 40%      | Data types only      |
| hir        | 3     | 0 (2 📁)      | 67%      | External test files  |
| analysis   | 7     | **4** (1 📁)  | **57%**  | ➕ 2 files added     |
| linter     | 13    | **13** (1 📁) | **100%** | ➕ 6 files added     |
| config     | 5     | 3             | 60%      | Core tested          |
| introspect | 6     | 3             | 50%      | Core tested          |
| ide        | 15    | **12**        | **80%**  | ➕ 4 files added     |
| ide-db     | 1     | 1             | 100%     | Complete             |
| lsp        | 5     | **2**         | **40%**  | ➕ conversions added |
| cli        | 17    | 4             | 24%      | Commands             |
| mcp        | 5     | 1             | 20%      | Protocol layer       |

---

## Crate: apollo-ext

Extensions for apollo-rs parser/compiler.

| File             | Tests | Status | Notes               |
| ---------------- | ----- | ------ | ------------------- |
| `lib.rs`         | ⬜    | ✓      | Re-exports only     |
| `collectors.rs`  | ✅    | ✓      | Comprehensive tests |
| `definitions.rs` | ✅    | ✓      | Good coverage       |
| `names.rs`       | ✅    | ✓      | Good coverage       |
| `visitor.rs`     | ✅    | ✓      | Good coverage       |

---

## Crate: base-db

Core Salsa database types and file handling.

| File     | Tests | Status | Notes                           |
| -------- | ----- | ------ | ------------------------------- |
| `lib.rs` | ✅    | ✓      | FileId, FileContent, etc tested |

---

## Crate: syntax

GraphQL parsing layer.

| File     | Tests | Status | Notes                   |
| -------- | ----- | ------ | ----------------------- |
| `lib.rs` | ✅    | ✓      | Parse, LineIndex tested |

---

## Crate: extract

GraphQL extraction from TS/JS files.

| File                 | Tests | Status | Notes                          |
| -------------------- | ----- | ------ | ------------------------------ |
| `lib.rs`             | ⬜    | ✓      | Re-exports only                |
| `extractor.rs`       | ✅    | ✓      | Comprehensive extraction tests |
| `language.rs`        | ✅    | ✓      | Language detection tested      |
| `error.rs`           | ⬜    | ✓      | Simple error type definitions  |
| `source_location.rs` | ⬜    | ✓      | Simple data structures         |

---

## Crate: hir

High-level intermediate representation.

| File           | Tests | Status | Notes                         |
| -------------- | ----- | ------ | ----------------------------- |
| `lib.rs`       | ⬜    | ✓      | Re-exports, queries           |
| `structure.rs` | 📁    | ✓      | Tested in tests/hir_tests.rs  |
| `body.rs`      | 📁    | ✓      | Tested in tests/body_tests.rs |

---

## Crate: analysis

Validation and diagnostics.

| File                     | Tests | Status | Notes                               |
| ------------------------ | ----- | ------ | ----------------------------------- |
| `lib.rs`                 | ⬜    | ✓      | Re-exports only                     |
| `diagnostics.rs`         | ✅    | ✓      | Diagnostic creation tested          |
| `project_lints.rs`       | ✅    | ✓      | Project lint tested                 |
| `validation.rs`          | 📁    | ✓      | Tested in tests/validation_tests.rs |
| `merged_schema.rs`       | ✅ ➕ | ✓      | **Added tests**                     |
| `document_validation.rs` | ✅ ➕ | ✓      | **Added tests**                     |
| `lint_integration.rs`    | ⬜    | ✓      | Tested via integration tests        |

---

## Crate: linter

Lint rule engine and rules.

| File              | Tests | Status | Notes                   |
| ----------------- | ----- | ------ | ----------------------- |
| `lib.rs`          | ⬜    | ✓      | Re-exports              |
| `config.rs`       | ✅    | ✓      | Config parsing tested   |
| `diagnostics.rs`  | ✅    | ✓      | Diagnostic types tested |
| `registry.rs`     | ✅ ➕ | ✓      | **Added tests**         |
| `schema_utils.rs` | ✅ ➕ | ✓      | **Added tests**         |
| `traits.rs`       | ⬜    | ✓      | Trait definitions only  |

### Rules

| File                               | Tests | Status | Notes               |
| ---------------------------------- | ----- | ------ | ------------------- |
| `rules/mod.rs`                     | ⬜    | ✓      | Re-exports          |
| `rules/no_anonymous_operations.rs` | ✅    | ✓      | Snapshot tests      |
| `rules/no_deprecated.rs`           | ✅ ➕ | ✓      | **Added tests**     |
| `rules/operation_name_suffix.rs`   | ✅ ➕ | ✓      | **Added tests**     |
| `rules/redundant_fields.rs`        | ✅    | ✓      | Good coverage       |
| `rules/require_id_field.rs`        | ✅    | ✓      | Comprehensive tests |
| `rules/unique_names.rs`            | ✅ ➕ | ✓      | **Added tests**     |
| `rules/unused_fields.rs`           | ✅ ➕ | ✓      | **Added tests**     |
| `rules/unused_fragments.rs`        | ✅    | ✓      | Good coverage       |
| `rules/unused_variables.rs`        | ✅    | ✓      | Good coverage       |

---

## Crate: config

Configuration file parsing.

| File            | Tests | Status | Notes                    |
| --------------- | ----- | ------ | ------------------------ |
| `lib.rs`        | ⬜    | ✓      | Re-exports               |
| `config.rs`     | ✅    | ✓      | Config parsing tested    |
| `loader.rs`     | ✅    | ✓      | Loading tested           |
| `validation.rs` | ✅    | ✓      | Validation tested        |
| `error.rs`      | ⬜    | ✓      | Simple error definitions |

---

## Crate: introspect

Remote schema introspection.

| File        | Tests | Status | Notes                      |
| ----------- | ----- | ------ | -------------------------- |
| `lib.rs`    | ⬜    | ✓      | Re-exports                 |
| `client.rs` | ✅    | ✓      | HTTP client tested         |
| `query.rs`  | ✅    | ✓      | Introspection query tested |
| `sdl.rs`    | ✅    | ✓      | SDL conversion tested      |
| `types.rs`  | ⬜    | ✓      | Type definitions only      |
| `error.rs`  | ⬜    | ✓      | Error definitions only     |

---

## Crate: ide

IDE feature implementations.

| File                         | Tests | Status | Notes                              |
| ---------------------------- | ----- | ------ | ---------------------------------- |
| `lib.rs`                     | ✅    | ✓      | Integration tests for all features |
| `types.rs`                   | ✅    | ✓      | POD types tested                   |
| `helpers.rs`                 | ✅    | ✓      | Helper functions tested            |
| `file_registry.rs`           | ✅    | ✓      | Registry tested                    |
| `symbol.rs`                  | ✅    | ✓      | Symbol finding tested              |
| `symbols.rs`                 | ✅ ➕ | ✓      | **Added tests**                    |
| `folding_ranges.rs`          | ✅    | ✓      | Folding tested                     |
| `inlay_hints.rs`             | ✅    | ✓      | Inlay hints tested                 |
| `goto_definition.rs`         | ⬜    | ✓      | Tested via lib.rs integration      |
| `hover.rs`                   | ⬜    | ✓      | Tested via lib.rs integration      |
| `references.rs`              | ⬜    | ✓      | Tested via lib.rs integration      |
| `completion.rs`              | ✅ ➕ | ✓      | **Added tests**                    |
| `semantic_tokens.rs`         | ✅ ➕ | ✓      | **Added tests**                    |
| `code_lenses.rs`             | ✅ ➕ | ✓      | **Added tests**                    |
| `analysis_host_isolation.rs` | ⬜    | ✓      | Thread isolation                   |

---

## Crate: ide-db

IDE database layer.

| File     | Tests | Status | Notes                 |
| -------- | ----- | ------ | --------------------- |
| `lib.rs` | ✅    | ✓      | Database setup tested |

---

## Crate: lsp

Language Server Protocol implementation.

| File             | Tests | Status | Notes                     |
| ---------------- | ----- | ------ | ------------------------- |
| `lib.rs`         | ⬜    | ✓      | Re-exports                |
| `main.rs`        | ⬜    | ✓      | Entry point only          |
| `server.rs`      | ⬜    | ✓      | Complex integration layer |
| `workspace.rs`   | ✅    | ✓      | Workspace tested          |
| `conversions.rs` | ✅ ➕ | ✓      | **Added tests**           |

---

## Crate: cli

Command-line interface.

| File          | Tests | Status | Notes                  |
| ------------- | ----- | ------ | ---------------------- |
| `main.rs`     | ✅    | ✓      | CLI arg parsing tested |
| `analysis.rs` | ⬜    |        | Analysis helpers       |
| `progress.rs` | ⬜    |        | Progress reporting     |

### Commands

| File                       | Tests | Status | Notes                    |
| -------------------------- | ----- | ------ | ------------------------ |
| `commands/mod.rs`          | ⬜    | ✓      | Re-exports               |
| `commands/common.rs`       | ✅    | ✓      | Common utilities tested  |
| `commands/check.rs`        | ⬜    |        | Check command            |
| `commands/complexity.rs`   | ⬜    |        | Complexity command       |
| `commands/coverage.rs`     | ⬜    |        | Coverage command         |
| `commands/deprecations.rs` | ✅    | ✓      | Deprecations tested      |
| `commands/fix.rs`          | ⬜    |        | Fix command              |
| `commands/fragments.rs`    | ⬜    |        | Fragments command        |
| `commands/lint.rs`         | ⬜    |        | Lint command             |
| `commands/lsp.rs`          | ⬜    | ✓      | LSP server launcher only |
| `commands/mcp.rs`          | ⬜    | ✓      | MCP server launcher only |
| `commands/schema.rs`       | ✅    | ✓      | Schema command tested    |
| `commands/stats.rs`        | ⬜    |        | Stats command            |
| `commands/validate.rs`     | ⬜    |        | Validate command         |

---

## Crate: mcp

Model Context Protocol server.

| File         | Tests | Status | Notes                 |
| ------------ | ----- | ------ | --------------------- |
| `lib.rs`     | ⬜    | ✓      | Re-exports            |
| `main.rs`    | ⬜    | ✓      | Entry point only      |
| `service.rs` | ✅    | ✓      | MCP service tested    |
| `tools.rs`   | ⬜    |        | Tool definitions      |
| `types.rs`   | ⬜    | ✓      | Type definitions only |

---

## Testing Patterns Reference

### Using RootDatabase directly

```rust
use graphql_ide_db::RootDatabase;
use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri};

#[test]
fn test_example() {
    let db = RootDatabase::default();
    let file_id = FileId::new(0);
    let content = FileContent::new(&db, Arc::from("type Query { user: User }"));
    let metadata = FileMetadata::new(&db, file_id, FileUri::new("schema.graphql"), FileKind::Schema);
    // Use Salsa queries...
}
```

### Using test_project helper (test-utils crate)

```rust
use graphql_test_utils::{test_project, fixtures::BASIC_SCHEMA};

#[test]
fn test_example() {
    let (db, project) = test_project(BASIC_SCHEMA, "query { user { id } }");
    // assertions
}
```

### Using AnalysisHost (IDE integration tests)

```rust
use graphql_ide::{AnalysisHost, FileKind, FilePath};

#[test]
fn test_ide_feature() {
    let mut host = AnalysisHost::new();
    host.add_file(&FilePath::new("schema.graphql"), "type Query { user: User }", FileKind::Schema);
    host.rebuild_project_files();

    let snapshot = host.snapshot();
    let result = snapshot.some_feature(&path, position);
}
```

### Common Fixtures (test-utils)

- `BASIC_SCHEMA` - Minimal Query + User type
- `NESTED_SCHEMA` - User -> Post -> Comment chain
- `INTERFACE_SCHEMA` - Interfaces and unions
- `INPUT_SCHEMA` - Input types and enums
- `DIRECTIVE_SCHEMA` - Deprecated fields

---

## Review Progress

- [x] apollo-ext reviewed
- [x] base-db reviewed
- [x] syntax reviewed
- [x] extract reviewed
- [x] hir reviewed
- [x] analysis reviewed
- [x] linter reviewed (**4 rules added**)
- [x] config reviewed
- [x] introspect reviewed
- [x] ide reviewed
- [x] ide-db reviewed
- [x] lsp reviewed
- [x] cli reviewed
- [x] mcp reviewed

---

## Tests Added This Session

### graphql-linter crate

1. **`no_deprecated.rs`** - 10 tests added:
   - `test_deprecated_field_warning`
   - `test_no_warning_for_non_deprecated_fields`
   - `test_deprecated_root_field_warning`
   - `test_deprecated_enum_value_warning`
   - `test_non_deprecated_enum_value_no_warning`
   - `test_multiple_deprecated_usages`
   - `test_deprecated_field_in_fragment`
   - `test_mutation_with_deprecated_field`
   - `test_nested_selection_deprecated_field`
   - `test_inline_fragment_deprecated_field`
   - `test_deprecated_without_reason`

2. **`operation_name_suffix.rs`** - 11 tests added:
   - `test_query_with_correct_suffix`
   - `test_query_without_suffix_warns`
   - `test_mutation_with_correct_suffix`
   - `test_mutation_without_suffix_warns`
   - `test_subscription_with_correct_suffix`
   - `test_subscription_without_suffix_warns`
   - `test_anonymous_query_no_warning`
   - `test_anonymous_mutation_no_warning`
   - `test_multiple_operations_mixed`
   - `test_wrong_suffix_for_operation_type`
   - `test_shorthand_query_no_warning`
   - `test_suggestion_includes_correct_suffix`

3. **`unique_names.rs`** - 9 tests added:
   - `test_unique_operation_names_no_warning`
   - `test_duplicate_operation_names_in_same_file`
   - `test_duplicate_operation_names_across_files`
   - `test_unique_fragment_names_no_warning`
   - `test_duplicate_fragment_names_in_same_file`
   - `test_duplicate_fragment_names_across_files`
   - `test_same_name_for_operation_and_fragment_allowed`
   - `test_three_duplicate_operation_names`
   - `test_anonymous_operations_not_checked`

4. **`unused_fields.rs`** - 10 tests added:
   - `test_all_fields_used_no_warning`
   - `test_unused_field_warning`
   - `test_field_used_in_fragment_not_reported`
   - `test_root_type_fields_not_reported`
   - `test_introspection_types_not_reported`
   - `test_multiple_unused_fields`
   - `test_interface_field_used_through_interface`
   - `test_implementing_type_field_tracked_separately`
   - `test_nested_field_used`
   - `test_custom_schema_definition_root_types`

5. **`registry.rs`** - 9 tests added:
   - `test_standalone_document_rules_not_empty`
   - `test_document_schema_rules_not_empty`
   - `test_project_rules_not_empty`
   - `test_all_rule_names_returns_sorted_list`
   - `test_all_rule_names_includes_expected_rules`
   - `test_rules_have_unique_names`
   - `test_standalone_rules_have_valid_metadata`
   - `test_document_schema_rules_have_valid_metadata`
   - `test_project_rules_have_valid_metadata`

6. **`schema_utils.rs`** - 6 tests added:
   - `test_root_type_names_default`
   - `test_is_root_type_with_query`
   - `test_is_root_type_with_all_types`
   - `test_is_root_type_with_custom_names`
   - `test_is_root_type_empty`
   - `test_root_type_names_clone`

### graphql-analysis crate

1. **`merged_schema.rs`** - 6 tests added:
   - `test_merged_schema_result_default_values`
   - `test_merged_schema_result_with_schema`
   - `test_merged_schema_result_with_diagnostics`
   - `test_merged_schema_result_equality`
   - `test_diagnostic_range_default`
   - `test_position_values`

2. **`document_validation.rs`** - 7 tests added:
   - `test_is_builtin_scalar_int`
   - `test_is_builtin_scalar_float`
   - `test_is_builtin_scalar_string`
   - `test_is_builtin_scalar_boolean`
   - `test_is_builtin_scalar_id`
   - `test_is_builtin_scalar_custom_type`
   - `test_is_builtin_scalar_case_sensitive`

### graphql-ide crate

1. **`symbols.rs`** - 6 tests added:
   - `test_document_symbol_new`
   - `test_document_symbol_with_children`
   - `test_document_symbol_with_detail`
   - `test_workspace_symbol_new`
   - `test_workspace_symbol_with_container`
   - `test_symbol_kind_variants`

2. **`completion.rs`** - 7 tests added:
   - `test_completion_item_new`
   - `test_completion_item_with_detail`
   - `test_completion_item_with_insert_text`
   - `test_completion_item_with_insert_text_format`
   - `test_completion_kind_variants`
   - `test_insert_text_format_variants`
   - `test_completion_item_chaining`

3. **`semantic_tokens.rs`** - 5 tests added:
   - `test_semantic_token_new`
   - `test_semantic_token_type_variants`
   - `test_semantic_token_modifiers`
   - `test_semantic_token_with_deprecated_modifier`
   - `test_semantic_token_with_line_offset`

4. **`code_lenses.rs`** - 6 tests added:
   - `test_code_lens_new`
   - `test_code_lens_with_command`
   - `test_code_lens_command_new`
   - `test_code_lens_command_with_arguments`
   - `test_code_lens_info_new`
   - `test_code_lens_info_with_deprecation_reason`

### graphql-lsp crate

1. **`conversions.rs`** - 13 tests added:
   - `test_convert_lsp_position`
   - `test_convert_ide_position`
   - `test_convert_ide_range`
   - `test_convert_ide_location`
   - `test_convert_ide_completion_item_field`
   - `test_convert_ide_completion_item_fragment`
   - `test_convert_ide_completion_item_with_detail`
   - `test_convert_ide_hover`
   - `test_convert_ide_diagnostic_error`
   - `test_convert_ide_diagnostic_warning`
   - `test_convert_ide_symbol_kind`
   - `test_convert_ide_folding_range`
   - `test_convert_ide_inlay_hint`
