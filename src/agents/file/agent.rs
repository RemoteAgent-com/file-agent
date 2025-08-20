use crate::agent::Agent;
use crate::tool::Tool;
use super::claude::FileAgentClaude;
use super::tools::{LsTool, GlobTool, FindTool, GrepTool, TodoWriteTool, ReadTool, WriteTool, EditTool, MultiEditTool, BashTool};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Single unified file operations agent
pub struct FileAgent {
    claude: FileAgentClaude,
    tools: HashMap<String, Box<dyn Tool>>,
}


impl FileAgent {
    pub fn new() -> Result<Self> {
        let claude = FileAgentClaude::new()?;
        
        // Initialize all file tools (all tools in one agent)
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();
        
        // Discovery tools
        tools.insert("ls".to_string(), Box::new(LsTool::new()));
        tools.insert("glob".to_string(), Box::new(GlobTool::new()));
        tools.insert("find".to_string(), Box::new(FindTool::new()));
        
        // Search tools
        tools.insert("grep".to_string(), Box::new(GrepTool::new()));
        tools.insert("todo_write".to_string(), Box::new(TodoWriteTool::new()));
        
        // Modification tools
        tools.insert("read".to_string(), Box::new(ReadTool::new()));
        tools.insert("write".to_string(), Box::new(WriteTool::new()));
        tools.insert("edit".to_string(), Box::new(EditTool::new()));
        tools.insert("multi_edit".to_string(), Box::new(MultiEditTool::new()));
        
        // Operations tools
        tools.insert("bash".to_string(), Box::new(BashTool::new()));
        
        Ok(Self {
            claude,
            tools,
        })
    }

}

#[async_trait::async_trait]
impl Agent for FileAgent {
    fn name(&self) -> &str {
        "file_agent"
    }

    fn description(&self) -> &str {
        "Single unified file operations agent. Handles all file management tasks including discovery, search, modification, analysis, and operations."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "File operation task to execute"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(&self, task: &str) -> Result<String> {
        log::info!("FileAgent executing task: {}", task);
        
        // Delegate to Claude handler following orchestrator pattern
        self.claude.execute_task(task, &self.tools).await
    }
}