#![allow(clippy::too_many_lines)]

use crate::{DocumentIndex, Position, Range, SchemaIndex};
use apollo_parser::{
    cst::{self, CstNode},
    Parser,
};

/// Location information for find references
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceLocation {
    /// File path where the reference is located
    pub file_path: String,
    /// Range of the reference
    pub range: Range,
}

impl ReferenceLocation {
    #[must_use]
    pub const fn new(file_path: String, range: Range) -> Self {
        Self { file_path, range }
    }
}

/// Find references provider
pub struct FindReferencesProvider;

impl FindReferencesProvider {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Find all references to the element at a position in a GraphQL document
    ///
    /// Returns all locations where the element at the given position is referenced.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn find_references(
        &self,
        source: &str,
        position: Position,
        document_index: &DocumentIndex,
        schema_index: &SchemaIndex,
        all_documents: &[(String, String)],
        include_declaration: bool,
    ) -> Option<Vec<ReferenceLocation>> {
        tracing::info!(
            "FindReferencesProvider::find_references called with position: {:?}",
            position
        );

        let parser = Parser::new(source);
        let tree = parser.parse();

        if tree.errors().count() > 0 {
            tracing::info!("Returning None due to parser errors");
            return None;
        }

        let doc = tree.document();
        let byte_offset = Self::position_to_offset(source, position)?;
        let element_type = Self::find_element_at_position(&doc, byte_offset, source, schema_index)?;

        tracing::info!("Finding references for element: {:?}", element_type);

        let references = Self::find_all_references(
            &element_type,
            document_index,
            schema_index,
            all_documents,
            include_declaration,
        )?;

        Some(references)
    }

    fn position_to_offset(source: &str, position: Position) -> Option<usize> {
        let mut current_line = 0;
        let mut current_col = 0;
        let mut offset = 0;

        for ch in source.chars() {
            if current_line == position.line && current_col == position.character {
                return Some(offset);
            }

            if ch == '\n' {
                current_line += 1;
                current_col = 0;
            } else {
                current_col += 1;
            }

            offset += ch.len_utf8();
        }

        if current_line == position.line && current_col == position.character {
            Some(offset)
        } else {
            None
        }
    }

    fn offset_to_position(source: &str, offset: usize) -> (usize, usize) {
        let mut line = 0;
        let mut column = 0;
        let mut current_offset = 0;

        for ch in source.chars() {
            if current_offset >= offset {
                break;
            }

            if ch == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }

            current_offset += ch.len_utf8();
        }

        (line, column)
    }

    fn find_element_at_position(
        doc: &cst::Document,
        byte_offset: usize,
        _source: &str,
        _schema_index: &SchemaIndex,
    ) -> Option<ElementType> {
        for definition in doc.definitions() {
            match definition {
                cst::Definition::FragmentDefinition(frag) => {
                    // Check if cursor is on the fragment definition name
                    if let Some(frag_name) = frag.fragment_name().and_then(|n| n.name()) {
                        let range = frag_name.syntax().text_range();
                        let start: usize = range.start().into();
                        let end: usize = range.end().into();

                        if byte_offset >= start && byte_offset < end {
                            return Some(ElementType::FragmentDefinition {
                                fragment_name: frag_name.text().to_string(),
                            });
                        }
                    }

                    // Check selection set for fragment spreads
                    if let Some(selection_set) = frag.selection_set() {
                        if let Some(element) =
                            Self::check_selection_set_for_spreads(&selection_set, byte_offset)
                        {
                            return Some(element);
                        }
                    }
                }
                cst::Definition::OperationDefinition(op) => {
                    // Check selection set for fragment spreads
                    if let Some(selection_set) = op.selection_set() {
                        if let Some(element) =
                            Self::check_selection_set_for_spreads(&selection_set, byte_offset)
                        {
                            return Some(element);
                        }
                    }
                }
                _ => {}
            }
        }

        None
    }

    fn check_selection_set_for_spreads(
        selection_set: &cst::SelectionSet,
        byte_offset: usize,
    ) -> Option<ElementType> {
        for selection in selection_set.selections() {
            match selection {
                cst::Selection::FragmentSpread(spread) => {
                    if let Some(frag_name) = spread.fragment_name().and_then(|f| f.name()) {
                        let range = frag_name.syntax().text_range();
                        let start: usize = range.start().into();
                        let end: usize = range.end().into();

                        if byte_offset >= start && byte_offset < end {
                            return Some(ElementType::FragmentSpread {
                                fragment_name: frag_name.text().to_string(),
                            });
                        }
                    }
                }
                cst::Selection::Field(field) => {
                    if let Some(nested_selection_set) = field.selection_set() {
                        if let Some(element) = Self::check_selection_set_for_spreads(
                            &nested_selection_set,
                            byte_offset,
                        ) {
                            return Some(element);
                        }
                    }
                }
                cst::Selection::InlineFragment(inline_frag) => {
                    if let Some(selection_set) = inline_frag.selection_set() {
                        if let Some(element) =
                            Self::check_selection_set_for_spreads(&selection_set, byte_offset)
                        {
                            return Some(element);
                        }
                    }
                }
            }
        }

        None
    }

    fn find_all_references(
        element_type: &ElementType,
        document_index: &DocumentIndex,
        _schema_index: &SchemaIndex,
        all_documents: &[(String, String)],
        include_declaration: bool,
    ) -> Option<Vec<ReferenceLocation>> {
        match element_type {
            ElementType::FragmentDefinition { fragment_name } => {
                // Find all fragment spreads that use this fragment
                let mut references =
                    Self::find_fragment_spread_references(fragment_name, all_documents)?;

                // Add fragment definitions if requested
                if include_declaration {
                    if let Some(fragments) = document_index.get_fragments_by_name(fragment_name) {
                        for frag in fragments {
                            let range = Range {
                                start: Position {
                                    line: frag.line,
                                    character: frag.column,
                                },
                                end: Position {
                                    line: frag.line,
                                    character: frag.column + fragment_name.len(),
                                },
                            };
                            references.push(ReferenceLocation::new(frag.file_path.clone(), range));
                        }
                    }
                }

                Some(references)
            }
            ElementType::FragmentSpread { fragment_name } => {
                // When on a spread, find all spreads (same as definition)
                Self::find_fragment_spread_references(fragment_name, all_documents)
            }
        }
    }

    fn find_fragment_spread_references(
        fragment_name: &str,
        all_documents: &[(String, String)],
    ) -> Option<Vec<ReferenceLocation>> {
        let mut references = Vec::new();

        for (file_path, source) in all_documents {
            let parser = Parser::new(source);
            let tree = parser.parse();

            if tree.errors().count() > 0 {
                continue;
            }

            let doc = tree.document();
            Self::collect_fragment_spreads(&doc, fragment_name, file_path, source, &mut references);
        }

        if references.is_empty() {
            None
        } else {
            Some(references)
        }
    }

    fn collect_fragment_spreads(
        doc: &cst::Document,
        target_fragment: &str,
        file_path: &str,
        source: &str,
        references: &mut Vec<ReferenceLocation>,
    ) {
        for definition in doc.definitions() {
            match definition {
                cst::Definition::OperationDefinition(op) => {
                    if let Some(selection_set) = op.selection_set() {
                        Self::collect_fragment_spreads_from_selection_set(
                            &selection_set,
                            target_fragment,
                            file_path,
                            source,
                            references,
                        );
                    }
                }
                cst::Definition::FragmentDefinition(frag) => {
                    if let Some(selection_set) = frag.selection_set() {
                        Self::collect_fragment_spreads_from_selection_set(
                            &selection_set,
                            target_fragment,
                            file_path,
                            source,
                            references,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_fragment_spreads_from_selection_set(
        selection_set: &cst::SelectionSet,
        target_fragment: &str,
        file_path: &str,
        source: &str,
        references: &mut Vec<ReferenceLocation>,
    ) {
        for selection in selection_set.selections() {
            match selection {
                cst::Selection::Field(field) => {
                    if let Some(nested_selection_set) = field.selection_set() {
                        Self::collect_fragment_spreads_from_selection_set(
                            &nested_selection_set,
                            target_fragment,
                            file_path,
                            source,
                            references,
                        );
                    }
                }
                cst::Selection::FragmentSpread(spread) => {
                    if let Some(frag_name) = spread.fragment_name().and_then(|f| f.name()) {
                        if frag_name.text() == target_fragment {
                            let range = frag_name.syntax().text_range();
                            let (line, column) =
                                Self::offset_to_position(source, range.start().into());
                            let range = Range {
                                start: Position {
                                    line,
                                    character: column,
                                },
                                end: Position {
                                    line,
                                    character: column + target_fragment.len(),
                                },
                            };
                            references.push(ReferenceLocation::new(file_path.to_string(), range));
                        }
                    }
                }
                cst::Selection::InlineFragment(inline_frag) => {
                    if let Some(selection_set) = inline_frag.selection_set() {
                        Self::collect_fragment_spreads_from_selection_set(
                            &selection_set,
                            target_fragment,
                            file_path,
                            source,
                            references,
                        );
                    }
                }
            }
        }
    }
}

impl Default for FindReferencesProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ElementType {
    FragmentSpread { fragment_name: String },
    FragmentDefinition { fragment_name: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FragmentInfo;

    #[test]
    fn test_find_fragment_spread_references() {
        let mut doc_index = DocumentIndex::new();
        doc_index.add_fragment(
            "UserFields".to_string(),
            FragmentInfo {
                name: "UserFields".to_string(),
                type_condition: "User".to_string(),
                file_path: "/path/to/fragments.graphql".to_string(),
                line: 0,
                column: 9,
            },
        );

        let schema = SchemaIndex::new();
        let provider = FindReferencesProvider::new();

        let fragment_doc = r"
fragment UserFields on User {
    id
    name
}
";

        let query_doc = r"
query GetUser {
    user {
        ...UserFields
    }
}
";

        let all_documents = vec![
            (
                "/path/to/fragments.graphql".to_string(),
                fragment_doc.to_string(),
            ),
            (
                "/path/to/queries.graphql".to_string(),
                query_doc.to_string(),
            ),
        ];

        // Position on fragment definition name
        let position = Position {
            line: 1,
            character: 12,
        };

        let references = provider
            .find_references(
                fragment_doc,
                position,
                &doc_index,
                &schema,
                &all_documents,
                false, // exclude declaration
            )
            .expect("Should find references");

        // Should find the fragment spread in query_doc
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].file_path, "/path/to/queries.graphql");
    }

    #[test]
    fn test_find_references_include_declaration() {
        let mut doc_index = DocumentIndex::new();
        doc_index.add_fragment(
            "UserFields".to_string(),
            FragmentInfo {
                name: "UserFields".to_string(),
                type_condition: "User".to_string(),
                file_path: "/path/to/fragments.graphql".to_string(),
                line: 1,
                column: 9,
            },
        );

        let schema = SchemaIndex::new();
        let provider = FindReferencesProvider::new();

        let fragment_doc = r"
fragment UserFields on User {
    id
    name
}
";

        let query_doc = r"
query GetUser {
    user {
        ...UserFields
    }
}
";

        let all_documents = vec![
            (
                "/path/to/fragments.graphql".to_string(),
                fragment_doc.to_string(),
            ),
            (
                "/path/to/queries.graphql".to_string(),
                query_doc.to_string(),
            ),
        ];

        let position = Position {
            line: 1,
            character: 12,
        };

        let references = provider
            .find_references(
                fragment_doc,
                position,
                &doc_index,
                &schema,
                &all_documents,
                true, // include declaration
            )
            .expect("Should find references");

        // Should find the spread + the definition
        assert_eq!(references.len(), 2);
    }
}
