# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a sophisticated file operations agent system written in Rust that provides intelligent file management capabilities through a unified agent architecture. The system implements a simplified version of Claude Code's file management approach, with a focus on clarity and maintainability.

## Key Commands

### Running the System
```bash
# Run with a task message
cargo run -- -m "your file operation task"

# Build the project
cargo build

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- -m "your task"
```

### Environment Setup
- Copy `dev.env` to `.env` and update with your credentials
- Set `CLAUDE_API_KEY` environment variable

## Current Architecture

### Core Components

1. **Agent System** (`src/agent.rs`): Core trait defining the agent interface with `execute()` method
2. **Orchestrator Agent** (`src/agents/orchestrator/`): Domain-agnostic task routing and Claude API integration
3. **File Agent** (`src/agents/file/`): Unified file operations agent with all tools
4. **Tool System** (`src/tool.rs`): Core trait for executable tools with JSON schemas

### Agent Hierarchy (Simplified Design)
```
OrchestratorAgent (domain-agnostic task routing, Claude API integration)
└── FileAgent (unified file operations)
    ├── File discovery tools (ls, glob, find)
    ├── Search tools (grep)
    ├── Modification tools (read, write, edit, multi_edit)
    ├── Management tools (todo_write)
    └── Operation tools (bash for all file operations)
```

### Directory Structure
```
src/
├── main.rs                    # CLI entry point with task message parsing
├── lib.rs                     # ClaudeConfig and core exports
├── agent.rs                   # Core Agent trait definition
├── tool.rs                    # Core Tool trait definition
├── utils.rs                   # Global sequences, file management
└── agents/
    ├── mod.rs                 # Agent module exports
    ├── orchestrator/          # Domain-agnostic task routing
    │   ├── mod.rs
    │   └── claude.rs          # Claude API integration and task routing
    └── file/                  # Unified file agent
        ├── mod.rs
        ├── agent.rs           # File agent implementation
        ├── claude.rs          # File agent Claude API handler
        ├── context_manager.rs # Context window optimization
        └── tools/             # All file tools in one flat structure
            ├── mod.rs         # Single module file exporting all tools
            ├── ls.rs          # List directory contents
            ├── glob.rs        # Pattern-based file finding
            ├── find.rs        # Advanced file search
            ├── grep.rs        # Text search in files
            ├── todo_write.rs  # Todo management
            ├── read.rs        # Read file contents
            ├── write.rs       # Write file contents
            ├── edit.rs        # Edit files (single change)
            ├── multi_edit.rs  # Edit files (multiple changes)
            └── bash.rs        # Execute shell commands
```

## File Agent - Core Architecture

The FileAgent implements a simplified version of Claude Code's approach:

1. **Single Agent with All Tools**: One agent handles all file operations
2. **Context Management**: Smart truncation and result processing for large outputs
3. **Parallel Tool Execution**: Multiple tools called simultaneously when safe
4. **Todo Management**: Task breakdown for complex operations
5. **Direct Tool Access**: All tools available directly without nested categories

### Tool Inventory

**Discovery Tools**:
- `ls`: Smart directory listing with size analysis and filtering
- `glob`: Pattern-based file finding with result optimization
- `find`: Advanced file search with metadata analysis

**Search Tools**:
- `grep`: Intelligent text search with context-aware truncation

**Management Tools**:
- `todo_write`: Create and manage structured task lists for complex coding sessions

**Modification Tools**:
- `read`: Context-aware file reading with smart sampling
- `write`: Safe file creation with validation
- `edit`: Precise string replacement with context verification
- `multi_edit`: Atomic batch operations with rollback support

**Operation Tools**:
- `bash`: Execute shell commands for all file operations (copy, move, delete, etc.)

### Context Management System

#### Smart Result Processing
The system implements intelligent result processing to manage Claude's context window:

```rust
// Context thresholds (matching Claude Code behavior)
pub const GREP_TRUNCATE_THRESHOLD: usize = 30;        // Lines before truncation
pub const READ_TRUNCATE_THRESHOLD: usize = 2000;      // Lines before sampling
pub const LINE_CHAR_LIMIT: usize = 2000;              // Max chars per line
pub const TOOL_OUTPUT_LIMIT: usize = 30000;           // Total output char limit
```

#### Parallel Tool Execution
```rust
impl FileAgent {
    async fn execute_tools_parallel(&self, tool_calls: Vec<ToolCall>) -> Result<ProcessedResults> {
        // Execute multiple tools simultaneously for efficiency
        let futures: Vec<_> = tool_calls.into_iter()
            .map(|call| self.execute_single_tool(call))
            .collect();
        
        let results = join_all(futures).await;
        // Process and return results with error handling
    }
}
```

### Todo Management System

The TodoWrite tool enables structured task management:

```rust
#[derive(Debug, Clone)]
pub struct Todo {
    pub id: String,
    pub content: String,
    pub status: TodoStatus, // Pending, InProgress, Completed
    pub priority: TodoPriority, // High, Medium, Low
}
```

**Use Cases**:
- Complex multi-step tasks requiring 3+ distinct actions
- Non-trivial tasks needing careful planning or multiple operations
- User explicitly requests todo list management
- Multiple tasks provided by user (numbered or comma-separated)

### Design Philosophy

This system prioritizes:

1. **Simplicity**: Flat tool structure, single agent design
2. **Maintainability**: Clear separation of concerns, minimal nested modules
3. **Functionality**: Focus on essential file operations without redundancy
4. **Claude Code Compatibility**: Similar patterns and behavior where applicable

### Key Differences from Full Claude Code

1. **No Task Tool**: The task tool that would spawn autonomous agents is not implemented since our file agent already runs under Claude control
2. **Bash for Operations**: Copy, move, delete operations use bash commands instead of dedicated tools
3. **Simplified Structure**: Flat tools directory instead of nested categories
4. **Direct API Integration**: Uses direct HTTP requests to Claude API instead of SDK

## Configuration

### Environment Variables
- `CLAUDE_API_KEY`: Required - Your Claude API key
- `CLAUDE_API_URL`: Optional - API endpoint (defaults to Anthropic's API)
- `CLAUDE_MODEL`: Optional - Model to use (defaults to claude-sonnet-4-20250514)
- `CLAUDE_MAX_TOKENS`: Optional - Max tokens per request (defaults to 8192)
- `CLAUDE_TEMPERATURE`: Optional - Response temperature (defaults to 0.7)
- `CLAUDE_TIMEOUT`: Optional - Request timeout in seconds (defaults to 300)

### Usage Pattern

The system is designed to be invoked with natural language tasks:

```bash
cargo run -- -m "Find all TODO comments in Rust files"
cargo run -- -m "Replace all instances of 'old_function' with 'new_function'"
cargo run -- -m "Analyze the project structure and create a summary"
```

The orchestrator routes tasks to the file agent, which uses Claude to intelligently select and execute the appropriate tools to complete the requested operation.

## Development Guidelines

### Adding New Tools
1. Create tool file in `src/agents/file/tools/`
2. Implement the `Tool` trait
3. Add module declaration and re-export in `tools/mod.rs`
4. Register tool in `FileAgent::new()`
5. Update documentation

### Tool Implementation Pattern
```rust
use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};

pub struct NewTool;

impl NewTool {
    pub fn new() -> Self { Self }
}

#[async_trait::async_trait]
impl Tool for NewTool {
    fn name(&self) -> &str { "new_tool" }
    fn description(&self) -> &str { "Tool description" }
    fn parameters(&self) -> Value { /* JSON schema */ }
    async fn execute(&self, arguments: &str) -> Result<String> { /* Implementation */ }
}
```

### Error Handling
- All tools return `Result<String>` for consistent error propagation
- Failed tools don't break the entire operation - other tools continue
- Detailed error messages help with debugging

### Testing Strategy
- Unit tests for individual tools
- Integration tests for agent interactions
- Manual testing with various task types

This architecture provides a clean, maintainable foundation for file operations while maintaining compatibility with Claude Code's operational patterns.