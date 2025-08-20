// Re-export Agent trait and helper functions from main agent module
pub use crate::agent::{agents_to_json, Agent};

// Agent implementation modules
pub mod orchestrator;
pub mod file;

// Re-export main agents
pub use orchestrator::OrchestratorAgent;
pub use file::FileAgent;
