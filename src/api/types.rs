use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize)]
pub struct GameState {
    pub tilemap: TilemapState,
    pub player: PlayerState,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TilemapState {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub player_spawn: (f32, f32),
    pub goal: Option<(i32, i32)>,
}

#[derive(Serialize, Clone)]
pub struct PlayerState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    pub alive: bool,
}

#[derive(Deserialize, Clone)]
pub struct SetLevelRequest {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub player_spawn: Option<(f32, f32)>,
    pub goal: Option<(i32, i32)>,
}

#[derive(Deserialize)]
pub struct SaveSlotRequest {
    pub slot: String,
}

#[derive(Deserialize)]
pub struct TeleportRequest {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
}

impl ApiResponse<()> {
    pub fn ok() -> ApiResponse<String> {
        ApiResponse {
            ok: true,
            data: Some("ok".to_string()),
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> ApiResponse<String> {
        ApiResponse {
            ok: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

// === Entity API types ===

#[derive(Serialize, Deserialize, Clone)]
pub struct EntitySpawnRequest {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub components: Vec<ComponentDef>,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_player: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ComponentDef {
    #[serde(rename = "collider")]
    Collider { width: f32, height: f32 },
    #[serde(rename = "gravity_body")]
    GravityBody,
    #[serde(rename = "horizontal_mover")]
    HorizontalMover {
        speed: f32,
        #[serde(default = "default_left")]
        left_action: String,
        #[serde(default = "default_right")]
        right_action: String,
    },
    #[serde(rename = "jumper")]
    Jumper {
        velocity: f32,
        #[serde(default = "default_jump")]
        action: String,
        #[serde(default = "default_fall_mult")]
        fall_multiplier: f32,
        #[serde(default = "default_true")]
        variable_height: bool,
        #[serde(default = "default_coyote")]
        coyote_frames: u32,
        #[serde(default = "default_buffer")]
        buffer_frames: u32,
    },
    #[serde(rename = "top_down_mover")]
    TopDownMover {
        speed: f32,
        #[serde(default = "default_up")]
        up_action: String,
        #[serde(default = "default_down")]
        down_action: String,
        #[serde(default = "default_left")]
        left_action: String,
        #[serde(default = "default_right")]
        right_action: String,
    },
    #[serde(rename = "health")]
    Health { current: f32, max: f32 },
    #[serde(rename = "contact_damage")]
    ContactDamage {
        amount: f32,
        #[serde(default = "default_damage_cooldown")]
        cooldown_frames: u32,
        #[serde(default)]
        knockback: f32,
        damage_tag: String,
    },
    #[serde(rename = "trigger_zone")]
    TriggerZone {
        radius: f32,
        trigger_tag: String,
        event_name: String,
        #[serde(default)]
        one_shot: bool,
    },
    #[serde(rename = "pickup")]
    Pickup {
        pickup_tag: String,
        effect: PickupEffectDef,
    },
    #[serde(rename = "projectile")]
    Projectile {
        speed: f32,
        direction: Vec2Def,
        lifetime_frames: u32,
        damage: f32,
        owner_id: u64,
        damage_tag: String,
    },
    #[serde(rename = "hitbox")]
    Hitbox {
        width: f32,
        height: f32,
        #[serde(default)]
        offset: Vec2Def,
        #[serde(default)]
        active: bool,
        damage: f32,
        damage_tag: String,
    },
    #[serde(rename = "moving_platform")]
    MovingPlatform {
        waypoints: Vec<Vec2Def>,
        speed: f32,
        #[serde(default)]
        loop_mode: PlatformLoopModeDef,
        #[serde(default)]
        pause_frames: u32,
        #[serde(default = "default_true")]
        carry_riders: bool,
        #[serde(default)]
        current_waypoint: usize,
        #[serde(default = "default_platform_direction")]
        direction: i8,
    },
    #[serde(rename = "animation_controller")]
    AnimationController {
        graph: String,
        #[serde(default = "default_animation_state")]
        state: String,
        #[serde(default)]
        frame: usize,
        #[serde(default)]
        timer: f32,
        #[serde(default = "default_f32_one")]
        speed: f32,
        #[serde(default = "default_true")]
        playing: bool,
        #[serde(default = "default_true")]
        facing_right: bool,
        #[serde(default)]
        auto_from_velocity: bool,
    },
    #[serde(rename = "path_follower")]
    PathFollower {
        target: Vec2Def,
        #[serde(default = "default_recalculate_interval")]
        recalculate_interval: u32,
        #[serde(default)]
        path_type: PathTypeDef,
        speed: f32,
    },
    #[serde(rename = "ai_behavior")]
    AiBehavior { behavior: AiBehaviorDef },
    #[serde(rename = "particle_emitter")]
    ParticleEmitter {
        #[serde(flatten, default)]
        emitter: crate::particles::ParticleEmitter,
    },
}

fn default_left() -> String {
    "left".into()
}
fn default_right() -> String {
    "right".into()
}
fn default_up() -> String {
    "up".into()
}
fn default_down() -> String {
    "down".into()
}
fn default_jump() -> String {
    "jump".into()
}
fn default_fall_mult() -> f32 {
    1.5
}
fn default_true() -> bool {
    true
}
fn default_coyote() -> u32 {
    5
}
fn default_buffer() -> u32 {
    4
}
fn default_damage_cooldown() -> u32 {
    12
}
fn default_recalculate_interval() -> u32 {
    20
}
fn default_platform_direction() -> i8 {
    1
}
fn default_animation_state() -> String {
    "idle".to_string()
}
fn default_f32_one() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum PickupEffectDef {
    #[serde(rename = "heal")]
    Heal { amount: f32 },
    #[serde(rename = "score_add")]
    ScoreAdd { amount: i32 },
    #[serde(rename = "custom")]
    Custom { name: String },
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Vec2Def {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum PathTypeDef {
    #[default]
    TopDown,
    Platformer,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlatformLoopModeDef {
    #[default]
    Loop,
    PingPong,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum AiBehaviorDef {
    #[serde(rename = "patrol")]
    Patrol { waypoints: Vec<Vec2Def>, speed: f32 },
    #[serde(rename = "chase")]
    Chase {
        #[serde(default = "default_player_tag")]
        target_tag: String,
        speed: f32,
        detection_radius: f32,
        give_up_radius: f32,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    #[serde(rename = "flee")]
    Flee {
        threat_tag: String,
        speed: f32,
        detection_radius: f32,
        give_up_radius: f32,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    #[serde(rename = "guard")]
    Guard {
        position: Vec2Def,
        radius: f32,
        chase_radius: f32,
        speed: f32,
        #[serde(default = "default_player_tag")]
        target_tag: String,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    #[serde(rename = "wander")]
    Wander {
        speed: f32,
        radius: f32,
        #[serde(default = "default_pause_frames")]
        pause_frames: u32,
    },
    #[serde(rename = "custom")]
    Custom { script: String },
}

fn default_player_tag() -> String {
    "player".to_string()
}
fn default_pause_frames() -> u32 {
    30
}

#[derive(Serialize, Clone)]
pub struct EntityInfo {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_id: Option<u64>,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub components: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_health: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_behavior: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_target_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_target: Option<Vec2Def>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_len: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation_graph: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation_frame: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation_facing_right: Option<bool>,
}

#[derive(Deserialize)]
pub struct PresetRequest {
    pub preset: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Deserialize)]
pub struct DamageRequest {
    pub amount: f32,
}

#[derive(Deserialize)]
pub struct EntityAnimationRequest {
    pub animation: String,
}

#[derive(Deserialize, Clone)]
pub struct EntityParticlesRequest {
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub emitter: Option<crate::particles::ParticlePresetDef>,
}

#[derive(Deserialize)]
pub struct RaycastRequest {
    pub origin: [f32; 2],
    pub direction: [f32; 2],
    pub max_distance: f32,
}

#[derive(Deserialize)]
pub struct EntityRaycastRequest {
    pub origin: [f32; 2],
    pub direction: [f32; 2],
    pub max_distance: f32,
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct EntityRaycastHit {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub distance: f32,
}

#[derive(Deserialize)]
pub struct PathfindRequest {
    pub from: [f32; 2],
    pub to: [f32; 2],
    #[serde(default)]
    pub path_type: Option<String>,
}

#[derive(Deserialize)]
pub struct LineOfSightRequest {
    pub from: [f32; 2],
    pub to: [f32; 2],
}

#[derive(Serialize)]
pub struct GameRuntimeState {
    pub state: String,
    pub time_in_state_seconds: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_transition: Option<GameTransitionInfo>,
    pub transition_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_transition: Option<GameActiveTransitionInfo>,
}

#[derive(Serialize, Clone)]
pub struct GameTransitionInfo {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    pub duration: f32,
    pub at_unix_ms: u64,
}

#[derive(Serialize, Clone)]
pub struct GameActiveTransitionInfo {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    pub duration: f32,
    pub elapsed_seconds: f32,
    pub remaining_seconds: f32,
    pub progress: f32,
}

#[derive(Deserialize)]
pub struct SetGameStateRequest {
    pub state: String,
}

#[derive(Deserialize)]
pub struct GameTransitionRequest {
    pub to: String,
    #[serde(default)]
    pub effect: Option<String>,
    #[serde(default)]
    pub duration: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub struct GameLoadLevelRequest {
    pub template: String,
    pub difficulty: f32,
    #[serde(default = "default_load_seed")]
    pub seed: u64,
    #[serde(default)]
    pub width: Option<usize>,
    #[serde(default)]
    pub height: Option<usize>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub config_overrides: serde_json::Value,
}

fn default_load_seed() -> u64 {
    42
}

#[derive(Deserialize)]
pub struct ReplayRecordRequest {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct ReplayStopRequest {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct ReplayPlayRequest {
    pub name: String,
}

#[derive(Deserialize)]
pub struct DebugOverlayRequest {
    pub show: bool,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Serialize)]
pub struct DebugOverlayState {
    pub show: bool,
    pub features: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LevelPackRequest {
    pub name: String,
    pub levels: Vec<LevelPackLevelRequest>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LevelPackLevelRequest {
    pub template: String,
    pub difficulty: f32,
    #[serde(default = "default_pack_seed")]
    pub seed: u64,
    #[serde(default)]
    pub width: Option<usize>,
    #[serde(default)]
    pub height: Option<usize>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub config_overrides: serde_json::Value,
}

fn default_pack_seed() -> u64 {
    42
}

#[derive(Serialize, Clone)]
pub struct LevelPackProgressEntry {
    pub level_index: usize,
    pub template: String,
    pub difficulty: f32,
    pub time_seconds: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

#[derive(Serialize, Clone)]
pub struct LevelPackProgressResponse {
    pub name: String,
    pub total_levels: usize,
    pub started: bool,
    pub current_level: usize,
    pub completed: bool,
    pub current_level_elapsed_seconds: f32,
    pub history: Vec<LevelPackProgressEntry>,
}

#[derive(Serialize, Clone)]
pub struct ExampleGameInfo {
    pub name: String,
    pub description: String,
    pub genre: String,
    pub template: String,
    pub default_difficulty: f32,
    pub default_seed: u64,
    pub constraints: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub struct ExampleLoadRequest {
    #[serde(default)]
    pub difficulty: Option<f32>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub config_overrides: serde_json::Value,
}

#[derive(Deserialize, Clone)]
pub struct ExportWebRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub levels: Option<String>,
    #[serde(default)]
    pub embed_assets: Option<bool>,
    #[serde(default)]
    pub strip_incompatible_scripts: Option<bool>,
    #[serde(default)]
    pub release: Option<bool>,
}

#[derive(Deserialize, Clone)]
pub struct ExportDesktopRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub release: Option<bool>,
}

#[derive(Deserialize, Clone)]
pub struct AudioSfxRequest {
    #[serde(default)]
    pub effects: HashMap<String, crate::audio::SfxDefinition>,
}

#[derive(Deserialize, Clone)]
pub struct AudioMusicRequest {
    #[serde(default)]
    pub tracks: HashMap<String, crate::audio::MusicDefinition>,
}

#[derive(Deserialize, Clone)]
pub struct AudioPlayRequest {
    #[serde(default)]
    pub sfx: Option<String>,
    #[serde(default)]
    pub music: Option<String>,
    #[serde(default)]
    pub fade_in: Option<f32>,
    #[serde(default)]
    pub volume: Option<f32>,
    #[serde(default)]
    pub pitch: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub struct AudioStopRequest {
    #[serde(default)]
    pub music: Option<bool>,
    #[serde(default)]
    pub fade_out: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub struct AudioConfigRequest {
    #[serde(default)]
    pub master_volume: Option<f32>,
    #[serde(default)]
    pub sfx_volume: Option<f32>,
    #[serde(default)]
    pub music_volume: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub struct AudioTriggerRequest {
    #[serde(default)]
    pub mappings: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CameraBoundsRequest {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

#[derive(Deserialize, Clone)]
pub struct CameraConfigRequest {
    #[serde(default)]
    pub follow_target: Option<u64>,
    #[serde(default)]
    pub follow_speed: Option<f32>,
    #[serde(default)]
    pub zoom: Option<f32>,
    #[serde(default)]
    pub offset: Option<[f32; 2]>,
    #[serde(default)]
    pub deadzone: Option<[f32; 2]>,
    #[serde(default)]
    pub bounds: Option<CameraBoundsRequest>,
    #[serde(default)]
    pub look_at: Option<[f32; 2]>,
}

#[derive(Deserialize, Clone)]
pub struct CameraShakeRequest {
    pub intensity: f32,
    pub duration: f32,
    #[serde(default)]
    pub decay: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub struct CameraLookAtRequest {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub speed: Option<f32>,
}

#[derive(Serialize, Clone)]
pub struct CameraStateResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 2]>,
    pub zoom: f32,
    pub follow_speed: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_target: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look_at: Option<[f32; 2]>,
    pub offset: [f32; 2],
    pub deadzone: [f32; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<CameraBoundsRequest>,
    pub shake_remaining: f32,
}

#[derive(Deserialize, Clone)]
pub struct UiScreenRequest {
    pub name: String,
    #[serde(default)]
    pub layer: i32,
    #[serde(default)]
    pub nodes: Vec<crate::ui::UiNode>,
}

#[derive(Deserialize, Clone)]
pub struct UiNodeUpdateRequest {
    #[serde(default)]
    pub node_type: Option<crate::ui::UiNodeType>,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub value: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub struct DialogueConversationRequest {
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<crate::ui::DialogueNode>,
}

#[derive(Deserialize, Clone)]
pub struct DialogueStartRequest {
    pub conversation: String,
}

#[derive(Deserialize, Clone)]
pub struct DialogueChooseRequest {
    pub choice: usize,
}

#[derive(Deserialize, Clone)]
pub struct AnimationGraphRequest {
    pub graph: crate::animation::AnimationGraphDef,
}

#[derive(Deserialize, Clone)]
pub struct SpriteSheetUpsertRequest {
    pub name: String,
    pub path: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
    #[serde(default)]
    pub animations: HashMap<String, SpriteSheetAnimationRequest>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SpriteSheetAnimationRequest {
    #[serde(default)]
    pub frames: Vec<usize>,
    pub fps: f32,
    #[serde(default = "default_true")]
    pub looping: bool,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub events: Vec<crate::animation::AnimFrameEventDef>,
}

#[derive(Deserialize, Clone)]
pub struct ParticlePresetRequest {
    #[serde(default)]
    pub presets: HashMap<String, crate::particles::ParticlePresetDef>,
}
