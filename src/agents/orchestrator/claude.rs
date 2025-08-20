use crate::agent::Agent;
use crate::utils;
use crate::ClaudeConfig;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct OrchestratorClaude {
    config: ClaudeConfig,
}

impl OrchestratorClaude {
    pub fn new() -> Result<Self> {
        let config = ClaudeConfig::new()?;
        Ok(Self { config })
    }

    /// Call Claude API with agent chaining (following ra-core pattern)
    pub async fn call_claude_api(
        &self,
        task: &str,
        agents_json: &[Value],
        agent_map: &HashMap<String, &dyn Agent>,
    ) -> Result<String> {
        let system_prompt =
            "You are a File Operations Orchestrator Agent that routes file-related tasks to the appropriate FileAgent.

Your job is to:
1. Analyze incoming tasks to determine if they are file-related
2. Route file operations to the FileAgent which has comprehensive file management capabilities
3. Coordinate complex multi-step file operations

The FileAgent you can delegate to specializes in:
- File CRUD operations (Create, Read, Update, Delete)
- Directory management and traversal  
- File search and pattern matching
- Content manipulation and transformation
- File metadata operations
- Batch file processing
- Code analysis and refactoring

For any task involving files, directories, code analysis, or file system operations, delegate to the FileAgent.

Examples of file-related tasks:
- 'Find all TypeScript files in the project'
- 'Read the contents of config.json'
- 'List all files in the src directory'
- 'Search for TODO comments in the codebase'
- 'Rename all instances of UserService to AccountService'
- 'Analyze the project structure'
- 'Create a new file with specific content'";

        let mut messages = vec![json!({
            "role": "user",
            "content": task
        })];

        let mut round = 0;
        let max_rounds = 100;
        let mut last_message = String::new();

        loop {
            round += 1;
            if round > max_rounds {
                log::warn!("Orchestrator reached maximum rounds: {}", max_rounds);
                break;
            }

            let request_payload = json!({
                "model": self.config.model,
                "max_tokens": self.config.max_tokens,
                "temperature": self.config.temperature,
                "system": system_prompt,
                "messages": messages,
                "tools": agents_json
            });

            log::debug!("Orchestrator calling Claude API - Round {}", round);

            let response = self
                .config
                .client
                .post(&self.config.api_url)
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&request_payload)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow::anyhow!("Claude API error: {}", error_text));
            }

            let claude_response: Value = response.json().await?;

            // Store the full Claude response
            utils::store_claude_message("orchestrator", &claude_response)?;

            // Check for tool calls in the response
            if let Some(content) = claude_response.get("content") {
                if let Some(array) = content.as_array() {
                    let mut continue_conversation = false;
                    let mut assistant_content_blocks = Vec::new();

                    for content_element in array {
                        assistant_content_blocks.push(content_element.clone());

                        if let Some(content_type) = content_element.get("type") {
                            if content_type.as_str() == Some("tool_use") {
                                if let Some(tool_id) =
                                    content_element.get("id").and_then(|id| id.as_str())
                                {
                                    if let Some(agent_name) =
                                        content_element.get("name").and_then(|name| name.as_str())
                                    {
                                        if let Some(input) = content_element.get("input") {
                                            log::info!(
                                                "Orchestrator calling agent: {}",
                                                agent_name
                                            );

                                            // Call the agent
                                            let agent_result = if let Some(&agent) =
                                                agent_map.get(agent_name)
                                            {
                                                match input.get("task").and_then(|t| t.as_str()) {
                                                    Some(agent_task) => {
                                                        match agent.execute(agent_task).await {
                                                            Ok(result) => {
                                                                log::info!("Agent {} completed successfully", agent_name);
                                                                result
                                                            }
                                                            Err(e) => {
                                                                log::error!(
                                                                    "Agent {} failed: {}",
                                                                    agent_name,
                                                                    e
                                                                );
                                                                format!(
                                                                    "Agent execution failed: {}",
                                                                    e
                                                                )
                                                            }
                                                        }
                                                    }
                                                    None => "No task provided to agent".to_string(),
                                                }
                                            } else {
                                                format!("Agent not found: {}", agent_name)
                                            };

                                            // Add assistant message with tool_use
                                            messages.push(json!({
                                                "role": "assistant",
                                                "content": assistant_content_blocks
                                            }));

                                            // Add agent result as user message
                                            messages.push(json!({
                                                "role": "user",
                                                "content": [{
                                                    "type": "tool_result",
                                                    "tool_use_id": tool_id,
                                                    "content": agent_result
                                                }]
                                            }));

                                            continue_conversation = true;
                                        }
                                    }
                                }
                            } else if content_type.as_str() == Some("text") {
                                if let Some(text) = content_element.get("text") {
                                    last_message = text.as_str().unwrap_or("").to_string();
                                }
                            }
                        }
                    }

                    if continue_conversation {
                        continue;
                    }

                    // No tool calls, conversation complete
                    break;
                }
            }
        }

        if last_message.trim().is_empty() {
            Ok("Task completed successfully".to_string())
        } else {
            Ok(last_message)
        }
    }
}
