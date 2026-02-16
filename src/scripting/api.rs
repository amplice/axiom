use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone)]
pub struct ScriptUpsertRequest {
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub global: bool,
    #[serde(default)]
    pub always_run: Option<bool>,
}

#[derive(Serialize, Clone)]
pub struct ScriptInfo {
    pub name: String,
    pub global: bool,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct ScriptSource {
    pub name: String,
    pub source: String,
    pub global: bool,
}

#[derive(Deserialize, Clone)]
pub struct ScriptTestRequest {
    pub source: String,
}

#[derive(Serialize, Clone)]
pub struct ScriptStats {
    pub loaded_scripts: usize,
    pub global_scripts: usize,
    pub disabled_global_scripts: usize,
    pub recent_event_buffer_len: usize,
    pub dropped_events: u64,
    pub recent_error_buffer_len: usize,
    pub entity_budget_ms: u64,
    pub global_budget_ms: u64,
    pub hook_instruction_interval: u32,
    pub rhai_max_operations: u64,
    pub rhai_max_call_levels: usize,
}
