use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Atomic batch operations with rollback support. Matches Claude Code behavior.
pub struct MultiEditTool;

#[derive(serde::Deserialize)]
struct MultiEditParams {
    file_path: String,
    edits: Vec<EditOperation>,
}

#[derive(serde::Deserialize)]
struct EditOperation {
    old_string: String,
    new_string: String,
    #[serde(default = "default_false")]
    replace_all: bool,
}

fn default_false() -> bool {
    false
}

impl MultiEditTool {
    pub fn new() -> Self {
        Self
    }

    fn validate_edits(&self, edits: &[EditOperation]) -> Result<()> {
        if edits.is_empty() {
            return Err(anyhow::anyhow!("No edits provided"));
        }

        if edits.len() > 50 {
            return Err(anyhow::anyhow!(
                "Too many edits ({}). Maximum 50 edits per operation for safety.",
                edits.len()
            ));
        }

        for (i, edit) in edits.iter().enumerate() {
            if edit.old_string.is_empty() {
                return Err(anyhow::anyhow!(
                    "Edit #{}: old_string cannot be empty",
                    i + 1
                ));
            }

            if edit.old_string == edit.new_string {
                return Err(anyhow::anyhow!(
                    "Edit #{}: old_string and new_string are identical",
                    i + 1
                ));
            }
        }

        // Check for conflicting edits
        for i in 0..edits.len() {
            for j in (i + 1)..edits.len() {
                let edit1 = &edits[i];
                let edit2 = &edits[j];

                // Check if edits overlap or conflict
                if edit1.old_string.contains(&edit2.old_string)
                    || edit2.old_string.contains(&edit1.old_string)
                {
                    log::warn!(
                        "Potential conflict between edit #{} and #{}: overlapping old_strings",
                        i + 1,
                        j + 1
                    );
                }

                if edit1.new_string.contains(&edit2.old_string) {
                    log::warn!(
                        "Potential conflict: edit #{} creates text that edit #{} will modify",
                        i + 1,
                        j + 1
                    );
                }
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
                current_pos = absolute_pos + 1;
            } else {
                break; // No more matches found
            }
        }

        matches
    }

    fn apply_single_edit(&self, content: &str, edit: &EditOperation) -> Result<(String, usize)> {
        let matches = self.find_matches(content, &edit.old_string);

        if matches.is_empty() {
            return Err(anyhow::anyhow!(
                "old_string not found: '{}'",
                edit.old_string
            ));
        }

        if !edit.replace_all && matches.len() > 1 {
            return Err(anyhow::anyhow!(
                "old_string '{}' found {} times. Set replace_all=true or provide more specific context.",
                edit.old_string, matches.len()
            ));
        }

        let replacements_made = if edit.replace_all { matches.len() } else { 1 };

        let new_content = if edit.replace_all {
            content.replace(&edit.old_string, &edit.new_string)
        } else {
            content.replacen(&edit.old_string, &edit.new_string, 1)
        };

        Ok((new_content, replacements_made))
    }

    fn preview_changes(&self, original: &str, result: &str, _edits: &[EditOperation]) -> String {
        let original_lines: Vec<&str> = original.lines().collect();
        let result_lines: Vec<&str> = result.lines().collect();

        let mut preview = String::new();
        preview.push_str("Change Summary:\n");

        // Simple diff preview
        let mut changes_shown = 0;
        const MAX_PREVIEW_CHANGES: usize = 10;

        for (i, (orig_line, new_line)) in original_lines.iter().zip(result_lines.iter()).enumerate()
        {
            if orig_line != new_line && changes_shown < MAX_PREVIEW_CHANGES {
                preview.push_str(&format!("  Line {}:\n", i + 1));
                preview.push_str(&format!("  - {}\n", orig_line));
                preview.push_str(&format!("  + {}\n", new_line));
                preview.push_str("\n");
                changes_shown += 1;
            }
        }

        if changes_shown == MAX_PREVIEW_CHANGES && original_lines.len() > MAX_PREVIEW_CHANGES {
            preview.push_str("  ... (additional changes not shown)\n");
        }

        // Handle length differences
        if original_lines.len() != result_lines.len() {
            let diff = result_lines.len() as i32 - original_lines.len() as i32;
            if diff > 0 {
                preview.push_str(&format!("  {} lines added\n", diff));
            } else {
                preview.push_str(&format!("  {} lines removed\n", -diff));
            }
        }

        preview
    }
}

#[async_trait::async_trait]
impl Tool for MultiEditTool {
    fn name(&self) -> &str {
        "multi_edit"
    }

    fn description(&self) -> &str {
        "Atomic batch operations with rollback support. All edits succeed or all fail. Use for making multiple coordinated changes to a single file."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to modify"
                },
                "edits": {
                    "type": "array",
                    "description": "Array of edit operations to perform sequentially",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_string": {
                                "type": "string",
                                "description": "Text to replace"
                            },
                            "new_string": {
                                "type": "string",
                                "description": "Text to replace it with"
                            },
                            "replace_all": {
                                "type": "boolean",
                                "description": "Replace all occurrences (default: false)",
                                "default": false
                            }
                        },
                        "required": ["old_string", "new_string"]
                    },
                    "minItems": 1,
                    "maxItems": 50
                }
            },
            "required": ["file_path", "edits"]
        })
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let params: MultiEditParams = serde_json::from_str(arguments)?;

        // Validate edits
        self.validate_edits(&params.edits)?;

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

        // Read original content
        let original_content = fs::read_to_string(&file_path)?;

        // Apply all edits sequentially
        let mut current_content = original_content.clone();
        let mut total_replacements = 0;
        let mut edit_results = Vec::new();

        for (i, edit) in params.edits.iter().enumerate() {
            match self.apply_single_edit(&current_content, edit) {
                Ok((new_content, replacements)) => {
                    current_content = new_content;
                    total_replacements += replacements;
                    edit_results.push(format!(
                        "Edit #{}: {} replacement{}",
                        i + 1,
                        replacements,
                        if replacements == 1 { "" } else { "s" }
                    ));
                }
                Err(e) => {
                    // Rollback on any failure
                    log::error!("Multi-edit failed at edit #{}: {}", i + 1, e);
                    return Err(anyhow::anyhow!(
                        "Multi-edit failed at edit #{}: {}\n\
                        All changes have been rolled back. No modifications made to the file.",
                        i + 1,
                        e
                    ));
                }
            }
        }

        // Atomic write - either all changes succeed or none do
        match fs::write(&file_path, &current_content) {
            Ok(_) => {
                let mut result = format!(
                    "Successfully applied {} edits to: {}\n\
                     Total replacements made: {}\n\n",
                    params.edits.len(),
                    file_path.display(),
                    total_replacements
                );

                // Add individual edit results
                result.push_str("Edit Results:\n");
                for edit_result in edit_results {
                    result.push_str(&format!("  • {}\n", edit_result));
                }
                result.push_str("\n");

                // Add change preview
                result.push_str(&self.preview_changes(
                    &original_content,
                    &current_content,
                    &params.edits,
                ));

                // File statistics
                let orig_lines = original_content.lines().count();
                let new_lines = current_content.lines().count();
                let size_change = current_content.len() as i32 - original_content.len() as i32;

                result.push_str(&format!(
                    "File Statistics:\n\
                     - Lines: {} -> {} ({:+})\n\
                     - Size: {} -> {} bytes ({:+})\n",
                    orig_lines,
                    new_lines,
                    new_lines as i32 - orig_lines as i32,
                    original_content.len(),
                    current_content.len(),
                    size_change
                ));

                log::info!(
                    "Multi-edit completed successfully: {} ({} edits, {} replacements)",
                    file_path.display(),
                    params.edits.len(),
                    total_replacements
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
