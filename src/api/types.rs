use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::components::GameConfig;

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
    /// Optional extra decorative tile layers (visual-only, rendered on top).
    #[serde(default)]
    pub extra_layers: Vec<crate::tilemap::TileLayer>,
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
    #[serde(default)]
    pub invisible: bool,
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
        #[serde(default = "default_facing_direction")]
        facing_direction: u8,
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
    #[serde(rename = "render_layer")]
    RenderLayer {
        #[serde(default)]
        layer: i32,
    },
    #[serde(rename = "point_light")]
    PointLight {
        #[serde(default = "default_f32_one")]
        radius: f32,
        #[serde(default = "default_f32_one")]
        intensity: f32,
        #[serde(default = "default_light_color")]
        color: [f32; 3],
    },
    #[serde(rename = "collision_layer")]
    CollisionLayer {
        #[serde(default = "default_collision_layer")]
        layer: u16,
        #[serde(default = "default_collision_mask")]
        mask: u16,
    },
    #[serde(rename = "sprite_color_tint")]
    SpriteColorTint {
        #[serde(default = "default_tint_color")]
        color: [f32; 4],
        #[serde(default)]
        flash_color: Option<[f32; 4]>,
        #[serde(default)]
        flash_frames: u32,
    },
    #[serde(rename = "trail_effect")]
    TrailEffect {
        #[serde(default = "default_trail_interval")]
        interval: u32,
        #[serde(default = "default_trail_duration")]
        duration: f32,
        #[serde(default = "default_f32_one")]
        alpha_start: f32,
        #[serde(default)]
        alpha_end: f32,
    },
    #[serde(rename = "state_machine")]
    StateMachine {
        states: std::collections::HashMap<String, crate::state_machine::StateConfig>,
        #[serde(default = "default_state_machine_initial")]
        initial: String,
    },
    #[serde(rename = "inventory")]
    Inventory {
        #[serde(default = "default_max_slots")]
        max_slots: usize,
    },
    #[serde(rename = "invisible")]
    Invisible,
    #[serde(rename = "circle_collider")]
    CircleCollider {
        radius: f32,
    },
    #[serde(rename = "velocity_damping")]
    VelocityDamping {
        #[serde(default = "default_damping_factor")]
        factor: f32,
    },
    #[serde(rename = "knockback_impulse")]
    KnockbackImpulse {
        vx: f32,
        vy: f32,
    },
    #[serde(rename = "solid_body")]
    SolidBody,
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
fn default_facing_direction() -> u8 {
    5 // South (facing camera)
}
fn default_light_color() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
fn default_collision_layer() -> u16 {
    1
}
fn default_collision_mask() -> u16 {
    0xFFFF
}
fn default_tint_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
fn default_trail_interval() -> u32 {
    3
}
fn default_trail_duration() -> f32 {
    0.3
}
fn default_state_machine_initial() -> String {
    "idle".to_string()
}
fn default_max_slots() -> usize {
    20
}
fn default_damping_factor() -> f32 {
    0.1
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_layer: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision_layer: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision_mask: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory_slots: Option<usize>,
    // Physics diagnostics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coyote_frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_buffer_frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invincibility_frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounded: Option<bool>,
    // Interaction component details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_damage: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_knockback: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_effect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projectile_damage: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projectile_speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hitbox_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hitbox_damage: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
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
#[serde(untagged)]
pub enum CameraBoundsOption {
    Clear(#[allow(dead_code)] String),
    Set(CameraBoundsRequest),
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
    pub bounds: Option<CameraBoundsOption>,
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
    #[serde(default = "default_one_u32")]
    pub rows: u32,
    #[serde(default)]
    pub animations: HashMap<String, SpriteSheetAnimationRequest>,
    #[serde(default)]
    pub direction_map: Option<Vec<u8>>,
    #[serde(default = "default_anchor_y")]
    pub anchor_y: f32,
}

fn default_anchor_y() -> f32 {
    -0.15
}

fn default_one_u32() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SpriteSheetAnimationRequest {
    #[serde(default)]
    pub path: Option<String>,
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

// === Tween API types ===

#[derive(Deserialize, Clone)]
pub struct TweenRequest {
    pub property: String,
    pub to: f32,
    #[serde(default)]
    pub from: Option<f32>,
    pub duration: f32,
    #[serde(default)]
    pub easing: Option<String>,
    #[serde(default)]
    pub tween_id: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct TweenSequenceRequest {
    pub steps: Vec<TweenStepRequest>,
    #[serde(default)]
    pub sequence_id: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct TweenStepRequest {
    pub property: String,
    pub to: f32,
    #[serde(default)]
    pub from: Option<f32>,
    pub duration: f32,
    #[serde(default)]
    pub easing: Option<String>,
}

// === Screen Effect API types ===

#[derive(Deserialize, Clone)]
pub struct ScreenEffectRequest {
    pub effect: String,
    pub duration: f32,
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    #[serde(default)]
    pub alpha: Option<f32>,
}

// === Gamepad API types ===

#[derive(Deserialize, Clone)]
pub struct GamepadConfigRequest {
    #[serde(default)]
    pub deadzone: Option<f32>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Serialize, Clone)]
pub struct GamepadConfigResponse {
    pub enabled: bool,
    pub deadzone: f32,
    pub connected_count: usize,
}

// === Lighting API types ===

#[derive(Deserialize, Clone)]
pub struct LightingConfigRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub ambient_intensity: Option<f32>,
    #[serde(default)]
    pub ambient_color: Option<[f32; 3]>,
}

#[derive(Serialize, Clone)]
pub struct LightingStateResponse {
    pub enabled: bool,
    pub ambient_intensity: f32,
    pub ambient_color: [f32; 3],
    pub light_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_of_day: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_phase: Option<String>,
}

// === Tint API types ===

#[derive(Deserialize, Clone)]
pub struct TintRequest {
    #[serde(default = "default_tint_color")]
    pub color: [f32; 4],
    #[serde(default)]
    pub flash_color: Option<[f32; 4]>,
    #[serde(default)]
    pub flash_frames: u32,
}

// === Trail API types ===

#[derive(Deserialize, Clone)]
pub struct TrailRequest {
    #[serde(default = "default_trail_interval")]
    pub interval: u32,
    #[serde(default = "default_trail_duration")]
    pub duration: f32,
    #[serde(default = "default_f32_one")]
    pub alpha_start: f32,
    #[serde(default)]
    pub alpha_end: f32,
}

// === Input Bindings API types ===

#[derive(Serialize, Deserialize, Clone)]
pub struct InputBindingsRequest {
    #[serde(default)]
    pub keyboard: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub gamepad: HashMap<String, Vec<String>>,
}

#[derive(Serialize, Clone)]
pub struct InputBindingsResponse {
    pub keyboard: HashMap<String, Vec<String>>,
    pub gamepad: HashMap<String, Vec<String>>,
}

// === Day/Night Cycle API types ===

#[derive(Deserialize, Clone)]
pub struct DayNightRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub time_of_day: Option<f32>,
    #[serde(default)]
    pub speed: Option<f32>,
    #[serde(default)]
    pub phases: Option<Vec<DayPhaseRequest>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DayPhaseRequest {
    pub name: String,
    pub start_hour: f32,
    pub ambient_intensity: f32,
    #[serde(default = "default_light_color")]
    pub ambient_color: [f32; 3],
}

#[derive(Serialize, Clone)]
pub struct DayNightResponse {
    pub enabled: bool,
    pub time_of_day: f32,
    pub speed: f32,
    pub current_phase: String,
    pub phases: Vec<DayPhaseRequest>,
}

// === World Text API types ===

#[derive(Deserialize, Clone)]
pub struct WorldTextRequest {
    pub x: f32,
    pub y: f32,
    pub text: String,
    #[serde(default = "default_world_text_font_size")]
    pub font_size: f32,
    #[serde(default = "default_tint_color")]
    pub color: [f32; 4],
    #[serde(default)]
    pub duration: Option<f32>,
    #[serde(default)]
    pub fade: bool,
    #[serde(default)]
    pub rise_speed: f32,
    #[serde(default)]
    pub owner_id: Option<u64>,
}

fn default_world_text_font_size() -> f32 {
    16.0
}

// === State Machine API types ===

#[derive(Deserialize, Clone)]
pub struct StateTransitionRequest {
    pub state: String,
}

#[derive(Serialize, Clone)]
pub struct StateMachineResponse {
    pub current: String,
    pub previous: Option<String>,
    pub entered_at_frame: u64,
    pub states: Vec<String>,
}

// === Auto-Tile API types ===

#[derive(Deserialize, Clone)]
pub struct AutoTileRequest {
    pub rules: HashMap<String, AutoTileSetDef>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AutoTileSetDef {
    pub base_tile_id: u8,
    /// 4-bit bitmask (N/S/E/W) -> sprite index.
    pub variants: HashMap<u8, usize>,
}

// === 8-bit Material Auto-Tile API types ===

/// Request to register a terrain material with 8-bit autotiling.
#[derive(Deserialize, Clone)]
pub struct TerrainMaterialRequest {
    /// Human-readable name (e.g. "grass").
    pub name: String,
    /// Which tile type ID triggers this material's autotiling.
    pub tile_id: u8,
    /// Path to the autotile atlas PNG (13 or 20 frames).
    pub atlas: String,
    /// Frame width in pixels.
    #[serde(default = "default_frame_width")]
    pub frame_width: u32,
    /// Frame height in pixels.
    #[serde(default = "default_frame_height")]
    pub frame_height: u32,
    /// Optional custom slot-to-frame mapping (13 entries).
    /// If omitted, uses standard layout: fill=0, edge_N=1, ..., inner_SW=12.
    #[serde(default)]
    pub slots: Option<Vec<u16>>,
    /// Whether to enable 8-bit autotiling for this material.
    /// Default: true. Set to false for base materials (e.g. dirt) that just show a fill tile.
    #[serde(default = "default_autotile")]
    pub autotile: bool,
    /// Number of atlas columns (frame slots). Default: 13 (classic layout).
    /// Set to 20 for extended atlases with endcap/lane/full_surround slots.
    /// Frames beyond this count are clamped back to 0 (fill).
    #[serde(default)]
    pub columns: Option<u32>,
    /// Whether this material's tiles should be solid (unwalkable).
    /// Default: false (walkable).
    #[serde(default)]
    pub solid: Option<bool>,
}

fn default_autotile() -> bool { true }

fn default_frame_width() -> u32 { 128 }
fn default_frame_height() -> u32 { 256 }

/// Pre-computed 8-bit autotile rule: mask8 -> atlas frame index.
#[derive(Serialize, Deserialize, Clone)]
pub struct MaterialAutoTileRule {
    pub name: String,
    pub base_tile_id: u8,
    /// 256-entry lookup: mask8 value -> atlas frame index.
    pub mask_to_frame: Vec<u16>,
}

// === Tile Layer API types ===

#[derive(Deserialize, Clone)]
pub struct TileLayerRequest {
    pub name: String,
    pub tiles: Vec<u8>,
    #[serde(default)]
    pub z_offset: f32,
}

#[derive(Serialize, Clone)]
pub struct TileLayersResponse {
    pub layers: Vec<TileLayerInfo>,
}

#[derive(Serialize, Clone)]
pub struct TileLayerInfo {
    pub name: String,
    pub z_offset: f32,
    pub tile_count: usize,
}

// === Entity Pool API types ===

#[derive(Deserialize, Clone)]
pub struct PoolInitRequest {
    pub pool_name: String,
    pub preset: String,
    pub count: usize,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Deserialize, Clone)]
pub struct PoolAcquireRequest {
    pub pool_name: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Clone)]
pub struct PoolStatusResponse {
    pub pools: Vec<PoolInfo>,
}

#[derive(Serialize, Clone)]
pub struct PoolInfo {
    pub name: String,
    pub available: usize,
    pub active: usize,
}

// === Parallax API types ===

#[derive(Deserialize, Clone)]
pub struct ParallaxRequest {
    pub layers: Vec<crate::parallax::ParallaxLayerDef>,
}

#[derive(Serialize, Clone)]
pub struct ParallaxResponse {
    pub layers: Vec<crate::parallax::ParallaxLayerDef>,
}

// === Weather API types ===

#[derive(Deserialize, Clone)]
pub struct WeatherRequest {
    pub weather_type: String,
    #[serde(default = "default_f32_one")]
    pub intensity: f32,
    #[serde(default)]
    pub wind: f32,
}

#[derive(Serialize, Clone)]
pub struct WeatherResponse {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather_type: Option<String>,
    pub intensity: f32,
    pub wind: f32,
}

// === Inventory API types ===

#[derive(Deserialize, Clone)]
pub struct ItemDefineRequest {
    pub items: HashMap<String, crate::inventory::ItemDef>,
}

#[derive(Deserialize, Clone)]
pub struct InventoryActionRequest {
    pub action: String,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default = "default_item_count")]
    pub count: u32,
}

fn default_item_count() -> u32 {
    1
}

#[derive(Serialize, Clone)]
pub struct InventoryResponse {
    pub slots: Vec<crate::inventory::ItemSlot>,
    pub max_slots: usize,
}

// === Cutscene API types ===

#[derive(Deserialize, Clone)]
pub struct CutsceneDefineRequest {
    pub name: String,
    pub steps: Vec<crate::cutscene::CutsceneStep>,
}

#[derive(Deserialize, Clone)]
pub struct CutscenePlayRequest {
    pub name: String,
}

#[derive(Serialize, Clone)]
pub struct CutsceneStateResponse {
    pub playing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_steps: Option<usize>,
    pub defined_cutscenes: Vec<String>,
}

// === Spatial Grid (scene_describe ?grid=N) ===

#[derive(Serialize, Clone)]
pub struct SpatialGrid {
    pub rows: usize,
    pub cols: usize,
    pub cell_width: f32,
    pub cell_height: f32,
    pub cells: Vec<SpatialGridCell>,
}

#[derive(Serialize, Clone)]
pub struct SpatialGridCell {
    pub row: usize,
    pub col: usize,
    pub world_min: [f32; 2],
    pub world_max: [f32; 2],
    pub tile_types: Vec<String>,
    pub entity_ids: Vec<u64>,
    pub entity_tags: Vec<String>,
    pub entity_count: usize,
}

// === Screenshot Analysis ===

#[derive(Serialize, Clone)]
pub struct ScreenshotAnalysis {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub quadrant_colors: Vec<QuadrantInfo>,
    pub entity_bboxes: Vec<EntityBBox>,
    pub overlap_pairs: Vec<(u64, u64)>,
}

#[derive(Serialize, Clone)]
pub struct QuadrantInfo {
    pub name: String,
    pub avg_color: [u8; 3],
    pub avg_brightness: f32,
}

#[derive(Serialize, Clone)]
pub struct EntityBBox {
    pub id: u64,
    pub screen_x: f32,
    pub screen_y: f32,
    pub width: f32,
    pub height: f32,
}

// === Screenshot Diff ===

#[derive(Deserialize, Clone)]
pub struct ScreenshotDiffRequest {
    #[serde(default)]
    pub baseline_path: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct ScreenshotDiffResult {
    pub diff_percentage: f32,
    pub quadrant_diffs: Vec<QuadrantDiff>,
    pub baseline_path: String,
    pub current_path: String,
}

#[derive(Serialize, Clone)]
pub struct QuadrantDiff {
    pub name: String,
    pub diff_percentage: f32,
}

// === Gameplay Telemetry ===

#[derive(Serialize, Deserialize, Clone, Default, Resource)]
pub struct GameplayTelemetry {
    pub death_locations: Vec<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_reached_at: Option<u64>,
    pub input_counts: HashMap<String, u64>,
    pub entity_count_samples: Vec<(u64, usize)>,
    pub damage_taken: f32,
    pub damage_dealt: f32,
    pub pickups_collected: u64,
    pub total_frames: u64,
}

// === Scenario Testing ===

#[derive(Deserialize, Clone)]
pub struct ScenarioRequest {
    pub setup: Vec<ScenarioStep>,
    pub frames: u32,
    #[serde(default)]
    pub inputs: Vec<crate::simulation::SimInput>,
    pub assertions: Vec<Assertion>,
}

#[derive(Deserialize, Clone)]
pub struct ScenarioStep {
    pub action: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Deserialize, Clone)]
pub struct Assertion {
    pub check: String,
    #[serde(default)]
    pub expected: serde_json::Value,
}

#[derive(Serialize, Clone)]
pub struct ScenarioResult {
    pub passed: bool,
    pub assertions: Vec<AssertionResult>,
    pub frames_run: u32,
    pub events: Vec<crate::events::GameEvent>,
    pub final_vars: serde_json::Value,
}

#[derive(Serialize, Clone)]
pub struct AssertionResult {
    pub check: String,
    pub passed: bool,
    pub expected: serde_json::Value,
    pub actual: serde_json::Value,
}

// === World Simulation ===

#[derive(Deserialize, Clone)]
pub struct WorldSimRequest {
    pub frames: u32,
    #[serde(default)]
    pub record_interval: Option<u32>,
    #[serde(default)]
    pub inputs: Vec<crate::simulation::SimInput>,
    #[serde(default)]
    pub real: bool,
}

// === Playtest ===

#[derive(Deserialize, Clone)]
pub struct PlaytestRequest {
    #[serde(default = "default_playtest_frames")]
    pub frames: u32,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
}

fn default_playtest_frames() -> u32 {
    600
}

#[derive(Serialize, Clone)]
pub struct PlaytestResult {
    pub frames_played: u32,
    pub alive: bool,
    pub goal_reached: bool,
    pub deaths: Vec<PlaytestEvent>,
    pub damage_taken: f32,
    pub distance_traveled: f32,
    pub tiles_explored: usize,
    pub events: Vec<PlaytestEvent>,
    pub stuck_count: u32,
    pub input_summary: HashMap<String, u32>,
    pub difficulty_rating: String,
    pub notes: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct PlaytestEvent {
    pub frame: u32,
    pub event_type: String,
    pub x: f32,
    pub y: f32,
    pub detail: String,
}

#[derive(Serialize, Clone)]
pub struct WorldSimResult {
    pub frames_run: u32,
    pub snapshots: Vec<WorldSimSnapshot>,
    pub events: Vec<crate::events::GameEvent>,
    pub script_errors: Vec<crate::scripting::ScriptError>,
    pub final_vars: serde_json::Value,
}

#[derive(Serialize, Clone)]
pub struct WorldSimSnapshot {
    pub frame: u32,
    pub entities: Vec<EntityInfo>,
    pub vars: serde_json::Value,
}

// === Enriched Sim Entities ===

#[derive(Serialize, Deserialize, Clone)]
pub struct SimEntity {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    #[serde(default = "default_sim_entity_size")]
    pub width: f32,
    #[serde(default = "default_sim_entity_size")]
    pub height: f32,
    #[serde(default)]
    pub entity_type: String,
    #[serde(default)]
    pub damage: Option<f32>,
}

fn default_sim_entity_size() -> f32 {
    16.0
}

// === Import Result ===

#[derive(Serialize, Clone)]
pub struct ImportResult {
    pub entities_spawned: usize,
    pub scripts_loaded: usize,
    pub scripts_failed: Vec<String>,
    pub config_applied: bool,
    pub tilemap_applied: bool,
    pub animation_graphs: usize,
    pub sprite_sheets: usize,
    pub warnings: Vec<String>,
}

// === Atomic Build ===

#[derive(Deserialize, Clone)]
pub struct BuildRequest {
    #[serde(default)]
    pub config: Option<GameConfig>,
    #[serde(default)]
    pub tilemap: Option<SetLevelRequest>,
    #[serde(default)]
    pub entities: Vec<EntitySpawnRequest>,
    #[serde(default)]
    pub scripts: HashMap<String, String>,
    #[serde(default)]
    pub global_scripts: Vec<String>,
    #[serde(default)]
    pub game_vars: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub animation_graphs: Option<HashMap<String, crate::animation::AnimationGraphDef>>,
    #[serde(default)]
    pub sprite_sheets: Option<HashMap<String, crate::sprites::SpriteSheetDef>>,
    #[serde(default)]
    pub presets: Option<HashMap<String, EntitySpawnRequest>>,
    #[serde(default)]
    pub validate_first: Option<Vec<String>>,
}

#[derive(Serialize, Clone)]
pub struct BuildResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_result: Option<ImportResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<crate::constraints::ValidateResult>,
    pub errors: Vec<String>,
}

// === Asset Pipeline ===

#[derive(Deserialize, Clone)]
pub struct AssetUploadRequest {
    pub name: String,
    pub data: String,
    #[serde(default)]
    pub asset_type: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct AssetInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Deserialize, Clone)]
pub struct AssetGenerateRequest {
    pub name: String,
    #[serde(default = "default_asset_gen_size")]
    pub width: u32,
    #[serde(default = "default_asset_gen_size")]
    pub height: u32,
    #[serde(default = "default_asset_color")]
    pub color: [u8; 3],
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
}

fn default_asset_gen_size() -> u32 {
    32
}

fn default_asset_color() -> [u8; 3] {
    [128, 128, 128]
}

// === Entity Mutation API types ===

#[derive(Deserialize)]
pub struct EntityPositionRequest {
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize)]
pub struct EntityVelocityRequest {
    pub vx: f32,
    pub vy: f32,
}

#[derive(Deserialize)]
pub struct EntityTagsRequest {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Deserialize)]
pub struct EntityHealthRequest {
    #[serde(default)]
    pub current: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
}

// === Bulk Entity Operations ===

#[derive(Deserialize)]
pub struct BulkEntityRequest {
    /// Filter criteria (same as GET /entities query params)
    #[serde(default)]
    pub filter: BulkEntityFilter,
    /// Mutations to apply to each matched entity
    pub mutations: BulkEntityMutations,
}

#[derive(Deserialize, Default)]
pub struct BulkEntityFilter {
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub component: Option<String>,
    #[serde(default)]
    pub alive: Option<bool>,
    #[serde(default)]
    pub has_script: Option<bool>,
    #[serde(default)]
    pub entity_state: Option<String>,
    /// Explicit entity IDs to target (overrides other filters)
    #[serde(default)]
    pub ids: Option<Vec<u64>>,
}

#[derive(Deserialize, Default)]
pub struct BulkEntityMutations {
    #[serde(default)]
    pub health_current: Option<f32>,
    #[serde(default)]
    pub health_max: Option<f32>,
    #[serde(default)]
    pub add_tags: Option<Vec<String>>,
    #[serde(default)]
    pub remove_tags: Option<Vec<String>>,
    #[serde(default)]
    pub contact_damage: Option<f32>,
    #[serde(default)]
    pub contact_knockback: Option<f32>,
    #[serde(default)]
    pub hitbox_active: Option<bool>,
    #[serde(default)]
    pub hitbox_damage: Option<f32>,
    #[serde(default)]
    pub alive: Option<bool>,
}

#[derive(Serialize, Clone)]
pub struct BulkEntityResult {
    pub matched: usize,
    pub mutated: usize,
}

// === Script Var Diff ===

#[derive(Serialize, Clone)]
pub struct ScriptVarDiff {
    pub changed: std::collections::HashMap<String, serde_json::Value>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub snapshot_id: u64,
}

// === Combat Component Mutation types ===

#[derive(Deserialize)]
pub struct EntityContactDamageRequest {
    #[serde(default)]
    pub amount: Option<f32>,
    #[serde(default)]
    pub cooldown_frames: Option<u32>,
    #[serde(default)]
    pub knockback: Option<f32>,
}

#[derive(Deserialize)]
pub struct EntityHitboxRequest {
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub damage: Option<f32>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
}

#[derive(Deserialize)]
pub struct TilemapQueryRequest {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

#[derive(Serialize, Clone)]
pub struct TilemapQueryResult {
    pub solid_tiles: Vec<TileQueryHit>,
    pub total_tiles: usize,
    pub solid_count: usize,
}

#[derive(Serialize, Clone)]
pub struct TileQueryHit {
    pub col: i32,
    pub row: i32,
    pub tile_id: u8,
    pub tile_type: String,
}

// === Window Config API types ===

#[derive(Deserialize, Clone)]
pub struct WindowConfigRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub background: Option<[f32; 3]>,
}

// === Health Check API types ===

#[derive(Serialize, Clone)]
pub struct HealthCheckResult {
    pub status: String,
    pub has_player: bool,
    pub entity_count: usize,
    pub script_error_count: usize,
    pub game_state: String,
    pub game_vars_count: usize,
    pub tilemap_set: bool,
    pub issues: Vec<String>,
}

// === Diagnose API types ===

#[derive(Serialize, Clone)]
pub struct DiagnoseResult {
    pub entity_count: usize,
    pub issues_count: usize,
    pub entities: Vec<EntityDiagnosis>,
}

#[derive(Serialize, Clone)]
pub struct EntityDiagnosis {
    pub id: u64,
    pub tags: Vec<String>,
    pub issues: Vec<ComponentIssue>,
}

#[derive(Serialize, Clone)]
pub struct ComponentIssue {
    pub component: String,
    pub severity: String,
    pub message: String,
    pub missing: Vec<String>,
}

// === Evaluation API types ===

#[derive(Serialize, Clone)]
pub struct EvaluationResult {
    pub scores: EvaluationScores,
    pub issues: Vec<String>,
    pub overall: String,
}

#[derive(Serialize, Clone)]
pub struct EvaluationScores {
    pub has_player: bool,
    pub has_enemies: bool,
    pub has_scripts: bool,
    pub script_errors: usize,
    pub entity_count: usize,
    pub tile_variety: usize,
    pub has_goal: bool,
    pub game_vars_count: usize,
}
