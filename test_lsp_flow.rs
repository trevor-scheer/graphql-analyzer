use graphql_ide::{AnalysisHost, FilePath};
use graphql_db::FileKind;
use std::fs;

fn main() {
    let source = fs::read_to_string("test-workspace/src/services/pokemon-service.ts")
        .expect("Failed to read file");

    println!("Original TypeScript source: {} bytes", source.len());

    // Extract GraphQL (simulating what LSP does)
    use graphql_extract::{extract_from_source, ExtractConfig, Language};
    let config = ExtractConfig::default();
    let (extracted, line_offset) = match extract_from_source(&source, Language::TypeScript, &config) {
        Ok(blocks) if !blocks.is_empty() => {
            let combined: Vec<String> = blocks.iter().map(|b| b.source.clone()).collect();
            let offset = blocks[0].location.range.start.line.saturating_sub(1) as u32;
            (combined.join("\n\n"), offset)
        }
        _ => (String::new(), 0),
    };

    println!("Extracted GraphQL: {} bytes, line_offset: {}", extracted.len(), line_offset);
    println!("First 100 chars of extracted: {:?}\n", &extracted[..100.min(extracted.len())]);

    // Add to AnalysisHost (simulating what LSP does)
    let mut host = AnalysisHost::new();
    let path = FilePath::new("file:///test/pokemon-service.ts".to_string());

    println!("Adding file to host with extracted GraphQL...");
    host.add_file(&path, &extracted, FileKind::TypeScript, line_offset);

    // Get diagnostics
    println!("Getting diagnostics...");
    let diagnostics = host.file_diagnostics(&path);

    println!("\nDiagnostics: {} total", diagnostics.len());
    for (i, diag) in diagnostics.iter().enumerate() {
        println!("  {}: {:?} - {}", i+1, diag.severity, diag.message);
    }

    if diagnostics.iter().any(|d| d.message.contains("import") || d.message.contains("Unexpected")) {
        eprintln!("\n❌ FAIL: Found TypeScript syntax errors!");
        std::process::exit(1);
    } else {
        println!("\n✓ SUCCESS: No TypeScript syntax errors");
    }
}
