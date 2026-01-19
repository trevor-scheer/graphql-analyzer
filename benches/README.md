# GraphQL LSP Performance Benchmarks

This crate contains performance benchmarks for the GraphQL LSP, validating the benefits of the Salsa-based incremental computation architecture.

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench parse_cold

# Run with custom output directory
cargo bench -- --save-baseline my-baseline
```

## Benchmark Categories

### Parse Benchmarks

- **parse_cold**: First-time parsing of a GraphQL schema
- **parse_warm**: Repeated parsing of the same schema (tests Salsa caching)

These benchmarks validate that Salsa's memoization works correctly. The warm parse should be orders of magnitude faster than the cold parse.

### Schema Type Extraction Benchmarks

- **schema_types_cold**: First-time extraction of type definitions from schema
- **schema_types_warm**: Repeated extraction (tests caching)

These test the `schema_types` query performance.

### Structure/Body Separation Benchmark

- **structure_body_separation_schema_after_edit**: Measures schema type query performance after editing an operation body

This benchmark validates the **structure/body separation invariant**: editing a document's body doesn't invalidate schema knowledge. The schema types query should be instant (< 100ns) after a body edit because the schema hasn't changed.

```
# Example scenario this benchmark validates:
1. Parse schema and operations (cold)
2. Query schema_types() (caches result)
3. Edit an operation's selection set
4. Query schema_types() again → should be ~0ns (cache hit)
```

### Fragment Resolution Benchmarks

- **fragment_resolution_cold**: First-time resolution of all fragments in project
- **fragment_resolution_warm**: Repeated resolution (tests caching)

These test cross-file fragment resolution performance.

### AnalysisHost Benchmarks

- **analysis_host_add_file**: Time to add a new file to the AnalysisHost
- **analysis_host_diagnostics**: Time to get diagnostics for a file (tests full validation pipeline)

These test the high-level IDE API performance.

## Expected Results

If the cache invariants hold, you should see:

- **Basic Memoization**: 100-1000x speedup for warm vs cold queries
- **Structure/Body Separation**: < 100 nanoseconds for schema query after body edit
- **Index Stability**: Fragment index queries instant after body edits
- **O(1) Updates**: Editing 1 of N files should not scale with N

If you don't see these improvements, a cache invariant is being violated somewhere in the incremental computation setup.

## Interpreting Results

Criterion generates HTML reports in `target/criterion/`. Open `target/criterion/report/index.html` in a browser to see detailed results including:

- Performance distributions
- Regression detection
- Comparison with previous runs

## Adding New Benchmarks

To add a new benchmark:

1. Add a benchmark function following the existing patterns
2. Use `criterion::black_box()` to prevent compiler optimizations
3. Use `iter_batched` with `BatchSize::SmallInput` for benchmarks that need fresh setup
4. Add the benchmark to the `criterion_group!` macro at the bottom

Example:

```rust
fn bench_my_feature(c: &mut Criterion) {
    c.bench_function("my_feature", |b| {
        b.iter_batched(
            || setup(),
            |data| black_box(my_feature(data)),
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, ..., bench_my_feature);
```

## Performance Regression Detection

Criterion can detect performance regressions by comparing against saved baselines:

```bash
# Save current performance as baseline
cargo bench -- --save-baseline main

# After making changes, compare against baseline
cargo bench -- --baseline main
```

If performance regresses significantly, Criterion will warn you.

## CI Integration

These benchmarks can be run in CI to catch performance regressions:

```yaml
- name: Run benchmarks
  run: cargo bench --no-fail-fast
```

Note: Benchmarks are noisy in CI environments. Consider using a dedicated benchmarking machine or service like [Bencher](https://bencher.dev/) for reliable regression detection.
