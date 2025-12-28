// Body extraction - extracts selection sets and field selections
// These are computed lazily and only when needed for validation

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

/// Extract selections from a selection set
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
