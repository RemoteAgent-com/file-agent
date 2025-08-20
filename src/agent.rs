use anyhow::Result;
use serde_json::Value;

/// Core trait that all agents must implement
/// Agents can delegate to sub-agents or execute tools
#[async_trait::async_trait]
pub trait Agent: Send + Sync {
    /// Get the name of the agent
    fn name(&self) -> &str;

    /// Get the description of what this agent handles
    fn description(&self) -> &str;

    /// Get the input schema for this agent (like tools have parameters)
    fn input_schema(&self) -> Value;

    /// Entry point for agent execution with logging
    async fn call(&self, task: &str) -> Result<String> {
        log::info!("Agent call start: {} - task: {}", self.name(), task);

        let result = self.execute(task).await;

        match &result {
            Ok(response) => log::info!(
                "Agent call success: {} - response: {}",
                self.name(), response
            ),
            Err(e) => log::error!("Agent call error: {} - error: {}", self.name(), e),
        }

        result
    }

    /// Actual implementation of the agent execution
    /// This is where agent chaining logic or tool execution happens
    async fn execute(&self, task: &str) -> Result<String>;
}

/// Helper function to convert agents to JSON format for Claude API (like tools_to_json)
pub fn agents_to_json(agents: &[&dyn Agent]) -> Vec<Value> {
    agents
        .iter()
        .map(|agent| {
            serde_json::json!({
                "name": agent.name(),
                "description": agent.description(),
                "input_schema": agent.input_schema()
            })
        })
        .collect()
}
