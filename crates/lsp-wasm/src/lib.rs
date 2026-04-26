use lsp_server::{Connection, Message};
use std::path::Path;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();
}

#[wasm_bindgen]
pub struct Server {
    connection: Connection,
    state: graphql_lsp::GlobalState,
    inbound: crossbeam_channel::Sender<Message>,
    outbound: crossbeam_channel::Receiver<Message>,
}

#[wasm_bindgen]
impl Server {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let (inbound_tx, inbound_rx) = crossbeam_channel::unbounded::<Message>();
        let (outbound_tx, outbound_rx) = crossbeam_channel::unbounded::<Message>();
        let connection = Connection { sender: outbound_tx, receiver: inbound_rx };
        let dispatcher: Box<dyn graphql_lsp::TaskDispatcher> =
            Box::new(graphql_lsp::InlineDispatcher);
        let (intro_req_tx, _) = crossbeam_channel::unbounded();
        let (_, intro_res_rx) = crossbeam_channel::unbounded();
        let state = graphql_lsp::GlobalState::new(
            connection.sender.clone(),
            dispatcher,
            intro_req_tx,
            intro_res_rx,
        );
        Server { connection, state, inbound: inbound_tx, outbound: outbound_rx }
    }

    /// Feed an inbound LSP JSON-RPC message (already unframed). Returns an array
    /// of outbound JSON strings the worker should write back to the client.
    #[wasm_bindgen(js_name = handleMessage)]
    pub fn handle_message(&mut self, json: &str) -> Result<js_sys::Array, JsValue> {
        let msg: Message = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        // The `initialize` request is handled outside the normal dispatch loop
        // (native does it via Connection::initialize before the loop).
        // For wasm, intercept it here and respond directly.
        if let Message::Request(ref req) = msg {
            if req.method == "initialize" {
                return self.handle_initialize(req.id.clone(), req.params.clone());
            }
        }

        // The `initialized` notification triggers workspace bootstrap.
        if let Message::Notification(ref not) = msg {
            if not.method == "initialized" {
                self.handle_initialized();
                // Still push through tick so any state side-effects run.
            }
        }

        self.inbound.send(msg).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let _ = graphql_lsp::tick(&self.connection, &mut self.state);
        self.collect_outbound()
    }

    fn collect_outbound(&mut self) -> Result<js_sys::Array, JsValue> {
        let arr = js_sys::Array::new();
        while let Ok(out) = self.outbound.try_recv() {
            let s = serde_json::to_string(&out)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
            arr.push(&JsValue::from_str(&s));
        }
        Ok(arr)
    }

    fn handle_initialize(
        &mut self,
        id: lsp_server::RequestId,
        params: serde_json::Value,
    ) -> Result<js_sys::Array, JsValue> {
        // Parse and store client capabilities + initializationOptions.
        if let Ok(init_params) = serde_json::from_value::<lsp_types::InitializeParams>(params) {
            self.state.client_capabilities = Some(init_params.capabilities);

            if let Some(init_options) = init_params.initialization_options {
                // Workspace root is virtual under wasm; use "inmemory://" as the root.
                let workspace_uri = "inmemory://";
                let workspace_path = Path::new("/");
                if let Err(e) = graphql_lsp::install_workspace_from_init_options(
                    &mut self.state,
                    workspace_uri,
                    workspace_path,
                    init_options,
                ) {
                    tracing::warn!("failed to install workspace from init options: {e}");
                }
            }
        }

        let caps = graphql_lsp::build_server_capabilities();
        let result = lsp_types::InitializeResult {
            capabilities: caps,
            server_info: Some(lsp_types::ServerInfo {
                name: "graphql-analyzer".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        };
        let value = serde_json::to_value(&result)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let response = lsp_server::Response::new_ok(id, value);
        let out = serde_json::to_string(&Message::Response(response))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let arr = js_sys::Array::new();
        arr.push(&JsValue::from_str(&out));
        Ok(arr)
    }

    fn handle_initialized(&mut self) {
        // After the handshake, load all files covered by the workspace config
        // so diagnostics fire on didOpen.
        graphql_lsp::load_wasm_workspace(&mut self.state);
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
