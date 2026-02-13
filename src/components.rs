use bevy::prelude::*;

/// Marks the player entity
#[derive(Component)]
pub struct Player;

/// Tile component for tilemap entities
#[derive(Component, Clone, Copy)]
pub struct Tile {
    pub tile_type: TileType,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum TileType {
    Empty = 0,
    Solid = 1,
    Spike = 2,
    Goal = 3,
}

impl TileType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => TileType::Solid,
            2 => TileType::Spike,
            3 => TileType::Goal,
            _ => TileType::Empty,
        }
    }

    pub fn is_solid(self) -> bool {
        self == TileType::Solid
    }
}

/// Grid position (integer tile coordinates)
#[derive(Component, Clone, Copy)]
pub struct GridPosition {
    pub x: i32,
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

/// Physics constants (as a resource so they can be tuned)
#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhysicsConfig {
    pub gravity: f32,
    pub jump_velocity: f32,
    pub move_speed: f32,
    pub fall_multiplier: f32,
    pub coyote_frames: u32,
    pub jump_buffer_frames: u32,
    pub tile_size: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: 980.0,
            jump_velocity: 400.0,
            move_speed: 200.0,
            fall_multiplier: 1.5,
            coyote_frames: 5,
            jump_buffer_frames: 4,
            tile_size: 16.0,
        }
    }
}
