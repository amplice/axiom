use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::components::{
    AiBehavior, AiState, BehaviorType, GameConfig, GamePosition, Grounded, Jumper, NetworkId,
    PathFollower, PathType, Tags, TileType, Velocity,
};
use crate::pathfinding;
use crate::spatial_hash::SpatialHash;
use crate::tilemap::Tilemap;

pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PathCache::default()).add_systems(
            FixedPreUpdate,
            (update_ai_behaviors, update_path_followers)
                .chain()
                .run_if(crate::game_runtime::gameplay_systems_enabled),
        );
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct PathCacheKey {
    from_tx: i32,
    from_ty: i32,
    to_tx: i32,
    to_ty: i32,
    tile_size_bits: u32,
    config_hash: u64,
}

#[derive(Resource)]
struct PathCache {
    top_down: HashMap<PathCacheKey, Option<Vec<Vec2>>>,
    platformer: HashMap<PathCacheKey, Option<Vec<Vec2>>>,
    max_entries: usize,
}

impl Default for PathCache {
    fn default() -> Self {
        Self {
            top_down: HashMap::new(),
            platformer: HashMap::new(),
            max_entries: 4096,
        }
    }
}

impl PathCache {
    fn clear(&mut self) {
        self.top_down.clear();
        self.platformer.clear();
    }

    fn prune_if_needed(&mut self) {
        if self.top_down.len() > self.max_entries {
            self.top_down.clear();
        }
        if self.platformer.len() > self.max_entries {
            self.platformer.clear();
        }
    }
}

#[derive(Clone)]
struct TargetSnapshot {
    pos: Vec2,
    id: u64,
    tags: HashSet<String>,
}

type PathFollowerQueryItem<'a> = (
    &'a GamePosition,
    &'a mut Velocity,
    Option<&'a Grounded>,
    Option<&'a Jumper>,
    &'a mut PathFollower,
);

fn update_ai_behaviors(
    tilemap: Res<Tilemap>,
    config: Res<GameConfig>,
    spatial: Option<Res<SpatialHash>>,
    targets: Query<(Entity, &GamePosition, &NetworkId, Option<&Tags>)>,
    mut ai_query: Query<(
        Entity,
        &GamePosition,
        &NetworkId,
        &mut AiBehavior,
        &mut PathFollower,
    )>,
) {
    let target_cache: HashMap<Entity, TargetSnapshot> = targets
        .iter()
        .map(|(entity, pos, nid, tags)| {
            (
                entity,
                TargetSnapshot {
                    pos: Vec2::new(pos.x, pos.y),
                    id: nid.0,
                    tags: tags.map(|t| t.0.clone()).unwrap_or_default(),
                },
            )
        })
        .collect();

    for (entity, pos, network_id, mut ai, mut follower) in ai_query.iter_mut() {
        let self_pos = Vec2::new(pos.x, pos.y);
        let behavior = ai.behavior.clone();
        match behavior {
            BehaviorType::Patrol { waypoints, speed } => {
                if waypoints.is_empty() {
                    continue;
                }
                let mut idx = match ai.state {
                    AiState::Patrolling { waypoint_index } => waypoint_index % waypoints.len(),
                    _ => 0,
                };
                if self_pos.distance(waypoints[idx]) <= 8.0 {
                    idx = (idx + 1) % waypoints.len();
                }
                ai.state = AiState::Patrolling {
                    waypoint_index: idx,
                };
                follower.target = waypoints[idx];
                follower.speed = speed;
                follower.path_type = PathType::Platformer;
            }
            BehaviorType::Chase {
                target_tag,
                speed,
                detection_radius,
                give_up_radius,
                require_line_of_sight,
            } => {
                let nearest = find_nearest_tagged(
                    &target_cache,
                    spatial.as_deref(),
                    entity,
                    self_pos,
                    detection_radius,
                    &target_tag,
                    require_line_of_sight.then_some((&tilemap, config.tile_size)),
                );
                if let Some((target_id, target_pos)) = nearest {
                    ai.state = AiState::Chasing { target_id };
                    follower.target = target_pos;
                    follower.speed = speed;
                    follower.path_type = PathType::TopDown;
                } else if let AiState::Chasing { .. } = ai.state {
                    if self_pos.distance(follower.target) > give_up_radius {
                        ai.state = AiState::Idle;
                        follower.path.clear();
                    }
                }
            }
            BehaviorType::Flee {
                threat_tag,
                speed,
                detection_radius,
                give_up_radius,
                require_line_of_sight,
            } => {
                let nearest = find_nearest_tagged(
                    &target_cache,
                    spatial.as_deref(),
                    entity,
                    self_pos,
                    detection_radius,
                    &threat_tag,
                    require_line_of_sight.then_some((&tilemap, config.tile_size)),
                );
                if let Some((threat_id, threat_pos)) = nearest {
                    ai.state = AiState::Fleeing { threat_id };
                    let away = (self_pos - threat_pos).normalize_or_zero();
                    let fallback = if away.length_squared() > 0.0 {
                        away
                    } else {
                        Vec2::X
                    };
                    let flee_distance = detection_radius.clamp(48.0, 240.0);
                    let mut target = self_pos + fallback * flee_distance;
                    target.x = target.x.clamp(0.0, tilemap.width as f32 * config.tile_size);
                    target.y = target
                        .y
                        .clamp(0.0, tilemap.height as f32 * config.tile_size);
                    follower.target = target;
                    follower.speed = speed;
                    follower.path_type = PathType::TopDown;
                } else if let AiState::Fleeing { .. } = ai.state {
                    let still_threatened = find_nearest_tagged(
                        &target_cache,
                        spatial.as_deref(),
                        entity,
                        self_pos,
                        give_up_radius,
                        &threat_tag,
                        require_line_of_sight.then_some((&tilemap, config.tile_size)),
                    );
                    if still_threatened.is_none() {
                        ai.state = AiState::Idle;
                        follower.path.clear();
                    }
                }
            }
            BehaviorType::Guard {
                position,
                radius,
                chase_radius,
                speed,
                target_tag,
                require_line_of_sight,
            } => {
                let nearest = find_nearest_tagged(
                    &target_cache,
                    spatial.as_deref(),
                    entity,
                    self_pos,
                    chase_radius,
                    &target_tag,
                    require_line_of_sight.then_some((&tilemap, config.tile_size)),
                );
                if let Some((target_id, target_pos)) = nearest {
                    ai.state = AiState::Chasing { target_id };
                    follower.target = target_pos;
                    follower.speed = speed;
                    follower.path_type = PathType::TopDown;
                } else if self_pos.distance(position) > radius {
                    ai.state = AiState::Returning;
                    follower.target = position;
                    follower.speed = speed;
                    follower.path_type = PathType::TopDown;
                } else {
                    ai.state = AiState::Idle;
                    follower.path.clear();
                }
            }
            BehaviorType::Wander {
                speed,
                radius,
                pause_frames,
            } => match ai.state {
                AiState::Wandering {
                    pause_frames: remaining,
                } if remaining > 0 => {
                    ai.state = AiState::Wandering {
                        pause_frames: remaining - 1,
                    };
                    follower.path.clear();
                }
                _ => {
                    let seed = network_id
                        .0
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add((self_pos.x * 13.0).to_bits() as u64)
                        .wrapping_add((self_pos.y * 29.0).to_bits() as u64);
                    let angle = ((seed % 628) as f32) / 100.0;
                    let offset = Vec2::new(angle.cos(), angle.sin()) * radius;
                    ai.state = AiState::Wandering { pause_frames };
                    follower.target = self_pos + offset;
                    follower.speed = speed;
                    follower.path_type = PathType::TopDown;
                }
            },
            BehaviorType::Custom(_) => {}
        }
    }
}

fn update_path_followers(
    tilemap: Res<Tilemap>,
    config: Res<GameConfig>,
    mut cache: ResMut<PathCache>,
    mut query: Query<PathFollowerQueryItem<'_>>,
) {
    let ts = config.tile_size;
    if tilemap.is_added() || tilemap.is_changed() {
        cache.clear();
    }
    for (pos, mut vel, grounded, jumper, mut follower) in query.iter_mut() {
        let pos_v = Vec2::new(pos.x, pos.y);
        let target_changed = match follower.path.last().copied() {
            Some(last) => last.distance(follower.target) > ts,
            None => true,
        };
        let needs_recalc =
            follower.path.is_empty() || follower.frames_until_recalc == 0 || target_changed;

        if needs_recalc {
            let key = make_path_cache_key(pos_v, follower.target, &config, follower.path_type);
            follower.path = match follower.path_type {
                PathType::TopDown => {
                    let path = cache
                        .top_down
                        .entry(key)
                        .or_insert_with(|| top_down_path(&tilemap, ts, pos_v, follower.target));
                    path.clone().unwrap_or_else(|| vec![follower.target])
                }
                PathType::Platformer => {
                    let path = cache.platformer.entry(key).or_insert_with(|| {
                        pathfinding::find_platformer_path_points(
                            &tilemap,
                            &config,
                            pos_v,
                            follower.target,
                        )
                    });
                    path.clone()
                        .unwrap_or_else(|| vec![Vec2::new(follower.target.x, pos.y)])
                }
            };
            follower.frames_until_recalc = follower.recalculate_interval.max(1);
            cache.prune_if_needed();
        } else if follower.frames_until_recalc > 0 {
            follower.frames_until_recalc -= 1;
        }

        while let Some(next) = follower.path.first().copied() {
            if pos_v.distance(next) <= 4.0 {
                follower.path.remove(0);
            } else {
                break;
            }
        }

        if let Some(next) = follower.path.first().copied() {
            match follower.path_type {
                PathType::TopDown => {
                    let dir = (next - pos_v).normalize_or_zero();
                    vel.x = dir.x * follower.speed;
                    vel.y = dir.y * follower.speed;
                }
                PathType::Platformer => {
                    let dx = next.x - pos_v.x;
                    vel.x = if dx.abs() <= 2.0 {
                        0.0
                    } else {
                        dx.signum() * follower.speed
                    };
                    let wants_jump = next.y > pos_v.y + ts * 0.6;
                    if wants_jump && grounded.is_some_and(|g| g.0) && vel.y <= 1.0 {
                        vel.y = jumper.map_or(380.0, |j| j.velocity);
                    }
                }
            }
        } else {
            vel.x = 0.0;
            if follower.path_type == PathType::TopDown {
                vel.y = 0.0;
            }
        }
    }
}

fn make_path_cache_key(
    from: Vec2,
    to: Vec2,
    config: &GameConfig,
    path_type: PathType,
) -> PathCacheKey {
    let ts = config.tile_size.max(0.0001);
    let from_tx = (from.x / ts).floor() as i32;
    let from_ty = (from.y / ts).floor() as i32;
    let to_tx = (to.x / ts).floor() as i32;
    let to_ty = (to.y / ts).floor() as i32;
    let config_hash = match path_type {
        PathType::TopDown => 0,
        PathType::Platformer => {
            let mut h = 0u64;
            h ^= config.move_speed.to_bits() as u64;
            h = h.rotate_left(7) ^ config.jump_velocity.to_bits() as u64;
            h = h.rotate_left(7) ^ config.gravity.x.to_bits() as u64;
            h = h.rotate_left(7) ^ config.gravity.y.to_bits() as u64;
            h = h.rotate_left(7) ^ config.fall_multiplier.to_bits() as u64;
            h
        }
    };
    PathCacheKey {
        from_tx,
        from_ty,
        to_tx,
        to_ty,
        tile_size_bits: ts.to_bits(),
        config_hash,
    }
}

fn find_nearest_tagged(
    targets: &HashMap<Entity, TargetSnapshot>,
    spatial: Option<&SpatialHash>,
    self_entity: Entity,
    from: Vec2,
    radius: f32,
    target_tag: &str,
    los: Option<(&Tilemap, f32)>,
) -> Option<(u64, Vec2)> {
    let candidate_entities: Vec<Entity> = if let Some(hash) = spatial {
        hash.query_radius(from.x, from.y, radius)
    } else {
        targets.keys().copied().collect()
    };

    let mut best: Option<(u64, Vec2, f32)> = None;
    for candidate in candidate_entities {
        if candidate == self_entity {
            continue;
        }
        let Some(snapshot) = targets.get(&candidate) else {
            continue;
        };
        if !snapshot.tags.contains(target_tag) {
            continue;
        }
        let dist = from.distance(snapshot.pos);
        if dist > radius {
            continue;
        }
        if let Some((tilemap, tile_size)) = los {
            if !has_line_of_sight_points(tilemap, tile_size, from, snapshot.pos) {
                continue;
            }
        }
        match best {
            Some((_, _, best_dist)) if dist >= best_dist => {}
            _ => best = Some((snapshot.id, snapshot.pos, dist)),
        }
    }

    best.map(|(id, pos, _)| (id, pos))
}

pub fn has_line_of_sight_points(tilemap: &Tilemap, tile_size: f32, from: Vec2, to: Vec2) -> bool {
    let delta = to - from;
    let dist = delta.length();
    if dist <= 0.001 {
        return true;
    }
    let dir = delta / dist;
    let mut d = 0.0f32;
    let step = (tile_size * 0.25).clamp(0.25, 4.0);
    while d <= dist {
        let p = from + dir * d;
        let tx = (p.x / tile_size).floor() as i32;
        let ty = (p.y / tile_size).floor() as i32;
        if tilemap.is_solid(tx, ty) {
            return false;
        }
        d += step;
    }
    true
}

pub fn find_top_down_path_points(
    tilemap: &Tilemap,
    tile_size: f32,
    from: Vec2,
    to: Vec2,
) -> Option<Vec<Vec2>> {
    top_down_path(tilemap, tile_size, from, to)
}

fn top_down_path(tilemap: &Tilemap, tile_size: f32, from: Vec2, to: Vec2) -> Option<Vec<Vec2>> {
    let start = (
        (from.x / tile_size).floor() as i32,
        (from.y / tile_size).floor() as i32,
    );
    let goal = (
        (to.x / tile_size).floor() as i32,
        (to.y / tile_size).floor() as i32,
    );
    if start == goal {
        return Some(vec![to]);
    }

    if !top_down_cell_walkable(tilemap, tile_size, goal.0, goal.1) {
        return None;
    }

    let mut queue = VecDeque::new();
    let mut parent: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    queue.push_back(start);
    parent.insert(start, start);

    let neighbors = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];

    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in neighbors {
            let nx = x + dx;
            let ny = y + dy;
            if dx != 0 && dy != 0 {
                let can_move_h = top_down_cell_walkable(tilemap, tile_size, x + dx, y);
                let can_move_v = top_down_cell_walkable(tilemap, tile_size, x, y + dy);
                if !can_move_h || !can_move_v {
                    continue;
                }
            }
            if !top_down_cell_walkable(tilemap, tile_size, nx, ny) {
                continue;
            }
            if parent.contains_key(&(nx, ny)) {
                continue;
            }
            parent.insert((nx, ny), (x, y));
            if (nx, ny) == goal {
                let mut tiles = vec![goal];
                let mut cur = goal;
                while cur != start {
                    cur = parent[&cur];
                    tiles.push(cur);
                }
                tiles.reverse();
                let mut points: Vec<Vec2> = tiles
                    .into_iter()
                    .skip(1)
                    .map(|(tx, ty)| {
                        Vec2::new((tx as f32 + 0.5) * tile_size, (ty as f32 + 0.5) * tile_size)
                    })
                    .collect();
                points.push(to);
                return Some(points);
            }
            queue.push_back((nx, ny));
        }
    }

    None
}

fn top_down_cell_walkable(tilemap: &Tilemap, tile_size: f32, tx: i32, ty: i32) -> bool {
    if tx < 0 || ty < 0 || tx >= tilemap.width as i32 || ty >= tilemap.height as i32 {
        return false;
    }
    let tile = tilemap.get(tx, ty);
    if tile.is_solid() || tile == TileType::Spike {
        return false;
    }

    // Approximate top-down actor footprint (12x14 at 16px tiles) so planned paths
    // remain traversable by default player-sized colliders.
    let half_w = tile_size * 0.375;
    let half_h = tile_size * 0.4375;
    let cx = (tx as f32 + 0.5) * tile_size;
    let cy = (ty as f32 + 0.5) * tile_size;
    let min_tx = ((cx - half_w) / tile_size).floor() as i32;
    let max_tx = ((cx + half_w) / tile_size).floor() as i32;
    let min_ty = ((cy - half_h) / tile_size).floor() as i32;
    let max_ty = ((cy + half_h) / tile_size).floor() as i32;

    for sy in min_ty..=max_ty {
        for sx in min_tx..=max_tx {
            if sx < 0 || sy < 0 || sx >= tilemap.width as i32 || sy >= tilemap.height as i32 {
                return false;
            }
            let sample = tilemap.get(sx, sy);
            if sample.is_solid() || sample == TileType::Spike {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tilemap(width: usize, height: usize, solids: &[(usize, usize)]) -> Tilemap {
        let mut tiles = vec![0u8; width * height];
        for (x, y) in solids {
            tiles[y * width + x] = crate::components::TileType::Solid as u8;
        }
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
    fn top_down_path_avoids_solid_cells() {
        let tm = test_tilemap(6, 4, &[(2, 1), (2, 2)]);
        let from = Vec2::new(8.0, 24.0);
        let to = Vec2::new(72.0, 24.0);
        let path = top_down_path(&tm, 16.0, from, to).expect("path should exist");
        assert!(path.len() > 1);
        for p in path {
            let tx = (p.x / 16.0).floor() as i32;
            let ty = (p.y / 16.0).floor() as i32;
            assert!(!tm.is_solid(tx, ty));
        }
    }

    #[test]
    fn top_down_cell_walkable_rejects_invalid_cells() {
        let tm = test_tilemap(4, 4, &[(1, 1)]);
        assert!(top_down_cell_walkable(&tm, 16.0, 0, 0));
        assert!(!top_down_cell_walkable(&tm, 16.0, 1, 1));
        assert!(!top_down_cell_walkable(&tm, 16.0, -1, 0));
        assert!(!top_down_cell_walkable(&tm, 16.0, 0, 5));

        let mut hazard_tiles = vec![0u8; 4 * 4];
        hazard_tiles[2 * 4 + 2] = crate::components::TileType::Spike as u8;
        let hazard_tm = Tilemap {
            width: 4,
            height: 4,
            tiles: hazard_tiles,
            player_spawn: (8.0, 8.0),
            goal: None,
            ..Default::default()
        };
        assert!(!top_down_cell_walkable(&hazard_tm, 16.0, 2, 2));
    }

    #[test]
    fn line_of_sight_is_blocked_by_solid_tile() {
        let tm = test_tilemap(6, 4, &[(2, 1)]);
        let from = Vec2::new(8.0, 24.0);
        let to = Vec2::new(72.0, 24.0);
        assert!(!has_line_of_sight_points(&tm, 16.0, from, to));
    }

    #[test]
    fn top_down_path_supports_diagonal_steps_without_corner_cutting() {
        let tm_open = test_tilemap(4, 4, &[]);
        let from = Vec2::new(8.0, 8.0);
        let to = Vec2::new(40.0, 40.0);
        let path =
            top_down_path(&tm_open, 16.0, from, to).expect("open diagonal path should exist");
        assert!(!path.is_empty());
        let first = path[0];
        assert!(
            (
                (first.x / 16.0).floor() as i32,
                (first.y / 16.0).floor() as i32
            ) == (1, 1),
            "expected first step to be diagonal in open space"
        );

        // Blocks at (1,0) and (0,1) should prevent corner-cutting to (1,1).
        let tm_blocked = test_tilemap(4, 4, &[(1, 0), (0, 1)]);
        let blocked = top_down_path(&tm_blocked, 16.0, from, to);
        assert!(
            blocked.is_none(),
            "path should not cut diagonally through blocked corner"
        );
    }
}
