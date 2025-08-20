use crate::tool::Tool;
use anyhow::Result;
use serde_json::{json, Value};
use std::process::Command;
use std::time::Duration;

/// Safe command execution with validation
pub struct BashTool;

#[derive(serde::Deserialize)]
struct BashParams {
    command: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    timeout: Option<u64>, // in milliseconds
}

impl BashTool {
    pub fn new() -> Self {
        Self
    }

    fn validate_command(&self, command: &str) -> Result<()> {
        let command_lower = command.to_lowercase();

        // Extremely dangerous commands - absolutely forbidden
        let forbidden_commands = [
            "rm -rf /",
            "rm -rf /*",
            ":(){ :|:& };:",
            "mv / /dev/null",
            "dd if=/dev/zero",
            "mkfs",
            "fdisk",
            "format c:",
            "del /f /s /q c:\\",
            "shutdown",
            "reboot",
            "halt",
            "poweroff",
            "init 0",
            "init 6",
            "chmod -R 777 /",
            "chown -R root /",
            "killall -9",
        ];

        for forbidden in &forbidden_commands {
            if command_lower.contains(forbidden) {
                return Err(anyhow::anyhow!(
                    "Forbidden command detected: '{}'. This command could cause system damage.",
                    forbidden
                ));
            }
        }

        // Risky patterns - warn but allow
        let risky_patterns = [
            "rm -rf",
            "rm -r",
            "sudo rm",
            "sudo dd",
            "sudo chmod",
            "sudo chown",
            "sudo mv",
            "> /dev/",
            "curl | sh",
            "wget | sh",
            "eval ",
            "exec ",
            "source /",
            ". /",
            "sudo -i",
            "su -",
        ];

        for risky in &risky_patterns {
            if command_lower.contains(risky) {
                log::warn!(
                    "Risky command pattern detected: '{}' in command: {}",
                    risky,
                    command
                );
            }
        }

        // Check for command injection attempts
        let injection_chars = [";", "&&", "||", "|", "`", "$(", "${"];
        let mut has_injection = false;
        for inject in &injection_chars {
            if command.contains(inject) {
                has_injection = true;
                log::warn!(
                    "Command contains potential injection character: '{}'",
                    inject
                );
            }
        }

        if has_injection {
            log::warn!(
                "Command contains shell metacharacters. Ensure this is intentional: {}",
                command
            );
        }

        Ok(())
    }

    fn sanitize_output(&self, output: &str, max_length: usize) -> String {
        let mut sanitized = output.to_string();

        // Remove potential ANSI escape sequences
        let ansi_regex = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap_or_else(|_| {
            // Fallback if regex fails
            return regex::Regex::new(r"").unwrap();
        });
        sanitized = ansi_regex.replace_all(&sanitized, "").to_string();

        // Truncate if too long
        if sanitized.len() > max_length {
            sanitized.truncate(max_length);
            sanitized.push_str("\n... [OUTPUT TRUNCATED] ...");
        }

        sanitized
    }

    fn get_safe_environment() -> Vec<(String, String)> {
        // Provide a minimal, safe environment
        let mut env = Vec::new();

        // Essential environment variables
        if let Ok(path) = std::env::var("PATH") {
            env.push(("PATH".to_string(), path));
        }
        if let Ok(home) = std::env::var("HOME") {
            env.push(("HOME".to_string(), home));
        }
        if let Ok(user) = std::env::var("USER") {
            env.push(("USER".to_string(), user));
        }

        // Set safe defaults
        env.push(("SHELL".to_string(), "/bin/sh".to_string()));
        env.push(("TERM".to_string(), "xterm".to_string()));

        env
    }

    fn get_working_directory(&self) -> Result<std::path::PathBuf> {
        // Always use current directory for safety
        std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))
    }
}

#[async_trait::async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Safe command execution with validation. Prefer specialized file tools over bash commands for file operations. Use for system commands, builds, tests, and utilities."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this command does (5-10 words)"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (max 600000ms / 10 minutes). Default: 120000ms (2 minutes)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, arguments: &str) -> Result<String> {
        let params: BashParams = serde_json::from_str(arguments)?;

        // Validate command safety
        self.validate_command(&params.command)?;

        let description = params
            .description
            .unwrap_or_else(|| "Executing command".to_string());
        log::info!("Bash executing: {} - {}", description, params.command);

        // Set timeout (default 2 minutes, max 10 minutes)
        let timeout_ms = params.timeout.unwrap_or(120_000).min(600_000);
        let _timeout_duration = Duration::from_millis(timeout_ms);

        // Get safe working directory
        let working_dir = self.get_working_directory()?;

        // Prepare environment
        let env_vars = Self::get_safe_environment();

        // Execute command with timeout using shell
        let start_time = std::time::Instant::now();

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&params.command)
            .current_dir(&working_dir)
            .env_clear(); // Clear all environment variables first

        // Add safe environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let output = match cmd.output() {
            Ok(output) => output,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to execute command '{}': {}",
                    params.command,
                    e
                ));
            }
        };

        let execution_time = start_time.elapsed();

        // Check if command was successful
        let success = output.status.success();
        let exit_code = output.status.code().unwrap_or(-1);

        // Process output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Sanitize and truncate output
        const MAX_OUTPUT_LENGTH: usize = 30_000; // Output length limit
        let clean_stdout = self.sanitize_output(&stdout, MAX_OUTPUT_LENGTH);
        let clean_stderr = self.sanitize_output(&stderr, MAX_OUTPUT_LENGTH / 2);

        // Format result
        let mut result = String::new();

        // Header
        result.push_str(&format!("Command: {}\n", params.command));
        if !description.is_empty() && description != "Executing command" {
            result.push_str(&format!("Description: {}\n", description));
        }
        result.push_str(&format!("Working Directory: {}\n", working_dir.display()));
        result.push_str(&format!("Execution Time: {:?}\n", execution_time));
        result.push_str(&format!(
            "Exit Code: {} ({})\n\n",
            exit_code,
            if success { "Success" } else { "Failed" }
        ));

        // Output
        if !clean_stdout.is_empty() {
            result.push_str("Standard Output:\n");
            result.push_str(&clean_stdout);
            if !clean_stdout.ends_with('\n') {
                result.push_str("\n");
            }
            result.push_str("\n");
        }

        if !clean_stderr.is_empty() {
            result.push_str("Standard Error:\n");
            result.push_str(&clean_stderr);
            if !clean_stderr.ends_with('\n') {
                result.push_str("\n");
            }
            result.push_str("\n");
        }

        if clean_stdout.is_empty() && clean_stderr.is_empty() {
            result.push_str("No output produced\n\n");
        }

        // Summary
        result.push_str(&format!(
            "Summary: Command {} in {:?}",
            if success { "succeeded" } else { "failed" },
            execution_time
        ));

        if !success {
            log::error!(
                "Command failed: {} (exit code: {})",
                params.command,
                exit_code
            );
        } else {
            log::info!(
                "Command succeeded: {} ({}ms)",
                params.command,
                execution_time.as_millis()
            );
        }

        Ok(result)
    }
}
