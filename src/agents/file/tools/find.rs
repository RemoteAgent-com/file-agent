use crate::tool::Tool;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindArgs {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub file_type: Option<String>, // "file", "dir", "symlink"
    #[serde(default)]
    pub size: Option<String>, // "+1M", "-100K", "50K"
    #[serde(default)]
    pub modified: Option<String>, // "+7d", "-24h", "-30m"
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    1000
}

pub struct FindTool;

impl FindTool {
    pub fn new() -> Self {
        Self
    }

    /// Parse size filter (e.g., "+1M", "-100K", "50K")
    fn parse_size_filter(&self, size_str: &str) -> Result<(char, u64)> {
        let size_str = size_str.trim();
        let (op, size_part) = if size_str.starts_with('+') {
            ('+', &size_str[1..])
        } else if size_str.starts_with('-') {
            ('-', &size_str[1..])
        } else {
            ('=', size_str)
        };

        let multiplier = match size_part.chars().last() {
            Some('K') | Some('k') => 1024,
            Some('M') | Some('m') => 1024 * 1024,
            Some('G') | Some('g') => 1024 * 1024 * 1024,
            _ => 1,
        };

        let numeric_part = if multiplier > 1 {
            &size_part[..size_part.len() - 1]
        } else {
            size_part
        };

        let size_bytes = numeric_part
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("Invalid size format: {}", size_str))?
            * multiplier;

        Ok((op, size_bytes))
    }

    /// Parse time filter (e.g., "+7d", "-24h", "-30m")
    fn parse_time_filter(&self, time_str: &str) -> Result<(char, Duration)> {
        let time_str = time_str.trim();
        let (op, time_part) = if time_str.starts_with('+') {
            ('+', &time_str[1..])
        } else if time_str.starts_with('-') {
            ('-', &time_str[1..])
        } else {
            return Err(anyhow::anyhow!("Time filter must start with + or -"));
        };

        let unit = time_part
            .chars()
            .last()
            .ok_or_else(|| anyhow::anyhow!("Invalid time format"))?;

        let numeric_part = &time_part[..time_part.len() - 1];
        let value = numeric_part
            .parse::<i64>()
            .map_err(|_| anyhow::anyhow!("Invalid time value: {}", time_str))?;

        let duration = match unit {
            'm' => Duration::minutes(value),
            'h' => Duration::hours(value),
            'd' => Duration::days(value),
            'w' => Duration::weeks(value),
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid time unit: {}. Use m, h, d, or w",
                    unit
                ))
            }
        };

        Ok((op, duration))
    }

    /// Check if entry matches all filters
    fn matches_filters(&self, entry: &DirEntry, args: &FindArgs) -> Result<bool> {
        let metadata = entry
            .metadata()
            .map_err(|e| anyhow::anyhow!("Failed to read metadata: {}", e))?;

        // Type filter
        if let Some(file_type) = &args.file_type {
            let matches_type = match file_type.as_str() {
                "file" => metadata.is_file(),
                "dir" => metadata.is_dir(),
                "symlink" => metadata.file_type().is_symlink(),
                _ => return Err(anyhow::anyhow!("Invalid file type: {}", file_type)),
            };
            if !matches_type {
                return Ok(false);
            }
        }

        // Name filter (exact match or pattern)
        if let Some(name_filter) = &args.name {
            let file_name = entry.file_name().to_string_lossy();
            let matches = if args.case_sensitive {
                file_name.contains(name_filter)
            } else {
                file_name
                    .to_lowercase()
                    .contains(&name_filter.to_lowercase())
            };
            if !matches {
                return Ok(false);
            }
        }

        // Regex pattern filter
        if let Some(pattern) = &args.pattern {
            let file_path = entry.path().to_string_lossy();
            let regex = if args.case_sensitive {
                Regex::new(pattern)
            } else {
                Regex::new(&format!("(?i){}", pattern))
            }
            .map_err(|e| anyhow::anyhow!("Invalid regex pattern: {}", e))?;

            if !regex.is_match(&file_path) {
                return Ok(false);
            }
        }

        // Size filter
        if let Some(size_filter) = &args.size {
            let (op, filter_size) = self.parse_size_filter(size_filter)?;
            let file_size = metadata.len();

            let matches = match op {
                '+' => file_size > filter_size,
                '-' => file_size < filter_size,
                _ => file_size == filter_size,
            };
            if !matches {
                return Ok(false);
            }
        }

        // Modified time filter
        if let Some(modified_filter) = &args.modified {
            let (op, duration) = self.parse_time_filter(modified_filter)?;
            let modified_time = metadata
                .modified()
                .map_err(|e| anyhow::anyhow!("Failed to read modified time: {}", e))?;

            let modified_datetime: DateTime<Utc> = modified_time.into();
            let now = Utc::now();
            let threshold = if op == '-' {
                now - duration
            } else {
                now - duration
            };

            let matches = if op == '-' {
                modified_datetime > threshold // Modified within the last duration
            } else {
                modified_datetime < threshold // Modified before the duration ago
            };
            if !matches {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Get file info for display
    fn format_entry(&self, entry: &DirEntry) -> Result<String> {
        let path = entry.path();
        let metadata = entry.metadata()?;

        let file_type = if metadata.is_dir() {
            "[DIR]"
        } else if metadata.is_file() {
            "[FILE]"
        } else if metadata.file_type().is_symlink() {
            "[LINK]"
        } else {
            "[OTHER]"
        };

        let size = if metadata.is_file() {
            format!("{:>10}", self.format_size(metadata.len()))
        } else {
            format!("{:>10}", "-")
        };

        let modified = metadata
            .modified()
            .map(|t| {
                let datetime: DateTime<Utc> = t.into();
                datetime.format("%Y-%m-%d %H:%M").to_string()
            })
            .unwrap_or_else(|_| "Unknown".to_string());

        Ok(format!(
            "{} {} {} {}",
            file_type,
            size,
            modified,
            path.display()
        ))
    }

    /// Format file size for display
    fn format_size(&self, size: u64) -> String {
        const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
        let mut size = size as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        if unit_idx == 0 {
            format!("{:.0}{}", size, UNITS[unit_idx])
        } else {
            format!("{:.1}{}", size, UNITS[unit_idx])
        }
    }
}

#[async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str {
        "find"
    }

    fn description(&self) -> &str {
        "Advanced file search with filters for name, type, size, modification time, and regex patterns. More powerful than basic glob."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Starting directory path for search"
                },
                "name": {
                    "type": "string",
                    "description": "Filter by file name (partial match)"
                },
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to match against full path"
                },
                "file_type": {
                    "type": "string",
                    "enum": ["file", "dir", "symlink"],
                    "description": "Filter by file type"
                },
                "size": {
                    "type": "string",
                    "description": "Filter by size: +1M (larger than 1MB), -100K (smaller than 100KB), 50K (exactly 50KB)"
                },
                "modified": {
                    "type": "string",
                    "description": "Filter by modification time: -24h (last 24 hours), +7d (older than 7 days)"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum directory depth to search"
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Case sensitive name/pattern matching (default: false)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 1000)"
                }
            },
            "required": ["path"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: &str) -> Result<String> {
        log::info!("Find tool executing with args: {}", args);

        let parsed_args: FindArgs = serde_json::from_str(args)
            .map_err(|e| anyhow::anyhow!("Failed to parse find arguments: {}", e))?;

        // Validate path
        let search_path = Path::new(&parsed_args.path);
        if !search_path.exists() {
            return Err(anyhow::anyhow!("Path does not exist: {}", parsed_args.path));
        }

        // Build walker with max depth
        let mut walker = WalkDir::new(search_path);
        if let Some(max_depth) = parsed_args.max_depth {
            walker = walker.max_depth(max_depth);
        }

        // Collect matching entries
        let mut results = Vec::new();
        let mut total_checked = 0;
        let mut errors = Vec::new();

        for entry in walker {
            match entry {
                Ok(entry) => {
                    total_checked += 1;

                    match self.matches_filters(&entry, &parsed_args) {
                        Ok(true) => {
                            if let Ok(formatted) = self.format_entry(&entry) {
                                results.push(formatted);
                                if results.len() >= parsed_args.limit {
                                    break;
                                }
                            }
                        }
                        Ok(false) => continue,
                        Err(e) => errors.push(format!("Filter error: {}", e)),
                    }
                }
                Err(e) => errors.push(format!("Walk error: {}", e)),
            }
        }

        // Build summary
        let mut summary = format!(
            "Found {} matches (checked {} items, limit: {})\n\n",
            results.len(),
            total_checked,
            parsed_args.limit
        );

        if results.is_empty() {
            summary.push_str("No files found matching the specified criteria.");
        } else {
            summary.push_str(&results.join("\n"));

            if results.len() >= parsed_args.limit {
                summary.push_str(&format!(
                    "\n\n... Results limited to {} items",
                    parsed_args.limit
                ));
            }
        }

        if !errors.is_empty() {
            summary.push_str(&format!("\n\nErrors encountered:\n{}", errors.join("\n")));
        }

        log::info!("Find completed: {} matches found", results.len());
        Ok(summary)
    }
}
