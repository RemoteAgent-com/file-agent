use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Context-aware file reading with smart sampling
pub struct ReadTool;

#[derive(serde::Deserialize)]
struct ReadParams {
    file_path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

impl ReadTool {
    pub fn new() -> Self {
        Self
    }

    fn format_line_numbers(&self, content: &str, start_line: usize) -> String {
        content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                // Truncate long lines for readability
                let truncated_line = if line.len() > 2000 {
                    format!("{}... [TRUNCATED]", &line[..2000])
                } else {
                    line.to_string()
                };
                format!("{:5}→{}", start_line + i + 1, truncated_line)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn smart_sample_large_file(&self, file_path: &Path) -> Result<String> {
        let content = fs::read_to_string(file_path)?;
        let lines: Vec<&str> = content.lines().collect();

        // Threshold: 2000 lines before sampling
        if lines.len() <= 2000 {
            return Ok(self.format_line_numbers(&content, 0));
        }

        // Smart sampling strategy with beginning, middle, and end sections
        let mut sampled_content = String::new();

        // Header with file info
        sampled_content.push_str(&format!(
            "=== FILE PREVIEW (Large file: {} lines, {} KB) ===\n\n",
            lines.len(),
            content.len() / 1024
        ));

        // Beginning section (first 100 lines)
        sampled_content.push_str("BEGINNING (first 100 lines):\n");
        let beginning: String = lines
            .iter()
            .take(100)
            .map(|s| *s)
            .collect::<Vec<_>>()
            .join("\n");
        sampled_content.push_str(&self.format_line_numbers(&beginning, 0));
        sampled_content.push_str("\n\n");

        // Middle section (around line count/2)
        let middle_start = (lines.len() / 2).saturating_sub(50);
        let middle_end = (lines.len() / 2 + 50).min(lines.len());
        sampled_content.push_str(&format!("MIDDLE (around line {}):\n", lines.len() / 2));
        let middle: String = lines[middle_start..middle_end].join("\n");
        sampled_content.push_str(&self.format_line_numbers(&middle, middle_start));
        sampled_content.push_str("\n\n");

        // End section (last 100 lines)
        let end_start = lines.len().saturating_sub(100);
        sampled_content.push_str(&format!(
            "END (last 100 lines, starting from line {}):\n",
            end_start + 1
        ));
        let end: String = lines[end_start..].join("\n");
        sampled_content.push_str(&self.format_line_numbers(&end, end_start));

        // Footer with usage info
        sampled_content.push_str(&format!(
            "\n\n=== SAMPLING SUMMARY ===\n\
            Total lines: {}\n\
            Shown: ~300 lines (beginning, middle, end)\n\
            Use offset/limit parameters to read specific sections",
            lines.len()
        ));

        Ok(sampled_content)
    }

    fn read_with_range(&self, file_path: &Path, offset: usize, limit: usize) -> Result<String> {
        let content = fs::read_to_string(file_path)?;
        let lines: Vec<&str> = content.lines().collect();

        if offset >= lines.len() {
            return Ok(format!(
                "Offset {} exceeds file length ({} lines)",
                offset,
                lines.len()
            ));
        }

        let end_line = (offset + limit).min(lines.len());
        let selected_lines: String = lines[offset..end_line].join("\n");

        let mut result = format!(
            "=== LINES {}-{} of {} ===\n\n",
            offset + 1,
            end_line,
            lines.len()
        );
        result.push_str(&self.format_line_numbers(&selected_lines, offset));

        if end_line < lines.len() {
            result.push_str(&format!(
                "\n\n... {} more lines follow ...",
                lines.len() - end_line
            ));
        }

        Ok(result)
    }

    fn get_file_info(&self, file_path: &Path) -> String {
        match fs::metadata(file_path) {
            Ok(metadata) => {
                let size_kb = metadata.len() / 1024;
                let size_str = if size_kb == 0 {
                    format!("{} bytes", metadata.len())
                } else if size_kb < 1024 {
                    format!("{} KB", size_kb)
                } else {
                    format!("{:.1} MB", metadata.len() as f64 / (1024.0 * 1024.0))
                };

                let file_type = file_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| format!(" ({})", ext.to_uppercase()))
                    .unwrap_or_default();

                format!(
                    "File: {}{} - Size: {}",
                    file_path.display(),
                    file_type,
                    size_str
                )
            }
            Err(_) => format!("File: {}", file_path.display()),
        }
    }

    fn detect_binary_file(&self, file_path: &Path) -> Result<bool> {
        // Read first 1KB to check for binary content
        let mut buffer = [0u8; 1024];
        match std::fs::File::open(file_path) {
            Ok(mut file) => {
                use std::io::Read;
                match file.read(&mut buffer) {
                    Ok(bytes_read) => {
                        // Check for null bytes (common binary indicator)
                        let has_nulls = buffer[..bytes_read].contains(&0);
                        Ok(has_nulls)
                    }
                    Err(_) => Ok(false), // Assume text if can't read
                }
            }
            Err(_) => Ok(false),
        }
    }
}

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Context-aware file reading with smart sampling for large files. Automatically handles line numbering and intelligent truncation."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                },
                "offset": {
                    "type": "number",
                    "description": "Line number to start reading from (0-based). Optional."
                },
                "limit": {
                    "type": "number",
                    "description": "Number of lines to read. Optional - if not specified, reads entire file with smart sampling."
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let params: ReadParams = serde_json::from_str(arguments)?;

        // Handle both absolute and relative paths
        let file_path = if Path::new(&params.file_path).is_absolute() {
            Path::new(&params.file_path).to_path_buf()
        } else {
            std::env::current_dir()?.join(&params.file_path)
        };

        if !file_path.exists() {
            return Err(anyhow::anyhow!("File does not exist: {}", params.file_path));
        }

        if !file_path.is_file() {
            return Err(anyhow::anyhow!("Path is not a file: {}", params.file_path));
        }

        // Check if file is binary
        if self.detect_binary_file(&file_path)? {
            let file_info = self.get_file_info(&file_path);
            return Ok(format!(
                "{}\n\n⚠️  This appears to be a binary file and cannot be displayed as text.\n\
                Consider using specialized tools for binary file analysis.",
                file_info
            ));
        }

        let file_info = self.get_file_info(&file_path);
        let mut result = format!("{}\n\n", file_info);

        // Handle range reading vs full file reading
        match (params.offset, params.limit) {
            (Some(offset), Some(limit)) => {
                // Read specific range
                result.push_str(&self.read_with_range(&file_path, offset, limit)?);
            }
            (Some(offset), None) => {
                // Read from offset to end (with reasonable limit)
                let default_limit = 1000; // Reasonable default
                result.push_str(&self.read_with_range(&file_path, offset, default_limit)?);
            }
            (None, Some(limit)) => {
                // Read first N lines
                result.push_str(&self.read_with_range(&file_path, 0, limit)?);
            }
            (None, None) => {
                // Full file read with smart sampling
                match self.smart_sample_large_file(&file_path) {
                    Ok(content) => result.push_str(&content),
                    Err(e) => {
                        return Err(anyhow::anyhow!("Failed to read file: {}", e));
                    }
                }
            }
        }

        Ok(result)
    }
}
