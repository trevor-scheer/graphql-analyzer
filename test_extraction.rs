use std::fs;

fn main() {
    let source = fs::read_to_string("test-workspace/src/services/pokemon-service.ts")
        .expect("Failed to read file");

    println!("File length: {} bytes", source.len());
    println!("First 100 chars: {:?}", &source[..100.min(source.len())]);

    use graphql_extract::{extract_from_source, ExtractConfig, Language};

    let config = ExtractConfig::default();
    match extract_from_source(&source, Language::TypeScript, &config) {
        Ok(extracted) => {
            println!("\n✓ Extraction succeeded!");
            println!("Found {} GraphQL blocks", extracted.len());
            for (i, block) in extracted.iter().enumerate() {
                println!("\nBlock {}:", i + 1);
                println!("  Line: {}", block.location.range.start.line);
                println!("  Length: {} chars", block.source.len());
                println!("  First 50 chars: {:?}", &block.source[..50.min(block.source.len())]);
            }
        }
        Err(e) => {
            eprintln!("\n✗ Extraction failed: {}", e);
            std::process::exit(1);
        }
    }
}
