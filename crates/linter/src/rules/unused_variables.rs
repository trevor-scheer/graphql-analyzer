use crate::diagnostics::{CodeFix, LintDiagnostic, LintSeverity, TextEdit};
use crate::traits::{LintRule, StandaloneDocumentLintRule};
use apollo_parser::cst;
use graphql_apollo_ext::{walk_operation, CstVisitor, DocumentExt, NameExt, RangeExt};
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
            for operation in doc.tree.operations() {
                check_operation_for_unused_variables(&operation, &doc, &mut diagnostics);
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
}

/// Visitor that collects all variable references (excluding definitions)
struct VariableCollector {
    /// Track when we're inside a variable definition (to skip those)
    in_variable_definition: bool,
    variables: HashSet<String>,
}

impl VariableCollector {
    fn new() -> Self {
        Self {
            in_variable_definition: false,
            variables: HashSet::new(),
        }
    }
}

impl CstVisitor for VariableCollector {
    fn enter_variable_definition(&mut self, _var_def: &cst::VariableDefinition) {
        self.in_variable_definition = true;
    }

    fn exit_variable_definition(&mut self, _var_def: &cst::VariableDefinition) {
        self.in_variable_definition = false;
    }

    fn visit_variable(&mut self, var: &cst::Variable) {
        // Skip variables in variable definitions (those are declarations, not usages)
        if self.in_variable_definition {
            return;
        }
        if let Some(name) = var.name_text() {
            self.variables.insert(name);
        }
    }
}

/// Check a single operation for unused variables
fn check_operation_for_unused_variables(
    operation: &cst::OperationDefinition,
    doc: &graphql_syntax::DocumentRef<'_>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    // Step 1: Collect all declared variables with their ranges
    let mut declared_variables: Vec<DeclaredVariable> = Vec::new();

    if let Some(variable_definitions) = operation.variable_definitions() {
        for variable_def in variable_definitions.variable_definitions() {
            if let Some(variable) = variable_def.variable() {
                if let Some(name) = variable.name_text() {
                    let name_range = variable
                        .name_range()
                        .unwrap_or_else(|| variable.byte_range());
                    let def_range = variable_def.byte_range();

                    declared_variables.push(DeclaredVariable {
                        name,
                        name_start: name_range.start,
                        name_end: name_range.end,
                        def_start: def_range.start,
                        def_end: def_range.end,
                    });
                }
            }
        }
    }

    // If no variables declared, nothing to check
    if declared_variables.is_empty() {
        return;
    }

    // Step 2: Collect all used variables using the visitor
    let mut collector = VariableCollector::new();
    walk_operation(&mut collector, operation);

    // Step 3: Report unused variables with fixes
    for var in declared_variables {
        if !collector.variables.contains(&var.name) {
            let message = format!("Variable '${}' is declared but never used", var.name);
            let fix = compute_variable_removal_fix(&var);

            diagnostics.push(
                LintDiagnostic::warning(
                    doc.span(var.name_start, var.name_end),
                    message,
                    "unused_variables",
                )
                .with_fix(fix),
            );
        }
    }
}

/// Compute the fix for removing an unused variable
fn compute_variable_removal_fix(var: &DeclaredVariable) -> CodeFix {
    let label = format!("Remove unused variable '${}'", var.name);
    CodeFix::new(label, vec![TextEdit::delete(var.def_start, var.def_end)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_base_db::{
        DocumentKind, FileContent, FileId, FileMetadata, FileUri, Language, ProjectFiles,
    };
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
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
            Language::GraphQL,
            DocumentKind::Executable,
        );
        let project_files = create_test_project_files(&db);

        let diagnostics = rule.check(&db, file_id, content, metadata, project_files, None);

        assert_eq!(diagnostics.len(), 0);
    }
}
