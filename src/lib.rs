use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use std::env;

pub mod agent;
pub mod agents;
pub mod tool;
pub mod utils;

#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    pub api_key: String,
    pub api_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub timeout_seconds: u64,
    pub client: Client,
}

impl ClaudeConfig {
    pub fn new() -> Result<Self> {
        let api_key = env::var("CLAUDE_API_KEY")
            .map_err(|_| anyhow::anyhow!("CLAUDE_API_KEY environment variable not set"))?;
        let api_url = env::var("CLAUDE_API_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string());
        let model =
            env::var("CLAUDE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());
        let max_tokens = env::var("CLAUDE_MAX_TOKENS")
            .unwrap_or_else(|_| "8192".to_string())
            .parse()
            .unwrap_or(8192);
        let temperature = env::var("CLAUDE_TEMPERATURE")
            .unwrap_or_else(|_| "0.7".to_string())
            .parse()
            .unwrap_or(0.7);
        let timeout_seconds = env::var("CLAUDE_TIMEOUT")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .unwrap_or(300);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_seconds))
            .build()?;

        Ok(Self {
            api_key,
            api_url,
            model,
            max_tokens,
            temperature,
            timeout_seconds,
            client,
        })
    }
}

// Re-export main agents for external use
use agent::Agent;
pub use agents::orchestrator::OrchestratorAgent;

/// Main handler function called by Raworc (sync wrapper)
pub fn process_message_sync(message: &str, context: &Value) -> String {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(process_message(message, context))
}

/// Process message handler for raworc integration
async fn process_message(task: &str, context: &Value) -> String {
    // Clear bin directory and reset counter for new request
    if let Err(e) = utils::clear_bin_directory() {
        log::warn!("Failed to clear bin directory: {}", e);
    }

    // Initialize orchestrator and process task
    let orchestrator = OrchestratorAgent::new().unwrap();

    match orchestrator.call(task).await {
        Ok(result) => {
            log::info!("Task completed successfully");
            result.to_string()
        }
        Err(e) => {
            log::error!("Task failed: {}", e);
            e.to_string()
        }
    }
}
