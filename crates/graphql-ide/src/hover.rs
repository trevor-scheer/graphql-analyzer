//! Hover feature implementation.
//!
//! This module provides IDE hover functionality including:
//! - Type information for fields
//! - Type definitions with descriptions
//! - Fragment information

use std::fmt::Write as _;

use crate::helpers::{find_block_for_position, format_type_ref, position_to_offset};
use crate::symbol::{find_parent_type_at_offset, find_symbol_at_offset, Symbol};
use crate::types::{FilePath, HoverResult, Position};
use crate::FileRegistry;

/// Get hover information at a position in a file.
///
/// Returns documentation, type information, etc. for the symbol at the position.
#[allow(clippy::too_many_lines)]
pub fn hover(
    db: &dyn graphql_hir::GraphQLHirDatabase,
    registry: &FileRegistry,
    project_files: Option<graphql_db::ProjectFiles>,
    file: &FilePath,
    position: Position,
) -> Option<HoverResult> {
    let (content, metadata) = {
        let file_id = registry.get_file_id(file)?;

        let content = registry.get_content(file_id)?;
        let metadata = registry.get_metadata(file_id)?;

        (content, metadata)
    };

    let parse = graphql_syntax::parse(db, content, metadata);

    let line_index = graphql_syntax::line_index(db, content);

    let metadata_line_offset = metadata.line_offset(db);
    let (block_context, adjusted_position) =
        find_block_for_position(&parse, position, metadata_line_offset)?;

    tracing::debug!(
        "Hover: original position {:?}, block line_offset {}, adjusted position {:?}",
        position,
        block_context.line_offset,
        adjusted_position
    );

    let offset = if let Some(block_source) = block_context.block_source {
        let block_line_index = graphql_syntax::LineIndex::new(block_source);
        position_to_offset(&block_line_index, adjusted_position)?
    } else {
        position_to_offset(&line_index, adjusted_position)?
    };

    // Try to find the symbol at the offset even if there are parse errors
    let symbol = find_symbol_at_offset(block_context.tree, offset);

    // If we couldn't find a symbol and there are parse errors, show the errors
    if symbol.is_none() && !parse.errors.is_empty() {
        let error_messages: Vec<&str> = parse.errors.iter().map(|e| e.message.as_str()).collect();
        return Some(HoverResult::new(format!(
            "**Syntax Errors**\n\n{}",
            error_messages.join("\n")
        )));
    }

    let symbol = symbol?;
    let project_files = project_files?;

    match symbol {
        Symbol::FieldName { name } => {
            let types = graphql_hir::schema_types(db, project_files);
            let parent_ctx = find_parent_type_at_offset(&parse.tree, offset)?;

            // Use walk_type_stack_to_offset to properly resolve the parent type
            let parent_type_name = crate::symbol::walk_type_stack_to_offset(
                &parse.tree,
                types,
                offset,
                &parent_ctx.root_type,
            )?;

            tracing::debug!(
                "Hover: resolved parent type '{}' for field '{}' (root: {})",
                parent_type_name,
                name,
                parent_ctx.root_type
            );

            // Look up the field in the parent type
            let parent_type = types.get(parent_type_name.as_str())?;
            let field = parent_type
                .fields
                .iter()
                .find(|f| f.name.as_ref() == name)?;

            let mut hover_text = format!("**Field:** `{name}`\n\n");
            let field_type = format_type_ref(&field.type_ref);
            write!(hover_text, "**Type:** `{field_type}`\n\n").ok();

            if let Some(desc) = &field.description {
                write!(hover_text, "---\n\n{desc}\n\n").ok();
            }

            Some(HoverResult::new(hover_text))
        }
        Symbol::TypeName { name } => {
            let types = graphql_hir::schema_types(db, project_files);
            let type_def = types.get(name.as_str())?;

            let mut hover_text = format!("**Type:** `{name}`\n\n");
            let kind_str = match type_def.kind {
                graphql_hir::TypeDefKind::Object => "Object",
                graphql_hir::TypeDefKind::Interface => "Interface",
                graphql_hir::TypeDefKind::Union => "Union",
                graphql_hir::TypeDefKind::Enum => "Enum",
                graphql_hir::TypeDefKind::Scalar => "Scalar",
                graphql_hir::TypeDefKind::InputObject => "Input Object",
            };
            write!(hover_text, "**Kind:** {kind_str}\n\n").ok();

            if let Some(desc) = &type_def.description {
                write!(hover_text, "---\n\n{desc}\n\n").ok();
            }

            Some(HoverResult::new(hover_text))
        }
        Symbol::FragmentSpread { name } => {
            let fragments = graphql_hir::all_fragments(db, project_files);
            let fragment = fragments.get(name.as_str())?;

            let hover_text = format!(
                "**Fragment:** `{}`\n\n**On Type:** `{}`\n\n",
                name, fragment.type_condition
            );

            Some(HoverResult::new(hover_text))
        }
        _ => {
            // For other symbols, show basic info
            Some(HoverResult::new(format!("Symbol: {symbol:?}")))
        }
    }
}
