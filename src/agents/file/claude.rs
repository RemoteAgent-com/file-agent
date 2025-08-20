use crate::tool::Tool;
use crate::ClaudeConfig;
use crate::utils;
use super::context_manager::{ContextManager, ProcessedResults};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use futures::future::join_all;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments: String,
    pub tool_use_id: String,
}

pub struct FileAgentClaude {
    config: ClaudeConfig,
    context_manager: ContextManager,
}

impl FileAgentClaude {
    pub fn new() -> Result<Self> {
        let config = ClaudeConfig::new()?;
        let context_manager = ContextManager::new();
        
        Ok(Self {
            config,
            context_manager,
        })
    }

    /// Execute tools in parallel with error handling
    pub async fn execute_tools_parallel(&self, tool_calls: Vec<ToolCall>, tools: &HashMap<String, Box<dyn Tool>>) -> Result<Vec<(String, String, String)>> {
        log::info!("Executing {} tools in parallel", tool_calls.len());
        
        // Execute multiple tools simultaneously for efficiency
        let futures: Vec<_> = tool_calls.into_iter()
            .map(|call| self.execute_single_tool_with_id(call, tools))
            .collect();

        let results = join_all(futures).await;

        // Collect results and handle errors, preserving order
        let mut tool_results = Vec::new();
        for result in results.into_iter() {
            match result {
                Ok((tool_use_id, tool_name, output)) => {
                    if output.starts_with("Tool execution failed:") || output.starts_with("Tool not found:") {
                        log::error!("Tool {} had error: {}", tool_name, output);
                    } else {
                        log::info!("Tool {} completed successfully", tool_name);
                    }
                    tool_results.push((tool_use_id, tool_name, output));
                }
                Err(e) => {
                    // This should rarely happen now since execute_single_tool handles errors
                    log::error!("Unexpected tool error: {}", e);
                }
            }
        }

        Ok(tool_results)
    }

    /// Execute a single tool call with tool_use_id preservation
    async fn execute_single_tool_with_id(&self, call: ToolCall, tools: &HashMap<String, Box<dyn Tool>>) -> Result<(String, String, String)> {
        if let Some(tool) = tools.get(&call.name) {
            match tool.execute(&call.arguments).await {
                Ok(result) => {
                    // Store tool-specific message using tool name
                    let tool_message = json!({
                        "tool": call.name,
                        "arguments": call.arguments,
                        "result": result.clone(),
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    });
                    utils::store_claude_message(&call.name, &tool_message)?;
                    
                    Ok((call.tool_use_id.clone(), call.name.clone(), result))
                },
                Err(e) => Ok((call.tool_use_id.clone(), call.name.clone(), format!("Tool execution failed: {}", e))),
            }
        } else {
            Ok((call.tool_use_id.clone(), call.name.clone(), format!("Tool not found: {}", call.name)))
        }
    }


    /// Execute tools with context management and result processing
    async fn execute_tools_with_context_management(&self, tools: Vec<ToolCall>, tool_map: &HashMap<String, Box<dyn Tool>>) -> Result<ProcessedResults> {
        // Execute tools in parallel for efficiency
        let raw_results = self.execute_tools_parallel(tools, tool_map).await?;
        
        // Process results locally to manage context window
        let processed_results = self.context_manager.process_results_locally(raw_results)?;
        
        Ok(processed_results)
    }

    /// Execute task with file management capabilities
    pub async fn execute_task(&self, task: &str, tools: &HashMap<String, Box<dyn Tool>>) -> Result<String> {
        // Send task to Claude API with file tools available
        self.call_claude_api(task, tools).await
    }

    /// Call Claude API with file management capabilities
    pub async fn call_claude_api(&self, task: &str, tools: &HashMap<String, Box<dyn Tool>>) -> Result<String> {
        let system_prompt = r#"
You are a sophisticated file operations agent with comprehensive file management capabilities.

Use the provided tools to complete file-related tasks efficiently and safely. Always read existing files before modifying them to understand their current state.

When working with multiple related operations, use the todo_write tool to break down the task into manageable steps and track your progress. Only mark one todo as "in_progress" at a time.

Choose the most appropriate tools for each operation and execute them in parallel when operations are independent of each other.
"#;

        let tools_json: Vec<Value> = tools.values()
            .map(|tool| {
                json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "input_schema": tool.parameters()
                })
            })
            .collect();

        let mut messages = vec![json!({
            "role": "user",
            "content": task
        })];

        let mut round = 0;
        let max_rounds = 100;

        loop {
            round += 1;
            if round > max_rounds {
                log::warn!("FileAgent reached maximum rounds: {}", max_rounds);
                break;
            }

            let request_payload = json!({
                "model": self.config.model,
                "max_tokens": self.config.max_tokens,
                "temperature": self.config.temperature,
                "system": system_prompt,
                "messages": messages,
                "tools": tools_json
            });

            log::debug!("FileAgent calling Claude API - Round {}", round);

            let response = self.config.client
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

            // Store the Claude response for debugging/analysis
            utils::store_claude_message("file_agent", &claude_response)?;

            // Process tool calls if any
            if let Some(content) = claude_response.get("content") {
                if let Some(array) = content.as_array() {
                    let mut tool_calls = Vec::new();
                    let mut text_response = String::new();

                    for content_element in array {
                        if let Some(content_type) = content_element.get("type") {
                            if content_type.as_str() == Some("tool_use") {
                                if let Some(tool_name) = content_element.get("name").and_then(|n| n.as_str()) {
                                    if let Some(input) = content_element.get("input") {
                                        if let Some(tool_use_id) = content_element.get("id").and_then(|id| id.as_str()) {
                                            tool_calls.push(ToolCall {
                                                name: tool_name.to_string(),
                                                arguments: input.to_string(),
                                                tool_use_id: tool_use_id.to_string(),
                                            });
                                        }
                                    }
                                }
                            } else if content_type.as_str() == Some("text") {
                                if let Some(text) = content_element.get("text").and_then(|t| t.as_str()) {
                                    text_response = text.to_string();
                                }
                            }
                        }
                    }

                    if !tool_calls.is_empty() {
                        // Execute tools with context management
                        let processed_results = self.execute_tools_with_context_management(tool_calls.clone(), tools).await?;
                        
                        // Add assistant message
                        messages.push(json!({
                            "role": "assistant", 
                            "content": content
                        }));

                        // Add tool results as user messages
                        for (tool_use_id, _tool_name, result) in processed_results.results.into_iter() {
                            messages.push(json!({
                                "role": "user",
                                "content": [{
                                    "type": "tool_result",
                                    "tool_use_id": tool_use_id,
                                    "content": self.context_manager.truncate_content(&result)
                                }]
                            }));
                        }

                        continue;
                    } else if !text_response.is_empty() {
                        return Ok(text_response);
                    }
                }
            }

            break;
        }

        Ok("Task completed successfully".to_string())
    }
}