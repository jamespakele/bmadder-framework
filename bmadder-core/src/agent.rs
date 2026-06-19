use serde::{Deserialize, Serialize};

/// Result of a pi.dev sub-agent invocation.
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Structured output from pi.dev when --json-output is used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiDevOutput {
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub output_summary: Option<String>,
}

impl AgentResult {
    pub fn from_pi_dev(output: PiDevOutput) -> Self {
        AgentResult {
            success: output.success,
            exit_code: if output.success { 0 } else { 1 },
            stdout: String::new(),
            stderr: output.error.unwrap_or_default(),
            timed_out: false,
        }
    }
}
