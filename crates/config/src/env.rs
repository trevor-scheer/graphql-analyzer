use std::env;

/// Interpolate environment variables in a configuration string.
///
/// Supports two syntaxes:
/// - `${VAR}` - replaced with the value of `VAR`, error if unset
/// - `${VAR:default}` - replaced with the value of `VAR`, or `default` if unset
///
/// This matches the graphql-config standard behavior where environment
/// variables can be used in endpoint URLs and auth headers.
pub fn interpolate_env_vars(input: &str) -> Result<String, EnvInterpolationError> {
    interpolate_env_vars_with(input, |name| env::var(name).ok())
}

/// Interpolation error for environment variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvInterpolationError {
    /// The variable name that was not found.
    pub variable: String,
}

impl std::fmt::Display for EnvInterpolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "environment variable '{}' is not set and no default was provided",
            self.variable
        )
    }
}

impl std::error::Error for EnvInterpolationError {}

/// Interpolate environment variables using a custom lookup function.
/// Useful for testing without modifying actual env vars.
fn interpolate_env_vars_with(
    input: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<String, EnvInterpolationError> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'

            // Read until '}' or ':' (for default value)
            let mut var_name = String::new();
            let mut default_value = None;
            let mut found_close = false;

            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    found_close = true;
                    break;
                }
                if c == ':' {
                    chars.next();
                    // Rest until '}' is the default value
                    let mut default = String::new();
                    while let Some(&d) = chars.peek() {
                        if d == '}' {
                            chars.next();
                            found_close = true;
                            break;
                        }
                        default.push(d);
                        chars.next();
                    }
                    default_value = Some(default);
                    break;
                }
                var_name.push(c);
                chars.next();
            }

            if !found_close {
                // Malformed: no closing brace, output literally
                result.push_str("${");
                result.push_str(&var_name);
                if let Some(ref default) = default_value {
                    result.push(':');
                    result.push_str(default);
                }
                continue;
            }

            match lookup(&var_name) {
                Some(value) => result.push_str(&value),
                None => match default_value {
                    Some(default) => result.push_str(&default),
                    None => {
                        return Err(EnvInterpolationError { variable: var_name });
                    }
                },
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    #[test]
    fn no_interpolation() {
        let input = "schema: schema.graphql";
        let result = interpolate_env_vars_with(input, |_| None).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn simple_var() {
        let lookup = make_lookup(&[("API_URL", "https://api.example.com/graphql")]);
        let result = interpolate_env_vars_with("schema:\n  url: ${API_URL}", lookup).unwrap();
        assert_eq!(result, "schema:\n  url: https://api.example.com/graphql");
    }

    #[test]
    fn var_with_default_uses_value() {
        let lookup = make_lookup(&[("API_URL", "https://prod.example.com")]);
        let result =
            interpolate_env_vars_with("url: ${API_URL:https://localhost:4000}", lookup).unwrap();
        assert_eq!(result, "url: https://prod.example.com");
    }

    #[test]
    fn var_with_default_uses_default() {
        let result =
            interpolate_env_vars_with("url: ${API_URL:https://localhost:4000}", |_| None).unwrap();
        assert_eq!(result, "url: https://localhost:4000");
    }

    #[test]
    fn missing_var_no_default_errors() {
        let result = interpolate_env_vars_with("url: ${API_URL}", |_| None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.variable, "API_URL");
    }

    #[test]
    fn multiple_vars() {
        let lookup = make_lookup(&[("HOST", "api.example.com"), ("TOKEN", "secret123")]);
        let input = "url: https://${HOST}/graphql\nheaders:\n  Authorization: Bearer ${TOKEN}";
        let result = interpolate_env_vars_with(input, lookup).unwrap();
        assert_eq!(
            result,
            "url: https://api.example.com/graphql\nheaders:\n  Authorization: Bearer secret123"
        );
    }

    #[test]
    fn empty_default() {
        let result = interpolate_env_vars_with("value: ${UNSET:}", |_| None).unwrap();
        assert_eq!(result, "value: ");
    }

    #[test]
    fn dollar_without_brace_preserved() {
        let result = interpolate_env_vars_with("price: $5", |_| None).unwrap();
        assert_eq!(result, "price: $5");
    }

    #[test]
    fn malformed_no_close_brace() {
        let result = interpolate_env_vars_with("value: ${UNCLOSED", |_| None).unwrap();
        assert_eq!(result, "value: ${UNCLOSED");
    }

    #[test]
    fn real_world_config() {
        let lookup = make_lookup(&[
            ("GRAPHQL_ENDPOINT", "https://api.prod.example.com/graphql"),
            ("AUTH_TOKEN", "eyJhbGciOi..."),
        ]);
        let input = r#"schema:
  url: ${GRAPHQL_ENDPOINT}
  headers:
    Authorization: "Bearer ${AUTH_TOKEN}"
  timeout: 30
documents: "src/**/*.graphql"
"#;
        let result = interpolate_env_vars_with(input, lookup).unwrap();
        assert!(result.contains("https://api.prod.example.com/graphql"));
        assert!(result.contains("eyJhbGciOi..."));
    }
}
