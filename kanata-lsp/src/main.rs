use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[derive(Debug)]
struct KanataLanguageServer {
    client: Client,
    diagnostics_cache: Arc<RwLock<HashMap<Url, Vec<Diagnostic>>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for KanataLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "kanata-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("kanata".to_string()),
                        inter_file_dependencies: false,
                        workspace_diagnostics: false,
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Kanata LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.validate_document(&params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.first() {
            self.validate_document(&params.text_document.uri, &change.text)
                .await;
        }
    }

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let uri = params.text_document.uri;
        let diagnostics = self.diagnostics_cache.read().await
            .get(&uri)
            .cloned()
            .unwrap_or_default();

        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(
                RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: None,
                        items: diagnostics,
                    },
                },
            ),
        ))
    }
}

impl KanataLanguageServer {
    fn extract_line_info(error_msg: &str) -> (u32, u32, u32) {
        // Try to extract line number from the visual range markers first
        // Format: "79 │ ╭─▶" to "85 │ ├─▶"
        let mut start_line = None;
        let mut end_line = None;
        
        for line in error_msg.lines() {
            // Look for the start marker
            if (line.contains("╭─▶") || line.contains("│ ╭─▶")) && start_line.is_none() {
                if let Some(num_str) = line.split('│').next() {
                    if let Ok(num) = num_str.trim().parse::<u32>() {
                        start_line = Some(num.saturating_sub(1)); // Convert to 0-based
                    }
                }
            }
            // Look for the end marker
            if line.contains("├─▶") || line.contains("╰──") {
                if let Some(num_str) = line.split('│').next() {
                    if let Ok(num) = num_str.trim().parse::<u32>() {
                        end_line = Some(num.saturating_sub(1)); // Convert to 0-based
                    }
                }
            }
        }
        
        // If we found both start and end markers, use them
        if let (Some(start), Some(end)) = (start_line, end_line) {
            return (start, 0, end);
        }
        
        // Fallback: Look for line:col in brackets like [file.kbd:78:1]
        if let Some(start) = error_msg.find(".kbd:") {
            let after_kbd = &error_msg[start + 5..];
            if let Some(end) = after_kbd.find(']') {
                let coords = &after_kbd[..end];
                let parts: Vec<&str> = coords.split(':').collect();
                
                if parts.len() >= 2 {
                    if let (Ok(line), Ok(col)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        let line_0 = line.saturating_sub(1);
                        let col_0 = col.saturating_sub(1);
                        return (line_0, col_0, line_0);
                    }
                }
            }
        }
        
        // Default to line 0 if we can't parse
        (0, 0, 0)
    }

    async fn validate_document(&self, uri: &Url, text: &str) {
        // Write text to a temporary file to parse it
        let temp_file = std::env::temp_dir().join("kanata-temp.kbd");
        let diagnostics = match std::fs::write(&temp_file, text) {
            Ok(_) => {
                // Parse the file and immediately convert to error string
                let parse_result = kanata_parser::cfg::new_from_file(&temp_file)
                    .map(|_| ())
                    .map_err(|e| format!("{:?}", e));
                
                match parse_result {
                    Ok(_) => {
                        // Parsing succeeded, no diagnostics
                        vec![]
                    }
                    Err(error_msg) => {
                        // Parse error - create diagnostic
                        
                        // Extract line information from error message
                        let (start_line, start_col, end_line) = Self::extract_line_info(&error_msg);
                        
                        // Get the actual line length to avoid going past end of line
                        let end_col = if start_line == end_line {
                            // Single line diagnostic - highlight from start_col to end of line
                            text.lines().nth(start_line as usize)
                                .map(|line| line.len() as u32)
                                .unwrap_or(start_col + 1)
                        } else {
                            // Multi-line diagnostic - highlight to end of end_line
                            text.lines().nth(end_line as usize)
                                .map(|line| line.len() as u32)
                                .unwrap_or(0)
                        };
                        
                        // Log the error message for debugging
                        self.client.log_message(
                            MessageType::INFO,
                            format!("Full error: {}", error_msg.lines().take(15).collect::<Vec<_>>().join(" || "))
                        ).await;
                        
                        // Extract just the text after "help:"
                        let display_message = error_msg.lines()
                            .find(|line| line.contains("help:"))
                            .and_then(|line| line.split("help:").nth(1))
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "Parse error".to_string());
                        
                        self.client.log_message(
                            MessageType::INFO,
                            format!("Extracted message: {}", display_message)
                        ).await;
                        
                        vec![Diagnostic {
                            range: Range {
                                start: Position {
                                    line: start_line,
                                    character: start_col,
                                },
                                end: Position {
                                    line: end_line,
                                    character: end_col,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: None,
                            code_description: None,
                            source: Some("kanata-lsp".to_string()),
                            message: display_message,
                            related_information: None,
                            tags: None,
                            data: None,
                        }]
                    }
                }
            }
            Err(e) => {
                vec![Diagnostic {
                    range: Range {
                        start: Position { line: 0, character: 0 },
                        end: Position { line: 0, character: 0 },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("kanata-lsp".to_string()),
                    message: format!("Failed to write temp file: {}", e),
                    related_information: None,
                    tags: None,
                    data: None,
                }]
            }
        };

        // Store diagnostics in cache for pull diagnostics
        self.diagnostics_cache.write().await.insert(uri.clone(), diagnostics.clone());

        // Also publish diagnostics for push model
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| KanataLanguageServer { 
        client,
        diagnostics_cache: Arc::new(RwLock::new(HashMap::new())),
    });
    
    Server::new(stdin, stdout, socket).serve(service).await;
}
