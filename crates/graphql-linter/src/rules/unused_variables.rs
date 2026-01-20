use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst::{self, CstNode};
use graphql_base_db::{FileContent, FileId, FileMetadata, ProjectFiles};
use std::collections::HashSet;

/// Lint rule that detects variables declared in operations that are never used
///
/// This rule checks for:
/// - Variables declared in operation definitions but never referenced in the selection set
/// - Variables never used in field arguments or directives
///
/// Example:
/// ```graphql
/// query GetUser($id: ID!, $unused: String) {  # $unused is never used
///   user(id: $id) {
///     name
///   }
/// }
/// ```
pub struct UnusedVariablesRuleImpl;

impl LintRule for UnusedVariablesRuleImpl {
    fn name(&self) -> &'static str {
        "unused_variables"
    }

    fn description(&self) -> &'static str {
        "Detects variables declared in operations that are never used"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneDocumentLintRule for UnusedVariablesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        _file_id: FileId,
        content: FileContent,
        metadata: FileMetadata,
        _project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        let parse = graphql_syntax::parse(db, content, metadata);
        if parse.has_errors() {
            return diagnostics;
        }

        // Unified: check all documents (works for both pure GraphQL and TS/JS)
        for doc in parse.documents() {
            let doc_cst = doc.tree.document();
            let mut doc_diagnostics = Vec::new();

            for definition in doc_cst.definitions() {
                if let cst::Definition::OperationDefinition(operation) = definition {
                    check_operation_for_unused_variables(&operation, &mut doc_diagnostics);
                }
            }

            // Add block context for embedded GraphQL (byte_offset > 0)
            if doc.byte_offset > 0 {
                for diag in doc_diagnostics {
                    diagnostics.push(diag.with_block_context(
                        doc.line_offset,
                        doc.byte_offset,
                        std::sync::Arc::from(doc.source),
                    ));
                }
            } else {
                diagnostics.extend(doc_diagnostics);
            }
        }

        diagnostics
    }
}

/// Information about a declared variable for fix computation
struct DeclaredVariable {
    /// Variable name (without $)
    name: String,
    /// Byte offset of the variable name (for diagnostic range)
    name_start: usize,
    /// Byte offset of the end of the variable name
    name_end: usize,
    /// Byte offset of the entire variable definition (including type, default value)
    def_start: usize,
    /// Byte offset of the end of the variable definition
    def_end: usize,
    /// Index of this variable in the variable definitions list (for future use)
    #[allow(dead_code)]
    index: usize,
    /// Total number of variables in the list (for future use)
    #[allow(dead_code)]
    total: usize,
}

/// Check a single operation for unused variables
fn check_operation_for_unused_variables(
    operation: &cst::OperationDefinition,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // Step 1: Collect all declared variables with their ranges
    let mut declared_variables: Vec<DeclaredVariable> = Vec::new();

    if let Some(variable_definitions) = operation.variable_definitions() {
        let var_defs: Vec<_> = variable_definitions.variable_definitions().collect();
        let total = var_defs.len();

        for (index, variable_def) in var_defs.iter().enumerate() {
            if let Some(variable) = variable_def.variable() {
                if let Some(name) = variable.name() {
                    let var_name = name.text().to_string();
                    let name_syntax = name.syntax();
                    let name_start: usize = name_syntax.text_range().start().into();
                    let name_end: usize = name_syntax.text_range().end().into();

                    // Get the full variable definition range
                    let def_syntax = variable_def.syntax();
                    let def_start: usize = def_syntax.text_range().start().into();
                    let def_end: usize = def_syntax.text_range().end().into();

                    declared_variables.push(DeclaredVariable {
                        name: var_name,
                        name_start,
                        name_end,
                        def_start,
                        def_end,
                        index,
                        total,
                    });
                }
            }
        }
    }

    // If no variables declared, nothing to check
    if declared_variables.is_empty() {
        return;
    }

    // Step 2: Collect all used variables
    let mut used_variables = HashSet::new();

    // Check directives on the operation itself
    if let Some(directives) = operation.directives() {
        collect_variables_from_directives(&directives, &mut used_variables);
    }

    // Check the selection set
    if let Some(selection_set) = operation.selection_set() {
        collect_variables_from_selection_set(&selection_set, &mut used_variables);
    }

    // Step 3: Report unused variables with fixes
    for var in declared_variables {
        if !used_variables.contains(&var.name) {
            let message = format!("Variable '${}' is declared but never used", var.name);

            // Compute the fix range
            // For a variable list like ($a: A, $b: B, $c: C):
            // - If removing first variable and there are more: delete from start to after the comma
            // - If removing middle/last variable: delete from before the comma to end
            // - If removing only variable: need to remove entire variable definitions ()
            let fix = compute_variable_removal_fix(&var);

            diagnostics.push(
                LintDiagnostic::warning(var.name_start, var.name_end, message, "unused_variables")
                    .with_fix(fix),
            );
        }
    }
}

/// Compute the fix for removing an unused variable
fn compute_variable_removal_fix(var: &DeclaredVariable) -> CodeFix {
    let label = format!("Remove unused variable '${}'", var.name);

    // For now, we use a simple approach: delete just the variable definition
    // The CLI fix command will handle multiple variables and cleanup
    //
    // More sophisticated approach would be:
    // - Track commas between variables
    // - Delete leading comma for non-first variables
    // - Delete trailing comma for first variables when there are more
    //
    // But this is complex with the CST API, so we'll use a simpler approach
    // that works well for single unused variables

    CodeFix::new(label, vec![TextEdit::delete(var.def_start, var.def_end)])
}

/// Recursively collect variable references from a selection set
fn collect_variables_from_selection_set(
    selection_set: &cst::SelectionSet,
    variables: &mut HashSet<String>,
) {
    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                // Check field arguments
                if let Some(arguments) = field.arguments() {
                    collect_variables_from_arguments(&arguments, variables);
                }

                // Check directives on the field
                if let Some(directives) = field.directives() {
                    collect_variables_from_directives(&directives, variables);
                }

                // Recursively check nested selection sets
                if let Some(nested_selection_set) = field.selection_set() {
                    collect_variables_from_selection_set(&nested_selection_set, variables);
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                // Check directives on the fragment spread
                if let Some(directives) = spread.directives() {
                    collect_variables_from_directives(&directives, variables);
                }
            }
            cst::Selection::InlineFragment(inline) => {
                // Check directives on the inline fragment
                if let Some(directives) = inline.directives() {
                    collect_variables_from_directives(&directives, variables);
                }

                // Recursively check nested selection sets
                if let Some(nested_selection_set) = inline.selection_set() {
                    collect_variables_from_selection_set(&nested_selection_set, variables);
                }
            }
        }
    }
}

/// Collect variable references from arguments
fn collect_variables_from_arguments(arguments: &cst::Arguments, variables: &mut HashSet<String>) {
    for argument in arguments.arguments() {
        if let Some(value) = argument.value() {
            collect_variables_from_value(&value, variables);
        }
    }
}

/// Collect variable references from directives
fn collect_variables_from_directives(
    directives: &cst::Directives,
    variables: &mut HashSet<String>,
) {
    for directive in directives.directives() {
        if let Some(arguments) = directive.arguments() {
            collect_variables_from_arguments(&arguments, variables);
        }
    }
}

/// Recursively collect variable references from a value
fn collect_variables_from_value(value: &cst::Value, variables: &mut HashSet<String>) {
    match value {
        cst::Value::Variable(var) => {
            if let Some(name) = var.name() {
                variables.insert(name.text().to_string());
            }
        }
        cst::Value::ListValue(list) => {
            for item in list.values() {
                collect_variables_from_value(&item, variables);
            }
        }
        cst::Value::ObjectValue(obj) => {
            for field in obj.object_fields() {
                if let Some(field_value) = field.value() {
                    collect_variables_from_value(&field_value, variables);
                }
            }
        }
        _ => {
            // Other value types (String, Int, Float, BooleanValue, EnumValue, NullValue) don't contain variables
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_base_db::{FileContent, FileId, FileKind, FileMetadata, FileUri, ProjectFiles};
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_test_project_files(db: &RootDatabase) -> ProjectFiles {
        let schema_file_ids = graphql_base_db::SchemaFileIds::new(db, Arc::new(vec![]));
        let document_file_ids = graphql_base_db::DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map =
            graphql_base_db::FileEntryMap::new(db, Arc::new(std::collections::HashMap::new()));
        ProjectFiles::new(db, schema_file_ids, document_file_ids, file_entry_map)
    }

    #[test]
    fn test_unused_variable() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query GetUser($id: ID!, $unused: String) {
  user(id: $id) {
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Variable '$unused' is declared but never used"
        );

        // Verify fix is provided
        assert!(diagnostics[0].has_fix());
        let fix = diagnostics[0].fix.as_ref().unwrap();
        assert!(fix.label.contains("Remove unused variable"));
        assert_eq!(fix.edits.len(), 1);
        // The fix should delete the variable definition
        assert_eq!(fix.edits[0].new_text, "");
    }

    #[test]
    fn test_all_variables_used() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query GetUser($id: ID!, $name: String) {
  user(id: $id, name: $name) {
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_variable_in_directive() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query GetUser($id: ID!, $skip: Boolean!) {
  user(id: $id) @skip(if: $skip) {
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_variable_in_nested_field() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query GetUser($id: ID!, $postId: ID!) {
  user(id: $id) {
    name
    post(id: $postId) {
      title
    }
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_variable_in_list() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query GetUsers($ids: [ID!]!, $id1: ID!, $id2: ID!) {
  users(ids: [$id1, $id2]) {
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        // $ids is unused
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("$ids"));
    }

    #[test]
    fn test_variable_in_object() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query CreateUser($name: String!, $email: String!) {
  createUser(input: { name: $name, email: $email }) {
    id
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_multiple_unused_variables() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query GetUser($id: ID!, $unused1: String, $unused2: Int, $limit: Int) {
  user(id: $id, limit: $limit) {
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 2);
        let messages: Vec<_> = diagnostics.iter().map(|d| &d.message).collect();
        assert!(messages
            .iter()
            .any(|m| m.contains("$unused1") && m.contains("never used")));
        assert!(messages
            .iter()
            .any(|m| m.contains("$unused2") && m.contains("never used")));
    }

    #[test]
    fn test_no_variables() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query GetUser {
  user {
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_mutation_with_unused_variable() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
mutation UpdateUser($id: ID!, $name: String!, $unused: Boolean) {
  updateUser(id: $id, name: $name) {
    id
    name
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("$unused"));
    }

    #[test]
    fn test_variable_in_inline_fragment_directive() {
        let db = RootDatabase::default();
        let rule = UnusedVariablesRuleImpl;

        let source = "
query GetUser($id: ID!, $include: Boolean!) {
  user(id: $id) {
    name
    ... @include(if: $include) {
      email
    }
  }
}
";

        let file_id = FileId::new(0);
        let content = FileContent::new(&db, Arc::from(source));
        let metadata = FileMetadata::new(
            &db,
            file_id,
            FileUri::new("file:///test.graphql"),
            FileKind::ExecutableGraphQL,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }
}
