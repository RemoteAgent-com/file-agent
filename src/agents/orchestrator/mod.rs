use crate::agent::{Agent, agents_to_json};
use crate::agents::file::FileAgent;
use anyhow::Result;
use log;
use serde_json::{json, Value};
use std::collections::HashMap;

mod claude;
use claude::OrchestratorClaude;

pub struct OrchestratorAgent {
    claude: OrchestratorClaude,
}

impl OrchestratorAgent {
    pub fn new() -> Result<Self> {
        let claude = OrchestratorClaude::new()?;
        log::info!("Orchestrator initialized");
        Ok(Self { claude })
    }

    fn get_agents(&self) -> Vec<Box<dyn Agent>> {
        vec![Box::new(FileAgent::new().unwrap())]
    }
}

#[async_trait::async_trait]
impl Agent for OrchestratorAgent {
    fn name(&self) -> &str {
        "orchestrator"
    }

    fn description(&self) -> &str {
        "Orchestrates tasks by routing them to appropriate domain agents"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The task to analyze and route to appropriate domain agent"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(&self, task: &str) -> Result<String> {
        log::info!("Orchestrator processing task: {}", task);

        // Get available agents
        let agents = self.get_agents();
        let agent_refs: Vec<&dyn Agent> = agents.iter().map(|a| a.as_ref()).collect();

        // Convert agents to JSON for Claude using helper function
        let agents_json = agents_to_json(&agent_refs);

        // Create agent lookup map
        let agent_map: HashMap<String, &dyn Agent> = agent_refs
            .iter()
            .map(|agent| (agent.name().to_string(), *agent))
            .collect();

        // Use async executor to handle the async call_claude_api
        let response = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.claude
                    .call_claude_api(task, &agents_json, &agent_map)
                    .await
            })
        })?;

        log::info!("Orchestrator task completed");
        Ok(response)
    }
}
