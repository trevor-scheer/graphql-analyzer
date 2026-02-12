//! Foundation types for GraphQL LSP.
//!
//! This crate provides shared types used across the GraphQL LSP stack.
//! It has zero external dependencies, making it suitable as a foundation layer.
//!
//! # Type Categories
//!
//! - **File types**: [`FileId`], [`FileUri`], [`Language`], [`DocumentKind`]
//! - **Position types**: [`Position`], [`Range`], [`OffsetRange`]
//! - **Severity types**: [`DiagnosticSeverity`], [`RuleSeverity`]
//! - **Edit types**: [`TextEdit`], [`CodeFix`]

mod edits;
mod file;
mod position;
mod severity;

pub use edits::{CodeFix, TextEdit};
pub use file::{DocumentKind, FileId, FileUri, Language};
pub use position::{OffsetRange, Position, Range, SourceSpan};
pub use severity::{DiagnosticSeverity, RuleSeverity};
