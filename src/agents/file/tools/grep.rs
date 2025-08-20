use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

/// Intelligent text search with context-aware truncation
pub struct GrepTool;

#[derive(serde::Deserialize)]
struct GrepParams {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    output_mode: Option<String>, // "content", "files_with_matches", "count"
    #[serde(default, rename = "-i")]
    case_insensitive: Option<bool>,
    #[serde(default, rename = "-B")]
    context_before: Option<usize>,
    #[serde(default, rename = "-A")]
    context_after: Option<usize>,
    #[serde(default, rename = "-C")]
    context_around: Option<usize>,
    #[serde(default, rename = "-n")]
    line_numbers: Option<bool>,
    #[serde(default)]
    head_limit: Option<usize>,
    #[serde(default = "default_false")]
    multiline: bool,
}

fn default_false() -> bool {
    false
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }

    fn get_file_type_extensions(file_type: &str) -> Vec<&'static str> {
        match file_type {
            "js" => vec!["js", "jsx", "mjs"],
            "ts" => vec!["ts", "tsx"],
            "py" => vec!["py", "pyx", "pyi"],
            "rust" => vec!["rs"],
            "go" => vec!["go"],
            "java" => vec!["java"],
            "cpp" => vec!["cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx"],
            "c" => vec!["c", "h"],
            "css" => vec!["css", "scss", "sass", "less"],
            "html" => vec!["html", "htm", "xhtml"],
            "json" => vec!["json"],
            "xml" => vec!["xml", "xsd", "xsl"],
            "yaml" => vec!["yaml", "yml"],
            "md" => vec!["md", "markdown"],
            "txt" => vec!["txt"],
            _ => vec![],
        }
    }

    fn build_ripgrep_command(&self, params: &GrepParams, search_path: &Path) -> Command {
        // Try to find ripgrep binary, fallback to common locations
        let rg_binary = if std::process::Command::new("rg")
            .arg("--version")
            .output()
            .is_ok()
        {
            "rg"
        } else if std::process::Command::new("/usr/local/bin/rg")
            .arg("--version")
            .output()
            .is_ok()
        {
            "/usr/local/bin/rg"
        } else if std::process::Command::new("/opt/homebrew/bin/rg")
            .arg("--version")
            .output()
            .is_ok()
        {
            "/opt/homebrew/bin/rg"
        } else if std::process::Command::new(
            "/usr/local/lib/node_modules/@anthropic-ai/claude-code/vendor/ripgrep/arm64-darwin/rg",
        )
        .arg("--version")
        .output()
        .is_ok()
        {
            "/usr/local/lib/node_modules/@anthropic-ai/claude-code/vendor/ripgrep/arm64-darwin/rg"
        } else {
            "grep" // Fallback to system grep
        };

        let mut cmd = Command::new(rg_binary);

        // Basic pattern and path
        cmd.arg(&params.pattern);
        cmd.arg(search_path);

        // Case sensitivity
        if params.case_insensitive.unwrap_or(false) {
            cmd.arg("-i");
        }

        // Output mode
        let output_mode = params
            .output_mode
            .as_deref()
            .unwrap_or("files_with_matches");
        match output_mode {
            "content" => {
                // Default behavior shows content
                if params.line_numbers.unwrap_or(false) {
                    cmd.arg("-n");
                }
                if let Some(before) = params.context_before {
                    cmd.arg(format!("-B{}", before));
                }
                if let Some(after) = params.context_after {
                    cmd.arg(format!("-A{}", after));
                }
                if let Some(around) = params.context_around {
                    cmd.arg(format!("-C{}", around));
                }
            }
            "files_with_matches" => {
                cmd.arg("-l"); // List files with matches
            }
            "count" => {
                cmd.arg("-c"); // Count matches per file
            }
            _ => {
                // Default to files_with_matches
                cmd.arg("-l");
            }
        }

        // File type filtering
        if let Some(file_type) = &params.r#type {
            let extensions = Self::get_file_type_extensions(file_type);
            if !extensions.is_empty() {
                for ext in extensions {
                    cmd.arg("--glob").arg(format!("*.{}", ext));
                }
            }
        }

        // Glob pattern filtering
        if let Some(glob_pattern) = &params.glob {
            cmd.arg("--glob").arg(glob_pattern);
        }

        // Multiline mode
        if params.multiline {
            cmd.arg("-U").arg("--multiline-dotall");
        }

        // Always use --no-heading for consistent parsing
        cmd.arg("--no-heading");

        // Limit results if specified
        if let Some(limit) = params.head_limit {
            cmd.arg("--max-count").arg(limit.to_string());
        }

        cmd
    }

    fn process_grep_output(&self, output: String, params: &GrepParams) -> Result<String> {
        let lines: Vec<&str> = output.lines().collect();

        if lines.is_empty() {
            return Ok(format!("No matches found for pattern: {}", params.pattern));
        }

        // Threshold: 30 lines before truncation
        const TRUNCATE_THRESHOLD: usize = 30;

        let output_mode = params
            .output_mode
            .as_deref()
            .unwrap_or("files_with_matches");

        match output_mode {
            "content" => {
                if lines.len() <= TRUNCATE_THRESHOLD {
                    return Ok(output);
                }

                // Smart truncation for content mode
                let unique_files = self.count_unique_files(&lines);
                let mut result = format!(
                    "Found {} matches across {} files.\n\nFirst {} matches:\n",
                    lines.len(),
                    unique_files,
                    TRUNCATE_THRESHOLD.min(lines.len())
                );

                // Show first matches
                for line in lines.iter().take(TRUNCATE_THRESHOLD) {
                    result.push_str(&format!("{}\n", line));
                }

                if lines.len() > TRUNCATE_THRESHOLD {
                    result.push_str(&format!(
                        "\n... [TRUNCATED {} additional matches] ...\n",
                        lines.len() - TRUNCATE_THRESHOLD
                    ));

                    // Show last few matches for context
                    let last_count = 5.min(lines.len().saturating_sub(TRUNCATE_THRESHOLD));
                    if last_count > 0 {
                        result.push_str("\nLast few matches:\n");
                        for line in lines.iter().skip(lines.len() - last_count) {
                            result.push_str(&format!("{}\n", line));
                        }
                    }
                }

                Ok(result)
            }
            "files_with_matches" => {
                if lines.len() <= 100 {
                    return Ok(format!(
                        "Found matches in {} files:\n\n{}",
                        lines.len(),
                        output
                    ));
                }

                // Truncate file list for large results
                let mut result = format!("Found matches in {} files:\n\n", lines.len());
                result.push_str("First 50 files:\n");
                for line in lines.iter().take(50) {
                    result.push_str(&format!("{}\n", line));
                }

                result.push_str(&format!(
                    "\n... [TRUNCATED {} additional files] ...\n",
                    lines.len() - 50
                ));

                // Show last 10 files
                for line in lines.iter().skip(lines.len() - 10) {
                    result.push_str(&format!("{}\n", line));
                }

                Ok(result)
            }
            "count" => {
                if lines.len() <= 50 {
                    return Ok(format!("Match counts per file:\n\n{}", output));
                }

                // Truncate count output
                let mut result = format!("Match counts for {} files:\n\n", lines.len());
                for line in lines.iter().take(50) {
                    result.push_str(&format!("{}\n", line));
                }

                if lines.len() > 50 {
                    result.push_str(&format!(
                        "\n... [TRUNCATED {} additional files] ...\n",
                        lines.len() - 50
                    ));
                }

                Ok(result)
            }
            _ => Ok(output),
        }
    }

    fn count_unique_files(&self, lines: &[&str]) -> usize {
        let mut files = std::collections::HashSet::new();
        for line in lines {
            if let Some(colon_pos) = line.find(':') {
                files.insert(&line[..colon_pos]);
            }
        }
        files.len()
    }
}

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Intelligent text search with context-aware truncation. Supports regex patterns, file filtering, and multiple output modes."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (defaults to current directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., '*.js', '*.{ts,tsx}')"
                },
                "type": {
                    "type": "string",
                    "description": "File type to search (js, py, rust, go, java, etc.)"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode: 'content' shows matching lines, 'files_with_matches' shows file paths, 'count' shows match counts",
                    "default": "files_with_matches"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "-B": {
                    "type": "number",
                    "description": "Number of lines to show before each match (content mode only)"
                },
                "-A": {
                    "type": "number",
                    "description": "Number of lines to show after each match (content mode only)"
                },
                "-C": {
                    "type": "number",
                    "description": "Number of lines to show before and after each match (content mode only)"
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers in output (content mode only)"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N results"
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where patterns can span lines",
                    "default": false
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let params: GrepParams = serde_json::from_str(arguments)?;

        // Determine search path
        let search_path = match &params.path {
            Some(path) => {
                let path_buf = if Path::new(path).is_absolute() {
                    Path::new(path).to_path_buf()
                } else {
                    std::env::current_dir()?.join(path)
                };
                path_buf
            }
            None => std::env::current_dir()?,
        };

        if !search_path.exists() {
            return Err(anyhow::anyhow!(
                "Search path does not exist: {}",
                search_path.display()
            ));
        }

        // Build and execute ripgrep command
        let mut cmd = self.build_ripgrep_command(&params, &search_path);

        log::debug!("Executing ripgrep command: {:?}", cmd);

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such file or directory") || stderr.contains("no matches found") {
                return Ok(format!("No matches found for pattern: {}", params.pattern));
            }
            return Err(anyhow::anyhow!("Ripgrep failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        // Process output with intelligent truncation
        self.process_grep_output(stdout, &params)
    }
}
