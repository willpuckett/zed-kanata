use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

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
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
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
        
        let text = match std::fs::read_to_string(file_path.as_ref().unwrap()) {
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

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        
        // Get the document text
        let file_path = uri.to_file_path().ok();
        if file_path.is_none() {
            return Ok(None);
        }
        
        let text = match std::fs::read_to_string(file_path.as_ref().unwrap()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        
        // Get the word at the cursor position
        let word = Self::get_word_at_position(&text, position);
        if word.is_empty() {
            return Ok(None);
        }
        
        // Determine if it's an alias or layer
        let (search_word, is_alias) = if word.starts_with('@') {
            (&word[1..], true)
        } else {
            (word.as_str(), false)
        };
        
        let mut locations = Vec::new();
        
        // Search through all documents in the cache
        let symbols = self.symbols_cache.read().await;
        for (doc_uri, _doc_symbols) in symbols.iter() {
            // Read the document to find references
            if let Ok(doc_path) = doc_uri.to_file_path() {
                if let Ok(doc_text) = std::fs::read_to_string(&doc_path) {
                    let lines: Vec<&str> = doc_text.lines().collect();
                    
                    for (line_idx, line) in lines.iter().enumerate() {
                        if is_alias {
                            // Look for @word references
                            let search_pattern = format!("@{}", search_word);
                            let mut start = 0;
                            while let Some(pos) = line[start..].find(&search_pattern) {
                                let actual_pos = start + pos;
                                locations.push(Location {
                                    uri: doc_uri.clone(),
                                    range: Range {
                                        start: Position {
                                            line: line_idx as u32,
                                            character: actual_pos as u32,
                                        },
                                        end: Position {
                                            line: line_idx as u32,
                                            character: (actual_pos + search_pattern.len()) as u32,
                                        },
                                    },
                                });
                                start = actual_pos + 1;
                            }
                        } else {
                            // Look for layer name references (without @)
                            // This is trickier - we need to find word boundaries
                            for (char_idx, _) in line.char_indices() {
                                let remaining = &line[char_idx..];
                                if remaining.starts_with(search_word) {
                                    // Check if it's a word boundary
                                    let before_ok = char_idx == 0 || !line.chars().nth(char_idx - 1).unwrap().is_alphanumeric();
                                    let after_idx = char_idx + search_word.len();
                                    let after_ok = after_idx >= line.len() || !line.chars().nth(after_idx).map(|c| c.is_alphanumeric()).unwrap_or(false);
                                    
                                    if before_ok && after_ok {
                                        locations.push(Location {
                                            uri: doc_uri.clone(),
                                            range: Range {
                                                start: Position {
                                                    line: line_idx as u32,
                                                    character: char_idx as u32,
                                                },
                                                end: Position {
                                                    line: line_idx as u32,
                                                    character: (char_idx + search_word.len()) as u32,
                                                },
                                            },
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = &params.new_name;
        
        // Get the document text
        let file_path = uri.to_file_path().ok();
        if file_path.is_none() {
            return Ok(None);
        }
        
        let text = match std::fs::read_to_string(file_path.as_ref().unwrap()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        
        // Get the word at the cursor position
        let word = Self::get_word_at_position(&text, position);
        if word.is_empty() {
            return Ok(None);
        }
        
        // Find all references to this symbol
        let references_params = ReferenceParams {
            text_document_position: params.text_document_position.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        };
        
        let locations = match self.references(references_params).await? {
            Some(locs) => locs,
            None => return Ok(None),
        };
        
        // Determine if we're renaming an alias (and need to add/remove @)
        let is_alias = word.starts_with('@');
        
        // Create text edits for all references
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        
        for location in locations {
            let edit = TextEdit {
                range: location.range,
                new_text: if is_alias {
                    format!("@{}", new_name)
                } else {
                    new_name.to_string()
                },
            };
            
            changes.entry(location.uri.clone())
                .or_insert_with(Vec::new)
                .push(edit);
        }
        
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn goto_implementation(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        // For Kanata, implementation is the same as definition
        // (finding where aliases/layers are defined)
        self.goto_definition(params).await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        
        // Get the document text
        let file_path = uri.to_file_path().ok();
        if file_path.is_none() {
            return Ok(None);
        }
        
        let text = match std::fs::read_to_string(file_path.unwrap()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        
        // Format the document
        let formatted = Self::format_document(&text);
        
        if formatted == text {
            // No changes needed
            return Ok(None);
        }
        
        // Calculate the range covering the entire document
        let line_count = text.lines().count() as u32;
        let last_line_len = text.lines().last().map(|l| l.len()).unwrap_or(0) as u32;
        
        Ok(Some(vec![TextEdit {
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: line_count.saturating_sub(1), character: last_line_len },
            },
            new_text: formatted,
        }]))
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
    fn format_document(text: &str) -> String {
        // Parse defsrc layout
        let defsrc_layout = match Self::parse_defsrc_layout(text) {
            Some(layout) => layout,
            None => return text.to_string(), // No defsrc found, no formatting
        };
        
        // Apply layout to all deflayers
        Self::apply_defsrc_layout_to_deflayers(text, &defsrc_layout)
    }
    
    fn parse_defsrc_layout(text: &str) -> Option<Vec<Vec<usize>>> {
        let lines: Vec<&str> = text.lines().collect();
        let mut in_defsrc = false;
        let mut defsrc_items: Vec<String> = Vec::new();
        let mut paren_depth = 0;
        
        for line in lines {
            let trimmed = line.trim();
            
            if trimmed.starts_with("(defsrc") {
                in_defsrc = true;
                paren_depth = 1;
                // Extract content after (defsrc
                let content = trimmed.trim_start_matches("(defsrc").trim();
                if !content.is_empty() && !content.starts_with(')') {
                    let mut current = String::new();
                    for ch in content.chars() {
                        if ch == '(' {
                            paren_depth += 1;
                        } else if ch == ')' {
                            paren_depth -= 1;
                            if paren_depth == 0 {
                                if !current.trim().is_empty() {
                                    defsrc_items.push(current.clone());
                                }
                                in_defsrc = false;
                                break;
                            }
                        } else if ch.is_whitespace() && paren_depth == 1 {
                            if !current.trim().is_empty() {
                                defsrc_items.push(current.clone());
                                current.clear();
                            }
                        } else {
                            current.push(ch);
                        }
                    }
                    if !current.trim().is_empty() && in_defsrc {
                        defsrc_items.push(current);
                    }
                }
                continue;
            }
            
            if in_defsrc {
                for ch in trimmed.chars() {
                    if ch == '(' {
                        paren_depth += 1;
                    } else if ch == ')' {
                        paren_depth -= 1;
                        if paren_depth == 0 {
                            in_defsrc = false;
                            break;
                        }
                    }
                }
                
                if in_defsrc && paren_depth == 1 {
                    // Parse items from this line
                    let mut current = String::new();
                    for ch in trimmed.chars() {
                        if ch == ')' {
                            if !current.trim().is_empty() {
                                defsrc_items.push(current.clone());
                            }
                            in_defsrc = false;
                            break;
                        } else if ch.is_whitespace() {
                            if !current.trim().is_empty() {
                                defsrc_items.push(current.clone());
                                current.clear();
                            }
                        } else {
                            current.push(ch);
                        }
                    }
                    if !current.trim().is_empty() && in_defsrc {
                        defsrc_items.push(current);
                    }
                }
            }
        }
        
        if defsrc_items.is_empty() {
            return None;
        }
        
        // Calculate layout (grapheme widths for each item)
        let layout: Vec<Vec<usize>> = defsrc_items
            .iter()
            .map(|item| vec![item.graphemes(true).count()])
            .collect();
        
        Some(layout)
    }
    
    fn apply_defsrc_layout_to_deflayers(text: &str, layout: &[Vec<usize>]) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let mut result = Vec::new();
        let mut i = 0;
        
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim();
            
            if trimmed.starts_with("(deflayer") {
                // Format this deflayer
                let formatted_deflayer = Self::format_deflayer(&lines, i, layout);
                result.push(formatted_deflayer.0);
                i = formatted_deflayer.1;
            } else {
                result.push(line.to_string());
                i += 1;
            }
        }
        
        result.join("\n")
    }
    
    fn format_deflayer(lines: &[&str], start_idx: usize, layout: &[Vec<usize>]) -> (String, usize) {
        let mut result = String::new();
        let first_line = lines[start_idx];
        let indent = first_line.len() - first_line.trim_start().len();
        
        // Extract layer name
        let trimmed = first_line.trim();
        let after_deflayer = trimmed.trim_start_matches("(deflayer").trim();
        let layer_name = after_deflayer
            .split_whitespace()
            .next()
            .unwrap_or("");
        
        result.push_str(&" ".repeat(indent));
        result.push_str("(deflayer ");
        result.push_str(layer_name);
        
        // Parse all items in this deflayer
        let mut items = Vec::new();
        let mut i = start_idx;
        let mut in_deflayer = true;
        let mut paren_depth = 1;
        
        // Skip past layer name on first line
        let first_line_rest = after_deflayer.trim_start_matches(layer_name).trim();
        let mut current = String::new();
        
        for ch in first_line_rest.chars() {
            if ch == '(' {
                paren_depth += 1;
                current.push(ch);
            } else if ch == ')' {
                paren_depth -= 1;
                if paren_depth == 0 {
                    if !current.trim().is_empty() {
                        items.push(current.clone());
                    }
                    in_deflayer = false;
                    break;
                } else {
                    current.push(ch);
                }
            } else if ch.is_whitespace() && paren_depth == 1 {
                if !current.trim().is_empty() {
                    items.push(current.clone());
                    current.clear();
                }
            } else {
                current.push(ch);
            }
        }
        
        if !current.trim().is_empty() && in_deflayer {
            items.push(current);
        }
        
        // Continue parsing subsequent lines
        i += 1;
        while i < lines.len() && in_deflayer {
            let line = lines[i].trim();
            current = String::new();
            
            for ch in line.chars() {
                if ch == '(' {
                    paren_depth += 1;
                    current.push(ch);
                } else if ch == ')' {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        if !current.trim().is_empty() {
                            items.push(current.clone());
                        }
                        in_deflayer = false;
                        break;
                    } else {
                        current.push(ch);
                    }
                } else if ch.is_whitespace() && paren_depth == 1 {
                    if !current.trim().is_empty() {
                        items.push(current.clone());
                        current.clear();
                    }
                } else {
                    current.push(ch);
                }
            }
            
            if !current.trim().is_empty() && in_deflayer {
                items.push(current);
            }
            i += 1;
        }
        
        // Only format if item count matches defsrc
        if items.len() != layout.len() {
            // Return original lines unchanged
            let mut original = String::new();
            for idx in start_idx..i {
                if idx > start_idx {
                    original.push('\n');
                }
                original.push_str(lines[idx]);
            }
            return (original, i);
        }
        
        // Apply layout
        for (idx, item) in items.iter().enumerate() {
            let item_width = item.graphemes(true).count();
            let target_width = layout[idx][0];
            
            result.push('\n');
            result.push_str(&" ".repeat(indent + 2));
            result.push_str(item);
            
            // Add padding if item is shorter than target
            if item_width < target_width {
                result.push_str(&" ".repeat(target_width - item_width));
            }
        }
        
        result.push('\n');
        result.push_str(&" ".repeat(indent));
        result.push(')');
        
        (result, i)
    }
    
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
        
        // Convert text to bytes with line/column tracking
        let lines: Vec<&str> = text.lines().collect();
        
        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            
            // Check if this line starts with (defalias or (deflayer
            if trimmed.starts_with("(defalias") {
                // Check if the name is on the same line
                let after_keyword = trimmed.trim_start_matches("(defalias").trim_start();
                if !after_keyword.is_empty() && !after_keyword.starts_with('(') && !after_keyword.starts_with(')') {
                    // Name is on the same line
                    let name_end = after_keyword.find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                        .unwrap_or(after_keyword.len());
                    let alias_name = &after_keyword[..name_end];
                    let name_col = indent + "(defalias".len() + (after_keyword.as_ptr() as usize - trimmed.trim_start_matches("(defalias").as_ptr() as usize);
                    
                    aliases.insert(alias_name.to_string(), Definition {
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: line_idx as u32,
                                character: name_col as u32,
                            },
                            end: Position {
                                line: line_idx as u32,
                                character: (name_col + alias_name.len()) as u32,
                            },
                        },
                    });
                } else {
                    // Name might be on the next line(s)
                    for (offset, next_line) in lines[(line_idx + 1)..].iter().enumerate() {
                        let next_trimmed = next_line.trim_start();
                        let next_indent = next_line.len() - next_trimmed.len();
                        
                        if next_trimmed.is_empty() || next_trimmed.starts_with(';') {
                            continue;
                        }
                        
                        if next_trimmed.starts_with(')') {
                            break;
                        }
                        
                        // This should be the alias name
                        let name_end = next_trimmed.find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                            .unwrap_or(next_trimmed.len());
                        let alias_name = &next_trimmed[..name_end];
                        
                        if !alias_name.is_empty() {
                            let actual_line = line_idx + 1 + offset;
                            
                            aliases.insert(alias_name.to_string(), Definition {
                                uri: uri.clone(),
                                range: Range {
                                    start: Position {
                                        line: actual_line as u32,
                                        character: next_indent as u32,
                                    },
                                    end: Position {
                                        line: actual_line as u32,
                                        character: (next_indent + alias_name.len()) as u32,
                                    },
                                },
                            });
                            break;
                        }
                    }
                }
            } else if trimmed.starts_with("(deflayer") {
                // Check if the name is on the same line
                let after_keyword = trimmed.trim_start_matches("(deflayer").trim_start();
                if !after_keyword.is_empty() && !after_keyword.starts_with('(') && !after_keyword.starts_with(')') {
                    // Name is on the same line
                    let name_end = after_keyword.find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                        .unwrap_or(after_keyword.len());
                    let layer_name = &after_keyword[..name_end];
                    let name_col = indent + "(deflayer".len() + (after_keyword.as_ptr() as usize - trimmed.trim_start_matches("(deflayer").as_ptr() as usize);
                    
                    layers.insert(layer_name.to_string(), Definition {
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: line_idx as u32,
                                character: name_col as u32,
                            },
                            end: Position {
                                line: line_idx as u32,
                                character: (name_col + layer_name.len()) as u32,
                            },
                        },
                    });
                } else {
                    // Name might be on the next line(s)
                    for (offset, next_line) in lines[(line_idx + 1)..].iter().enumerate() {
                        let next_trimmed = next_line.trim_start();
                        let next_indent = next_line.len() - next_trimmed.len();
                        
                        if next_trimmed.is_empty() || next_trimmed.starts_with(';') {
                            continue;
                        }
                        
                        if next_trimmed.starts_with(')') {
                            break;
                        }
                        
                        // This should be the layer name
                        let name_end = next_trimmed.find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                            .unwrap_or(next_trimmed.len());
                        let layer_name = &next_trimmed[..name_end];
                        
                        if !layer_name.is_empty() {
                            let actual_line = line_idx + 1 + offset;
                            
                            layers.insert(layer_name.to_string(), Definition {
                                uri: uri.clone(),
                                range: Range {
                                    start: Position {
                                        line: actual_line as u32,
                                        character: next_indent as u32,
                                    },
                                    end: Position {
                                        line: actual_line as u32,
                                        character: (next_indent + layer_name.len()) as u32,
                                    },
                                },
                            });
                            break;
                        }
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
                            let line_len = text.lines().nth(start_line as usize)
                                .map(|line| line.len() as u32)
                                .unwrap_or(start_col + 1);
                            // Ensure end_col is at least 1 character after start_col
                            line_len.max(start_col + 1)
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
                        
                        // Ensure the range is valid
                        let (final_start_line, final_start_col, final_end_line, final_end_col) = 
                            if start_line > end_line || (start_line == end_line && start_col >= end_col) {
                                // Invalid range, use a minimal valid range at start position
                                (start_line, start_col, start_line, start_col + 1)
                            } else {
                                (start_line, start_col, end_line, end_col)
                            };
                        
                        self.client.log_message(
                            MessageType::INFO,
                            format!("Diagnostic range: {}:{} to {}:{}", 
                                final_start_line, final_start_col, final_end_line, final_end_col)
                        ).await;
                        
                        vec![Diagnostic {
                            range: Range {
                                start: Position {
                                    line: final_start_line,
                                    character: final_start_col,
                                },
                                end: Position {
                                    line: final_end_line,
                                    character: final_end_col,
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
