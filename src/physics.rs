use bevy::prelude::*;
use crate::components::*;
use crate::tilemap::Tilemap;

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (
            apply_gravity,
            player_movement,
            apply_velocity,
            check_grounded,
            update_coyote_timer,
        ).chain());
    }
}

const PLAYER_WIDTH: f32 = 12.0;
const PLAYER_HEIGHT: f32 = 14.0;

/// Player AABB in world space
struct Aabb {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl Aabb {
    fn from_center(x: f32, y: f32) -> Self {
        let hw = PLAYER_WIDTH / 2.0;
        let hh = PLAYER_HEIGHT / 2.0;
        Self {
            min_x: x - hw,
            min_y: y - hh,
            max_x: x + hw,
            max_y: y + hh,
        }
    }
}

fn tile_aabb(tx: i32, ty: i32, tile_size: f32) -> (f32, f32, f32, f32) {
    let x = tx as f32 * tile_size;
    let y = ty as f32 * tile_size;
    (x, y, x + tile_size, y + tile_size)
}

fn apply_gravity(
    config: Res<PhysicsConfig>,
    time: Res<Time<Fixed>>,
    mut query: Query<(&mut Velocity, &Grounded), With<Player>>,
) {
    let dt = time.delta_secs();
    for (mut vel, grounded) in query.iter_mut() {
        if !grounded.0 {
            let mult = if vel.y < 0.0 { config.fall_multiplier } else { 1.0 };
            vel.y -= config.gravity * mult * dt;
        }
    }
}

fn player_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    config: Res<PhysicsConfig>,
    mut query: Query<(&mut Velocity, &Grounded, &mut CoyoteTimer, &mut JumpBuffer), With<Player>>,
) {
    for (mut vel, grounded, mut coyote, mut jump_buf) in query.iter_mut() {
        // Horizontal
        let mut dir = 0.0f32;
        if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
            dir -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
            dir += 1.0;
        }
        vel.x = dir * config.move_speed;

        // Jump buffer
        if keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::KeyW) || keyboard.just_pressed(KeyCode::ArrowUp) {
            jump_buf.0 = config.jump_buffer_frames;
        }
        if jump_buf.0 > 0 {
            jump_buf.0 -= 1;
        }

        // Jump
        let can_jump = grounded.0 || coyote.0 > 0;
        let wants_jump = jump_buf.0 > 0 || keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::KeyW) || keyboard.just_pressed(KeyCode::ArrowUp);

        if can_jump && wants_jump {
            vel.y = config.jump_velocity;
            coyote.0 = 0;
            jump_buf.0 = 0;
        }

        // Variable jump height: release early = cut velocity
        if !keyboard.pressed(KeyCode::Space) && !keyboard.pressed(KeyCode::KeyW) && !keyboard.pressed(KeyCode::ArrowUp) {
            if vel.y > 0.0 {
                vel.y *= 0.5; // Cut upward velocity when jump button released
            }
        }
    }
}

fn apply_velocity(
    config: Res<PhysicsConfig>,
    time: Res<Time<Fixed>>,
    tilemap: Res<Tilemap>,
    mut query: Query<(&mut GamePosition, &mut Velocity, &mut Alive), With<Player>>,
) {
    let dt = time.delta_secs();
    let ts = config.tile_size;

    for (mut pos, mut vel, mut alive) in query.iter_mut() {
        // Move X first, then Y (separate axis resolution)
        let dx = vel.x * dt;
        let dy = vel.y * dt;

        // Resolve X
        let new_x = pos.x + dx;
        let aabb = Aabb::from_center(new_x, pos.y);
        if !collides_solid(&tilemap, &aabb, ts) {
            pos.x = new_x;
        } else {
            // Snap to edge of tile
            if dx > 0.0 {
                // Moving right, snap to left edge of blocking tile
                let tile_x = ((aabb.max_x) / ts).floor() as i32;
                pos.x = tile_x as f32 * ts - PLAYER_WIDTH / 2.0 - 0.01;
            } else if dx < 0.0 {
                // Moving left, snap to right edge of blocking tile
                let tile_x = ((aabb.min_x) / ts).floor() as i32;
                pos.x = (tile_x + 1) as f32 * ts + PLAYER_WIDTH / 2.0 + 0.01;
            }
            vel.x = 0.0;
        }

        // Resolve Y
        let new_y = pos.y + dy;
        let aabb = Aabb::from_center(pos.x, new_y);
        if !collides_solid(&tilemap, &aabb, ts) {
            pos.y = new_y;
        } else {
            if dy < 0.0 {
                // Falling, snap to top of tile
                let tile_y = ((aabb.min_y) / ts).floor() as i32;
                pos.y = (tile_y + 1) as f32 * ts + PLAYER_HEIGHT / 2.0;
            } else if dy > 0.0 {
                // Rising, snap to bottom of tile (head bonk)
                let tile_y = ((aabb.max_y) / ts).floor() as i32;
                pos.y = tile_y as f32 * ts - PLAYER_HEIGHT / 2.0 - 0.01;
            }
            vel.y = 0.0;
        }

        // Check spikes
        let aabb = Aabb::from_center(pos.x, pos.y);
        if collides_type(&tilemap, &aabb, ts, TileType::Spike) {
            alive.0 = false;
            // Respawn
            pos.x = tilemap.player_spawn.0;
            pos.y = tilemap.player_spawn.1;
            vel.x = 0.0;
            vel.y = 0.0;
            alive.0 = true;
        }

        // Check goal
        if collides_type(&tilemap, &aabb, ts, TileType::Goal) {
            // For now just print
            // In the full game this would trigger level completion
        }

        // Fall out of world → respawn
        if pos.y < -100.0 {
            pos.x = tilemap.player_spawn.0;
            pos.y = tilemap.player_spawn.1;
            vel.x = 0.0;
            vel.y = 0.0;
        }
    }
}

fn check_grounded(
    config: Res<PhysicsConfig>,
    tilemap: Res<Tilemap>,
    mut query: Query<(&GamePosition, &mut Grounded), With<Player>>,
) {
    let ts = config.tile_size;
    for (pos, mut grounded) in query.iter_mut() {
        // Check one pixel below player feet
        let check_y = pos.y - PLAYER_HEIGHT / 2.0 - 0.5;
        let left_x = pos.x - PLAYER_WIDTH / 2.0 + 1.0;
        let right_x = pos.x + PLAYER_WIDTH / 2.0 - 1.0;

        let left_tile_x = (left_x / ts).floor() as i32;
        let right_tile_x = (right_x / ts).floor() as i32;
        let tile_y = (check_y / ts).floor() as i32;

        let mut on_ground = false;
        for tx in left_tile_x..=right_tile_x {
            if tilemap.is_solid(tx, tile_y) {
                on_ground = true;
                break;
            }
        }
        grounded.0 = on_ground;
    }
}

fn update_coyote_timer(
    config: Res<PhysicsConfig>,
    mut query: Query<(&Grounded, &mut CoyoteTimer), With<Player>>,
) {
    for (grounded, mut coyote) in query.iter_mut() {
        if grounded.0 {
            coyote.0 = config.coyote_frames;
        } else if coyote.0 > 0 {
            coyote.0 -= 1;
        }
    }
}

/// Check if AABB overlaps any solid tiles
fn collides_solid(tilemap: &Tilemap, aabb: &Aabb, tile_size: f32) -> bool {
    let min_tx = (aabb.min_x / tile_size).floor() as i32;
    let max_tx = ((aabb.max_x - 0.01) / tile_size).floor() as i32;
    let min_ty = (aabb.min_y / tile_size).floor() as i32;
    let max_ty = ((aabb.max_y - 0.01) / tile_size).floor() as i32;

    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            if tilemap.is_solid(tx, ty) {
                // Check AABB overlap with this tile
                let (t_min_x, t_min_y, t_max_x, t_max_y) = tile_aabb(tx, ty, tile_size);
                if aabb.max_x > t_min_x && aabb.min_x < t_max_x
                    && aabb.max_y > t_min_y && aabb.min_y < t_max_y
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if AABB overlaps any tiles of a specific type
fn collides_type(tilemap: &Tilemap, aabb: &Aabb, tile_size: f32, target: TileType) -> bool {
    let min_tx = (aabb.min_x / tile_size).floor() as i32;
    let max_tx = ((aabb.max_x - 0.01) / tile_size).floor() as i32;
    let min_ty = (aabb.min_y / tile_size).floor() as i32;
    let max_ty = ((aabb.max_y - 0.01) / tile_size).floor() as i32;

    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            if tilemap.get(tx, ty) == target {
                let (t_min_x, t_min_y, t_max_x, t_max_y) = tile_aabb(tx, ty, tile_size);
                if aabb.max_x > t_min_x && aabb.min_x < t_max_x
                    && aabb.max_y > t_min_y && aabb.min_y < t_max_y
                {
                    return true;
                }
            }
        }
    }
    false
}
