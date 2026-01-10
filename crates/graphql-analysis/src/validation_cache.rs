// Validation caching module
// Provides hash-based caching for apollo-compiler validation to avoid
// re-running validation on every keystroke when the document content
// hasn't meaningfully changed.
//
// Problem: apollo-compiler's doc.validate() is not cached by Salsa,
// so every call to validate_file re-runs full validation even for
// identical content.
//
// Solution: Compute stable hashes of validation inputs (document AST,
// schema, fragment dependencies) and use these as keys for cached
// validation results. Uses a thread-safe hash map for storage since
// apollo_compiler::Schema doesn't implement Hash.

use crate::{Diagnostic, DiagnosticRange, GraphQLAnalysisDatabase, Position, Severity};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

/// A content-based hash that identifies unique validation inputs.
/// Two documents with the same `ValidationKeyHash` will produce identical
/// validation results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(clippy::struct_field_names)]
pub struct ValidationKeyHash {
    /// Hash of the document text (normalized)
    pub document_hash: u64,
    /// Hash of the schema content
    pub schema_hash: u64,
    /// Hash of all fragment sources that this document depends on
    pub fragments_hash: u64,
}

/// Cached validation result
#[derive(Clone, Debug)]
pub struct ValidationResult {
    pub diagnostics: Vec<Diagnostic>,
}

/// Thread-safe validation cache
/// Uses `RwLock` for concurrent read access with exclusive write access
static VALIDATION_CACHE: RwLock<Option<HashMap<ValidationKeyHash, Arc<ValidationResult>>>> =
    RwLock::new(None);

/// Initialize the cache if needed
fn ensure_cache_initialized() {
    let read_guard = VALIDATION_CACHE.read().unwrap();
    if read_guard.is_none() {
        drop(read_guard);
        let mut write_guard = VALIDATION_CACHE.write().unwrap();
        if write_guard.is_none() {
            *write_guard = Some(HashMap::new());
        }
    }
}

/// Look up a cached validation result
fn cache_lookup(key: &ValidationKeyHash) -> Option<Arc<ValidationResult>> {
    ensure_cache_initialized();
    let read_guard = VALIDATION_CACHE.read().unwrap();
    read_guard.as_ref()?.get(key).cloned()
}

/// Insert a validation result into the cache
fn cache_insert(key: ValidationKeyHash, result: Arc<ValidationResult>) {
    ensure_cache_initialized();
    let mut write_guard = VALIDATION_CACHE.write().unwrap();
    if let Some(cache) = write_guard.as_mut() {
        // Limit cache size to prevent unbounded memory growth
        // In a real system, you might use an LRU cache
        const MAX_CACHE_SIZE: usize = 1000;
        if cache.len() >= MAX_CACHE_SIZE {
            // Simple eviction: clear the cache when full
            // A production system would use LRU eviction
            cache.clear();
        }
        cache.insert(key, result);
    }
}

/// Compute a stable hash of GraphQL document text.
/// Normalizes whitespace to avoid re-validation for formatting changes.
fn hash_document_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Normalize the text by removing leading/trailing whitespace per line
    // and filtering empty lines
    let normalized: String = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    normalized.hash(&mut hasher);
    hasher.finish()
}

/// Compute a stable hash of schema content
fn hash_schema(schema: &apollo_compiler::Schema) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Hash the schema by serializing to SDL and hashing the result
    // This is deterministic and captures the semantic content
    let sdl = schema.to_string();
    sdl.hash(&mut hasher);
    hasher.finish()
}

/// Compute a stable hash of fragment sources
fn hash_fragments(fragments: &[(Arc<str>, Arc<str>)]) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Sort fragments by name to ensure deterministic ordering
    let mut sorted: Vec<_> = fragments.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, source) in &sorted {
        name.hash(&mut hasher);
        source.hash(&mut hasher);
    }
    hasher.finish()
}

/// Create a validation key hash from the inputs
pub fn create_validation_key_hash(
    document_text: &str,
    schema: &apollo_compiler::Schema,
    fragments: &[(Arc<str>, Arc<str>)],
) -> ValidationKeyHash {
    ValidationKeyHash {
        document_hash: hash_document_text(document_text),
        schema_hash: hash_schema(schema),
        fragments_hash: hash_fragments(fragments),
    }
}

/// Perform cached validation. This function is the key optimization:
/// validation only re-runs when the inputs actually change.
///
/// Returns cached results when the validation key matches a previous
/// validation with identical inputs.
#[allow(clippy::too_many_arguments)]
pub fn cached_validate(
    _db: &dyn GraphQLAnalysisDatabase,
    document_text: &str,
    schema: &Arc<apollo_compiler::Schema>,
    fragment_sources: &[(Arc<str>, Arc<str>)],
    uri: &str,
    line_offset: usize,
) -> Arc<ValidationResult> {
    // Create the cache key
    let key = create_validation_key_hash(document_text, schema, fragment_sources);

    // Check cache first
    if let Some(cached) = cache_lookup(&key) {
        tracing::debug!(
            document_hash = key.document_hash,
            schema_hash = key.schema_hash,
            fragments_hash = key.fragments_hash,
            "Validation cache hit"
        );
        return cached;
    }

    tracing::debug!(
        document_hash = key.document_hash,
        schema_hash = key.schema_hash,
        fragments_hash = key.fragments_hash,
        "Validation cache miss - running validation"
    );

    // Cache miss - perform validation
    let result = perform_validation(document_text, schema, fragment_sources, uri, line_offset);
    let result = Arc::new(result);

    // Store in cache
    cache_insert(key, result.clone());

    result
}

/// Actually perform the validation (called on cache miss)
fn perform_validation(
    document_text: &str,
    schema: &Arc<apollo_compiler::Schema>,
    fragment_sources: &[(Arc<str>, Arc<str>)],
    uri: &str,
    line_offset: usize,
) -> ValidationResult {
    let valid_schema = apollo_compiler::validation::Valid::assume_valid_ref(schema.as_ref());
    let mut errors = apollo_compiler::validation::DiagnosticList::new(Arc::default());
    let mut builder = apollo_compiler::ExecutableDocument::builder(Some(valid_schema), &mut errors);

    // Parse the document
    apollo_compiler::parser::Parser::new().parse_into_executable_builder(
        document_text,
        uri,
        &mut builder,
    );

    // Add fragment sources
    for (fragment_name, fragment_source) in fragment_sources {
        apollo_compiler::parser::Parser::new().parse_into_executable_builder(
            fragment_source.as_ref(),
            format!("fragment:{fragment_name}"),
            &mut builder,
        );
    }

    let doc = builder.build();
    let mut diagnostics = Vec::new();

    match if errors.is_empty() {
        doc.validate(valid_schema)
            .map(|_| ())
            .map_err(|with_errors| with_errors.errors)
    } else {
        Err(errors)
    } {
        Ok(_valid_document) => {}
        Err(error_list) => {
            for apollo_diag in error_list.iter() {
                use apollo_compiler::diagnostic::ToCliReport;
                if let Some(location) = apollo_diag.error.location() {
                    let file_id = location.file_id();
                    if let Some(source_file) = apollo_diag.sources.get(&file_id) {
                        let diag_file_path = source_file.path();
                        if diag_file_path != uri {
                            continue;
                        }
                    }
                }

                #[allow(clippy::cast_possible_truncation)]
                let range = apollo_diag.line_column_range().map_or_else(
                    DiagnosticRange::default,
                    |loc_range| DiagnosticRange {
                        start: Position {
                            line: (loc_range.start.line.saturating_sub(1) + line_offset) as u32,
                            character: loc_range.start.column.saturating_sub(1) as u32,
                        },
                        end: Position {
                            line: (loc_range.end.line.saturating_sub(1) + line_offset) as u32,
                            character: loc_range.end.column.saturating_sub(1) as u32,
                        },
                    },
                );

                let message: Arc<str> = Arc::from(apollo_diag.error.to_string());
                if message.contains("must be used in an operation") {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message,
                    range,
                    source: "apollo-compiler".into(),
                    code: None,
                });
            }
        }
    }

    ValidationResult { diagnostics }
}

/// Clear the validation cache. Useful for testing or when project
/// configuration changes.
#[allow(dead_code)]
pub fn clear_cache() {
    let mut write_guard = VALIDATION_CACHE.write().unwrap();
    if let Some(cache) = write_guard.as_mut() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_document_text_whitespace_normalization() {
        // Test that leading/trailing whitespace per line is normalized
        let text1 = "query {\nhello\n}";
        let text2 = "query {\n  hello\n}";
        // text2's "  hello" line trims to "hello", matching text1

        let hash1 = hash_document_text(text1);
        let hash2 = hash_document_text(text2);

        // After normalization (trimming each line), these should have the same hash
        assert_eq!(
            hash1, hash2,
            "Leading/trailing whitespace per line should normalize"
        );

        // But intra-line whitespace is preserved (important for strings and semantics)
        let text3 = "query  {\nhello\n}"; // Two spaces between query and {
        let hash3 = hash_document_text(text3);
        assert_ne!(hash1, hash3, "Intra-line whitespace should be preserved");
    }

    #[test]
    fn test_hash_document_text_different_content() {
        let text1 = "query { hello }";
        let text2 = "query { world }";

        let hash1 = hash_document_text(text1);
        let hash2 = hash_document_text(text2);

        assert_ne!(
            hash1, hash2,
            "Different content should produce different hashes"
        );
    }

    #[test]
    fn test_hash_fragments_ordering() {
        let fragments1 = vec![
            (Arc::from("A"), Arc::from("fragment A on User { id }")),
            (Arc::from("B"), Arc::from("fragment B on User { name }")),
        ];
        let fragments2 = vec![
            (Arc::from("B"), Arc::from("fragment B on User { name }")),
            (Arc::from("A"), Arc::from("fragment A on User { id }")),
        ];

        let hash1 = hash_fragments(&fragments1);
        let hash2 = hash_fragments(&fragments2);

        // Order shouldn't matter - hashes should be equal
        assert_eq!(hash1, hash2, "Fragment order should not affect hash");
    }

    #[test]
    fn test_hash_fragments_different_content() {
        let fragments1 = vec![(Arc::from("A"), Arc::from("fragment A on User { id }"))];
        let fragments2 = vec![(Arc::from("A"), Arc::from("fragment A on User { name }"))];

        let hash1 = hash_fragments(&fragments1);
        let hash2 = hash_fragments(&fragments2);

        // Different content should produce different hashes
        assert_ne!(
            hash1, hash2,
            "Different fragment content should produce different hash"
        );
    }

    #[test]
    fn test_cache_operations() {
        clear_cache();

        let key = ValidationKeyHash {
            document_hash: 123,
            schema_hash: 456,
            fragments_hash: 789,
        };

        // Should return None for unknown key
        assert!(cache_lookup(&key).is_none());

        // Insert and lookup
        let result = Arc::new(ValidationResult {
            diagnostics: vec![],
        });
        cache_insert(key, result.clone());

        let cached = cache_lookup(&key);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().diagnostics.len(), 0);

        clear_cache();
    }
}
