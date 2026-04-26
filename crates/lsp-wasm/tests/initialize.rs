use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

// Runtime verification of `initialize` requires the wasm-side handshake bootstrap
// that `tick` doesn't yet handle (native path uses `Connection::initialize` outside
// the dispatch loop). This compiles to catch wasm-side type errors; the actual
// round-trip will be exercised via Playwright e2e once the handshake is wired up.
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
