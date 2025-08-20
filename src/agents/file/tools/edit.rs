use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Precise string replacement with context verification
pub struct EditTool;

#[derive(serde::Deserialize)]
struct EditParams {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default = "default_false")]
    replace_all: bool,
}

fn default_false() -> bool {
    false
}

impl EditTool {
    pub fn new() -> Self {
        Self
    }

    fn validate_edit_params(&self, params: &EditParams) -> Result<()> {
        if params.old_string.is_empty() {
            return Err(anyhow::anyhow!("old_string cannot be empty"));
        }

        if params.old_string == params.new_string {
            return Err(anyhow::anyhow!(
                "old_string and new_string are identical - no changes needed"
            ));
        }

        // Warn about potentially dangerous replacements
        let dangerous_patterns = [
            "import ",
            "use ",
            "fn main",
            "class ",
            "def __init__",
            "package ",
            "module.exports",
            "export default",
        ];

        for pattern in &dangerous_patterns {
            if params.old_string.contains(pattern) || params.new_string.contains(pattern) {
                log::warn!("Edit affects important code structure: {}", pattern);
            }
        }

        Ok(())
    }

    fn find_matches(&self, content: &str, search_string: &str) -> Vec<(usize, usize, usize)> {
        let mut matches = Vec::new();
        let mut current_pos = 0;
        let mut line_num = 1;
        let mut line_start = 0;

        while current_pos < content.len() {
            if let Some(pos) = content[current_pos..].find(search_string) {
                let absolute_pos = current_pos + pos;

                // Count lines up to this position
                while line_start <= absolute_pos && line_start < content.len() {
                    if let Some(newline_pos) = content[line_start..].find('\n') {
                        let abs_newline = line_start + newline_pos;
                        if abs_newline < absolute_pos {
                            line_num += 1;
                            line_start = abs_newline + 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                matches.push((absolute_pos, absolute_pos + search_string.len(), line_num));
                current_pos = absolute_pos + 1; // Move past this match for next search
            } else {
                break; // No more matches found
            }
        }

        matches
    }

    fn get_context_around_match(
        &self,
        content: &str,
        start: usize,
        _end: usize,
        context_lines: usize,
    ) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let match_line = content[..start].matches('\n').count();

        let start_line = match_line.saturating_sub(context_lines);
        let end_line = (match_line + context_lines + 1).min(lines.len());

        let mut result = String::new();
        for (i, line) in lines[start_line..end_line].iter().enumerate() {
            let line_num = start_line + i + 1;
            let marker = if start_line + i == match_line {
                "→"
            } else {
                " "
            };
            result.push_str(&format!("{:4}{} {}\n", line_num, marker, line));
        }

        result
    }

    fn validate_result(
        &self,
        original: &str,
        result: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<String> {
        let original_matches = self.find_matches(original, old_string);
        let result_matches = self.find_matches(result, old_string);

        if replace_all {
            if !result_matches.is_empty() {
                return Err(anyhow::anyhow!(
                    "replace_all failed: {} instances of old_string still remain",
                    result_matches.len()
                ));
            }
        } else {
            if original_matches.len() == result_matches.len() {
                return Err(anyhow::anyhow!(
                    "No replacements were made - old_string not found or already replaced"
                ));
            }
            if result_matches.len() != original_matches.len() - 1 {
                return Err(anyhow::anyhow!("Unexpected number of replacements made"));
            }
        }

        // Check that new_string was added
        let new_matches = self.find_matches(result, new_string);
        if new_matches.is_empty() && !new_string.is_empty() {
            log::warn!(
                "new_string not found in result - this might be intentional if new_string is empty"
            );
        }

        Ok(result.to_string())
    }
}

#[async_trait::async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Precise string replacement with context verification. Always use Read tool first to understand file contents."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact text to replace (must be unique in file unless replace_all=true)"
                },
                "new_string": {
                    "type": "string",
                    "description": "Text to replace it with"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences of old_string (default: false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let params: EditParams = serde_json::from_str(arguments)?;

        // Validate parameters
        self.validate_edit_params(&params)?;

        // Handle both absolute and relative paths
        let file_path = if Path::new(&params.file_path).is_absolute() {
            Path::new(&params.file_path).to_path_buf()
        } else {
            std::env::current_dir()?.join(&params.file_path)
        };

        if !file_path.exists() {
            return Err(anyhow::anyhow!(
                "File does not exist: {}. Use write tool to create new files.",
                params.file_path
            ));
        }

        if !file_path.is_file() {
            return Err(anyhow::anyhow!("Path is not a file: {}", params.file_path));
        }

        // Read current content
        let original_content = fs::read_to_string(&file_path)?;

        // Find matches
        let matches = self.find_matches(&original_content, &params.old_string);

        if matches.is_empty() {
            return Err(anyhow::anyhow!(
                "old_string not found in file: '{}'\n\
                Make sure the string matches exactly, including whitespace and indentation.",
                params.old_string
            ));
        }

        if !params.replace_all && matches.len() > 1 {
            let mut error_msg = format!(
                "old_string '{}' found {} times in file. Either:\n\
                1. Provide more context to make it unique, or\n\
                2. Set replace_all=true to replace all occurrences\n\n\
                Found at lines: ",
                params.old_string,
                matches.len()
            );

            for (i, (_start, _end, line_num)) in matches.iter().enumerate() {
                if i > 0 {
                    error_msg.push_str(", ");
                }
                error_msg.push_str(&line_num.to_string());
            }

            return Err(anyhow::anyhow!(error_msg));
        }

        // Perform replacement
        let new_content = if params.replace_all {
            original_content.replace(&params.old_string, &params.new_string)
        } else {
            original_content.replacen(&params.old_string, &params.new_string, 1)
        };

        // Validate the result
        let validated_content = self.validate_result(
            &original_content,
            &new_content,
            &params.old_string,
            &params.new_string,
            params.replace_all,
        )?;

        // Write the modified content
        match fs::write(&file_path, &validated_content) {
            Ok(_) => {
                let replacements_made = if params.replace_all { matches.len() } else { 1 };

                let mut result = format!(
                    "Successfully edited file: {}\n\
                     Made {} replacement{}\n\n",
                    file_path.display(),
                    replacements_made,
                    if replacements_made == 1 { "" } else { "s" }
                );

                // Show context around the first change
                if let Some((_start, _end, line_num)) = matches.first() {
                    result.push_str(&format!("Change made around line {}:\n", line_num));
                    let context = self.get_context_around_match(
                        &original_content,
                        matches[0].0,
                        matches[0].1,
                        2,
                    );
                    result.push_str(&context);

                    result.push_str("\nPreview of change:\n");
                    result.push_str(&format!("- {}\n", params.old_string.replace('\n', "\\n")));
                    result.push_str(&format!("+ {}\n", params.new_string.replace('\n', "\\n")));
                }

                log::info!(
                    "File edited successfully: {} ({} replacements)",
                    file_path.display(),
                    replacements_made
                );
                Ok(result)
            }
            Err(e) => Err(anyhow::anyhow!(
                "Failed to write modified file {}: {}",
                file_path.display(),
                e
            )),
        }
    }
}
