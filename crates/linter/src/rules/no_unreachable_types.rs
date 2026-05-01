use crate::diagnostics::{CodeSuggestion, LintDiagnostic, LintSeverity};
use crate::traits::{LintRule, StandaloneSchemaLintRule};
use graphql_base_db::{FileId, ProjectFiles};
use graphql_hir::TypeDefKind;
use std::collections::{HashMap, HashSet, VecDeque};

const BUILTIN_DIRECTIVES: &[&str] = &["deprecated", "skip", "include", "specifiedBy", "defer"];

/// Lint rule that detects unreachable types in the schema
///
/// Types that are not reachable from any root type (Query, Mutation, Subscription)
/// are dead code in the schema. They add complexity without being usable.
pub struct NoUnreachableTypesRuleImpl;

impl LintRule for NoUnreachableTypesRuleImpl {
    fn name(&self) -> &'static str {
        "noUnreachableTypes"
    }

    fn description(&self) -> &'static str {
        "Detects types that are not reachable from any root operation type"
    }

    fn default_severity(&self) -> LintSeverity {
        LintSeverity::Warning
    }
}

impl StandaloneSchemaLintRule for NoUnreachableTypesRuleImpl {
    fn check(
        &self,
        db: &dyn graphql_hir::GraphQLHirDatabase,
        project_files: ProjectFiles,
        _options: Option<&serde_json::Value>,
    ) -> HashMap<FileId, Vec<LintDiagnostic>> {
        let mut diagnostics_by_file: HashMap<FileId, Vec<LintDiagnostic>> = HashMap::new();
        let schema_types = graphql_hir::schema_types(db, project_files);
        let directive_defs = graphql_hir::schema_directives(db, project_files);

        let root_type_names =
            crate::schema_utils::extract_root_type_names(db, project_files, schema_types);

        // Build a reachability set starting from root types using BFS
        let mut reachable: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();

        // Seed with root type names
        if let Some(ref name) = root_type_names.query {
            queue.push_back(name.clone());
        }
        if let Some(ref name) = root_type_names.mutation {
            queue.push_back(name.clone());
        }
        if let Some(ref name) = root_type_names.subscription {
            queue.push_back(name.clone());
        }

        // Build a reverse-implementation map: interface name → all types that implement it.
        // When an interface becomes reachable, all its implementors become reachable too
        // (mirrors graphql-js `schema.getImplementations(type)`).
        let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
        for type_def in schema_types.values() {
            for iface in &type_def.implements {
                implementors
                    .entry(iface.to_string())
                    .or_default()
                    .push(type_def.name.to_string());
            }
        }

        // BFS to find all reachable types
        while let Some(type_name) = queue.pop_front() {
            if !reachable.insert(type_name.clone()) {
                continue;
            }

            if let Some(type_def) = schema_types.get(type_name.as_str()) {
                // Add types referenced by fields
                for field in &type_def.fields {
                    let referenced = field.type_ref.name.to_string();
                    if !reachable.contains(&referenced) {
                        queue.push_back(referenced);
                    }

                    // Add types referenced by arguments
                    for arg in &field.arguments {
                        let arg_type = arg.type_ref.name.to_string();
                        if !reachable.contains(&arg_type) {
                            queue.push_back(arg_type);
                        }
                    }

                    // When a directive is applied to a field, the argument types of
                    // that directive's definition become reachable. Upstream's visitor
                    // fires on `Directive` nodes during its AST walk, which causes it
                    // to visit the directive definition and mark its arg types reachable.
                    for directive_usage in &field.directives {
                        if let Some(dir_def) = directive_defs.get(&directive_usage.name) {
                            for dir_arg in &dir_def.arguments {
                                let arg_type = dir_arg.type_ref.name.to_string();
                                if !reachable.contains(&arg_type) {
                                    queue.push_back(arg_type);
                                }
                            }
                        }
                    }
                }

                // Add implemented interfaces (outgoing: this type → its interfaces)
                for iface in &type_def.implements {
                    let iface_name = iface.to_string();
                    if !reachable.contains(&iface_name) {
                        queue.push_back(iface_name);
                    }
                }

                // Add implementing types (incoming: interface → all types that implement it).
                // This mirrors graphql-js `schema.getImplementations(type)` which upstream
                // uses to mark concrete types reachable whenever their interface is reachable.
                if let Some(impls) = implementors.get(&type_name) {
                    for impl_name in impls {
                        if !reachable.contains(impl_name) {
                            queue.push_back(impl_name.clone());
                        }
                    }
                }

                // Add union members
                for member in &type_def.union_members {
                    let member_name = member.to_string();
                    if !reachable.contains(&member_name) {
                        queue.push_back(member_name);
                    }
                }
            }
        }

        // Directives with executable locations (QUERY, MUTATION, SUBSCRIPTION, FIELD,
        // FRAGMENT_DEFINITION, FRAGMENT_SPREAD, INLINE_FRAGMENT, VARIABLE_DEFINITION)
        // can appear in client documents, so their argument types must be reachable even
        // when they're not referenced from any schema type. This mirrors upstream's
        // `getReachableTypes` pass that iterates `schema.getDirectives()` and adds arg
        // type names for directives with request-side locations.
        for dir_def in directive_defs.values() {
            if dir_def.locations.iter().any(|loc| loc.is_executable()) {
                for dir_arg in &dir_def.arguments {
                    reachable.insert(dir_arg.type_ref.name.to_string());
                }
            }
        }

        // Report unreachable types. Scalars are included per upstream behavior.
        for type_def in schema_types.values() {
            if !reachable.contains(type_def.name.as_ref()) {
                let start: usize = type_def.name_range.start().into();
                let end: usize = type_def.name_range.end().into();
                let span = graphql_syntax::SourceSpan {
                    start,
                    end,
                    line_offset: 0,
                    byte_offset: 0,
                    source: None,
                };

                let kind_name = match type_def.kind {
                    TypeDefKind::Object => "Object type",
                    TypeDefKind::Interface => "Interface type",
                    TypeDefKind::Union => "Union type",
                    TypeDefKind::Enum => "Enum type",
                    TypeDefKind::InputObject => "Input object type",
                    TypeDefKind::Scalar => "Scalar type",
                    _ => "Type",
                };

                // Suggestion: remove the entire type def (matches upstream's
                // `fixer.remove(node.parent)`). Range comes from
                // `TypeDef.definition_range`, which the HIR already
                // populates with the full declaration's byte span.
                let def_start: usize = type_def.definition_range.start().into();
                let def_end: usize = type_def.definition_range.end().into();
                let suggestion = CodeSuggestion::delete(
                    format!("Remove `{}`", type_def.name),
                    def_start,
                    def_end,
                );

                diagnostics_by_file
                    .entry(type_def.file_id)
                    .or_default()
                    .push(
                        LintDiagnostic::new(
                            span,
                            LintSeverity::Warning,
                            format!("{kind_name} `{}` is unreachable.", type_def.name),
                            "noUnreachableTypes",
                        )
                        .with_message_id("no-unreachable-types")
                        .with_suggestion(suggestion)
                        .with_help(
                            "Remove the unreachable type, or reference it from a reachable type",
                        )
                        .with_tag(crate::diagnostics::DiagnosticTag::Unnecessary),
                    );
            }
        }

        // Also fire on extension declarations of unreachable types.  Upstream's
        // rule walks per-AST-node, so `extend type SuperUser { ... }` produces its
        // own diagnostic when `SuperUser` is unreachable.  Our merged HIR omits this
        // entry from `schema_types`, so we check raw defs for extension entries whose
        // base type is known-unreachable.
        let raw_defs = crate::schema_utils::raw_schema_type_defs(db, project_files);
        for (_, type_def) in &raw_defs {
            if !type_def.is_extension {
                continue;
            }
            if reachable.contains(type_def.name.as_ref()) {
                continue;
            }

            let start: usize = type_def.name_range.start().into();
            let end: usize = type_def.name_range.end().into();
            let span = graphql_syntax::SourceSpan {
                start,
                end,
                line_offset: 0,
                byte_offset: 0,
                source: None,
            };

            let kind_name = match type_def.kind {
                TypeDefKind::Object => "Object type",
                TypeDefKind::Interface => "Interface type",
                TypeDefKind::Union => "Union type",
                TypeDefKind::Enum => "Enum type",
                TypeDefKind::InputObject => "Input object type",
                TypeDefKind::Scalar => "Scalar type",
                _ => "Type",
            };

            let def_start: usize = type_def.definition_range.start().into();
            let def_end: usize = type_def.definition_range.end().into();
            let suggestion =
                CodeSuggestion::delete(format!("Remove `{}`", type_def.name), def_start, def_end);

            diagnostics_by_file
                .entry(type_def.file_id)
                .or_default()
                .push(
                    LintDiagnostic::new(
                        span,
                        LintSeverity::Warning,
                        format!("{kind_name} `{}` is unreachable.", type_def.name),
                        "noUnreachableTypes",
                    )
                    .with_message_id("no-unreachable-types")
                    .with_suggestion(suggestion)
                    .with_help("Remove the unreachable type, or reference it from a reachable type")
                    .with_tag(crate::diagnostics::DiagnosticTag::Unnecessary),
                );
        }

        // Report unreachable directive definitions. A directive is reachable if:
        //   a) it has executable (request-side) locations — already guaranteed to be
        //      reachable for arg-type purposes above, and the directive itself is
        //      implicitly considered reachable by upstream's request-location pass, OR
        //   b) it is actually applied somewhere in the schema (on a type def, field,
        //      argument, enum value, or schema def).
        // Built-in directives (@deprecated, @skip, @include, @specifiedBy, @defer) are
        // spec-defined and never emitted by user schemas, so we skip them.

        // Collect every directive name that is applied anywhere in the schema.
        let mut applied_directives: HashSet<String> = HashSet::new();
        for type_def in schema_types.values() {
            for d in &type_def.directives {
                applied_directives.insert(d.name.to_string());
            }
            for field in &type_def.fields {
                for d in &field.directives {
                    applied_directives.insert(d.name.to_string());
                }
                for arg in &field.arguments {
                    for d in &arg.directives {
                        applied_directives.insert(d.name.to_string());
                    }
                }
            }
            for ev in &type_def.enum_values {
                for d in &ev.directives {
                    applied_directives.insert(d.name.to_string());
                }
            }
        }

        // Directives applied directly on `schema { ... }` or `extend schema` nodes
        // are not captured by the HIR type map. Walk the raw AST to find them.
        // Upstream's visitor fires on every `Directive` AST node, including those on
        // schema definitions, so we need to cover this case too.
        let schema_ids = project_files.schema_file_ids(db).ids(db);
        for &file_id in schema_ids.iter() {
            let Some((content, metadata)) =
                graphql_base_db::file_lookup(db, project_files, file_id)
            else {
                continue;
            };
            let parse = graphql_syntax::parse(db, content, metadata);
            for doc in parse.documents() {
                for definition in &doc.ast.definitions {
                    let directives = match definition {
                        apollo_compiler::ast::Definition::SchemaDefinition(sd) => &sd.directives,
                        apollo_compiler::ast::Definition::SchemaExtension(se) => &se.directives,
                        _ => continue,
                    };
                    for d in directives {
                        applied_directives.insert(d.name.as_str().to_string());
                    }
                }
            }
        }

        for dir_def in directive_defs.values() {
            let name = dir_def.name.as_ref();
            if BUILTIN_DIRECTIVES.contains(&name) {
                continue;
            }
            // Directives with executable locations are considered reachable (used in
            // client documents). Upstream's request-location pass adds them to the
            // reachable set unconditionally, so we mirror that.
            if dir_def.locations.iter().any(|loc| loc.is_executable()) {
                continue;
            }
            if applied_directives.contains(name) {
                continue;
            }

            let start: usize = dir_def.name_range.start().into();
            let end: usize = dir_def.name_range.end().into();
            let span = graphql_syntax::SourceSpan {
                start,
                end,
                line_offset: 0,
                byte_offset: 0,
                source: None,
            };

            let def_start: usize = dir_def.definition_range.start().into();
            let def_end: usize = dir_def.definition_range.end().into();
            let suggestion =
                CodeSuggestion::delete(format!("Remove `@{}`", dir_def.name), def_start, def_end);

            diagnostics_by_file
                .entry(dir_def.file_id)
                .or_default()
                .push(
                LintDiagnostic::new(
                    span,
                    LintSeverity::Warning,
                    format!("Directive `{}` is unreachable.", dir_def.name),
                    "noUnreachableTypes",
                )
                .with_message_id("no-unreachable-types")
                .with_suggestion(suggestion)
                .with_help(
                    "Remove the unreachable directive, or apply it to a type, field, or argument",
                )
                .with_tag(crate::diagnostics::DiagnosticTag::Unnecessary),
            );
        }

        // Sort diagnostics within each file by span start so callers see a
        // deterministic, source-order output regardless of HashMap iteration order.
        for diags in diagnostics_by_file.values_mut() {
            diags.sort_by_key(|d| d.span.start);
        }

        diagnostics_by_file
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
    fn test_reachable_type() {
        let db = RootDatabase::default();
        let rule = NoUnreachableTypesRuleImpl;
        let schema = "type Query { user: User } type User { id: ID! }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let user_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("`User`"))
            .collect();
        assert!(user_warnings.is_empty());
    }

    #[test]
    fn test_unreachable_type() {
        let db = RootDatabase::default();
        let rule = NoUnreachableTypesRuleImpl;
        let schema =
            "type Query { user: User } type User { id: ID! } type OrphanType { name: String }";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let orphan_warnings: Vec<_> = diagnostics
            .values()
            .flatten()
            .filter(|d| d.message.contains("`OrphanType`"))
            .collect();
        assert_eq!(orphan_warnings.len(), 1);
    }

    // Regression test for #1037: object types that implement a reachable
    // interface must themselves be considered reachable. Clients reach them
    // via inline fragments and the resolver's `__resolveType` dispatches.
    // Mirrors the exact repro from the issue.
    #[test]
    fn test_interface_implementors_are_reachable_regression_1037() {
        let db = RootDatabase::default();
        let rule = NoUnreachableTypesRuleImpl;
        let schema = "
            interface Move { id: ID! }
            type PhysicalMove implements Move { id: ID!, makesContact: Boolean! }
            type SpecialMove implements Move { id: ID!, effectChance: Int }
            type StatusMove implements Move { id: ID!, inflicts: String }
            type Pokemon { moves: [Move!]! }
            type Query { pokemon: Pokemon }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        for n in ["PhysicalMove", "SpecialMove", "StatusMove"] {
            let hits: Vec<_> = all
                .iter()
                .filter(|d| d.message.contains(&format!("`{n}`")))
                .collect();
            assert!(
                hits.is_empty(),
                "expected `{n}` to be reachable via interface dispatch, got: {hits:?}"
            );
        }
    }

    // Companion regression test for #1037: union members are reachable via
    // the same dispatch mechanism (inline fragments on the union type).
    #[test]
    fn test_union_members_are_reachable_regression_1037() {
        let db = RootDatabase::default();
        let rule = NoUnreachableTypesRuleImpl;
        let schema = "
            type Cat { meows: Boolean! }
            type Dog { barks: Boolean! }
            union Pet = Cat | Dog
            type Query { pet: Pet }
        ";
        let project_files = create_schema_project(&db, schema);
        let diagnostics = rule.check(&db, project_files, None);
        let all: Vec<_> = diagnostics.values().flatten().collect();
        for n in ["Cat", "Dog"] {
            let hits: Vec<_> = all
                .iter()
                .filter(|d| d.message.contains(&format!("`{n}`")))
                .collect();
            assert!(
                hits.is_empty(),
                "expected union member `{n}` to be reachable, got: {hits:?}"
            );
        }
    }
}
