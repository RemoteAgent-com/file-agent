use anyhow::Result;

/// Context thresholds for efficient result processing
pub const GREP_TRUNCATE_THRESHOLD: usize = 30;
pub const READ_TRUNCATE_THRESHOLD: usize = 2000;
pub const LINE_CHAR_LIMIT: usize = 2000;
pub const TOOL_OUTPUT_LIMIT: usize = 30000;

/// Manages context window optimization for file operations
pub struct ContextManager;

#[derive(Debug)]
pub struct ProcessedResults {
    pub results: Vec<(String, String, String)>, // (tool_use_id, tool_name, result)
}

impl ProcessedResults {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    pub fn push(&mut self, tool_use_id: String, tool_name: String, result: String) {
        self.results.push((tool_use_id, tool_name, result));
    }

    pub fn get_by_name(&self, tool_name: &str) -> Option<&String> {
        self.results.iter()
            .find(|(_, name, _)| name == tool_name)
            .map(|(_, _, result)| result)
    }
}

impl ContextManager {
    pub fn new() -> Self {
        Self
    }

    /// Process tool results locally to manage context window
    pub fn process_results_locally(&self, results: Vec<(String, String, String)>) -> Result<ProcessedResults> {
        let mut processed = ProcessedResults::new();

        for (tool_use_id, tool_name, result) in results {
            let processed_result = match tool_name.as_str() {
                "grep" => self.process_grep_result(result)?,
                "read" => self.process_read_result(result)?,
                "ls" => self.process_ls_result(result)?,
                "glob" => self.process_glob_result(result)?,
                _ => result, // Pass through for smaller results
            };
            processed.push(tool_use_id, tool_name, processed_result);
        }

        Ok(processed)
    }

    /// Smart truncation for grep results with context preservation
    fn process_grep_result(&self, result: String) -> Result<String> {
        let lines: Vec<&str> = result.lines().collect();

        if lines.len() <= GREP_TRUNCATE_THRESHOLD {
            return Ok(result); // Small enough, return as-is
        }

        // Smart truncation with context preservation
        let file_count = self.count_unique_files(&lines);
        let first_matches = lines.iter().take(50).cloned().collect::<Vec<_>>().join("\n");
        let last_matches = if lines.len() > 10 {
            lines.iter().skip(lines.len() - 10).cloned().collect::<Vec<_>>().join("\n")
        } else {
            lines.join("\n")
        };

        let summary = format!(
            "Found {} matches across {} files.\n\nFirst 50 matches:\n{}\n\n... [TRUNCATED] ...\n\nLast 10 matches:\n{}",
            lines.len(),
            file_count,
            first_matches,
            last_matches
        );

        Ok(summary)
    }

    /// Auto-sampling for large files with intelligent sectioning
    fn process_read_result(&self, result: String) -> Result<String> {
        let lines: Vec<&str> = result.lines().collect();

        if lines.len() <= READ_TRUNCATE_THRESHOLD {
            return Ok(result); // Threshold: 2000 lines
        }

        // Auto-sampling for large files with beginning, middle, and end sections
        let beginning = lines.iter().take(50).cloned().collect::<Vec<_>>().join("\n");
        let middle_start = lines.len() / 2;
        let middle = lines.iter()
            .skip(middle_start.saturating_sub(25))
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let ending = lines.iter()
            .skip(lines.len().saturating_sub(50))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");

        let summary = format!(
            "=== FILE PREVIEW (Large file: {} lines) ===\n\nBEGINNING (50 lines):\n{}\n\nMIDDLE (around line {}):\n{}\n\nEND (last 50 lines):\n{}",
            lines.len(),
            beginning,
            middle_start,
            middle,
            ending
        );

        Ok(summary)
    }

    /// Process ls results for large directories
    fn process_ls_result(&self, result: String) -> Result<String> {
        let lines: Vec<&str> = result.lines().collect();
        
        if lines.len() <= 100 {
            return Ok(result);
        }

        // Summarize large directory listings
        let summary = format!(
            "Large directory listing ({} items):\n\nFirst 50 items:\n{}\n\n... [TRUNCATED] ...\n\nLast 10 items:\n{}",
            lines.len(),
            lines.iter().take(50).cloned().collect::<Vec<_>>().join("\n"),
            lines.iter().skip(lines.len() - 10).cloned().collect::<Vec<_>>().join("\n")
        );

        Ok(summary)
    }

    /// Process glob results for large file lists
    fn process_glob_result(&self, result: String) -> Result<String> {
        let lines: Vec<&str> = result.lines().collect();
        
        if lines.len() <= 100 {
            return Ok(result);
        }

        // Summarize large glob results
        let summary = format!(
            "Found {} matching files:\n\nFirst 50 files:\n{}\n\n... [TRUNCATED] ...\n\nLast 10 files:\n{}",
            lines.len(),
            lines.iter().take(50).cloned().collect::<Vec<_>>().join("\n"),
            lines.iter().skip(lines.len() - 10).cloned().collect::<Vec<_>>().join("\n")
        );

        Ok(summary)
    }

    /// Count unique files in grep output (helper function)
    fn count_unique_files(&self, lines: &[&str]) -> usize {
        let mut files = std::collections::HashSet::new();
        for line in lines {
            if let Some(colon_pos) = line.find(':') {
                let file_path = &line[..colon_pos];
                files.insert(file_path);
            }
        }
        files.len()
    }

    /// Truncate content to stay within limits
    pub fn truncate_content(&self, content: &str) -> String {
        if content.len() <= TOOL_OUTPUT_LIMIT {
            content.to_string()
        } else {
            format!("{}... [TRUNCATED - {} total chars]", 
                   &content[..TOOL_OUTPUT_LIMIT], 
                   content.len())
        }
    }
}