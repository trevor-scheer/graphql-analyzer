//! GraphQL introspection query execution and SDL conversion.
//!
//! This crate provides functionality to fetch GraphQL schemas from remote endpoints
//! via introspection and convert them to Schema Definition Language (SDL).
//!
//! # Examples
//!
//! ## One-step introspection to SDL
//!
//! ```no_run
//! use graphql_introspect::introspect_url_to_sdl;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let sdl = introspect_url_to_sdl("https://api.example.com/graphql").await?;
//!     println!("{}", sdl);
//!     Ok(())
//! }
//! ```
//!
//! ## Step-by-step usage
//!
//! ```no_run
//! use graphql_introspect::{execute_introspection, introspection_to_sdl};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Execute introspection query
//!     let introspection = execute_introspection("https://api.example.com/graphql").await?;
//!
//!     // Convert to SDL
//!     let sdl = introspection_to_sdl(&introspection);
//!
//!     println!("{}", sdl);
//!     Ok(())
//! }
//! ```
//!
//! ## With custom headers and retry
//!
//! ```no_run
//! use graphql_introspect::{IntrospectionClient, introspection_to_sdl};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = IntrospectionClient::new()
//!         .with_header("Authorization", "Bearer my-token")
//!         .with_timeout(Duration::from_secs(60))
//!         .with_retries(3);
//!
//!     let response = client.execute("https://api.example.com/graphql").await?;
//!     let sdl = introspection_to_sdl(&response);
//!     println!("{}", sdl);
//!     Ok(())
//! }
//! ```

mod client;
mod error;
mod query;
mod sdl;
mod types;

pub use client::IntrospectionClient;
pub use error::{IntrospectionError, Result};
pub use query::{execute_introspection, INTROSPECTION_QUERY};
pub use sdl::introspection_to_sdl;
pub use types::*;

/// Introspects a GraphQL endpoint and converts the result to SDL.
///
/// This is a convenience function that combines [`execute_introspection`] and
/// [`introspection_to_sdl`] into a single call.
///
/// # Arguments
///
/// * `url` - The GraphQL endpoint URL to introspect
///
/// # Returns
///
/// Returns the schema as an SDL string on success.
///
/// # Errors
///
/// Returns an error if:
/// - The network request fails
/// - The server returns an HTTP error
/// - The response cannot be parsed
/// - The response is invalid
///
/// # Examples
///
/// ```no_run
/// # use graphql_introspect::introspect_url_to_sdl;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let sdl = introspect_url_to_sdl("https://api.example.com/graphql").await?;
/// println!("{}", sdl);
/// # Ok(())
/// # }
/// ```
#[tracing::instrument]
pub async fn introspect_url_to_sdl(url: &str) -> Result<String> {
    tracing::info!("Starting introspection");
    let introspection = execute_introspection(url).await?;
    tracing::debug!("Converting introspection to SDL");
    let sdl = introspection_to_sdl(&introspection);
    tracing::info!(sdl_length = sdl.len(), "Introspection complete");
    Ok(sdl)
}
