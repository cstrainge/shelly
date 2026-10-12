
use std::{ collections::HashMap, io::{ self, BufReader, BufWriter, stdin, stdout },
           process::ExitCode };

use serde_json::{ Value, json };

use crate::{ language::{ analysis::SymbolSnapshot, interpreter::Interpreter },
             server::{ document::{ Document, TOKEN_TYPES },
                       transport::{ read_message, write_message } } };

mod document;
mod transport;


struct Server
{
    initialized: bool,
    shutdown: bool,
    snapshot: SymbolSnapshot,
    documents: HashMap<String, Document>,
}


fn response(id: &Value, result: Value) -> Value
{ json!({ "jsonrpc": "2.0", "id": id, "result": result }) }

fn error(id: &Value, code: i32, message: &str) -> Value
{ json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }) }

fn diagnostic_message(uri: &str, document: &Document) -> Value
{
    json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {
        "uri": uri, "version": document.version, "diagnostics": document.diagnostics } })
}


impl Server
{
    fn handle(&mut self, message: &Value) -> Vec<Value>
    {
        let id = message.get("id");
        let method = message.get("method").and_then(Value::as_str);
        if    message.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
           || method.is_none()
           || id.is_some_and(|id| !id.is_string() && !id.is_number() && !id.is_null())
        { return vec![error(&Value::Null, -32600, "Invalid JSON-RPC request")]; }
        let method = method.unwrap();
        let params = &message["params"];
        if method == "initialize"
        {
            let Some(id) = id else { return Vec::new(); };
            if self.initialized || self.shutdown
            { return vec![error(id, -32600, "Server is already initialized")]; }
            self.initialized = true;
            return vec![response(id, json!({ "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": { "openClose": true, "change": 1 },
                "semanticTokensProvider": {
                    "legend": { "tokenTypes": TOKEN_TYPES, "tokenModifiers": [] },
                    "full": true, "range": false } },
                "serverInfo": { "name": "shelly-shell", "version": env!("CARGO_PKG_VERSION") }
            }))];
        }
        if !self.initialized
        {
            return id.map_or_else(Vec::new, |id|
                vec![error(id, -32002, "Server has not been initialized")]);
        }
        if self.shutdown
        {
            return id.map_or_else(Vec::new, |id|
                vec![error(id, -32600, "Server has shut down")]);
        }
        match method
        {
            "initialized" | "$/cancelRequest" | "$/setTrace"
            | "workspace/didChangeConfiguration" => Vec::new(),
            "shutdown" =>
                {
                    if let Some(id) = id
                    {
                        self.shutdown = true;
                        self.documents.clear();
                        vec![response(id, Value::Null)]
                    }
                    else { Vec::new() }
                },
            "textDocument/didOpen" | "textDocument/didChange" =>
                {
                    if let Some(id) = id
                    { return vec![error(id, -32600, "Document updates are notifications")]; }
                    self.update(params, method == "textDocument/didOpen")
                },
            "textDocument/didClose" =>
                {
                    let Some(uri) = params["textDocument"]["uri"].as_str()
                        else { return Vec::new(); };
                    self.documents.remove(uri);
                    vec![json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                                 "params": { "uri": uri, "diagnostics": [] } })]
                },
            "textDocument/semanticTokens/full" =>
                {
                    let Some(id) = id else { return Vec::new(); };
                    let Some(uri) = params["textDocument"]["uri"].as_str()
                        else { return vec![error(id, -32602, "Missing document URI")]; };
                    match self.documents.get(uri)
                    {
                        Some(document) => vec![response(id, json!({
                            "resultId": document.version.to_string(), "data": document.tokens }))],
                        None => vec![error(id, -32602, "Document is not open")],
                    }
                },
            _ => id.map_or_else(Vec::new, |id| vec![error(id, -32601, "Method not found")]),
        }
    }

    fn update(&mut self, params: &Value, opening: bool) -> Vec<Value>
    {
        let (Some(uri), Some(version)) = (params["textDocument"]["uri"].as_str(),
                                         params["textDocument"]["version"].as_i64())
            else { return Vec::new(); };
        if !opening && !self.documents.contains_key(uri) { return Vec::new(); }
        if self.documents.get(uri).is_some_and(|document| version <= document.version)
        { return Vec::new(); }
        let source = if opening { params["textDocument"]["text"].as_str() }
            else
            {
                // Full synchronization applies all replacement events in order. A ranged
                // event is a client error; do not analyze it as a complete document.
                params["contentChanges"].as_array().filter(|changes|
                    !changes.is_empty() && changes.iter().all(|change|
                        change.get("range").is_none() && change["text"].is_string()))
                    .and_then(|changes| changes.last()).and_then(|change| change["text"].as_str())
            };
        let Some(source) = source else { return Vec::new(); };
        let document = Document::analyze(source, version, &self.snapshot);
        let publication = diagnostic_message(uri, &document);
        self.documents.insert(uri.into(), document);
        vec![publication]
    }
}


/**
 * A server owns source facts only. No shell startup, module execution, PATH discovery,
 * Reedline, terminal query, or user-defined conversion runs in this mode.
 */
pub fn run() -> io::Result<ExitCode>
{
    let mut server = Server
        {
            initialized: false, shutdown: false, snapshot: Interpreter::server_snapshot(),
            documents: HashMap::new(),
        };
    let mut input = BufReader::new(stdin().lock());
    let mut output = BufWriter::new(stdout().lock());
    while let Some(body) = read_message(&mut input)?
    {
        let message = match serde_json::from_slice::<Value>(&body)
            {
                Ok(message) => message,
                Err(_) =>
                    {
                        write_message(&mut output, &error(&Value::Null, -32700, "Invalid JSON"))?;
                        continue;
                    },
            };
        if    message["jsonrpc"] == "2.0"
           && message["method"] == "exit"
           && message.get("id").is_none()
        { return Ok(if server.shutdown { ExitCode::SUCCESS } else { ExitCode::FAILURE }); }
        for reply in server.handle(&message) { write_message(&mut output, &reply)?; }
    }
    // A disconnected editor must not leave a server running or execute stdin as code.
    Ok(if server.shutdown { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
