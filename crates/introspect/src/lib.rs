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

/// Parses a JSON introspection result and converts it to SDL.
///
/// Accepts two common JSON formats:
/// - Full response: `{ "data": { "__schema": { ... } } }`
/// - Data only: `{ "__schema": { ... } }`
///
/// This enables `schema: introspection.json` in graphql-config files,
/// useful for offline/CI workflows where the schema is saved as JSON.
///
/// # Errors
///
/// Returns an error if the JSON cannot be parsed as an introspection result.
pub fn introspection_json_to_sdl(json_str: &str) -> Result<String> {
    let value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| IntrospectionError::Parse(e.to_string()))?;

    // Try full response format: { "data": { "__schema": ... } }
    if value.get("data").and_then(|d| d.get("__schema")).is_some() {
        let response: IntrospectionResponse =
            serde_json::from_value(value).map_err(|e| IntrospectionError::Parse(e.to_string()))?;
        return Ok(introspection_to_sdl(&response));
    }

    // Try data-only format: { "__schema": ... }
    if value.get("__schema").is_some() {
        let data: IntrospectionData =
            serde_json::from_value(value).map_err(|e| IntrospectionError::Parse(e.to_string()))?;
        let response = IntrospectionResponse { data };
        return Ok(introspection_to_sdl(&response));
    }

    Err(IntrospectionError::Invalid(
        "JSON file does not contain a valid introspection result (expected \"data.__schema\" or \"__schema\" key)".to_string(),
    ))
}

/// Checks whether a JSON string looks like a GraphQL introspection result.
///
/// Returns `true` if the JSON contains the `__schema` key at the expected location.
/// This is a quick check that avoids full deserialization.
#[must_use]
pub fn is_introspection_json(json_str: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return false;
    };
    value.get("data").and_then(|d| d.get("__schema")).is_some() || value.get("__schema").is_some()
}

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

#[cfg(test)]
mod json_tests {
    use super::*;

    fn minimal_introspection_json() -> &'static str {
        r#"{
            "data": {
                "__schema": {
                    "queryType": { "name": "Query" },
                    "mutationType": null,
                    "subscriptionType": null,
                    "types": [
                        {
                            "kind": "OBJECT",
                            "name": "Query",
                            "description": null,
                            "fields": [
                                {
                                    "name": "hello",
                                    "description": null,
                                    "args": [],
                                    "type": { "kind": "SCALAR", "name": "String", "ofType": null },
                                    "isDeprecated": false,
                                    "deprecationReason": null
                                }
                            ],
                            "interfaces": []
                        },
                        {
                            "kind": "SCALAR",
                            "name": "String",
                            "description": null
                        }
                    ],
                    "directives": []
                }
            }
        }"#
    }

    #[test]
    fn is_introspection_json_full_response() {
        assert!(is_introspection_json(minimal_introspection_json()));
    }

    #[test]
    fn is_introspection_json_data_only() {
        let json = r#"{ "__schema": { "queryType": { "name": "Query" }, "types": [], "directives": [] } }"#;
        assert!(is_introspection_json(json));
    }

    #[test]
    fn is_introspection_json_not_introspection() {
        assert!(!is_introspection_json(r#"{"schema": "schema.graphql"}"#));
        assert!(!is_introspection_json("not json"));
    }

    #[test]
    fn introspection_json_to_sdl_full_response() {
        let sdl = introspection_json_to_sdl(minimal_introspection_json()).unwrap();
        assert!(sdl.contains("type Query"));
        assert!(sdl.contains("hello"));
    }

    #[test]
    fn introspection_json_to_sdl_data_only() {
        let json = r#"{
            "__schema": {
                "queryType": { "name": "Query" },
                "mutationType": null,
                "subscriptionType": null,
                "types": [
                    {
                        "kind": "OBJECT",
                        "name": "Query",
                        "description": null,
                        "fields": [
                            {
                                "name": "ping",
                                "description": null,
                                "args": [],
                                "type": { "kind": "SCALAR", "name": "String", "ofType": null },
                                "isDeprecated": false,
                                "deprecationReason": null
                            }
                        ],
                        "interfaces": []
                    },
                    { "kind": "SCALAR", "name": "String", "description": null }
                ],
                "directives": []
            }
        }"#;
        let sdl = introspection_json_to_sdl(json).unwrap();
        assert!(sdl.contains("type Query"));
        assert!(sdl.contains("ping"));
    }

    #[test]
    fn introspection_json_to_sdl_invalid() {
        let result = introspection_json_to_sdl(r#"{"schema": "schema.graphql"}"#);
        assert!(result.is_err());
    }
}
