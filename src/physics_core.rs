use crate::components::{GameConfig, TileType};
use crate::tilemap::Tilemap;

/// Maximum downward velocity to prevent tunneling through floors at extreme speeds.
pub const MAX_FALL_SPEED: f32 = 800.0;

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Aabb {
    pub fn from_center(x: f32, y: f32, width: f32, height: f32) -> Self {
        let hw = width / 2.0;
        let hh = height / 2.0;
        Self {
            min_x: x - hw,
            min_y: y - hh,
            max_x: x + hw,
            max_y: y + hh,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct PhysicsCounters {
    pub collision_checks: u64,
}

#[derive(Clone, Copy)]
pub struct MotionResult {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
}

#[derive(Clone, Copy)]
pub struct MotionParams {
    pub tile_size: f32,
    pub dt: f32,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy)]
pub struct CollisionQuery {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub tile_size: f32,
    pub target: TileType,
}

#[derive(Clone, Copy)]
struct LandingProbe {
    x: f32,
    prev_y: f32,
    new_y: f32,
    width: f32,
    height: f32,
    tile_size: f32,
}

#[derive(Clone, Copy)]
pub struct PlatformMotion {
    pub prev_x: f32,
    pub prev_y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub width: f32,
    pub height: f32,
}

pub fn apply_gravity(vy: &mut f32, grounded: bool, gravity: f32, fall_multiplier: f32, dt: f32) {
    if grounded {
        return;
    }
    let mult = if *vy < 0.0 { fall_multiplier } else { 1.0 };
    *vy -= gravity * mult * dt;
    *vy = vy.max(-MAX_FALL_SPEED);
}

pub fn horizontal_velocity(left: bool, right: bool, speed: f32) -> f32 {
    let mut dir = 0.0;
    if left {
        dir -= 1.0;
    }
    if right {
        dir += 1.0;
    }
    dir * speed
}

pub fn update_jump_buffer(jump_just_pressed: bool, jump_buffer: &mut u32, jump_buffer_frames: u32) {
    if jump_just_pressed {
        *jump_buffer = jump_buffer_frames;
    }
    if *jump_buffer > 0 {
        *jump_buffer -= 1;
    }
}

pub fn try_jump(
    grounded: bool,
    coyote: &mut u32,
    jump_buffer: &mut u32,
    jump_just_pressed: bool,
    jump_velocity: f32,
    vy: &mut f32,
) -> bool {
    let can_jump = grounded || *coyote > 0;
    let wants_jump = *jump_buffer > 0 || jump_just_pressed;
    if can_jump && wants_jump {
        *vy = jump_velocity;
        *coyote = 0;
        *jump_buffer = 0;
        return true;
    }
    false
}

pub fn apply_variable_jump(vy: &mut f32, jump_pressed: bool, variable_height: bool) {
    if variable_height && !jump_pressed && *vy > 0.0 {
        *vy *= 0.5;
    }
}

pub fn apply_surface_friction(vx: &mut f32, friction: f32) {
    let decay = 1.0 - friction.clamp(0.0, 1.0) * 0.25;
    *vx *= decay.clamp(0.0, 1.0);
    if vx.abs() < 0.1 {
        *vx = 0.0;
    }
}

pub fn surface_friction(
    tilemap: &Tilemap,
    config: &GameConfig,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> f32 {
    let ts = config.tile_size.max(0.0001);
    let min_x = x - width * 0.5 + 0.5;
    let max_x = x + width * 0.5 - 0.5;
    let probe_y = y - height * 0.5 - 0.25;
    let min_tx = (min_x / ts).floor() as i32;
    let max_tx = (max_x / ts).floor() as i32;
    let ty = (probe_y / ts).floor() as i32;

    let mut total = 0.0f32;
    let mut count = 0u32;
    for tx in min_tx..=max_tx {
        let id = tilemap.get_tile(tx, ty);
        total += config.tile_types.friction(id);
        count = count.saturating_add(1);
    }
    if count == 0 {
        1.0
    } else {
        total / count as f32
    }
}

pub fn rider_on_platform_top(
    rider_x: f32,
    rider_y: f32,
    rider_width: f32,
    rider_height: f32,
    motion: &PlatformMotion,
) -> bool {
    let rider_bottom = rider_y - rider_height * 0.5;
    let rider_min_x = rider_x - rider_width * 0.5;
    let rider_max_x = rider_x + rider_width * 0.5;
    let platform_top = motion.prev_y + motion.height * 0.5;
    let platform_min_x = motion.prev_x - motion.width * 0.5;
    let platform_max_x = motion.prev_x + motion.width * 0.5;
    let standing_on_top = (rider_bottom - platform_top).abs() <= 1.5;
    let overlaps_x = rider_max_x > platform_min_x && rider_min_x < platform_max_x;
    standing_on_top && overlaps_x
}

pub fn resolve_motion(
    tilemap: &Tilemap,
    params: MotionParams,
    counters: &mut PhysicsCounters,
) -> MotionResult {
    let MotionParams {
        tile_size,
        dt,
        x,
        y,
        vx,
        vy,
        width,
        height,
    } = params;
    let mut out_x = x;
    let mut out_y = y;
    let mut out_vx = vx;
    let mut out_vy = vy;

    let dx = vx * dt;
    let dy = vy * dt;

    let new_x = out_x + dx;
    let x_aabb = Aabb::from_center(new_x, out_y, width, height);
    if !collides_solid(tilemap, &x_aabb, tile_size, counters) {
        out_x = new_x;
    } else {
        if dx > 0.0 {
            let tile_x = (x_aabb.max_x / tile_size).floor() as i32;
            out_x = tile_x as f32 * tile_size - width / 2.0 - 0.01;
        } else if dx < 0.0 {
            let tile_x = (x_aabb.min_x / tile_size).floor() as i32;
            out_x = (tile_x + 1) as f32 * tile_size + width / 2.0 + 0.01;
        }
        out_vx = 0.0;
    }

    let new_y = out_y + dy;
    let y_aabb = Aabb::from_center(out_x, new_y, width, height);
    let platform_landing = if dy < 0.0 {
        find_platform_landing(
            tilemap,
            LandingProbe {
                x: out_x,
                prev_y: out_y,
                new_y,
                width,
                height,
                tile_size,
            },
            counters,
        )
    } else {
        None
    };
    let slope_landing = if dy < 0.0 {
        find_slope_landing(
            tilemap,
            LandingProbe {
                x: out_x,
                prev_y: out_y,
                new_y,
                width,
                height,
                tile_size,
            },
            counters,
        )
    } else {
        None
    };
    let landing_y = platform_landing
        .into_iter()
        .chain(slope_landing)
        .fold(None, |acc: Option<f32>, y| {
            Some(acc.map_or(y, |v| v.max(y)))
        });

    if !collides_solid(tilemap, &y_aabb, tile_size, counters) && landing_y.is_none() {
        out_y = new_y;
    } else {
        if let Some(land_y) = landing_y {
            out_y = land_y;
        } else if dy < 0.0 {
            let tile_y = (y_aabb.min_y / tile_size).floor() as i32;
            out_y = (tile_y + 1) as f32 * tile_size + height / 2.0;
        } else if dy > 0.0 {
            let tile_y = (y_aabb.max_y / tile_size).floor() as i32;
            out_y = tile_y as f32 * tile_size - height / 2.0 - 0.01;
        }
        out_vy = 0.0;
    }

    MotionResult {
        x: out_x,
        y: out_y,
        vx: out_vx,
        vy: out_vy,
    }
}

pub fn compute_grounded(
    tilemap: &Tilemap,
    tile_size: f32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    counters: &mut PhysicsCounters,
) -> bool {
    let check_y = y - height / 2.0 - 0.5;
    let left_x = x - width / 2.0 + 1.0;
    let right_x = x + width / 2.0 - 1.0;

    let left_tile_x = (left_x / tile_size).floor() as i32;
    let right_tile_x = (right_x / tile_size).floor() as i32;
    let tile_y = (check_y / tile_size).floor() as i32;

    for tx in left_tile_x..=right_tile_x {
        counters.collision_checks = counters.collision_checks.saturating_add(1);
        let tile = tilemap.get(tx, tile_y);
        if tile.is_solid() || tile.is_platform() {
            return true;
        }
        if tile.is_slope() {
            let bottom = y - height / 2.0;
            if let Some(surface) = slope_surface_y(tile, tx, tile_y, x, tile_size) {
                if bottom >= surface - 1.0 && bottom <= surface + 1.5 {
                    return true;
                }
            }
        }
    }
    false
}

pub fn update_coyote_timer(grounded: bool, coyote: &mut u32, coyote_frames: u32) {
    if grounded {
        *coyote = coyote_frames;
    } else if *coyote > 0 {
        *coyote -= 1;
    }
}

pub fn collides_type(
    tilemap: &Tilemap,
    query: CollisionQuery,
    counters: &mut PhysicsCounters,
) -> bool {
    let CollisionQuery {
        x,
        y,
        width,
        height,
        tile_size,
        target,
    } = query;
    let aabb = Aabb::from_center(x, y, width, height);
    let min_tx = (aabb.min_x / tile_size).floor() as i32;
    let max_tx = ((aabb.max_x - 0.01) / tile_size).floor() as i32;
    let min_ty = (aabb.min_y / tile_size).floor() as i32;
    let max_ty = ((aabb.max_y - 0.01) / tile_size).floor() as i32;

    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            counters.collision_checks = counters.collision_checks.saturating_add(1);
            if tilemap.get(tx, ty) == target {
                let t_min_x = tx as f32 * tile_size;
                let t_min_y = ty as f32 * tile_size;
                let t_max_x = t_min_x + tile_size;
                let t_max_y = t_min_y + tile_size;
                if aabb.max_x > t_min_x
                    && aabb.min_x < t_max_x
                    && aabb.max_y > t_min_y
                    && aabb.min_y < t_max_y
                {
                    return true;
                }
            }
        }
    }
    false
}

fn collides_solid(
    tilemap: &Tilemap,
    aabb: &Aabb,
    tile_size: f32,
    counters: &mut PhysicsCounters,
) -> bool {
    let min_tx = (aabb.min_x / tile_size).floor() as i32;
    let max_tx = ((aabb.max_x - 0.01) / tile_size).floor() as i32;
    let min_ty = (aabb.min_y / tile_size).floor() as i32;
    let max_ty = ((aabb.max_y - 0.01) / tile_size).floor() as i32;

    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            counters.collision_checks = counters.collision_checks.saturating_add(1);
            if tilemap.is_solid(tx, ty) {
                let t_min_x = tx as f32 * tile_size;
                let t_min_y = ty as f32 * tile_size;
                let t_max_x = t_min_x + tile_size;
                let t_max_y = t_min_y + tile_size;
                if aabb.max_x > t_min_x
                    && aabb.min_x < t_max_x
                    && aabb.max_y > t_min_y
                    && aabb.min_y < t_max_y
                {
                    return true;
                }
            }
        }
    }
    false
}

fn find_platform_landing(
    tilemap: &Tilemap,
    probe: LandingProbe,
    counters: &mut PhysicsCounters,
) -> Option<f32> {
    let LandingProbe {
        x,
        prev_y,
        new_y,
        width,
        height,
        tile_size,
    } = probe;
    let prev_bottom = prev_y - height / 2.0;
    let new_bottom = new_y - height / 2.0;
    if prev_bottom <= new_bottom {
        return None;
    }

    let left_x = x - width / 2.0 + 0.01;
    let right_x = x + width / 2.0 - 0.01;
    let left_tile_x = (left_x / tile_size).floor() as i32;
    let right_tile_x = (right_x / tile_size).floor() as i32;
    let min_ty = (new_bottom / tile_size).floor() as i32;
    let max_ty = ((prev_bottom - 0.01) / tile_size).floor() as i32;

    let mut best_top: Option<f32> = None;
    for ty in min_ty..=max_ty {
        let platform_top = (ty + 1) as f32 * tile_size;
        if prev_bottom < platform_top - 0.01 || new_bottom > platform_top {
            continue;
        }
        for tx in left_tile_x..=right_tile_x {
            counters.collision_checks = counters.collision_checks.saturating_add(1);
            if tilemap.get(tx, ty).is_platform() {
                best_top = Some(best_top.map_or(platform_top, |curr| curr.max(platform_top)));
            }
        }
    }
    best_top.map(|top| top + height / 2.0)
}

fn find_slope_landing(
    tilemap: &Tilemap,
    probe: LandingProbe,
    counters: &mut PhysicsCounters,
) -> Option<f32> {
    let LandingProbe {
        x,
        prev_y,
        new_y,
        width,
        height,
        tile_size,
    } = probe;
    let prev_bottom = prev_y - height / 2.0;
    let new_bottom = new_y - height / 2.0;
    if prev_bottom <= new_bottom {
        return None;
    }

    let left_x = x - width / 2.0 + 0.01;
    let right_x = x + width / 2.0 - 0.01;
    let left_tile_x = (left_x / tile_size).floor() as i32;
    let right_tile_x = (right_x / tile_size).floor() as i32;
    let min_ty = (new_bottom / tile_size).floor() as i32;
    let max_ty = ((prev_bottom - 0.01) / tile_size).floor() as i32;

    let mut best_surface: Option<f32> = None;
    for ty in min_ty..=max_ty {
        for tx in left_tile_x..=right_tile_x {
            counters.collision_checks = counters.collision_checks.saturating_add(1);
            let tile = tilemap.get(tx, ty);
            if !tile.is_slope() {
                continue;
            }
            let Some(surface) = slope_surface_y(tile, tx, ty, x, tile_size) else {
                continue;
            };
            if prev_bottom >= surface - 0.01 && new_bottom <= surface {
                best_surface = Some(best_surface.map_or(surface, |v| v.max(surface)));
            }
        }
    }
    best_surface.map(|surface| surface + height / 2.0)
}

fn slope_surface_y(tile: TileType, tx: i32, ty: i32, world_x: f32, tile_size: f32) -> Option<f32> {
    if !tile.is_slope() {
        return None;
    }
    let tile_left = tx as f32 * tile_size;
    let local_x = (world_x - tile_left).clamp(0.0, tile_size);
    let base_y = ty as f32 * tile_size;
    if tile.is_slope_up() {
        Some(base_y + local_x)
    } else if tile.is_slope_down() {
        Some(base_y + (tile_size - local_x))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tilemap_with_tiles(width: usize, height: usize, tiles: Vec<u8>) -> Tilemap {
        Tilemap {
            width,
            height,
            tiles,
            player_spawn: (8.0, 8.0),
            goal: None,
            ..Default::default()
        }
    }

    #[test]
    fn resolve_motion_stops_at_solid_tile() {
        let mut tiles = vec![0u8; 4 * 4];
        tiles[4 + 1] = TileType::Solid as u8;
        let tilemap = tilemap_with_tiles(4, 4, tiles);
        let mut counters = PhysicsCounters::default();
        let out = resolve_motion(
            &tilemap,
            MotionParams {
                tile_size: 16.0,
                dt: 1.0 / 60.0,
                x: 8.0,
                y: 24.0,
                vx: 1200.0,
                vy: 0.0,
                width: 12.0,
                height: 14.0,
            },
            &mut counters,
        );
        assert!(out.x < 28.0);
        assert_eq!(out.vx, 0.0);
    }

    #[test]
    fn grounded_detects_floor_tile() {
        let mut tiles = vec![0u8; 4 * 4];
        tiles[0] = TileType::Solid as u8;
        let tilemap = tilemap_with_tiles(4, 4, tiles);
        let mut counters = PhysicsCounters::default();
        let grounded = compute_grounded(&tilemap, 16.0, 8.0, 23.0, 12.0, 14.0, &mut counters);
        assert!(grounded);
    }

    #[test]
    fn grounded_detects_one_way_platform_tile() {
        let mut tiles = vec![0u8; 4 * 4];
        tiles[4 + 1] = TileType::Platform as u8;
        let tilemap = tilemap_with_tiles(4, 4, tiles);
        let mut counters = PhysicsCounters::default();
        let grounded = compute_grounded(&tilemap, 16.0, 24.0, 39.0, 12.0, 14.0, &mut counters);
        assert!(grounded);
    }

    #[test]
    fn resolve_motion_lands_on_one_way_platform_from_above() {
        let mut tiles = vec![0u8; 4 * 4];
        tiles[4 + 1] = TileType::Platform as u8;
        let tilemap = tilemap_with_tiles(4, 4, tiles);
        let mut counters = PhysicsCounters::default();
        let out = resolve_motion(
            &tilemap,
            MotionParams {
                tile_size: 16.0,
                dt: 1.0 / 60.0,
                x: 24.0,
                y: 45.0,
                vx: 0.0,
                vy: -1200.0,
                width: 12.0,
                height: 14.0,
            },
            &mut counters,
        );
        assert!((out.y - 39.0).abs() < 0.01);
        assert_eq!(out.vy, 0.0);
    }

    #[test]
    fn resolve_motion_passes_through_one_way_platform_from_below() {
        let mut tiles = vec![0u8; 4 * 4];
        tiles[4 + 1] = TileType::Platform as u8;
        let tilemap = tilemap_with_tiles(4, 4, tiles);
        let mut counters = PhysicsCounters::default();
        let out = resolve_motion(
            &tilemap,
            MotionParams {
                tile_size: 16.0,
                dt: 1.0 / 60.0,
                x: 24.0,
                y: 20.0,
                vx: 0.0,
                vy: 600.0,
                width: 12.0,
                height: 14.0,
            },
            &mut counters,
        );
        assert!(out.y > 20.0);
        assert_eq!(out.vy, 600.0);
    }

    #[test]
    fn resolve_motion_lands_on_slope_up() {
        let mut tiles = vec![0u8; 4 * 4];
        tiles[4 + 1] = TileType::SlopeUp as u8;
        let tilemap = tilemap_with_tiles(4, 4, tiles);
        let mut counters = PhysicsCounters::default();
        let out = resolve_motion(
            &tilemap,
            MotionParams {
                tile_size: 16.0,
                dt: 1.0 / 60.0,
                x: 20.0,
                y: 40.0,
                vx: 0.0,
                vy: -1200.0,
                width: 12.0,
                height: 14.0,
            },
            &mut counters,
        );
        assert!((out.y - 27.0).abs() < 0.1);
        assert_eq!(out.vy, 0.0);
    }

    #[test]
    fn resolve_motion_lands_on_slope_down() {
        let mut tiles = vec![0u8; 4 * 4];
        tiles[4 + 1] = TileType::SlopeDown as u8;
        let tilemap = tilemap_with_tiles(4, 4, tiles);
        let mut counters = PhysicsCounters::default();
        let out = resolve_motion(
            &tilemap,
            MotionParams {
                tile_size: 16.0,
                dt: 1.0 / 60.0,
                x: 20.0,
                y: 45.0,
                vx: 0.0,
                vy: -1200.0,
                width: 12.0,
                height: 14.0,
            },
            &mut counters,
        );
        assert!((out.y - 35.0).abs() < 0.1);
        assert_eq!(out.vy, 0.0);
    }

    #[test]
    fn coyote_timer_updates() {
        let mut coyote = 0;
        update_coyote_timer(true, &mut coyote, 5);
        assert_eq!(coyote, 5);
        update_coyote_timer(false, &mut coyote, 5);
        assert_eq!(coyote, 4);
    }

    #[test]
    fn collides_type_detects_overlap() {
        let mut tiles = vec![0u8; 4 * 4];
        tiles[4 + 1] = TileType::Spike as u8;
        let tilemap = tilemap_with_tiles(4, 4, tiles);
        let mut counters = PhysicsCounters::default();
        let hit = collides_type(
            &tilemap,
            CollisionQuery {
                x: 24.0,
                y: 24.0,
                width: 12.0,
                height: 14.0,
                tile_size: 16.0,
                target: TileType::Spike,
            },
            &mut counters,
        );
        assert!(hit);
    }
}
