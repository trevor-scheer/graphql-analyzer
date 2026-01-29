//! Extensions for `apollo-parser`: visitor pattern, name extraction, and collection utilities.
//!
//! **Note**: This crate is specifically tied to `apollo-parser`'s CST types. For parser-agnostic
//! utilities, see `graphql-utils`.
//!
//! This crate provides:
//! - **Visitor pattern** for traversing GraphQL CST nodes
//! - **Name extraction helpers** for getting names from CST nodes
//! - **Definition iterators** for filtering document definitions
//! - **Collection utilities** for gathering variables, fragments, fields
//!
//! # Example
//!
//! ```
//! use graphql_apollo_ext::{CstVisitor, walk_document};
//! use apollo_parser::cst;
//!
//! struct FragmentCollector {
//!     fragments: Vec<String>,
//! }
//!
//! impl CstVisitor for FragmentCollector {
//!     fn visit_fragment_spread(&mut self, spread: &cst::FragmentSpread) {
//!         if let Some(name) = spread.fragment_name().and_then(|n| n.name()) {
//!             self.fragments.push(name.text().to_string());
//!         }
//!     }
//! }
//!
//! let source = "query { ...UserFields }";
//! let tree = apollo_parser::Parser::new(source).parse();
//! let mut collector = FragmentCollector { fragments: vec![] };
//! walk_document(&mut collector, &tree);
//! assert_eq!(collector.fragments, vec!["UserFields"]);
//! ```

mod collectors;
mod definitions;
mod names;
mod visitor;

pub use collectors::*;
pub use definitions::*;
pub use names::*;
pub use visitor::*;
