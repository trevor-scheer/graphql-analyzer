// Body extraction - extracts selection sets and field selections
// These are computed lazily and only when needed for validation

use apollo_parser::ast::{self, AstNode};
use graphql_db::FileId;
use std::collections::HashSet;
use std::sync::Arc;

use crate::OperationId;

/// Body of an operation (selection set and fragment spreads)
/// This is expensive to compute, so we only do it when needed
#[salsa::tracked]
pub struct OperationBody {
    #[return_ref]
    pub selection_set: Vec<Selection>,
    #[return_ref]
    pub fragment_spreads: HashSet<Arc<str>>,
}

/// Body of a fragment (selection set and fragment spreads)
#[salsa::tracked]
pub struct FragmentBody {
    #[return_ref]
    pub selection_set: Vec<Selection>,
    #[return_ref]
    pub fragment_spreads: HashSet<Arc<str>>,
}

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

/// Get the body of an operation
/// This is computed lazily and only when needed for validation
#[salsa::tracked]
pub fn operation_body(
    db: &dyn crate::GraphQLHirDatabase,
    operation_id: OperationId,
) -> OperationBody {
    // For now, we'll need to look up the operation by scanning file structures
    // In a real implementation, we'd store a mapping from OperationId to (FileId, index)
    // This is a simplified implementation for Phase 2

    // Get all document files and find the operation
    let document_files = db.document_file_ids();

    for &file_id in document_files.iter() {
        let parse = graphql_syntax::parse(db, file_id);
        let structure = crate::structure::file_structure(db, file_id);

        // Check main tree
        let operations_in_file = structure.operations(db);
        if let Some(op_struct) = operations_in_file
            .iter()
            .find(|op| op.file_id == file_id && op.index == operation_id.as_id().as_u32() as usize)
        {
            // Found the operation, extract its body
            let document = parse.tree.document();
            if let Some(op_def) = document
                .definitions()
                .filter_map(|d| match d {
                    ast::Definition::OperationDefinition(op) => Some(op),
                    _ => None,
                })
                .nth(op_struct.index)
            {
                return extract_operation_body(db, op_def);
            }
        }

        // Check extracted blocks
        for block in &parse.blocks {
            let document = block.tree.document();
            for (idx, def) in document.definitions().enumerate() {
                if let ast::Definition::OperationDefinition(op) = def {
                    // Simple check: if this is the right index
                    if idx == operation_id.as_id().as_u32() as usize {
                        return extract_operation_body(db, op);
                    }
                }
            }
        }
    }

    // If not found, return empty body
    OperationBody::new(db, Vec::new(), HashSet::new())
}

/// Get the body of a fragment
#[salsa::tracked]
pub fn fragment_body(
    db: &dyn crate::GraphQLHirDatabase,
    file_id: FileId,
    fragment_name: Arc<str>,
) -> FragmentBody {
    let parse = graphql_syntax::parse(db, file_id);

    // Search main tree
    let document = parse.tree.document();
    for def in document.definitions() {
        if let ast::Definition::FragmentDefinition(frag) = def {
            if let Some(name) = frag.fragment_name().and_then(|n| n.name()) {
                if name.text() == fragment_name.as_ref() {
                    return extract_fragment_body(db, frag);
                }
            }
        }
    }

    // Search extracted blocks
    for block in &parse.blocks {
        let document = block.tree.document();
        for def in document.definitions() {
            if let ast::Definition::FragmentDefinition(frag) = def {
                if let Some(name) = frag.fragment_name().and_then(|n| n.name()) {
                    if name.text() == fragment_name.as_ref() {
                        return extract_fragment_body(db, frag);
                    }
                }
            }
        }
    }

    // If not found, return empty body
    FragmentBody::new(db, Vec::new(), HashSet::new())
}

fn extract_operation_body(
    db: &dyn crate::GraphQLHirDatabase,
    op: ast::OperationDefinition,
) -> OperationBody {
    let mut fragment_spreads = HashSet::new();

    let selection_set = if let Some(sel_set) = op.selection_set() {
        extract_selection_set(sel_set, &mut fragment_spreads)
    } else {
        Vec::new()
    };

    OperationBody::new(db, selection_set, fragment_spreads)
}

fn extract_fragment_body(
    db: &dyn crate::GraphQLHirDatabase,
    frag: ast::FragmentDefinition,
) -> FragmentBody {
    let mut fragment_spreads = HashSet::new();

    let selection_set = if let Some(sel_set) = frag.selection_set() {
        extract_selection_set(sel_set, &mut fragment_spreads)
    } else {
        Vec::new()
    };

    FragmentBody::new(db, selection_set, fragment_spreads)
}

fn extract_selection_set(
    sel_set: ast::SelectionSet,
    fragment_spreads: &mut HashSet<Arc<str>>,
) -> Vec<Selection> {
    sel_set
        .selections()
        .filter_map(|sel| extract_selection(sel, fragment_spreads))
        .collect()
}

fn extract_selection(
    selection: ast::Selection,
    fragment_spreads: &mut HashSet<Arc<str>>,
) -> Option<Selection> {
    match selection {
        ast::Selection::Field(field) => {
            let name = Arc::from(field.name()?.text().as_str());
            let alias = field
                .alias()
                .and_then(|a| a.name())
                .map(|n| Arc::from(n.text().as_str()));

            let arguments = field
                .arguments()
                .into_iter()
                .flat_map(|args| args.arguments())
                .filter_map(|arg| {
                    let arg_name = Arc::from(arg.name()?.text().as_str());
                    let value = Arc::from(arg.value()?.to_string().as_str());
                    Some((arg_name, value))
                })
                .collect();

            let selection_set = field
                .selection_set()
                .map(|ss| extract_selection_set(ss, fragment_spreads))
                .unwrap_or_default();

            Some(Selection::Field {
                name,
                alias,
                arguments,
                selection_set,
            })
        }
        ast::Selection::FragmentSpread(spread) => {
            let name = Arc::from(spread.fragment_name()?.name()?.text().as_str());
            fragment_spreads.insert(name.clone());
            Some(Selection::FragmentSpread { name })
        }
        ast::Selection::InlineFragment(inline) => {
            let type_condition = inline
                .type_condition()
                .and_then(|tc| tc.named_type())
                .and_then(|nt| nt.name())
                .map(|n| Arc::from(n.text().as_str()));

            let selection_set = inline
                .selection_set()
                .map(|ss| extract_selection_set(ss, fragment_spreads))
                .unwrap_or_default();

            Some(Selection::InlineFragment {
                type_condition,
                selection_set,
            })
        }
    }
}
