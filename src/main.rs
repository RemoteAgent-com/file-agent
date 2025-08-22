// Binary target required for cargo run
// This binary can call library functions when RAWORC_HANDLER is set

use file_agent::process_message_sync;
use serde_json::{json, Value};

fn main() {
    // Check if Raworc is calling this binary to execute a library function
    if let Ok(handler) = std::env::var("RAWORC_HANDLER") {
        if handler == "lib.process_message_sync" {
            // Get message and context from environment and args
            let args: Vec<String> = std::env::args().collect();
            let message = if args.len() > 1 { &args[1] } else { "" };

            let context: Value = if let Ok(context_str) = std::env::var("AGENT_CONTEXT") {
                serde_json::from_str(&context_str).unwrap_or_else(|_| json!({}))
            } else {
                json!({"session_id": "unknown", "space": "default"})
            };

            // Call the library function
            let response = process_message_sync(message, &context);
            println!("{}", response);
            return;
        }
    }

    // Standalone execution (not called by Raworc)
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let message = &args[1];
        let context = json!({"session_id": "standalone", "space": "default"});
        let response = process_message_sync(message, &context);
        println!("{}", response);
    } else {
        println!("File Agent - Use lib.process_message_sync handler");
    }
}
