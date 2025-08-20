use anyhow::Result;
use serde_json::Value;

/// Core Tool trait that all tools must implement
/// This is used by all agents across the system
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// Get the name of the tool
    fn name(&self) -> &str;

    /// Get the description of the tool
    fn description(&self) -> &str;

    /// Get the parameters schema for the tool
    fn parameters(&self) -> Value;

    /// Entry point for tool execution with logging
    async fn call(&self, arguments: &str) -> Result<String> {
        log::info!("Tool call start: {} - args: {}", self.name(), arguments);

        let result = self.execute(arguments).await;

        match &result {
            Ok(response) => log::info!(
                "Tool call success: {} - response: {}",
                self.name(),
                response
            ),
            Err(e) => log::error!("Tool call error: {} - error: {}", self.name(), e),
        }

        result
    }

    /// Actual implementation of the tool execution
    async fn execute(&self, arguments: &str) -> Result<String>;
}

/// Helper function to convert tools to JSON format for Claude API
pub fn tools_to_json(tools: &[Box<dyn Tool>]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name(),
                "description": tool.description(),
                "input_schema": tool.parameters()
            })
        })
        .collect()
}
