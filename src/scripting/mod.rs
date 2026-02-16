pub mod api;
pub(crate) mod lua_compat;
#[cfg(not(target_arch = "wasm32"))]
pub mod vm;
#[cfg(target_arch = "wasm32")]
#[path = "vm_wasm.rs"]
pub mod vm;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const DEFAULT_ENTITY_SCRIPT_BUDGET_MS: u64 = 8;
pub const DEFAULT_GLOBAL_SCRIPT_BUDGET_MS: u64 = 20;
pub const DEFAULT_SCRIPT_HOOK_INSTRUCTION_INTERVAL: u32 = 10_000;
pub const DEFAULT_RHAI_MAX_OPERATIONS: u64 = 500_000;
pub const DEFAULT_RHAI_MAX_CALL_LEVELS: usize = 64;

#[cfg(not(target_arch = "wasm32"))]
pub use vm::dry_run_script;
#[cfg(not(target_arch = "wasm32"))]
#[allow(unused_imports)]
pub use vm::{LuaScript, ScriptEngine, ScriptErrors, ScriptingPlugin};
#[cfg(target_arch = "wasm32")]
#[allow(unused_imports)]
pub use vm::{LuaScript, ScriptEngine, ScriptErrors, ScriptingPlugin};

#[derive(Serialize, Deserialize, Clone)]
pub struct ScriptError {
    pub script_name: String,
    pub entity_id: Option<u64>,
    pub error_message: String,
    pub frame: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScriptEvent {
    pub name: String,
    pub data: serde_json::Value,
    pub frame: u64,
    pub source_entity: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ScriptRuntimeSnapshot {
    pub scripts: HashMap<String, String>,
    pub global_scripts: HashSet<String>,
    pub vars: HashMap<String, serde_json::Value>,
}

const MAX_SCRIPT_LOG_ENTRIES: usize = 500;

#[derive(Serialize, Deserialize, Clone)]
pub struct ScriptLogEntry {
    pub level: String,
    pub message: String,
    pub script_name: String,
    pub frame: u64,
    pub entity_id: Option<u64>,
}

#[derive(Resource, Default)]
pub struct ScriptLogBuffer {
    pub entries: Vec<ScriptLogEntry>,
}

impl ScriptLogBuffer {
    pub fn push(&mut self, entry: ScriptLogEntry) {
        self.entries.push(entry);
        if self.entries.len() > MAX_SCRIPT_LOG_ENTRIES {
            let excess = self.entries.len() - MAX_SCRIPT_LOG_ENTRIES;
            self.entries.drain(0..excess);
        }
    }
}

pub trait ScriptBackend: Send + Sync {
    fn load_script(&mut self, name: String, source: String, global: bool) -> Result<(), String>;
    fn remove_script(&mut self, name: &str);
    fn list_scripts(&self) -> Vec<api::ScriptInfo>;
    fn set_vars(&mut self, vars: HashMap<String, serde_json::Value>);
    fn vars(&self) -> &HashMap<String, serde_json::Value>;
    fn snapshot(&self) -> ScriptRuntimeSnapshot;
    fn restore_snapshot(&mut self, snapshot: ScriptRuntimeSnapshot);
    fn get_script_source(&self, name: &str) -> Option<api::ScriptSource>;
    fn events(&self) -> &[ScriptEvent];
}
