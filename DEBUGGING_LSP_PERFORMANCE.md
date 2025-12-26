# Debugging LSP Performance Issues

This document describes how to debug performance and OOM issues when the LSP struggles with large codebases.

## Common Symptoms

- LSP never fully initializes
- Logs show endless database reads
- Process eventually runs out of memory (OOM)
- VSCode shows "Initializing..." forever

## Root Causes Fixed (December 2025)

### 1. O(n²) Performance During File Loading

**Problem**: The `AnalysisHost::add_file()` method was calling `rebuild_project_files()` after EVERY file added. For 10,000 files, this meant:
- File 1: rebuild index with 1 file
- File 2: rebuild index with 2 files
- ...
- File 10,000: rebuild index with 10,000 files
- Total: ~50 million operations

**Fix**: Changed `add_file()` to NOT rebuild automatically. Callers must explicitly call `rebuild_project_files()` once after batch loading files.

**Files Changed**:
- [graphql-ide/src/lib.rs](crates/graphql-ide/src/lib.rs#L325-L354) - Made `add_file()` fast, added `rebuild_project_files()` method
- [graphql-lsp/src/server.rs](crates/graphql-lsp/src/server.rs#L460-L468) - Call rebuild once after loading all files

### 2. No Limits on File Loading

**Problem**: The LSP would try to load ALL files matching glob patterns without any limits, even if there were 100,000+ files.

**Fix**: Added configurable limits:
- **Warning threshold**: 1,000 files - shows warning to user
- **Hard limit**: 10,000 files - stops loading and shows error
- **Progress logging**: Every 100 files during load

**Files Changed**:
- [graphql-lsp/src/server.rs](crates/graphql-lsp/src/server.rs#L295-L339) - Added limits and progress tracking

### 3. Insufficient Logging

**Problem**: Hard to diagnose what was happening during initialization.

**Fix**: Added comprehensive logging:
- File loading progress (every 100 files)
- Timing information for all phases
- Memory usage tracking (on Linux)
- Project and pattern information

## Debugging Tools

### 1. Enable Debug Logging

Set the `RUST_LOG` environment variable:

```bash
# In VSCode settings.json
"graphql-lsp.trace.server": "verbose",
"graphql-lsp.env": {
  "RUST_LOG": "debug"
}
```

Or run the LSP manually:
```bash
RUST_LOG=debug target/debug/graphql-lsp
```

### 2. Check Logs in VSCode

1. Open Output panel: View → Output
2. Select "GraphQL Language Server" from dropdown
3. Look for:
   - "Loading files for X project(s)"
   - "Loaded X files so far"
   - "Finished loading all project files in X.XXs"
   - Any warnings or errors

### 3. Monitor Memory Usage

On Linux, the LSP logs memory usage after loading files:
```
[INFO] Memory: VmRSS: 245632 kB
[INFO] Memory: VmSize: 2145728 kB
```

On macOS, use Activity Monitor or:
```bash
ps aux | grep graphql-lsp
```

### 4. Check Your Configuration

Overly broad glob patterns can match thousands of files:

**Bad** (matches everything):
```yaml
documents: "**/*"
```

**Good** (specific patterns):
```yaml
documents:
  - "src/**/*.graphql"
  - "src/**/*.gql"
  - "!src/**/*.test.graphql"
```

**Very specific** (for large codebases):
```yaml
documents:
  - "src/graphql/queries/**/*.graphql"
  - "src/graphql/mutations/**/*.graphql"
```

## Performance Best Practices

### 1. Use Specific Glob Patterns

Instead of broad patterns like `**/*.graphql`, use specific directories:
```yaml
documents:
  - "src/queries/**/*.graphql"
  - "src/mutations/**/*.graphql"
  - "src/fragments/**/*.graphql"
```

### 2. Exclude Generated Files

Always exclude:
- `node_modules/` (automatically excluded)
- Generated schema files
- Build output directories
- Test fixtures with large schemas

```yaml
documents:
  - "src/**/*.graphql"
  - "!src/**/*.generated.graphql"
  - "!src/fixtures/**"
```

### 3. Split Large Projects

If you have multiple GraphQL APIs in one codebase, use multi-project config:

```yaml
projects:
  api-v1:
    schema: "api-v1/schema.graphql"
    documents: "api-v1/**/*.graphql"

  api-v2:
    schema: "api-v2/schema.graphql"
    documents: "api-v2/**/*.graphql"
```

### 4. Monitor File Count

Check how many files match your patterns:
```bash
# Count GraphQL files
find . -name "*.graphql" | wc -l

# Count with specific pattern
find ./src/queries -name "*.graphql" | wc -l
```

If you see 1,000+ files, consider more specific patterns.

## Expected Performance

After the O(n²) fix, expected initialization times:

- **100 files**: < 1 second
- **1,000 files**: 1-3 seconds
- **10,000 files**: 10-30 seconds (hits warning/limit)

Memory usage should be roughly:
- Base: ~50MB
- Per file: ~10-50KB (depends on file size)
- 1,000 files: ~100-150MB
- 10,000 files: ~500MB-1GB

If you're seeing worse performance than this, check:
1. Glob patterns are specific
2. No duplicate files being loaded
3. No project-wide diagnostics running on every file

## Troubleshooting Checklist

- [ ] Check VSCode Output logs for errors
- [ ] Verify `.graphqlrc.yaml` has specific glob patterns
- [ ] Count actual files matching patterns (< 10,000)
- [ ] Check memory usage isn't growing infinitely
- [ ] Ensure `RUST_LOG=debug` is set for detailed logs
- [ ] Look for O(n²) patterns in logs (same operation repeating)
- [ ] Check if initialization eventually completes (even if slow)

## Related Files

- [graphql-ide/src/lib.rs](crates/graphql-ide/src/lib.rs) - `AnalysisHost` file management
- [graphql-ide/src/file_registry.rs](crates/graphql-ide/src/file_registry.rs) - File registry implementation
- [graphql-lsp/src/server.rs](crates/graphql-lsp/src/server.rs) - LSP server initialization
- [.claude/CLAUDE.md](.claude/CLAUDE.md) - Architecture overview

## Future Improvements

Potential optimizations for handling even larger codebases:

1. **Lazy loading**: Only load files when they're opened/referenced
2. **Incremental indexing**: Load files in batches with progress reporting
3. **Background indexing**: Load files after LSP reports "initialized"
4. **Persistent cache**: Cache parsed files to disk
5. **Memory-mapped files**: Reduce memory usage for very large files
6. **Streaming glob**: Process files as they're discovered, not all at once
