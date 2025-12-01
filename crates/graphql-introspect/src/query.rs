//! GraphQL introspection query execution.

use crate::{IntrospectionError, IntrospectionResponse, Result};

/// Standard GraphQL introspection query.
///
/// This query fetches the complete schema information including:
/// - Query, mutation, and subscription root types
/// - All type definitions with their fields and arguments
/// - Directive definitions
/// - Deprecation information
///
/// The query includes nested type references up to 7 levels deep to handle
/// complex type wrappers like `[[[String!]!]!]`.
pub const INTROSPECTION_QUERY: &str = r"
query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types {
      ...FullType
    }
    directives {
      name
      description
      locations
      args {
        ...InputValue
      }
    }
  }
}

fragment FullType on __Type {
  kind
  name
  description
  fields(includeDeprecated: true) {
    name
    description
    args {
      ...InputValue
    }
    type {
      ...TypeRef
    }
    isDeprecated
    deprecationReason
  }
  inputFields {
    ...InputValue
  }
  interfaces {
    ...TypeRef
  }
  enumValues(includeDeprecated: true) {
    name
    description
    isDeprecated
    deprecationReason
  }
  possibleTypes {
    ...TypeRef
  }
}

fragment InputValue on __InputValue {
  name
  description
  type {
    ...TypeRef
  }
  defaultValue
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
              }
            }
          }
        }
      }
    }
  }
}
";

/// Executes an introspection query against a GraphQL endpoint.
///
/// Sends a POST request with the standard introspection query to the specified URL
/// and deserializes the response into an [`IntrospectionResponse`].
///
/// # Arguments
///
/// * `url` - The GraphQL endpoint URL to query
///
/// # Returns
///
/// Returns the introspection response on success.
///
/// # Errors
///
/// Returns an error if:
/// - The network request fails ([`IntrospectionError::Network`])
/// - The server returns an HTTP error status ([`IntrospectionError::Http`])
/// - The response cannot be parsed as JSON ([`IntrospectionError::Parse`])
///
/// # Examples
///
/// ```no_run
/// # use graphql_introspect::execute_introspection;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let response = execute_introspection("https://api.example.com/graphql").await?;
/// println!("Schema has {} types", response.data.schema.types.len());
/// # Ok(())
/// # }
/// ```
pub async fn execute_introspection(url: &str) -> Result<IntrospectionResponse> {
    let client = reqwest::Client::new();

    let query_body = serde_json::json!({
        "query": INTROSPECTION_QUERY
    });

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&query_body)
        .send()
        .await
        .map_err(|e| IntrospectionError::Network(e.to_string()))?;

    if !response.status().is_success() {
        return Err(IntrospectionError::Http(
            response.status().as_u16(),
            response.text().await.unwrap_or_default(),
        ));
    }

    let introspection: IntrospectionResponse = response
        .json()
        .await
        .map_err(|e| IntrospectionError::Parse(e.to_string()))?;

    Ok(introspection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_introspection_query_is_valid() {
        // Basic sanity check that the query string contains expected content
        assert!(INTROSPECTION_QUERY.contains("IntrospectionQuery"));
        assert!(INTROSPECTION_QUERY.contains("__schema"));
        assert!(INTROSPECTION_QUERY.contains("types"));
        assert!(INTROSPECTION_QUERY.contains("directives"));
    }
}
