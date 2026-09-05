use crate::diagnostics::{CodeSuggestion, LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDef;
use serde::Deserialize;
use std::collections::HashMap;

/// Options for the `input_name` rule
///
/// Mirrors the `@graphql-eslint/eslint-plugin` rule of the same name. The
/// rule fires on field arguments of `Mutation` (and optionally `Query`) to
/// enforce a consistent argument name (`input`) and, optionally, a
/// matching input type name.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct InputNameOptions {
    /// Check that the input type name follows the convention `<FieldName>Input`.
    pub check_input_type: bool,
    /// Allow case differences when comparing the input type name.
    pub case_sensitive_input_type: bool,
    /// Apply the rule to fields of the Query root type.
    pub check_queries: bool,
    /// Apply the rule to fields of the Mutation root type.
    pub check_mutations: bool,
}

impl Default for InputNameOptions {
    fn default() -> Self {
        Self {
            check_input_type: false,
            case_sensitive_input_type: true,
            check_queries: false,
            check_mutations: true,
        }
    }
}

impl InputNameOptions {
    fn from_json(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Lint rule that enforces conventional naming for mutation/query field
/// arguments.
///
/// By default, every argument on a `Mutation` field must be named `input`.
/// When `checkInputType` is enabled, the argument's type name must equal
/// `<FieldName>Input`. Optionally applies the same checks to `Query`
/// fields when `checkQueries` is enabled.
pub struct InputNameRuleImpl;

impl LintRule for InputNameRuleImpl {
    fn name(&self) -> &'static str {
        "inputName"
    }

    fn description(&self) -> &'static str {
        "Require mutation/query field arguments to be named `input` and (optionally) typed as `<FieldName>Input`"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for InputNameRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let opts = InputNameOptions::from_json(options);
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();

        if !opts.check_mutations && !opts.check_queries {
            return diagnostics_by_file;
        }

        let schema_types = graphql_hir::schema_types(db, project_files);
        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        let mut roots: Vec<&TypeDef> = Vec::new();
        if opts.check_mutations {
            if let Some(name) = root_type_names.mutation.as_deref() {
                if let Some(td) = schema_types.get(name) {
                    roots.push(td);
                }
            }
        }
        if opts.check_queries {
            if let Some(name) = root_type_names.query.as_deref() {
                if let Some(td) = schema_types.get(name) {
                    roots.push(td);
                }
            }
        }

        for root_type in roots {
            for field in &root_type.fields {
                for arg in &field.arguments {
                    if arg.name.as_ref() != "input" {
                        let span = make_span(arg.name_range);
                        let name_start: usize = arg.name_range.start().into();
                        let name_end: usize = arg.name_range.end().into();
                        let suggestion = CodeSuggestion::replace(
                            "Rename to `input`".to_string(),
                            name_start,
                            name_end,
                            "input".to_string(),
                        );
                        diagnostics_by_file.entry(arg.file_id).or_default().push(
                            LintDiagnostic::new(
                                span,
                                LintSeverity::Warning,
                                format!(
                                    "Input \"{}\" should be named \"input\" for \"{}.{}\"",
                                    arg.name, root_type.name, field.name
                                ),
                                "inputName",
                            )
                            .with_suggestion(suggestion)
                            .with_help("Rename to `input`"),
                        );
                    }

                    if opts.check_input_type {
                        let expected = format!("{}Input", field.name);
                        let actual = arg.type_ref.name.as_ref();
                        let mismatch = if opts.case_sensitive_input_type {
                            actual != expected
                        } else {
                            !actual.eq_ignore_ascii_case(&expected)
                        };

                        if mismatch {
                            // Upstream's diagnostic points at `node.name` (the
                            // type's Name token) and the suggestion replaces
                            // that same token. We use `type_ref.name_range` for
                            // both so the byte ranges line up with upstream
                            // for parity.
                            let type_range = arg.type_ref.name_range;
                            let span = if type_range.start() == type_range.end() {
                                make_span(arg.name_range)
                            } else {
                                make_span(type_range)
                            };
                            let mut diag = LintDiagnostic::new(
                                span,
                                LintSeverity::Warning,
                                format!("Input type `{actual}` name should be `{expected}`."),
                                "inputName",
                            )
                            .with_help(format!("Rename to `{expected}`"));
                            if type_range.start() != type_range.end() {
                                let start: usize = type_range.start().into();
                                let end: usize = type_range.end().into();
                                diag = diag.with_suggestion(CodeSuggestion::replace(
                                    format!("Rename to `{expected}`"),
                                    start,
                                    end,
                                    expected.clone(),
                                ));
                            }
                            diagnostics_by_file
                                .entry(arg.file_id)
                                .or_default()
                                .push(diag);
                        }
                    }
                }
            }
        }

        diagnostics_by_file
    }
}

fn make_span(range: graphql_hir::TextRange) -> graphql_syntax::SourceSpan {
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    graphql_syntax::SourceSpan {
        start,
        end,
        line_offset: 0,
        byte_offset: 0,
        source: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StandaloneSchemaLintRule;
    use graphql_base_db::{
        DocumentFileIds, DocumentKind, FileContent, FileEntry, FileEntryMap, FileId, FileMetadata,
        FileUri, Language, ProjectFiles, SchemaFileIds,
    };
    use graphql_ide_db::RootDatabase;
    use std::sync::Arc;

    fn create_schema_project(db: &RootDatabase, schema: &str) -> ProjectFiles {
        let file_id = FileId::new(0);
        let content = FileContent::new(db, Arc::from(schema));
        let metadata = FileMetadata::new(
            db,
            file_id,
            FileUri::new("file:///schema.graphql"),
            Language::GraphQL,
            DocumentKind::Schema,
        );
        let entry = FileEntry::new(db, content, metadata);
        let mut entries = std::collections::HashMap::new();
        entries.insert(file_id, entry);
        let schema_file_ids = SchemaFileIds::new(db, Arc::new(vec![file_id]));
        let document_file_ids = DocumentFileIds::new(db, Arc::new(vec![]));
        let file_entry_map = FileEntryMap::new(db, Arc::new(entries));
        ProjectFiles::new(
            db,
            schema_file_ids,
            document_file_ids,
            graphql_base_db::ResolvedSchemaFileIds::new(db, std::sync::Arc::new(vec![])),
            file_entry_map,
            graphql_base_db::FilePathMap::new(
                db,
                Arc::new(std::collections::HashMap::new()),
                Arc::new(std::collections::HashMap::new()),
            ),
        )
    }

    #[test]
    fn mutation_argument_named_input_is_ok() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Mutation { setMessage(input: SetMessageInput): String } \
                      input SetMessageInput { message: String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn mutation_argument_not_named_input_reports() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Mutation { setMessage(message: String): String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Input \"message\" should be named \"input\" for \"Mutation.setMessage\""
        );
    }

    #[test]
    fn multiple_arguments_each_must_be_input() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Mutation { setMessage(a: String, b: Int): String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn query_arguments_skipped_by_default() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Query { user(id: ID!): String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn query_arguments_checked_when_enabled() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Query { user(id: ID!): String }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "checkQueries": true });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert!(all[0].message.contains("Query.user"));
    }

    #[test]
    fn mutations_can_be_disabled() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Mutation { setMessage(message: String): String }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "checkMutations": false });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn check_input_type_flags_mismatch() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Mutation { setMessage(input: InputMessage): String } \
                      input InputMessage { message: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "checkInputType": true });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].message,
            "Input type `InputMessage` name should be `setMessageInput`."
        );
    }

    #[test]
    fn check_input_type_passes_on_matching_name() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Mutation { setMessage(input: setMessageInput): String } \
                      input setMessageInput { message: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "checkInputType": true });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn case_insensitive_input_type_allows_case_differences() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Mutation { SetMessage(input: setmessageinput): String } \
                      input setmessageinput { message: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({
            "checkInputType": true,
            "caseSensitiveInputType": false,
        });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }

    #[test]
    fn check_input_type_and_argument_name_both_reported() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        let schema = "type Mutation { setMessage(message: InputMessage): String } \
                      input InputMessage { message: String }";
        let project_files = create_schema_project(&db, schema);
        let opts = serde_json::json!({ "checkInputType": true });
        let diagnostics = rule.check(&db, project_files, Some(&opts));
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert_eq!(all.len(), 2);
        assert!(all
            .iter()
            .any(|d| d.message.contains("should be named \"input\"")));
        assert!(all
            .iter()
            .any(|d| d.message.contains("name should be `setMessageInput`")));
    }

    #[test]
    fn input_object_type_definitions_are_not_checked() {
        let db = RootDatabase::default();
        let rule = InputNameRuleImpl;
        // No mutation/query fields - the rule should ignore input type
        // definitions entirely.
        let schema = "input SomeArbitraryName { name: String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        assert!(all.is_empty());
    }
}
