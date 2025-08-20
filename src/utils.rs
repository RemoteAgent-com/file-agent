use anyhow::Result;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Global counter for sequential file naming
static GLOBAL_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Clear the bin directory at the start of a new task
pub fn clear_bin_directory() -> Result<()> {
    let bin_path = Path::new("bin");
    
    if bin_path.exists() {
        // Remove all contents of bin directory
        fs::remove_dir_all(bin_path)?;
        log::info!("Cleared bin directory for new task");
    }
    
    // Recreate the bin directory structure for messages
    fs::create_dir_all("bin/messages/orchestrator")?;
    fs::create_dir_all("bin/messages/file_agent")?;
    
    // Reset counter
    GLOBAL_COUNTER.store(1, Ordering::SeqCst);
    
    log::info!("Bin directory structure recreated");
    Ok(())
}

/// Get the next global sequence number
pub fn get_next_global_sequence_number() -> usize {
    GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Generate a sequential filename for messages
pub fn generate_message_filename(agent_type: &str, sequence_number: usize) -> String {
    format!("{:03}_{}_message.json", sequence_number, agent_type)
}

/// Store Claude message in message history - shared across all agents
pub fn store_claude_message(agent_type: &str, message: &Value) -> Result<()> {
    let sequence_number = get_next_global_sequence_number();
    let filename = generate_message_filename(agent_type, sequence_number);
    let dir_path_str = format!("bin/messages/{}", agent_type);
    let dir_path = Path::new(&dir_path_str);
    fs::create_dir_all(dir_path)?;

    let file_path = dir_path.join(&filename);
    fs::write(file_path, serde_json::to_string_pretty(message)?)?;
    log::debug!("Stored {} Claude message: {}", agent_type, filename);
    Ok(())
}