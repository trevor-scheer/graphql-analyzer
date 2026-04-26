use lsp_server::{Connection, Message};
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
        self.inbound.send(msg).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let _ = graphql_lsp::tick(&self.connection, &mut self.state);
        let arr = js_sys::Array::new();
        while let Ok(out) = self.outbound.try_recv() {
            let s = serde_json::to_string(&out)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
            arr.push(&JsValue::from_str(&s));
        }
        Ok(arr)
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
