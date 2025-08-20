use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Smart directory listing with size analysis and filtering
pub struct LsTool;

#[derive(serde::Deserialize)]
struct LsParams {
    path: String,
    #[serde(default)]
    ignore: Vec<String>,
}

impl LsTool {
    pub fn new() -> Self {
        Self
    }

    fn format_file_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }

    fn should_ignore(&self, name: &str, ignore_patterns: &[String]) -> bool {
        for pattern in ignore_patterns {
            if name.contains(pattern) || name.starts_with('.') && pattern == ".*" {
                return true;
            }
        }
        false
    }
}

#[async_trait::async_trait]
impl Tool for LsTool {
    fn name(&self) -> &str {
        "ls"
    }

    fn description(&self) -> &str {
        "Smart directory listing with size analysis and filtering. Lists files and directories with metadata."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The absolute path to the directory to list (must be absolute, not relative)"
                },
                "ignore": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Glob patterns to ignore (e.g., [\".git\", \"node_modules\"])",
                    "default": []
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let params: LsParams = serde_json::from_str(arguments)?;
        
        // Convert relative paths to absolute paths
        let path = if Path::new(&params.path).is_absolute() {
            Path::new(&params.path).to_path_buf()
        } else {
            // Handle relative paths like "." or ".."
            std::env::current_dir()?.join(&params.path)
        };
        
        let path = path.as_path();
        if !path.exists() {
            return Err(anyhow::anyhow!("Path does not exist: {}", params.path));
        }

        if !path.is_dir() {
            return Err(anyhow::anyhow!("Path is not a directory: {}", params.path));
        }

        let entries = fs::read_dir(path)?;
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        let mut total_size = 0u64;

        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            
            // Skip ignored patterns
            if self.should_ignore(&name, &params.ignore) {
                continue;
            }

            let metadata = entry.metadata()?;
            let modified = metadata.modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();

            if metadata.is_dir() {
                dirs.push((name, modified));
            } else {
                let size = metadata.len();
                total_size += size;
                files.push((name, size, modified));
            }
        }

        // Sort by modification time (newest first)
        dirs.sort_by(|a, b| b.1.cmp(&a.1));
        files.sort_by(|a, b| b.2.cmp(&a.2));

        // Create intelligent summary
        let mut result = format!("Directory: {} ({})\n", params.path, path.display());
        
        if !dirs.is_empty() {
            result.push_str(&format!("\nSubdirectories ({}):\n", dirs.len()));
            let display_dirs = if dirs.len() > 20 {
                result.push_str(&format!("(Showing first 20 of {} directories)\n", dirs.len()));
                &dirs[..20]
            } else {
                &dirs
            };
            
            for (name, _) in display_dirs {
                result.push_str(&format!("  {}/\n", name));
            }
        }

        if !files.is_empty() {
            result.push_str(&format!("\nFiles ({}) - Total size: {}:\n", 
                                   files.len(), 
                                   Self::format_file_size(total_size)));
            
            let display_files = if files.len() > 30 {
                result.push_str(&format!("(Showing first 30 of {} files)\n", files.len()));
                &files[..30]
            } else {
                &files
            };

            for (name, size, _) in display_files {
                result.push_str(&format!("  {} ({})\n", name, Self::format_file_size(*size)));
            }

            // Show largest files summary
            if files.len() > 5 {
                let mut largest_files = files.clone();
                largest_files.sort_by(|a, b| b.1.cmp(&a.1));
                result.push_str("\nLargest files:\n");
                for (name, size, _) in largest_files.iter().take(5) {
                    result.push_str(&format!("  {} ({})\n", name, Self::format_file_size(*size)));
                }
            }
        }

        if dirs.is_empty() && files.is_empty() {
            result.push_str("(Empty directory)");
        }

        Ok(result)
    }
}