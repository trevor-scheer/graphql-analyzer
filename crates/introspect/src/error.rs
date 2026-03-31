use thiserror::Error;

pub type Result<T> = std::result::Result<T, IntrospectionError>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IntrospectionError {
    #[error("{}", format_network_error(.0))]
    Network(String),

    #[error("{}", format_http_error(*.0, .1))]
    Http(u16, String),

    #[error("Failed to parse introspection response: {0}")]
    Parse(String),

    #[error("Invalid introspection response: {0}")]
    Invalid(String),
}

/// Produce an actionable message for common network error patterns.
fn format_network_error(msg: &str) -> String {
    let lower = msg.to_lowercase();

    if lower.contains("connection refused") {
        return format!(
            "Connection refused: {msg}\n\n  \
            Hint: Is the GraphQL server running? Check the URL and port."
        );
    }
    if lower.contains("dns error")
        || lower.contains("no such host")
        || lower.contains("resolve")
        || lower.contains("name or service not known")
    {
        return format!(
            "DNS resolution failed: {msg}\n\n  \
            Hint: Check the URL for typos. The hostname could not be resolved."
        );
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return format!(
            "Request timed out: {msg}\n\n  \
            Hint: The server did not respond in time. Try increasing the timeout \
            with `--timeout` or the `timeout` config option."
        );
    }
    if lower.contains("certificate") || lower.contains("ssl") || lower.contains("tls") {
        return format!(
            "TLS/SSL error: {msg}\n\n  \
            Hint: There may be a certificate issue with the server."
        );
    }

    format!("Network error: {msg}")
}

/// Produce an actionable message for HTTP status codes.
fn format_http_error(status: u16, body: &str) -> String {
    let detail = if body.is_empty() {
        String::new()
    } else {
        format!("\n  Response: {body}")
    };

    match status {
        401 => format!(
            "HTTP 401 Unauthorized{detail}\n\n  \
            Hint: The server requires authentication. Add an Authorization header \
            in your config or via `--header 'Authorization: Bearer <token>'`."
        ),
        403 => format!(
            "HTTP 403 Forbidden{detail}\n\n  \
            Hint: The server rejected the request. Check that your credentials \
            have permission to run introspection queries."
        ),
        404 => format!(
            "HTTP 404 Not Found{detail}\n\n  \
            Hint: No GraphQL endpoint found at this URL. Verify the path is correct."
        ),
        405 => format!(
            "HTTP 405 Method Not Allowed{detail}\n\n  \
            Hint: The server does not accept POST requests at this URL. \
            Verify the endpoint path."
        ),
        status if status >= 500 => format!(
            "HTTP {status} Server Error{detail}\n\n  \
            Hint: The server returned an internal error. This may be transient; \
            try again with `--retry 2`."
        ),
        _ => format!("HTTP error {status}{detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_connection_refused_hint() {
        let msg = format_network_error("connection refused");
        assert!(msg.contains("Is the GraphQL server running?"));
    }

    #[test]
    fn network_dns_hint() {
        let msg =
            format_network_error("error trying to connect: dns error: Name or service not known");
        assert!(msg.contains("Check the URL for typos"));
    }

    #[test]
    fn network_timeout_hint() {
        let msg = format_network_error("request timed out");
        assert!(msg.contains("increasing the timeout"));
    }

    #[test]
    fn network_generic_preserved() {
        let msg = format_network_error("some other error");
        assert!(msg.starts_with("Network error:"));
    }

    #[test]
    fn http_401_hint() {
        let msg = format_http_error(401, "");
        assert!(msg.contains("Authorization header"));
    }

    #[test]
    fn http_403_hint() {
        let msg = format_http_error(403, "Forbidden");
        assert!(msg.contains("permission"));
        assert!(msg.contains("Forbidden"));
    }

    #[test]
    fn http_500_hint() {
        let msg = format_http_error(500, "");
        assert!(msg.contains("retry"));
    }

    #[test]
    fn http_404_hint() {
        let msg = format_http_error(404, "");
        assert!(msg.contains("Verify the path"));
    }
}
