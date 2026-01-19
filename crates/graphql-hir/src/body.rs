use apollo_compiler::executable;
use std::collections::HashSet;
use std::sync::Arc;

/// A selection in a selection set
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selection {
    Field {
        name: Arc<str>,
        alias: Option<Arc<str>>,
        arguments: Vec<(Arc<str>, Arc<str>)>,
        selection_set: Vec<Selection>,
    },
    FragmentSpread {
        name: Arc<str>,
    },
    InlineFragment {
        type_condition: Option<Arc<str>>,
        selection_set: Vec<Selection>,
    },
}

/// The body of an operation (selection set and metadata)
///
/// This is separated from `OperationStructure` to enable fine-grained invalidation:
/// editing the selection set only invalidates this body, not the operation's
/// name/variables (structure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationBody {
    /// The selections in this operation
    pub selections: Vec<Selection>,
    /// Fragment names directly referenced by this operation (not transitive)
    pub fragment_spreads: HashSet<Arc<str>>,
    /// Variable names used in this operation
    pub variable_usages: HashSet<Arc<str>>,
}

/// The body of a fragment (selection set and metadata)
///
/// This is separated from `FragmentStructure` to enable fine-grained invalidation:
/// editing the selection set only invalidates this body, not the fragment's
/// name/type condition (structure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentBody {
    /// The selections in this fragment
    pub selections: Vec<Selection>,
    /// Fragment names directly referenced by this fragment (not transitive)
    pub fragment_spreads: HashSet<Arc<str>>,
    /// Variable names used in this fragment
    pub variable_usages: HashSet<Arc<str>>,
}

/// Extract the body of an operation
///
/// This query only invalidates when the operation's selection set changes.
/// The operation's structure (name, variables) can change without invalidating this.
#[salsa::tracked]
pub fn operation_body(
    db: &dyn crate::GraphQLHirDatabase,
    file_content: graphql_base_db::FileContent,
    file_metadata: graphql_base_db::FileMetadata,
    operation_index: usize,
) -> Arc<OperationBody> {
    let parse = graphql_syntax::parse(db, file_content, file_metadata);

    let mut op_count = 0;

    for doc in parse.documents() {
        for definition in &doc.ast.definitions {
            if let apollo_compiler::ast::Definition::OperationDefinition(op) = definition {
                if op_count == operation_index {
                    return Arc::new(extract_operation_body_from_ast(op));
                }
                op_count += 1;
            }
        }
    }

    Arc::new(OperationBody {
        selections: Vec::new(),
        fragment_spreads: HashSet::new(),
        variable_usages: HashSet::new(),
    })
}

/// Extract the body of a fragment by name
///
/// This query only invalidates when the fragment's selection set changes.
#[salsa::tracked]
#[allow(clippy::needless_pass_by_value)] // Arc<str> needed for Salsa tracking
pub fn fragment_body(
    db: &dyn crate::GraphQLHirDatabase,
    file_content: graphql_base_db::FileContent,
    file_metadata: graphql_base_db::FileMetadata,
    fragment_name: Arc<str>,
) -> Arc<FragmentBody> {
    let parse = graphql_syntax::parse(db, file_content, file_metadata);

    for doc in parse.documents() {
        for definition in &doc.ast.definitions {
            if let apollo_compiler::ast::Definition::FragmentDefinition(frag) = definition {
                if frag.name.as_str() == fragment_name.as_ref() {
                    return Arc::new(extract_fragment_body_from_ast(frag));
                }
            }
        }
    }

    Arc::new(FragmentBody {
        selections: Vec::new(),
        fragment_spreads: HashSet::new(),
        variable_usages: HashSet::new(),
    })
}

/// Get all fragments transitively used by an operation
///
/// This handles circular fragment references gracefully by tracking visited fragments.
#[salsa::tracked]
pub fn operation_transitive_fragments(
    db: &dyn crate::GraphQLHirDatabase,
    file_content: graphql_base_db::FileContent,
    file_metadata: graphql_base_db::FileMetadata,
    operation_index: usize,
    project_files: graphql_base_db::ProjectFiles,
) -> Arc<HashSet<Arc<str>>> {
    let body = operation_body(db, file_content, file_metadata, operation_index);

    let mut visited = HashSet::new();
    let mut to_visit: Vec<Arc<str>> = body.fragment_spreads.iter().cloned().collect();

    let fragment_index = crate::fragment_file_index(db, project_files);

    while let Some(frag_name) = to_visit.pop() {
        if !visited.insert(frag_name.clone()) {
            continue;
        }

        if let Some((content, metadata)) = fragment_index.get(&frag_name) {
            let frag_body = fragment_body(db, *content, *metadata, frag_name.clone());

            for spread in &frag_body.fragment_spreads {
                if !visited.contains(spread) {
                    to_visit.push(spread.clone());
                }
            }
        }
    }

    Arc::new(visited)
}

/// Extract operation body from an AST operation definition
fn extract_operation_body_from_ast(
    op: &apollo_compiler::ast::OperationDefinition,
) -> OperationBody {
    let (selections, fragment_spreads) = extract_selections_from_ast(&op.selection_set);
    let variable_usages = extract_variable_usages_from_selections(&selections);

    OperationBody {
        selections,
        fragment_spreads,
        variable_usages,
    }
}

/// Extract fragment body from an AST fragment definition
fn extract_fragment_body_from_ast(frag: &apollo_compiler::ast::FragmentDefinition) -> FragmentBody {
    let (selections, fragment_spreads) = extract_selections_from_ast(&frag.selection_set);
    let variable_usages = extract_variable_usages_from_selections(&selections);

    FragmentBody {
        selections,
        fragment_spreads,
        variable_usages,
    }
}

/// Extract selections from an AST selection set (Vec<Selection>)
fn extract_selections_from_ast(
    selection_set: &[apollo_compiler::ast::Selection],
) -> (Vec<Selection>, HashSet<Arc<str>>) {
    let mut selections = Vec::new();
    let mut fragment_spreads = HashSet::new();

    for selection in selection_set {
        extract_selection_from_ast(selection, &mut selections, &mut fragment_spreads);
    }

    (selections, fragment_spreads)
}

fn extract_selection_from_ast(
    selection: &apollo_compiler::ast::Selection,
    selections: &mut Vec<Selection>,
    fragment_spreads: &mut HashSet<Arc<str>>,
) {
    match selection {
        apollo_compiler::ast::Selection::Field(field) => {
            let name = Arc::from(field.name.as_str());
            let alias = field.alias.as_ref().map(|a| Arc::from(a.as_str()));

            let arguments = field
                .arguments
                .iter()
                .map(|arg| {
                    let arg_name = Arc::from(arg.name.as_str());
                    let value = Arc::from(arg.value.to_string().as_str());
                    (arg_name, value)
                })
                .collect();

            let (nested_selections, nested_spreads) =
                extract_selections_from_ast(&field.selection_set);
            fragment_spreads.extend(nested_spreads);

            selections.push(Selection::Field {
                name,
                alias,
                arguments,
                selection_set: nested_selections,
            });
        }
        apollo_compiler::ast::Selection::FragmentSpread(spread) => {
            let name: Arc<str> = Arc::from(spread.fragment_name.as_str());
            fragment_spreads.insert(name.clone());
            selections.push(Selection::FragmentSpread { name });
        }
        apollo_compiler::ast::Selection::InlineFragment(inline) => {
            let type_condition = inline
                .type_condition
                .as_ref()
                .map(|tc| Arc::from(tc.as_str()));

            let (nested_selections, nested_spreads) =
                extract_selections_from_ast(&inline.selection_set);
            fragment_spreads.extend(nested_spreads);

            selections.push(Selection::InlineFragment {
                type_condition,
                selection_set: nested_selections,
            });
        }
    }
}

/// Extract variable usages from selections
fn extract_variable_usages_from_selections(selections: &[Selection]) -> HashSet<Arc<str>> {
    let mut usages = HashSet::new();

    for selection in selections {
        match selection {
            Selection::Field {
                arguments,
                selection_set,
                ..
            } => {
                for (_name, value) in arguments {
                    if let Some(var_name) = value.strip_prefix('$') {
                        usages.insert(Arc::from(var_name));
                    }
                }
                usages.extend(extract_variable_usages_from_selections(selection_set));
            }
            Selection::InlineFragment { selection_set, .. } => {
                usages.extend(extract_variable_usages_from_selections(selection_set));
            }
            Selection::FragmentSpread { .. } => {}
        }
    }

    usages
}

/// Extract selections from an executable selection set (for validation)
#[must_use]
pub fn extract_selections(
    selection_set: &executable::SelectionSet,
) -> (Vec<Selection>, HashSet<Arc<str>>) {
    let mut selections = Vec::new();
    let mut fragment_spreads = HashSet::new();

    for selection in &selection_set.selections {
        extract_selection(selection, &mut selections, &mut fragment_spreads);
    }

    (selections, fragment_spreads)
}

fn extract_selection(
    selection: &executable::Selection,
    selections: &mut Vec<Selection>,
    fragment_spreads: &mut HashSet<Arc<str>>,
) {
    match selection {
        executable::Selection::Field(field_node) => {
            let field = &**field_node;
            let name = Arc::from(field.name.as_str());
            let alias = field.alias.as_ref().map(|a| Arc::from(a.as_str()));

            let arguments = field
                .arguments
                .iter()
                .map(|arg| {
                    let arg_name = Arc::from(arg.name.as_str());
                    let value = Arc::from(arg.value.to_string().as_str());
                    (arg_name, value)
                })
                .collect();

            let selection_set = extract_selections(&field.selection_set).0;

            selections.push(Selection::Field {
                name,
                alias,
                arguments,
                selection_set,
            });
        }
        executable::Selection::FragmentSpread(spread_node) => {
            let spread = &**spread_node;
            let name: Arc<str> = Arc::from(spread.fragment_name.as_str());
            fragment_spreads.insert(name.clone());
            selections.push(Selection::FragmentSpread { name });
        }
        executable::Selection::InlineFragment(inline_node) => {
            let inline = &**inline_node;
            let type_condition = inline
                .type_condition
                .as_ref()
                .map(|tc| Arc::from(tc.as_str()));

            let selection_set = extract_selections(&inline.selection_set).0;

            selections.push(Selection::InlineFragment {
                type_condition,
                selection_set,
            });
        }
    }
}
