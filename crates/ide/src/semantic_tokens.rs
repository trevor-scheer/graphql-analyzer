//! Semantic tokens feature implementation.
//!
//! This module provides IDE semantic token functionality for syntax highlighting:
//! - Token types (keywords, types, fields, fragments)
//! - Token modifiers (deprecated)

use std::collections::HashMap;
use std::sync::Arc;

use crate::types::Position;
use crate::FileRegistry;
use crate::{SemanticToken, SemanticTokenModifiers, SemanticTokenType};

/// Get semantic tokens for a file.
///
/// Returns tokens for syntax highlighting with semantic information,
/// including deprecation status for fields.
pub fn semantic_tokens(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_base_db::ProjectFiles>,
    file: &crate::FilePath,
) -> Vec<SemanticToken> {
    let (content, metadata) = {
        let Some(file_id) = registry.get_file_id(file) else {
            return Vec::new();
        };

        let Some(content) = registry.get_content(file_id) else {
            return Vec::new();
        };
        let Some(metadata) = registry.get_metadata(file_id) else {
            return Vec::new();
        };

        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);
    if parse.has_errors() {
        return Vec::new();
    }

    let schema_types: Option<&HashMap<Arc<str>, graphql_hir::TypeDef>> =
        project_files.map(|pf| graphql_hir::schema_types(db, pf));

    let mut tokens = Vec::new();

    for doc in parse.documents() {
        let doc_line_index = graphql_syntax::LineIndex::new(doc.source);
        collect_semantic_tokens_from_document(
            &doc.tree.document(),
            &doc_line_index,
            doc.line_offset as u32,
            schema_types,
            &mut tokens,
        );
    }

    tokens.sort_by(|a, b| {
        a.start
            .line
            .cmp(&b.start.line)
            .then_with(|| a.start.character.cmp(&b.start.character))
    });

    tokens
}

/// Collect semantic tokens from a GraphQL document.
///
/// Walks the document and emits tokens for fields, types, fragments, etc.
/// Checks the schema to determine if fields are deprecated.
fn collect_semantic_tokens_from_document(
    doc_cst: &apollo_parser::cst::Document,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    schema_types: Option<&HashMap<Arc<str>, graphql_hir::TypeDef>>,
    tokens: &mut Vec<SemanticToken>,
) {
    use apollo_parser::cst::{self, CstNode};

    for definition in doc_cst.definitions() {
        match definition {
            cst::Definition::OperationDefinition(operation) => {
                if let Some(op_type) = operation.operation_type() {
                    if let Some(token) = op_type
                        .query_token()
                        .or_else(|| op_type.mutation_token())
                        .or_else(|| op_type.subscription_token())
                    {
                        emit_token_for_syntax_token(
                            &token,
                            line_index,
                            line_offset,
                            SemanticTokenType::Keyword,
                            SemanticTokenModifiers::NONE,
                            tokens,
                        );
                    }
                }

                let root_type_name = operation.operation_type().map_or("Query", |op_type| {
                    if op_type.query_token().is_some() {
                        "Query"
                    } else if op_type.mutation_token().is_some() {
                        "Mutation"
                    } else if op_type.subscription_token().is_some() {
                        "Subscription"
                    } else {
                        "Query"
                    }
                });

                if let Some(selection_set) = operation.selection_set() {
                    collect_tokens_from_selection_set(
                        &selection_set,
                        Some(root_type_name),
                        schema_types,
                        line_index,
                        line_offset,
                        tokens,
                    );
                }
            }
            cst::Definition::FragmentDefinition(fragment) => {
                if let Some(fragment_token) = fragment.fragment_token() {
                    emit_token_for_syntax_token(
                        &fragment_token,
                        line_index,
                        line_offset,
                        SemanticTokenType::Keyword,
                        SemanticTokenModifiers::NONE,
                        tokens,
                    );
                }

                if let Some(type_condition) = fragment.type_condition() {
                    if let Some(on_token) = type_condition.on_token() {
                        emit_token_for_syntax_token(
                            &on_token,
                            line_index,
                            line_offset,
                            SemanticTokenType::Keyword,
                            SemanticTokenModifiers::NONE,
                            tokens,
                        );
                    }
                    if let Some(named_type) = type_condition.named_type() {
                        if let Some(name) = named_type.name() {
                            emit_token_for_syntax_node(
                                name.syntax(),
                                line_index,
                                line_offset,
                                SemanticTokenType::Type,
                                SemanticTokenModifiers::NONE,
                                tokens,
                            );
                        }
                    }
                }

                let type_name = fragment
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|name| name.text().to_string());

                if let Some(selection_set) = fragment.selection_set() {
                    collect_tokens_from_selection_set(
                        &selection_set,
                        type_name.as_deref(),
                        schema_types,
                        line_index,
                        line_offset,
                        tokens,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Collect semantic tokens from a selection set.
fn collect_tokens_from_selection_set(
    selection_set: &apollo_parser::cst::SelectionSet,
    parent_type_name: Option<&str>,
    schema_types: Option<&HashMap<Arc<str>, graphql_hir::TypeDef>>,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    tokens: &mut Vec<SemanticToken>,
) {
    use apollo_parser::cst::{self, CstNode};

    let parent_type = parent_type_name.and_then(|name| schema_types?.get(name));

    for selection in selection_set.selections() {
        match selection {
            cst::Selection::Field(field) => {
                if let Some(field_name_node) = field.name() {
                    let field_name = field_name_node.text();

                    let is_deprecated = parent_type
                        .and_then(|pt| {
                            pt.fields
                                .iter()
                                .find(|f| f.name.as_ref() == field_name.as_ref())
                        })
                        .is_some_and(|f| f.is_deprecated);

                    let modifiers = if is_deprecated {
                        SemanticTokenModifiers::DEPRECATED
                    } else {
                        SemanticTokenModifiers::NONE
                    };

                    emit_token_for_syntax_node(
                        field_name_node.syntax(),
                        line_index,
                        line_offset,
                        SemanticTokenType::Property,
                        modifiers,
                        tokens,
                    );

                    let field_return_type = parent_type
                        .and_then(|pt| {
                            pt.fields
                                .iter()
                                .find(|f| f.name.as_ref() == field_name.as_ref())
                        })
                        .map(|f| f.type_ref.name.as_ref());

                    if let Some(nested_selection_set) = field.selection_set() {
                        collect_tokens_from_selection_set(
                            &nested_selection_set,
                            field_return_type,
                            schema_types,
                            line_index,
                            line_offset,
                            tokens,
                        );
                    }
                }
            }
            cst::Selection::FragmentSpread(spread) => {
                if let Some(name) = spread.fragment_name().and_then(|fn_| fn_.name()) {
                    emit_token_for_syntax_node(
                        name.syntax(),
                        line_index,
                        line_offset,
                        SemanticTokenType::Function,
                        SemanticTokenModifiers::NONE,
                        tokens,
                    );
                }
            }
            cst::Selection::InlineFragment(inline) => {
                if let Some(type_condition) = inline.type_condition() {
                    if let Some(on_token) = type_condition.on_token() {
                        emit_token_for_syntax_token(
                            &on_token,
                            line_index,
                            line_offset,
                            SemanticTokenType::Keyword,
                            SemanticTokenModifiers::NONE,
                            tokens,
                        );
                    }
                    if let Some(named_type) = type_condition.named_type() {
                        if let Some(name) = named_type.name() {
                            emit_token_for_syntax_node(
                                name.syntax(),
                                line_index,
                                line_offset,
                                SemanticTokenType::Type,
                                SemanticTokenModifiers::NONE,
                                tokens,
                            );
                        }
                    }
                }

                let type_name = inline
                    .type_condition()
                    .and_then(|tc| tc.named_type())
                    .and_then(|nt| nt.name())
                    .map(|name| name.text().to_string());

                let type_name_ref = type_name.as_deref().or(parent_type_name);

                if let Some(selection_set) = inline.selection_set() {
                    collect_tokens_from_selection_set(
                        &selection_set,
                        type_name_ref,
                        schema_types,
                        line_index,
                        line_offset,
                        tokens,
                    );
                }
            }
        }
    }
}

/// Emit a semantic token for a syntax node.
fn emit_token_for_syntax_node(
    node: &apollo_parser::SyntaxNode,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    token_type: SemanticTokenType,
    modifiers: SemanticTokenModifiers,
    tokens: &mut Vec<SemanticToken>,
) {
    let offset: usize = node.text_range().start().into();
    let len: u32 = node.text_range().len().into();

    let (line, col) = line_index.line_col(offset);
    tokens.push(SemanticToken::new(
        Position::new(line as u32 + line_offset, col as u32),
        len,
        token_type,
        modifiers,
    ));
}

/// Emit a semantic token for a syntax token (keyword, punctuation, etc.).
fn emit_token_for_syntax_token(
    token: &apollo_parser::SyntaxToken,
    line_index: &graphql_syntax::LineIndex,
    line_offset: u32,
    token_type: SemanticTokenType,
    modifiers: SemanticTokenModifiers,
    tokens: &mut Vec<SemanticToken>,
) {
    let offset: usize = token.text_range().start().into();
    let len: u32 = token.text_range().len().into();

    let (line, col) = line_index.line_col(offset);
    tokens.push(SemanticToken::new(
        Position::new(line as u32 + line_offset, col as u32),
        len,
        token_type,
        modifiers,
    ));
}
