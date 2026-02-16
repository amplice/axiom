use bevy::prelude::*;
use std::collections::HashSet;

/// Marks the player entity
#[derive(Component)]
pub struct Player;

/// Tile component for tilemap entities
#[derive(Component, Clone, Copy)]
pub struct Tile {
    #[allow(dead_code)]
    pub tile_type: TileType,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum TileType {
    Empty = 0,
    Solid = 1,
    Spike = 2,
    Goal = 3,
    Platform = 4,
    SlopeUp = 5,
    SlopeDown = 6,
    Ladder = 7,
}

impl TileType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => TileType::Solid,
            2 => TileType::Spike,
            3 => TileType::Goal,
            4 => TileType::Platform,
            5 => TileType::SlopeUp,
            6 => TileType::SlopeDown,
            7 => TileType::Ladder,
            _ => TileType::Empty,
        }
    }

    pub fn is_solid(self) -> bool {
        self == TileType::Solid
    }

    pub fn is_platform(self) -> bool {
        self == TileType::Platform
    }

    pub fn is_slope_up(self) -> bool {
        self == TileType::SlopeUp
    }

    pub fn is_slope_down(self) -> bool {
        self == TileType::SlopeDown
    }

    pub fn is_slope(self) -> bool {
        self.is_slope_up() || self.is_slope_down()
    }

    pub fn is_ground_like(self) -> bool {
        self.is_solid() || self.is_platform() || self.is_slope()
    }

    #[allow(dead_code)]
    pub fn is_ladder(self) -> bool {
        self == TileType::Ladder
    }
}

/// Grid position (integer tile coordinates)
#[derive(Component, Clone, Copy)]
pub struct GridPosition {
    #[allow(dead_code)]
    pub x: i32,
    #[allow(dead_code)]
    pub y: i32,
}

/// Sub-tile precision position (world units)
#[derive(Component, Clone, Copy, Default)]
pub struct GamePosition {
    pub x: f32,
    pub y: f32,
}

/// Velocity in world units per second
#[derive(Component, Clone, Copy, Default)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

/// Whether the entity is on the ground
#[derive(Component, Clone, Copy, Default)]
pub struct Grounded(pub bool);

/// Coyote time tracking (frames since leaving ground)
#[derive(Component, Default)]
pub struct CoyoteTimer(pub u32);

/// Input buffer for jump (frames since jump was pressed)
#[derive(Component, Default)]
pub struct JumpBuffer(pub u32);

/// Whether the entity is alive
#[derive(Component, Default)]
pub struct Alive(pub bool);

// === Behavior Components (Step 3) ===

/// AABB collider dimensions
#[derive(Component, Clone)]
pub struct Collider {
    pub width: f32,
    pub height: f32,
}

/// Entity affected by GameConfig.gravity
#[derive(Component)]
pub struct GravityBody;

/// Horizontal left/right movement
#[derive(Component, Clone)]
pub struct HorizontalMover {
    pub speed: f32,
    pub left_action: String,
    pub right_action: String,
}

/// Jump capability
#[derive(Component, Clone)]
pub struct Jumper {
    pub velocity: f32,
    pub action: String,
    pub fall_multiplier: f32,
    pub variable_height: bool,
    pub coyote_frames: u32,
    pub buffer_frames: u32,
}

/// 4/8-direction top-down movement
#[derive(Component, Clone)]
pub struct TopDownMover {
    pub speed: f32,
    pub up_action: String,
    pub down_action: String,
    pub left_action: String,
    pub right_action: String,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum PathType {
    TopDown,
    Platformer,
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathFollower {
    pub target: Vec2,
    pub path: Vec<Vec2>,
    pub recalculate_interval: u32,
    pub path_type: PathType,
    pub speed: f32,
    pub frames_until_recalc: u32,
}

impl PathFollower {
    pub fn new(target: Vec2, path_type: PathType, recalculate_interval: u32, speed: f32) -> Self {
        let interval = recalculate_interval.max(1);
        Self {
            target,
            path: Vec::new(),
            recalculate_interval: interval,
            path_type,
            speed,
            frames_until_recalc: 0,
        }
    }
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiBehavior {
    pub behavior: BehaviorType,
    pub state: AiState,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum BehaviorType {
    Patrol {
        waypoints: Vec<Vec2>,
        speed: f32,
    },
    Chase {
        target_tag: String,
        speed: f32,
        detection_radius: f32,
        give_up_radius: f32,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    Flee {
        threat_tag: String,
        speed: f32,
        detection_radius: f32,
        give_up_radius: f32,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    Guard {
        position: Vec2,
        radius: f32,
        chase_radius: f32,
        speed: f32,
        #[serde(default = "default_player_tag")]
        target_tag: String,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    Wander {
        speed: f32,
        radius: f32,
        pause_frames: u32,
    },
    Custom(String),
}

fn default_player_tag() -> String {
    "player".to_string()
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum AiState {
    Idle,
    Patrolling { waypoint_index: usize },
    Chasing { target_id: u64 },
    Fleeing { threat_id: u64 },
    Attacking { target_id: u64 },
    Returning,
    Wandering { pause_frames: u32 },
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Tags(pub HashSet<String>);

#[derive(Component, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct NetworkId(pub u64);

#[derive(Resource, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct NextNetworkId(pub u64);

impl Default for NextNetworkId {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContactDamage {
    pub amount: f32,
    pub cooldown_frames: u32,
    pub knockback: f32,
    pub damage_tag: String,
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct Invincibility {
    pub frames_remaining: u32,
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggerZone {
    pub radius: f32,
    pub trigger_tag: String,
    pub event_name: String,
    pub one_shot: bool,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum PickupEffect {
    Heal(f32),
    ScoreAdd(i32),
    Custom(String),
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct Pickup {
    pub pickup_tag: String,
    pub effect: PickupEffect,
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct Projectile {
    pub speed: f32,
    pub direction: Vec2,
    pub lifetime_frames: u32,
    pub damage: f32,
    pub owner_id: u64,
    pub damage_tag: String,
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct Hitbox {
    pub width: f32,
    pub height: f32,
    pub offset: Vec2,
    pub active: bool,
    pub damage: f32,
    pub damage_tag: String,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlatformLoopMode {
    #[default]
    Loop,
    PingPong,
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct MovingPlatform {
    pub waypoints: Vec<Vec2>,
    pub speed: f32,
    #[serde(default)]
    pub loop_mode: PlatformLoopMode,
    #[serde(default)]
    pub current_waypoint: usize,
    #[serde(default = "default_platform_direction")]
    pub direction: i8,
    #[serde(default)]
    pub pause_frames: u32,
    #[serde(default)]
    pub pause_timer: u32,
    #[serde(default = "default_platform_carry_riders")]
    pub carry_riders: bool,
}

fn default_platform_direction() -> i8 {
    1
}

fn default_platform_carry_riders() -> bool {
    true
}

fn default_animation_speed() -> f32 {
    1.0
}

fn default_animation_state() -> String {
    "idle".to_string()
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnimationController {
    pub graph: String,
    #[serde(default = "default_animation_state")]
    pub state: String,
    #[serde(default)]
    pub frame: usize,
    #[serde(default)]
    pub timer: f32,
    #[serde(default = "default_animation_speed")]
    pub speed: f32,
    #[serde(default = "default_true")]
    pub playing: bool,
    #[serde(default = "default_true")]
    pub facing_right: bool,
    #[serde(default)]
    pub auto_from_velocity: bool,
}

fn default_true() -> bool {
    true
}

// === Tile Type Registry ===

pub const TILE_SOLID: u8 = 0x01;
pub const TILE_DAMAGE: u8 = 0x02;
pub const TILE_TRIGGER: u8 = 0x04;
pub const TILE_PLATFORM: u8 = 0x10;
pub const TILE_CLIMBABLE: u8 = 0x20;

fn default_tile_friction() -> f32 {
    1.0
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TileTypeDef {
    pub name: String,
    pub flags: u8,
    #[serde(default = "default_tile_friction")]
    pub friction: f32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TileTypeRegistry {
    pub types: Vec<TileTypeDef>,
}

impl Default for TileTypeRegistry {
    fn default() -> Self {
        Self {
            types: vec![
                TileTypeDef {
                    name: "empty".into(),
                    flags: 0,
                    friction: 1.0,
                },
                TileTypeDef {
                    name: "solid".into(),
                    flags: TILE_SOLID,
                    friction: 1.0,
                },
                TileTypeDef {
                    name: "spike".into(),
                    flags: TILE_DAMAGE,
                    friction: 1.0,
                },
                TileTypeDef {
                    name: "goal".into(),
                    flags: TILE_TRIGGER,
                    friction: 1.0,
                },
                TileTypeDef {
                    name: "platform".into(),
                    flags: TILE_PLATFORM,
                    friction: 1.0,
                },
                TileTypeDef {
                    name: "slope_up".into(),
                    flags: TILE_SOLID,
                    friction: 1.0,
                },
                TileTypeDef {
                    name: "slope_down".into(),
                    flags: TILE_SOLID,
                    friction: 1.0,
                },
                TileTypeDef {
                    name: "ladder".into(),
                    flags: TILE_CLIMBABLE,
                    friction: 1.0,
                },
            ],
        }
    }
}

impl TileTypeRegistry {
    #[allow(dead_code)]
    pub fn is_solid(&self, id: u8) -> bool {
        self.types
            .get(id as usize)
            .is_some_and(|t| t.flags & TILE_SOLID != 0)
    }

    #[allow(dead_code)]
    pub fn is_damage(&self, id: u8) -> bool {
        self.types
            .get(id as usize)
            .is_some_and(|t| t.flags & TILE_DAMAGE != 0)
    }

    #[allow(dead_code)]
    pub fn is_trigger(&self, id: u8) -> bool {
        self.types
            .get(id as usize)
            .is_some_and(|t| t.flags & TILE_TRIGGER != 0)
    }

    #[allow(dead_code)]
    pub fn is_platform(&self, id: u8) -> bool {
        self.types
            .get(id as usize)
            .is_some_and(|t| t.flags & TILE_PLATFORM != 0)
    }

    pub fn is_climbable(&self, id: u8) -> bool {
        self.types
            .get(id as usize)
            .is_some_and(|t| t.flags & TILE_CLIMBABLE != 0)
    }

    pub fn friction(&self, id: u8) -> f32 {
        self.types
            .get(id as usize)
            .map_or(1.0, |t| t.friction)
            .clamp(0.0, 1.0)
    }
}

// === GameConfig (replaces PhysicsConfig) ===

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct GameConfig {
    pub gravity: Vec2,
    pub tile_size: f32,
    pub tile_types: TileTypeRegistry,
    // Default platformer controller values used for spawning/tuning.
    pub move_speed: f32,
    pub jump_velocity: f32,
    pub fall_multiplier: f32,
    pub coyote_frames: u32,
    pub jump_buffer_frames: u32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            gravity: Vec2::new(0.0, -980.0),
            tile_size: 16.0,
            tile_types: TileTypeRegistry::default(),
            move_speed: 200.0,
            jump_velocity: 400.0,
            fall_multiplier: 1.5,
            coyote_frames: 5,
            jump_buffer_frames: 4,
        }
    }
}

impl GameConfig {
    /// Gravity magnitude (positive, for legacy code that used scalar gravity)
    pub fn gravity_magnitude(&self) -> f32 {
        self.gravity.length()
    }
}

/// Marks whether we're running in headless mode (no window/rendering)
#[derive(Resource)]
pub struct HeadlessMode(pub bool);
