mod config;
mod env;
mod error;
mod loader;
pub mod suggestions;
mod validation;

pub use config::{
    ClientConfig, DocumentsConfig, GraphQLConfig, IntrospectionSchemaConfig, ProjectConfig,
    SchemaConfig,
};
pub use env::{interpolate_env_vars, EnvInterpolationError};
pub use error::{ConfigError, Result};
pub use loader::{find_config, load_config, load_config_from_str, CONFIG_FILES};
pub use validation::{
    extension_namespace_warnings, validate, ConfigValidationError, FileType, LintValidationContext,
    Location, Severity,
};
