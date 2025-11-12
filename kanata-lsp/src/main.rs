use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Definition {
    uri: Url,
    range: Range,
}

#[derive(Debug)]
struct DocumentSymbols {
    aliases: HashMap<String, Definition>,
    layers: HashMap<String, Definition>,
}

#[derive(Debug)]
struct KanataLanguageServer {
    client: Client,
    diagnostics_cache: Arc<RwLock<HashMap<Url, Vec<Diagnostic>>>>,
    symbols_cache: Arc<RwLock<HashMap<Url, DocumentSymbols>>>,
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
                definition_provider: Some(OneOf::Left(true)),
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

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        // Get the document text - we'll need to read it from the file
        let file_path = uri.to_file_path().ok();
        if file_path.is_none() {
            return Ok(None);
        }
        
        let text = match std::fs::read_to_string(file_path.unwrap()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        
        // Get the word at the cursor position
        let word = Self::get_word_at_position(&text, position);
        if word.is_empty() {
            return Ok(None);
        }
        
        // Check if it's an alias reference (starts with @)
        if word.starts_with('@') {
            let alias_name = &word[1..]; // Remove the @
            let symbols = self.symbols_cache.read().await;
            if let Some(doc_symbols) = symbols.get(uri) {
                if let Some(def) = doc_symbols.aliases.get(alias_name) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: def.uri.clone(),
                        range: def.range,
                    })));
                }
            }
        } else {
            // Check if it's a layer reference
            let symbols = self.symbols_cache.read().await;
            if let Some(doc_symbols) = symbols.get(uri) {
                if let Some(def) = doc_symbols.layers.get(&word) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: def.uri.clone(),
                        range: def.range,
                    })));
                }
            }
        }
        
        Ok(None)
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
    fn get_word_at_position(text: &str, position: Position) -> String {
        let lines: Vec<&str> = text.lines().collect();
        if position.line as usize >= lines.len() {
            return String::new();
        }
        
        let line = lines[position.line as usize];
        let char_pos = position.character as usize;
        
        if char_pos >= line.len() {
            return String::new();
        }
        
        // Find the start of the word (including @ for aliases)
        let mut start = char_pos;
        while start > 0 {
            let c = line.chars().nth(start - 1).unwrap();
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '@' {
                start -= 1;
            } else {
                break;
            }
        }
        
        // Find the end of the word
        let mut end = char_pos;
        while end < line.len() {
            let c = line.chars().nth(end).unwrap();
            if c.is_alphanumeric() || c == '_' || c == '-' {
                end += 1;
            } else {
                break;
            }
        }
        
        line[start..end].to_string()
    }
    
    fn extract_symbols(uri: &Url, text: &str) -> DocumentSymbols {
        let mut aliases = HashMap::new();
        let mut layers = HashMap::new();
        
        let lines: Vec<&str> = text.lines().collect();
        
        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            // Look for defalias definitions: (defalias name ...)
            if trimmed.starts_with("(defalias") {
                // Parse simple s-expression to extract alias names
                let content = trimmed.trim_start_matches("(defalias").trim();
                let mut depth = 0;
                let mut current_word = String::new();
                let mut in_string = false;
                let mut is_first_word = true;
                
                for (_char_idx, c) in content.chars().enumerate() {
                    match c {
                        '"' => in_string = !in_string,
                        '(' if !in_string => depth += 1,
                        ')' if !in_string => {
                            if depth > 0 {
                                depth -= 1;
                            } else {
                                break;
                            }
                        }
                        ' ' | '\t' | '\n' if !in_string && depth == 0 => {
                            if !current_word.is_empty() && is_first_word {
                                // This is an alias name
                                let start_char = line.find(&current_word).unwrap_or(0);
                                aliases.insert(current_word.clone(), Definition {
                                    uri: uri.clone(),
                                    range: Range {
                                        start: Position {
                                            line: line_num as u32,
                                            character: start_char as u32,
                                        },
                                        end: Position {
                                            line: line_num as u32,
                                            character: (start_char + current_word.len()) as u32,
                                        },
                                    },
                                });
                                is_first_word = false;
                            }
                            current_word.clear();
                        }
                        _ if !in_string || depth == 0 => current_word.push(c),
                        _ => {}
                    }
                }
            }
            
            // Look for deflayer definitions: (deflayer name ...)
            if trimmed.starts_with("(deflayer") {
                let content = trimmed.trim_start_matches("(deflayer").trim();
                // Get first word as layer name
                if let Some(end_idx) = content.find(|c: char| c.is_whitespace() || c == ')') {
                    let layer_name = &content[..end_idx];
                    if !layer_name.is_empty() {
                        let start_char = line.find(layer_name).unwrap_or(0);
                        layers.insert(layer_name.to_string(), Definition {
                            uri: uri.clone(),
                            range: Range {
                                start: Position {
                                    line: line_num as u32,
                                    character: start_char as u32,
                                },
                                end: Position {
                                    line: line_num as u32,
                                    character: (start_char + layer_name.len()) as u32,
                                },
                            },
                        });
                    }
                }
            }
        }
        
        DocumentSymbols { aliases, layers }
    }
    
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
        // Extract symbols from the document
        let symbols = Self::extract_symbols(uri, text);
        self.symbols_cache.write().await.insert(uri.clone(), symbols);
        
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
        symbols_cache: Arc::new(RwLock::new(HashMap::new())),
    });
    
    Server::new(stdin, stdout, socket).serve(service).await;
}
