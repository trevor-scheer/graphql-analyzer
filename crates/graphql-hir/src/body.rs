// Body extraction - extracts selection sets and field selections
// These are computed lazily and only when needed for validation

use apollo_parser::cst::{self, CstNode};
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
    selection_set: &cst::SelectionSet,
) -> (Vec<Selection>, HashSet<Arc<str>>) {
    let mut selections = Vec::new();
    let mut fragment_spreads = HashSet::new();

    for selection in selection_set.selections() {
        if let Some(sel) = extract_selection(selection, &mut fragment_spreads) {
            selections.push(sel);
        }
    }

    (selections, fragment_spreads)
}

fn extract_selection(
    selection: cst::Selection,
    fragment_spreads: &mut HashSet<Arc<str>>,
) -> Option<Selection> {
    match selection {
        cst::Selection::Field(field) => {
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
                    let value = Arc::from(arg.value()?.syntax().text().to_string().as_str());
                    Some((arg_name, value))
                })
                .collect();

            let selection_set = field
                .selection_set()
                .map(|ss| extract_selections(&ss).0)
                .unwrap_or_default();

            Some(Selection::Field {
                name,
                alias,
                arguments,
                selection_set,
            })
        }
        cst::Selection::FragmentSpread(spread) => {
            let name: Arc<str> = Arc::from(spread.fragment_name()?.name()?.text().as_str());
            fragment_spreads.insert(name.clone());
            Some(Selection::FragmentSpread { name })
        }
        cst::Selection::InlineFragment(inline) => {
            let type_condition = inline
                .type_condition()
                .and_then(|tc| tc.named_type())
                .and_then(|nt| nt.name())
                .map(|n| Arc::from(n.text().as_str()));

            let selection_set = inline
                .selection_set()
                .map(|ss| extract_selections(&ss).0)
                .unwrap_or_default();

            Some(Selection::InlineFragment {
                type_condition,
                selection_set,
            })
        }
    }
}
