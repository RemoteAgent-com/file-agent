use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Safe file creation with validation
pub struct WriteTool;

#[derive(serde::Deserialize)]
struct WriteParams {
    file_path: String,
    content: String,
    #[serde(default = "default_false")]
    overwrite: bool,
}

fn default_false() -> bool {
    false
}

impl WriteTool {
    pub fn new() -> Self {
        Self
    }

    fn validate_file_path(&self, file_path: &Path) -> Result<()> {
        // Check if parent directory exists
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                return Err(anyhow::anyhow!(
                    "Parent directory does not exist: {}. Create it first or provide a valid path.",
                    parent.display()
                ));
            }
        }

        // Check for dangerous paths
        let path_str = file_path.to_string_lossy();
        let dangerous_patterns = [
            "/etc/",
            "/usr/",
            "/bin/",
            "/sbin/",
            "/var/log/",
            "/.ssh/",
            "/root/",
            "/home/",
            "C:\\Windows\\",
            "C:\\Program Files\\",
        ];

        for pattern in &dangerous_patterns {
            if path_str.contains(pattern) {
                return Err(anyhow::anyhow!(
                    "Refusing to write to system directory: {}. For safety, only write to project directories.",
                    file_path.display()
                ));
            }
        }

        Ok(())
    }

    fn check_existing_file(&self, file_path: &Path, overwrite: bool) -> Result<()> {
        if file_path.exists() {
            if !overwrite {
                return Err(anyhow::anyhow!(
                    "File already exists: {}. Use Read tool first to understand the current contents, then use Edit tool to modify existing files, or set overwrite=true to replace entirely.",
                    file_path.display()
                ));
            }

            // Additional safety check for important files
            if let Some(file_name) = file_path.file_name() {
                let name = file_name.to_string_lossy().to_lowercase();
                let important_files = [
                    "cargo.toml",
                    "package.json",
                    "requirements.txt",
                    "go.mod",
                    "dockerfile",
                    "docker-compose.yml",
                    "makefile",
                    ".gitignore",
                    "readme.md",
                    "license",
                    "changelog.md",
                ];

                if important_files.contains(&name.as_str()) {
                    log::warn!("Overwriting important file: {}", file_path.display());
                }
            }
        }
        Ok(())
    }

    fn validate_content(&self, content: &str, file_path: &Path) -> Result<String> {
        // Check for potentially dangerous content
        let dangerous_patterns = [
            "rm -rf",
            "del /f",
            "format c:",
            "shutdown",
            "reboot",
            "DROP TABLE",
            "DELETE FROM",
            "TRUNCATE",
        ];

        for pattern in &dangerous_patterns {
            if content.contains(pattern) {
                log::warn!(
                    "Potentially dangerous content detected in {}: contains '{}'",
                    file_path.display(),
                    pattern
                );
            }
        }

        // Normalize line endings based on file type
        let normalized_content = if cfg!(windows) {
            content.replace('\n', "\r\n")
        } else {
            content.replace("\r\n", "\n")
        };

        // Ensure file ends with newline for text files
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let text_extensions = [
            "txt", "md", "rs", "js", "ts", "py", "go", "java", "cpp", "c", "h", "css", "html",
            "xml", "json", "yaml", "yml", "toml", "ini", "cfg",
        ];

        if text_extensions.contains(&extension) && !normalized_content.ends_with('\n') {
            Ok(format!("{}\n", normalized_content))
        } else {
            Ok(normalized_content)
        }
    }

    fn get_file_stats(&self, content: &str) -> (usize, usize, usize) {
        let lines = content.lines().count();
        let words = content.split_whitespace().count();
        let bytes = content.len();
        (lines, words, bytes)
    }
}

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Safe file creation with validation. Always prefer editing existing files in the codebase rather than creating new ones."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to create/write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Whether to overwrite existing files (default: false)",
                    "default": false
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let params: WriteParams = serde_json::from_str(arguments)?;

        // Handle both absolute and relative paths
        let file_path = if Path::new(&params.file_path).is_absolute() {
            Path::new(&params.file_path).to_path_buf()
        } else {
            std::env::current_dir()?.join(&params.file_path)
        };

        // Validate path safety
        self.validate_file_path(&file_path)?;

        // Check existing file
        self.check_existing_file(&file_path, params.overwrite)?;

        // Validate and normalize content
        let final_content = self.validate_content(&params.content, &file_path)?;

        // Write file
        match fs::write(&file_path, &final_content) {
            Ok(_) => {
                let (lines, words, bytes) = self.get_file_stats(&final_content);

                let mut result = format!(
                    "Successfully wrote file: {}\n\
                     Stats: {} lines, {} words, {} bytes",
                    file_path.display(),
                    lines,
                    words,
                    bytes
                );

                // Show content preview for small files
                if lines <= 20 {
                    result.push_str("\n\nContent preview:\n");
                    for (i, line) in final_content.lines().enumerate() {
                        result.push_str(&format!("{:3}│ {}\n", i + 1, line));
                    }
                } else {
                    result.push_str(&format!("\n\nContent preview (first 10 lines):\n"));
                    for (i, line) in final_content.lines().take(10).enumerate() {
                        result.push_str(&format!("{:3}│ {}\n", i + 1, line));
                    }
                    result.push_str(&format!("    ... {} more lines", lines - 10));
                }

                log::info!("File written successfully: {}", file_path.display());
                Ok(result)
            }
            Err(e) => Err(anyhow::anyhow!(
                "Failed to write file {}: {}",
                file_path.display(),
                e
            )),
        }
    }
}
