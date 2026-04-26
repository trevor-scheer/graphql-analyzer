use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

// Logically correct but requires `wasm-pack test --headless --chrome` to run.
// Playwright e2e provides equivalent coverage until that infrastructure is wired
// into CI.
#[wasm_bindgen_test]
#[ignore]
fn initialize_roundtrip() {
    let mut server = graphql_lsp_wasm::Server::new();
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "workspaceFolders": null,
            "initializationOptions": {
                "schema": "schema.graphql",
                "documents": ["**/*.graphql"]
            }
        }
    })
    .to_string();
    let outbound = server.handle_message(&init).unwrap();
    assert_eq!(outbound.length(), 1, "expected one response");
    let resp_str: String = outbound.get(0).as_string().unwrap();
    assert!(resp_str.contains("\"capabilities\""));
}
