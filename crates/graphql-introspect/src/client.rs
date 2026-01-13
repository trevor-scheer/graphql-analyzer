//! Configurable GraphQL introspection client.
//!
//! This module provides a flexible client for executing introspection queries
//! with support for custom headers, timeouts, and retry logic.

use crate::{IntrospectionError, IntrospectionResponse, Result, INTROSPECTION_QUERY};
use std::collections::HashMap;
use std::time::Duration;

/// Default timeout for introspection requests (30 seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default connection timeout (10 seconds).
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Default number of retry attempts.
const DEFAULT_RETRIES: u32 = 0;

/// A configurable client for executing GraphQL introspection queries.
///
/// The client supports:
/// - Custom HTTP headers (e.g., for authentication)
/// - Configurable request timeout
/// - Automatic retry with exponential backoff
///
/// # Examples
///
/// ## Basic usage
///
/// ```no_run
/// use graphql_introspect::IntrospectionClient;
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let client = IntrospectionClient::new();
/// let response = client.execute("https://api.example.com/graphql").await?;
/// # Ok(())
/// # }
/// ```
///
/// ## With authentication
///
/// ```no_run
/// use graphql_introspect::IntrospectionClient;
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let client = IntrospectionClient::new()
///     .with_header("Authorization", "Bearer my-token");
/// let response = client.execute("https://api.example.com/graphql").await?;
/// # Ok(())
/// # }
/// ```
///
/// ## With custom timeout and retries
///
/// ```no_run
/// use graphql_introspect::IntrospectionClient;
/// use std::time::Duration;
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let client = IntrospectionClient::new()
///     .with_timeout(Duration::from_secs(60))
///     .with_retries(3);
/// let response = client.execute("https://api.example.com/graphql").await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct IntrospectionClient {
    headers: HashMap<String, String>,
    timeout: Duration,
    connect_timeout: Duration,
    retries: u32,
}

impl Default for IntrospectionClient {
    fn default() -> Self {
        Self::new()
    }
}

impl IntrospectionClient {
    /// Creates a new introspection client with default settings.
    ///
    /// Default settings:
    /// - 30 second request timeout
    /// - 10 second connection timeout
    /// - No retries
    /// - No custom headers
    #[must_use]
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            connect_timeout: Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS),
            retries: DEFAULT_RETRIES,
        }
    }

    /// Adds a custom HTTP header to be sent with the introspection request.
    ///
    /// Headers are commonly used for authentication:
    ///
    /// ```no_run
    /// # use graphql_introspect::IntrospectionClient;
    /// let client = IntrospectionClient::new()
    ///     .with_header("Authorization", "Bearer token")
    ///     .with_header("X-API-Key", "my-api-key");
    /// ```
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Adds multiple HTTP headers from an iterator.
    ///
    /// ```no_run
    /// # use graphql_introspect::IntrospectionClient;
    /// let headers = vec![
    ///     ("Authorization", "Bearer token"),
    ///     ("X-Request-ID", "12345"),
    /// ];
    /// let client = IntrospectionClient::new().with_headers(headers);
    /// ```
    #[must_use]
    pub fn with_headers<I, K, V>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (name, value) in headers {
            self.headers.insert(name.into(), value.into());
        }
        self
    }

    /// Sets the request timeout.
    ///
    /// This is the maximum time allowed for the entire request (connection + transfer).
    /// Default is 30 seconds.
    ///
    /// ```no_run
    /// # use graphql_introspect::IntrospectionClient;
    /// # use std::time::Duration;
    /// let client = IntrospectionClient::new()
    ///     .with_timeout(Duration::from_secs(60));
    /// ```
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the connection timeout.
    ///
    /// This is the maximum time allowed to establish a connection.
    /// Default is 10 seconds.
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the number of retry attempts on failure.
    ///
    /// Retries use exponential backoff starting at 1 second.
    /// Default is 0 (no retries).
    ///
    /// ```no_run
    /// # use graphql_introspect::IntrospectionClient;
    /// let client = IntrospectionClient::new()
    ///     .with_retries(3); // Will retry up to 3 times
    /// ```
    #[must_use]
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// Executes an introspection query against the specified GraphQL endpoint.
    ///
    /// Returns the parsed introspection response on success.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The network request fails after all retry attempts
    /// - The server returns an HTTP error status
    /// - The response cannot be parsed as valid introspection data
    #[tracing::instrument(skip(self))]
    pub async fn execute(&self, url: &str) -> Result<IntrospectionResponse> {
        let mut last_error = None;
        let attempts = self.retries + 1;

        for attempt in 0..attempts {
            if attempt > 0 {
                let delay = Duration::from_secs(1 << (attempt - 1)); // 1s, 2s, 4s, ...
                tracing::info!(
                    attempt,
                    delay_secs = delay.as_secs(),
                    "Retrying after delay"
                );
                tokio::time::sleep(delay).await;
            }

            match self.execute_once(url).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "Request failed");
                    last_error = Some(e);

                    // Don't retry on non-retryable errors
                    if let Some(ref err) = last_error {
                        if !Self::is_retryable(err) {
                            break;
                        }
                    }
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| IntrospectionError::Network("No attempts made".to_string())))
    }

    /// Executes a single introspection request without retry logic.
    async fn execute_once(&self, url: &str) -> Result<IntrospectionResponse> {
        tracing::debug!("Creating HTTP client with timeouts");
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .connect_timeout(self.connect_timeout)
            .build()
            .map_err(|e| {
                IntrospectionError::Network(format!("Failed to create HTTP client: {e}"))
            })?;

        let query_body = serde_json::json!({
            "query": INTROSPECTION_QUERY
        });

        tracing::info!("Sending introspection query");
        let mut request = client.post(url).header("Content-Type", "application/json");

        // Add custom headers
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }

        let response = request
            .json(&query_body)
            .send()
            .await
            .map_err(|e| IntrospectionError::Network(e.to_string()))?;

        let status = response.status();
        tracing::debug!(status = status.as_u16(), "Received response");

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            tracing::error!(status = status.as_u16(), body = %error_body, "HTTP error response");
            return Err(IntrospectionError::Http(status.as_u16(), error_body));
        }

        tracing::debug!("Parsing introspection response");
        let introspection: IntrospectionResponse = response.json().await.map_err(|e| {
            tracing::error!(error = %e, "Failed to parse introspection response");
            IntrospectionError::Parse(e.to_string())
        })?;

        tracing::info!(
            types = introspection.data.schema.types.len(),
            directives = introspection.data.schema.directives.len(),
            "Introspection successful"
        );

        Ok(introspection)
    }

    /// Determines if an error is retryable.
    ///
    /// Network errors and 5xx server errors are retryable.
    /// Parse errors, 4xx client errors, and invalid responses are not.
    fn is_retryable(error: &IntrospectionError) -> bool {
        match error {
            IntrospectionError::Network(_) => true,
            IntrospectionError::Http(status, _) => *status >= 500,
            IntrospectionError::Parse(_) | IntrospectionError::Invalid(_) => false,
        }
    }

    /// Executes introspection and returns the raw JSON response.
    ///
    /// This is useful when you need the original introspection JSON format
    /// rather than SDL.
    #[tracing::instrument(skip(self))]
    pub async fn execute_raw(&self, url: &str) -> Result<serde_json::Value> {
        let mut last_error = None;
        let attempts = self.retries + 1;

        for attempt in 0..attempts {
            if attempt > 0 {
                let delay = Duration::from_secs(1 << (attempt - 1));
                tracing::info!(
                    attempt,
                    delay_secs = delay.as_secs(),
                    "Retrying after delay"
                );
                tokio::time::sleep(delay).await;
            }

            match self.execute_raw_once(url).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "Request failed");
                    last_error = Some(e);

                    if let Some(ref err) = last_error {
                        if !Self::is_retryable(err) {
                            break;
                        }
                    }
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| IntrospectionError::Network("No attempts made".to_string())))
    }

    /// Executes a single raw introspection request.
    async fn execute_raw_once(&self, url: &str) -> Result<serde_json::Value> {
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .connect_timeout(self.connect_timeout)
            .build()
            .map_err(|e| {
                IntrospectionError::Network(format!("Failed to create HTTP client: {e}"))
            })?;

        let query_body = serde_json::json!({
            "query": INTROSPECTION_QUERY
        });

        let mut request = client.post(url).header("Content-Type", "application/json");

        for (name, value) in &self.headers {
            request = request.header(name, value);
        }

        let response = request
            .json(&query_body)
            .send()
            .await
            .map_err(|e| IntrospectionError::Network(e.to_string()))?;

        let status = response.status();

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(IntrospectionError::Http(status.as_u16(), error_body));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| IntrospectionError::Parse(e.to_string()))?;

        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_default() {
        let client = IntrospectionClient::new();
        assert!(client.headers.is_empty());
        assert_eq!(client.timeout, Duration::from_secs(30));
        assert_eq!(client.retries, 0);
    }

    #[test]
    fn test_client_with_headers() {
        let client = IntrospectionClient::new()
            .with_header("Authorization", "Bearer token")
            .with_header("X-API-Key", "key123");

        assert_eq!(
            client.headers.get("Authorization"),
            Some(&"Bearer token".to_string())
        );
        assert_eq!(client.headers.get("X-API-Key"), Some(&"key123".to_string()));
    }

    #[test]
    fn test_client_with_headers_iterator() {
        let headers = vec![("Authorization", "Bearer token"), ("X-API-Key", "key123")];
        let client = IntrospectionClient::new().with_headers(headers);

        assert_eq!(client.headers.len(), 2);
    }

    #[test]
    fn test_client_with_timeout() {
        let client = IntrospectionClient::new().with_timeout(Duration::from_secs(60));
        assert_eq!(client.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_client_with_retries() {
        let client = IntrospectionClient::new().with_retries(3);
        assert_eq!(client.retries, 3);
    }

    #[test]
    fn test_is_retryable() {
        assert!(IntrospectionClient::is_retryable(
            &IntrospectionError::Network("timeout".into())
        ));
        assert!(IntrospectionClient::is_retryable(
            &IntrospectionError::Http(500, "error".into())
        ));
        assert!(IntrospectionClient::is_retryable(
            &IntrospectionError::Http(503, "error".into())
        ));
        assert!(!IntrospectionClient::is_retryable(
            &IntrospectionError::Http(401, "error".into())
        ));
        assert!(!IntrospectionClient::is_retryable(
            &IntrospectionError::Http(404, "error".into())
        ));
        assert!(!IntrospectionClient::is_retryable(
            &IntrospectionError::Parse("error".into())
        ));
        assert!(!IntrospectionClient::is_retryable(
            &IntrospectionError::Invalid("error".into())
        ));
    }
}
