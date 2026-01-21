//! Remote schema caching for the CLI.
//!
//! Caches introspected schemas to avoid re-fetching on every CLI invocation.
//! Cache entries are stored in `~/.cache/graphql-lsp/schemas/` with a 1-hour default TTL.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Default cache TTL (1 hour)
const DEFAULT_TTL_SECS: u64 = 3600;

/// Cache directory name
const CACHE_DIR: &str = "graphql-lsp";

/// Schemas subdirectory
const SCHEMAS_DIR: &str = "schemas";

/// Metadata about a cached schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// The original endpoint URL
    pub url: String,
    /// Timestamp when the schema was fetched
    pub fetched_at: u64,
    /// TTL in seconds
    pub ttl_secs: u64,
}

impl CacheMetadata {
    /// Check if the cache entry has expired
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now > self.fetched_at + self.ttl_secs
    }

    /// Create new metadata for a freshly fetched schema
    pub fn new(url: &str) -> Self {
        let fetched_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            url: url.to_string(),
            fetched_at,
            ttl_secs: DEFAULT_TTL_SECS,
        }
    }

    /// Get the age of the cache entry
    pub fn age(&self) -> Duration {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Duration::from_secs(now.saturating_sub(self.fetched_at))
    }
}

/// Schema cache for remote introspection results
pub struct SchemaCache {
    cache_dir: PathBuf,
}

impl SchemaCache {
    /// Create a new schema cache using the system cache directory
    pub fn new() -> Result<Self> {
        let cache_dir = Self::get_cache_dir()?;
        std::fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache directory: {}", cache_dir.display()))?;
        Ok(Self { cache_dir })
    }

    /// Get the cache directory path
    fn get_cache_dir() -> Result<PathBuf> {
        // Try XDG_CACHE_HOME first, then fallback to ~/.cache
        let base = std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|_| {
                dirs::home_dir()
                    .map(|h| h.join(".cache"))
                    .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))
            })?;
        Ok(base.join(CACHE_DIR).join(SCHEMAS_DIR))
    }

    /// Generate a cache key (hash) for a URL
    fn cache_key(url: &str) -> String {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Get the path to the schema file for a URL
    fn schema_path(&self, url: &str) -> PathBuf {
        let key = Self::cache_key(url);
        self.cache_dir.join(format!("{key}.graphql"))
    }

    /// Get the path to the metadata file for a URL
    fn metadata_path(&self, url: &str) -> PathBuf {
        let key = Self::cache_key(url);
        self.cache_dir.join(format!("{key}.meta.json"))
    }

    /// Get a cached schema if it exists and is not expired
    ///
    /// Returns `Some((sdl, metadata))` if the cache is valid, `None` otherwise.
    pub fn get(&self, url: &str) -> Option<(String, CacheMetadata)> {
        let schema_path = self.schema_path(url);
        let metadata_path = self.metadata_path(url);

        // Read metadata first to check expiration
        let metadata_content = std::fs::read_to_string(&metadata_path).ok()?;
        let metadata: CacheMetadata = serde_json::from_str(&metadata_content).ok()?;

        // Check if expired
        if metadata.is_expired() {
            tracing::debug!(url, "Cache entry expired");
            return None;
        }

        // Read the schema
        let sdl = std::fs::read_to_string(&schema_path).ok()?;
        tracing::debug!(url, age_secs = metadata.age().as_secs(), "Using cached schema");

        Some((sdl, metadata))
    }

    /// Store a schema in the cache
    pub fn set(&self, url: &str, sdl: &str) -> Result<()> {
        let schema_path = self.schema_path(url);
        let metadata_path = self.metadata_path(url);

        // Write schema
        std::fs::write(&schema_path, sdl)
            .with_context(|| format!("Failed to write schema cache: {}", schema_path.display()))?;

        // Write metadata
        let metadata = CacheMetadata::new(url);
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&metadata_path, metadata_json)
            .with_context(|| format!("Failed to write cache metadata: {}", metadata_path.display()))?;

        tracing::debug!(url, path = %schema_path.display(), "Cached schema");
        Ok(())
    }

    /// Clear the cache for a specific URL
    #[allow(dead_code)]
    pub fn clear(&self, url: &str) -> Result<()> {
        let schema_path = self.schema_path(url);
        let metadata_path = self.metadata_path(url);

        if schema_path.exists() {
            std::fs::remove_file(&schema_path)?;
        }
        if metadata_path.exists() {
            std::fs::remove_file(&metadata_path)?;
        }

        Ok(())
    }

    /// Clear all cached schemas
    #[allow(dead_code)]
    pub fn clear_all(&self) -> Result<()> {
        if self.cache_dir.exists() {
            std::fs::remove_dir_all(&self.cache_dir)?;
            std::fs::create_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }
}

impl Default for SchemaCache {
    fn default() -> Self {
        Self::new().expect("Failed to initialize schema cache")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_cache() -> SchemaCache {
        let temp = tempdir().unwrap();
        SchemaCache {
            cache_dir: temp.keep(),
        }
    }

    #[test]
    fn test_cache_key_consistency() {
        let url = "https://api.example.com/graphql";
        let key1 = SchemaCache::cache_key(url);
        let key2 = SchemaCache::cache_key(url);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_uniqueness() {
        let key1 = SchemaCache::cache_key("https://api1.example.com/graphql");
        let key2 = SchemaCache::cache_key("https://api2.example.com/graphql");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_set_and_get() {
        let cache = create_test_cache();
        let url = "https://api.example.com/graphql";
        let sdl = "type Query { hello: String }";

        cache.set(url, sdl).unwrap();
        let (cached_sdl, metadata) = cache.get(url).unwrap();

        assert_eq!(cached_sdl, sdl);
        assert_eq!(metadata.url, url);
        assert!(!metadata.is_expired());
    }

    #[test]
    fn test_cache_miss() {
        let cache = create_test_cache();
        assert!(cache.get("https://nonexistent.example.com/graphql").is_none());
    }

    #[test]
    fn test_metadata_expiration() {
        let mut metadata = CacheMetadata::new("https://example.com/graphql");
        assert!(!metadata.is_expired());

        // Set fetched_at to 2 hours ago
        metadata.fetched_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 7200;
        assert!(metadata.is_expired());
    }
}
