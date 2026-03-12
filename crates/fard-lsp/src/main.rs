use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[derive(Debug)]
struct FardLsp {
    client: Client,
    docs: Arc<RwLock<HashMap<String, String>>>,
}

async fn publish(client: &Client, uri: Url, text: &str) {
    let errors = fard_v0_5_language_gate::parse_check(text, &uri.to_string());
    let diags: Vec<Diagnostic> = errors.into_iter().map(|(line, col, msg)| {
        Diagnostic {
            range: Range {
                start: Position { line, character: col },
                end:   Position { line, character: col + 80 },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            message: msg,
            source: Some("fard-lsp".to_string()),
            ..Default::default()
        }
    }).collect();
    client.publish_diagnostics(uri, diags, None).await;
}

fn hover_for_word(word: &str) -> Option<String> {
    match word {
        "import"   => Some("`import(\"std/list\") as list` — load a stdlib or package module".to_string()),
        "artifact" => Some("`artifact name = \"sha256:...\"` — bind a prior verified run by RunID".to_string()),
        "let"      => Some("`let name = expr` — bind a value in the current scope".to_string()),
        "fn"       => Some("`fn name(params) { body }` — define a function".to_string()),
        "export"   => Some("`export { name, ... }` — export names from a module".to_string()),
        "match"    => Some("`match expr { pat => val, _ => default }` — pattern match".to_string()),
        "if"       => Some("`if cond then expr else expr` — conditional expression".to_string()),
        "while"    => Some("`while init cond_fn body_fn` — loop with state".to_string()),
        "return"   => Some("`return expr` — early return from a function".to_string()),
        "null"     => Some("`null` — the unit value".to_string()),
        "true" | "false" => Some(format!("`{}` — boolean literal", word)),
        "list"     => Some("**std/list** — `map`, `filter`, `fold`, `len`, `append`, `zip`, `sort`, `head`, `tail`, `find`".to_string()),
        "str"      => Some("**std/str** — `concat`, `join`, `split`, `len`, `slice`, `from_int`, `trim`, `contains`, `replace`".to_string()),
        "math"     => Some("**std/math** — `add`, `sub`, `mul`, `div`, `mod`, `pow`, `sqrt`, `floor`, `ceil`, `abs`".to_string()),
        "io"       => Some("**std/io** — `write_file`, `read_file`, `print`, `println`".to_string()),
        "hash"     => Some("**std/hash** — `sha256_bytes`, `sha256_text`".to_string()),
        "json"     => Some("**std/json** — `parse`, `stringify`".to_string()),
        "re"       => Some("**std/re** — `is_match`, `find`, `replace`".to_string()),
        "http"     => Some("**std/http** — `get`, `post`".to_string()),
        "witness"  => Some("**std/witness** — `self_digest()`, `deps()`, `verify(run_id)`, `verify_chain(run_id)`".to_string()),
        _ => None,
    }
}

fn word_at(text: &str, line: u32, col: u32) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let l = lines.get(line as usize).copied().unwrap_or("");
    let chars: Vec<char> = l.chars().collect();
    let c = col as usize;
    let start = (0..c).rev().take_while(|&i| chars.get(i).map(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '/').unwrap_or(false)).last().unwrap_or(c);
    let end = (c..chars.len()).take_while(|&i| chars.get(i).map(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '/').unwrap_or(false)).last().map(|i| i+1).unwrap_or(c);
    chars[start..end].iter().collect()
}

#[tower_lsp::async_trait]
impl LanguageServer for FardLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "fard-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "fard-lsp initialized").await;
    }

    async fn shutdown(&self) -> Result<()> { Ok(()) }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.docs.write().await.insert(uri.to_string(), text.clone());
        publish(&self.client, uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            self.docs.write().await.insert(uri.to_string(), change.text.clone());
            publish(&self.client, uri, &change.text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(text) = params.text {
            self.docs.write().await.insert(uri.to_string(), text.clone());
            publish(&self.client, uri, &text).await;
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let docs = self.docs.read().await;
        if let Some(text) = docs.get(&uri.to_string()) {
            let word = word_at(text, pos.line, pos.character);
            if let Some(doc) = hover_for_word(&word) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: doc,
                    }),
                    range: None,
                }));
            }
        }
        Ok(None)
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| FardLsp {
        client,
        docs: Arc::new(RwLock::new(HashMap::new())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
