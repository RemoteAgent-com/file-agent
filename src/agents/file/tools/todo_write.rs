use crate::tool::Tool;
use crate::utils;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub content: String,
    pub status: String,   // "pending", "in_progress", "completed"
    pub priority: String, // "high", "medium", "low"
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoWriteArgs {
    pub todos: Vec<TodoItem>,
}

pub struct TodoWriteTool;

impl TodoWriteTool {
    pub fn new() -> Self {
        Self
    }

    /// Store todos in message history for persistence
    fn store_todos(&self, todos: &[TodoItem]) -> Result<()> {
        let todos_json = json!({
            "todos": todos,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "action": "todo_update"
        });

        utils::store_claude_message("todos", &todos_json)?;
        log::info!("Stored {} todos in message history", todos.len());
        Ok(())
    }

    /// Generate todo status summary
    fn generate_summary(&self, todos: &[TodoItem]) -> String {
        let pending = todos.iter().filter(|t| t.status == "pending").count();
        let in_progress = todos.iter().filter(|t| t.status == "in_progress").count();
        let completed = todos.iter().filter(|t| t.status == "completed").count();
        let total = pending + in_progress + completed;

        let mut summary = String::new();

        // Progress indicator
        if total > 1 {
            let progress_percent = if total > 0 {
                (completed * 100) / total
            } else {
                0
            };
            summary.push_str(&format!(
                "Todo Progress: {}/{}  ({}% complete)\n",
                completed, total, progress_percent
            ));

            if in_progress > 0 {
                summary.push_str("Currently working on 1 task\n");
            }
            if pending > 0 {
                summary.push_str(&format!("{} tasks remaining\n", pending));
            }
            summary.push('\n');
        } else if total == 1 {
            summary.push_str("Single todo update\n\n");
        }

        // Add todo details with enhanced formatting
        for (i, todo) in todos.iter().enumerate() {
            let status_icon = match todo.status.as_str() {
                "completed" => "[DONE]",
                "in_progress" => "[ACTIVE]",
                "pending" => "[TODO]",
                _ => "[?]",
            };

            let priority_indicator = match todo.priority.as_str() {
                "high" => " [HIGH]",
                "medium" => " [MED]",
                "low" => " [LOW]",
                _ => "",
            };

            summary.push_str(&format!(
                "{}. {} {}{}\n",
                i + 1,
                status_icon,
                todo.content,
                priority_indicator
            ));
        }

        // Add guidance for next steps
        if todos.len() == 1 {
            let todo = &todos[0];
            match todo.status.as_str() {
                "in_progress" => {
                    summary.push_str(
                        "\nWorking on this task now. Will mark as completed when finished.",
                    );
                }
                "completed" => {
                    summary.push_str("\nTask completed successfully! Moving to next pending task.");
                }
                _ => {}
            }
        }

        summary.push_str("\n\nTodos have been updated and tracked for progress visibility.");
        summary
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Create and manage structured task lists for ANY multi-step tasks. Use for tasks with 2+ steps to provide progress tracking and user visibility. Always maintain single 'in_progress' todo rule."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "Array of todo items to create/update",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "Task description"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Current status of the task"
                            },
                            "priority": {
                                "type": "string",
                                "enum": ["high", "medium", "low"],
                                "description": "Task priority level"
                            },
                            "id": {
                                "type": "string",
                                "description": "Unique identifier for the task"
                            }
                        },
                        "required": ["content", "status", "priority", "id"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["todos"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: &str) -> Result<String> {
        log::info!("TodoWrite tool executing with args: {}", args);

        let parsed_args: TodoWriteArgs = serde_json::from_str(args)
            .map_err(|e| anyhow::anyhow!("Failed to parse TodoWrite arguments: {}", e))?;

        // Validate todo items
        for todo in &parsed_args.todos {
            if todo.content.trim().is_empty() {
                return Err(anyhow::anyhow!("Todo content cannot be empty"));
            }

            if !["pending", "in_progress", "completed"].contains(&todo.status.as_str()) {
                return Err(anyhow::anyhow!(
                    "Invalid todo status: {}. Must be 'pending', 'in_progress', or 'completed'",
                    todo.status
                ));
            }

            if !["high", "medium", "low"].contains(&todo.priority.as_str()) {
                return Err(anyhow::anyhow!(
                    "Invalid todo priority: {}. Must be 'high', 'medium', or 'low'",
                    todo.priority
                ));
            }

            if todo.id.trim().is_empty() {
                return Err(anyhow::anyhow!("Todo ID cannot be empty"));
            }
        }

        // Enforce single "in_progress" rule
        let in_progress_count = parsed_args
            .todos
            .iter()
            .filter(|todo| todo.status == "in_progress")
            .count();

        if in_progress_count > 1 {
            return Err(anyhow::anyhow!(
                "Invalid state: {} todos marked as 'in_progress'. Only ONE todo can be 'in_progress' at a time",
                in_progress_count
            ));
        }

        // Store todos for persistence
        self.store_todos(&parsed_args.todos)?;

        // Generate summary response
        let summary = self.generate_summary(&parsed_args.todos);

        log::info!(
            "TodoWrite completed successfully with {} todos",
            parsed_args.todos.len()
        );
        Ok(summary)
    }
}
