use super::*;

#[derive(Resource, Clone)]
pub(super) struct RuntimeStoreHandle(pub Arc<RwLock<GameRuntimeStore>>);

pub(super) struct SnapshotData {
    pub tilemap: Tilemap,
    pub physics: GameConfig,
}

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) sender: Sender<ApiCommand>,
    pub(super) snapshot: Arc<RwLock<SnapshotData>>,
    pub(super) level_packs: Arc<RwLock<LevelPackStore>>,
    pub(super) game_runtime: Arc<RwLock<GameRuntimeStore>>,
    pub(super) replay_store: Arc<RwLock<ReplayStore>>,
    pub(super) var_snapshot: Arc<RwLock<VarDiffStore>>,
}

#[derive(Default)]
pub(super) struct VarDiffStore {
    pub(super) last_vars: HashMap<String, serde_json::Value>,
    pub(super) snapshot_id: u64,
}

#[derive(Default)]
pub(super) struct LevelPackStore {
    pub(super) packs: HashMap<String, LevelPackRequest>,
    pub(super) progress: HashMap<String, LevelPackProgressState>,
}

#[derive(Clone)]
pub(super) struct LevelPackProgressState {
    pub(super) current_level: usize,
    pub(super) completed: bool,
    pub(super) history: Vec<LevelPackProgressEntry>,
    pub(super) level_started_at: std::time::Instant,
}

pub(super) struct GameRuntimeStore {
    pub(super) state: String,
    pub(super) entered_at: std::time::Instant,
    pub(super) last_loaded: Option<LoadedLevelSnapshot>,
    pub(super) transitions: Vec<RuntimeTransition>,
    pub(super) active_transition: Option<RuntimeActiveTransition>,
}

#[derive(Clone)]
pub(super) struct LoadedLevelSnapshot {
    pub(super) config: GameConfig,
    pub(super) generated: crate::generation::GenerateResult,
}

#[derive(Clone)]
pub(super) struct RuntimeTransition {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) effect: Option<String>,
    pub(super) duration: f32,
    pub(super) at_unix_ms: u64,
}

#[derive(Clone)]
pub(super) struct RuntimeActiveTransition {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) effect: Option<String>,
    pub(super) duration: f32,
    pub(super) started_at: std::time::Instant,
}

#[derive(Default)]
pub(super) struct ReplayStore {
    pub(super) active: Option<ReplaySession>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(super) struct ReplaySession {
    pub(super) name: String,
    pub(super) recorded_at_unix_ms: u64,
    pub(super) initial_tilemap: Tilemap,
    pub(super) initial_config: GameConfig,
    pub(super) steps: Vec<ReplayStep>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(super) struct ReplayStep {
    pub(super) request: SimulationRequest,
    pub(super) result: SimulationResult,
}
