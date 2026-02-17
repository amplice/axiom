use crate::components::GameConfig;
use crate::tilemap::Tilemap;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct SaveGameData {
    pub version: u32,
    pub config: GameConfig,
    pub tilemap: Tilemap,
    #[serde(default = "default_game_state")]
    pub game_state: String,
    #[serde(default = "default_next_network_id")]
    pub next_network_id: u64,
    pub entities: Vec<SaveEntity>,
    pub scripts: std::collections::HashMap<String, String>,
    pub global_scripts: Vec<String>,
    pub game_vars: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub animation_graphs: std::collections::HashMap<String, crate::animation::AnimationGraphDef>,
    #[serde(default)]
    pub sprite_sheets: std::collections::HashMap<String, crate::sprites::SpriteSheetDef>,
    #[serde(default)]
    pub particle_presets: std::collections::HashMap<String, crate::particles::ParticlePresetDef>,
}

fn default_next_network_id() -> u64 {
    1
}

fn default_game_state() -> String {
    "Playing".to_string()
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum SaveAiState {
    Idle,
    Patrolling { waypoint_index: usize },
    Chasing { target_id: u64 },
    Fleeing { threat_id: u64 },
    Attacking { target_id: u64 },
    Returning,
    Wandering { pause_frames: u32 },
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct SaveEntity {
    #[serde(default)]
    pub network_id: Option<u64>,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub vx: f32,
    #[serde(default)]
    pub vy: f32,
    pub is_player: bool,
    pub components: Vec<crate::api::types::ComponentDef>,
    pub script: Option<String>,
    #[serde(default)]
    pub script_state: Option<serde_json::Value>,
    pub tags: Vec<String>,
    pub alive: bool,
    #[serde(default)]
    pub ai_state: Option<SaveAiState>,
    #[serde(default)]
    pub invincibility_frames: Option<u32>,
    #[serde(default)]
    pub path_follower_path: Vec<(f32, f32)>,
    #[serde(default)]
    pub path_follower_frames_until_recalc: Option<u32>,
    #[serde(default)]
    pub inventory_slots: Vec<crate::inventory::ItemSlot>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct ProjectExportData {
    pub version: u32,
    pub save: SaveGameData,
    #[serde(default)]
    pub level_packs: Vec<crate::api::types::LevelPackRequest>,
}
