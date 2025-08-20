use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;
use std::fs;

/// Pattern-based file finding with result optimization
pub struct GlobTool;

#[derive(serde::Deserialize)]
struct GlobParams {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
}

impl GlobTool {
    pub fn new() -> Self {
        Self
    }

    /// Simple glob pattern matching implementation
    fn matches_pattern(&self, file_path: &str, pattern: &str) -> bool {
        // Handle common glob patterns
        if pattern.contains("**") {
            // Recursive pattern like "**/*.rs"
            if let Some(suffix) = pattern.strip_prefix("**/") {
                return file_path.ends_with(&suffix.replace("*.", "."));
            }
        }
        
        if pattern.starts_with("*.") {
            // Simple extension pattern like "*.rs"
            let ext = &pattern[2..];
            return file_path.ends_with(&format!(".{}", ext));
        }

        if pattern.contains('*') {
            // Simple wildcard matching
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                return file_path.starts_with(parts[0]) && file_path.ends_with(parts[1]);
            }
        }

        // Exact match
        file_path.contains(pattern) || file_path.ends_with(pattern)
    }

    fn collect_files(&self, dir: &Path, pattern: &str, results: &mut Vec<(String, u64)>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let path_str = path.to_string_lossy().to_string();

            // Skip hidden files and common ignored directories
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }

            if path.is_file() {
                if self.matches_pattern(&path_str, pattern) {
                    let metadata = entry.metadata()?;
                    let modified = metadata.modified()?
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs();
                    results.push((path_str, modified));
                }
            } else if path.is_dir() {
                // Recurse into subdirectories
                self.collect_files(&path, pattern, results)?;
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Pattern-based file finding with result optimization. Finds files matching glob patterns like '**/*.rs' or '*.txt'."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match files (e.g., '**/*.rs', '*.txt', 'src/**/*.js')"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used. Must be a valid directory path if provided."
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let params: GlobParams = serde_json::from_str(arguments)?;
        
        let search_path = match &params.path {
            Some(path) => {
                // Handle both absolute and relative paths (like ls tool)
                if Path::new(path).is_absolute() {
                    std::env::current_dir()?.join(path).canonicalize()?
                } else {
                    std::env::current_dir()?.join(path).canonicalize()?
                }
            }
            None => {
                // Use current working directory
                std::env::current_dir()?
            }
        };

        if !search_path.exists() {
            return Err(anyhow::anyhow!("Search path does not exist: {}", search_path.display()));
        }

        let mut results = Vec::new();
        self.collect_files(&search_path, &params.pattern, &mut results)?;

        if results.is_empty() {
            return Ok(format!("No files found matching pattern: {}", params.pattern));
        }

        // Sort by modification time (newest first)
        results.sort_by(|a, b| b.1.cmp(&a.1));

        // Format results with intelligent truncation
        let mut output = format!("Found {} files matching pattern '{}':\n\n", 
                                results.len(), 
                                params.pattern);

        if results.len() <= 100 {
            // Show all results for reasonable counts
            for (path, _) in results {
                output.push_str(&format!("{}\n", path));
            }
        } else {
            // Intelligent truncation for large result sets
            output.push_str(&format!("(Showing first 50 and last 10 of {} total files)\n\n", results.len()));
            
            // First 50 files
            for (path, _) in results.iter().take(50) {
                output.push_str(&format!("{}\n", path));
            }
            
            output.push_str("\n... [TRUNCATED] ...\n\n");
            
            // Last 10 files
            for (path, _) in results.iter().skip(results.len() - 10) {
                output.push_str(&format!("{}\n", path));
            }
        }

        Ok(output)
    }
}