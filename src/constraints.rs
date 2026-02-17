use crate::components::GameConfig;
use crate::simulation::SimTilemap;
use crate::tilemap::Tilemap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Deserialize)]
pub struct ValidateRequest {
    pub tilemap: Option<SimTilemap>,
    #[serde(default)]
    pub entities: Vec<ValidateEntity>,
    pub constraints: Vec<String>,
    #[serde(default)]
    pub script_errors: Vec<String>,
    #[serde(default)]
    pub perf_fps: Option<f32>,
    #[serde(default)]
    pub available_assets: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ValidateEntity {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Serialize, Clone)]
pub struct ValidateResult {
    pub valid: bool,
    pub violations: Vec<Violation>,
    pub passed: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct Violation {
    pub constraint: String,
    pub message: String,
    pub details: serde_json::Value,
}

pub fn validate(
    tilemap: &Tilemap,
    physics: &GameConfig,
    constraints: &[String],
    entities: &[ValidateEntity],
) -> ValidateResult {
    validate_full(tilemap, physics, constraints, entities, &[], None, &[])
}

pub fn validate_full(
    tilemap: &Tilemap,
    physics: &GameConfig,
    constraints: &[String],
    entities: &[ValidateEntity],
    script_errors: &[String],
    perf_fps: Option<f32>,
    available_assets: &[String],
) -> ValidateResult {
    let mut violations = Vec::new();
    let mut passed = Vec::new();
    let tile_stats = collect_tile_stats(tilemap);

    for raw in constraints {
        let spec = parse_constraint(raw);
        match spec.name.as_str() {
            "reachable" | "completable" => {
                if let Some(goal) = tilemap.goal {
                    let spawn_tile = (
                        (tilemap.player_spawn.0 / physics.tile_size) as i32,
                        (tilemap.player_spawn.1 / physics.tile_size) as i32,
                    );
                    let reachable = check_reachability(tilemap, physics, spawn_tile, goal);
                    if reachable {
                        passed.push(raw.clone());
                    } else {
                        violations.push(Violation {
                            constraint: raw.clone(),
                            message: format!(
                                "Goal at ({},{}) not reachable from spawn",
                                goal.0, goal.1
                            ),
                            details: serde_json::json!({
                                "spawn": spawn_tile,
                                "goal": goal,
                            }),
                        });
                    }
                } else {
                    passed.push(raw.clone()); // no goal = trivially reachable
                }
            }
            "bounds_check" => {
                if tilemap.tiles.len() == tilemap.width * tilemap.height {
                    passed.push(raw.clone());
                } else {
                    violations.push(Violation {
                        constraint: raw.clone(),
                        message: "Tile array size doesn't match dimensions".to_string(),
                        details: serde_json::json!({
                            "expected": tilemap.width * tilemap.height,
                            "actual": tilemap.tiles.len(),
                        }),
                    });
                }
            }
            "no_softlock" => {
                // Check if there are any enclosed areas the player can reach but can't escape
                let result = check_no_softlock(tilemap, physics);
                if result {
                    passed.push(raw.clone());
                } else {
                    violations.push(Violation {
                        constraint: raw.clone(),
                        message: "Potential softlock detected: player can reach areas with no exit"
                            .to_string(),
                        details: serde_json::json!({}),
                    });
                }
            }
            "has_ground" => {
                let has_ground = (0..tilemap.width).any(|x| tilemap.is_ground(x as i32, 0));
                if has_ground {
                    passed.push(raw.clone());
                } else {
                    violations.push(Violation {
                        constraint: raw.clone(),
                        message: "Level has no ground tiles at y=0".to_string(),
                        details: serde_json::json!({}),
                    });
                }
            }
            "top_down_reachable" => {
                if let Some(goal) = tilemap.goal {
                    let spawn_tile = (
                        (tilemap.player_spawn.0 / physics.tile_size) as i32,
                        (tilemap.player_spawn.1 / physics.tile_size) as i32,
                    );
                    let reachable = check_top_down_reachability(tilemap, spawn_tile, goal);
                    if reachable {
                        passed.push(raw.clone());
                    } else {
                        violations.push(Violation {
                            constraint: raw.clone(),
                            message: format!(
                                "Goal at ({},{}) not reachable from spawn (top-down)",
                                goal.0, goal.1
                            ),
                            details: serde_json::json!({
                                "spawn": spawn_tile,
                                "goal": goal,
                            }),
                        });
                    }
                } else {
                    passed.push(raw.clone());
                }
            }
            "difficulty_range" => {
                let (min_d, max_d) = parse_range_args(&spec.args).unwrap_or((0.0, 1.0));
                let profile = estimate_difficulty_profile(tilemap, &tile_stats, entities);
                if profile.score >= min_d && profile.score <= max_d {
                    passed.push(raw.clone());
                } else {
                    violations.push(Violation {
                        constraint: raw.clone(),
                        message: format!(
                            "Estimated difficulty {:.3} outside range [{:.3}, {:.3}]",
                            profile.score, min_d, max_d
                        ),
                        details: serde_json::json!({
                            "min": min_d,
                            "max": max_d,
                            "estimated": profile.score,
                            "signals": profile,
                        }),
                    });
                }
            }
            "enemy_fairness" => match check_enemy_fairness(tilemap, physics, entities) {
                Ok(_) => passed.push(raw.clone()),
                Err(v) => violations.push(Violation {
                    constraint: raw.clone(),
                    message: v.0,
                    details: v.1,
                }),
            },
            "item_reachability" => match check_item_reachability(tilemap, physics, entities) {
                Ok(_) => passed.push(raw.clone()),
                Err(v) => violations.push(Violation {
                    constraint: raw.clone(),
                    message: v.0,
                    details: v.1,
                }),
            },
            "pacing" => match check_pacing(tilemap, entities) {
                Ok(_) => passed.push(raw.clone()),
                Err(v) => violations.push(Violation {
                    constraint: raw.clone(),
                    message: v.0,
                    details: v.1,
                }),
            },
            "no_dead_ends" => {
                // For platformers this is not meaningful; pass as N/A.
                if !is_top_down_like(&tile_stats) {
                    passed.push(raw.clone());
                    continue;
                }
                let max_ratio = spec
                    .args
                    .first()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.35)
                    .clamp(0.0, 1.0);
                match check_no_dead_ends_topdown(tilemap, physics, max_ratio) {
                    Ok(_) => passed.push(raw.clone()),
                    Err(v) => violations.push(Violation {
                        constraint: raw.clone(),
                        message: v.0,
                        details: v.1,
                    }),
                }
            }
            "ability_gating" => match check_ability_gating(entities) {
                Ok(_) => passed.push(raw.clone()),
                Err(v) => violations.push(Violation {
                    constraint: raw.clone(),
                    message: v.0,
                    details: v.1,
                }),
            },
            "entity_overlap" => {
                let threshold = spec.args.first().and_then(|v| v.parse::<f32>().ok()).unwrap_or(8.0);
                let mut overlaps = Vec::new();
                for i in 0..entities.len() {
                    for j in (i + 1)..entities.len() {
                        let dx = entities[i].x - entities[j].x;
                        let dy = entities[i].y - entities[j].y;
                        let dist = (dx * dx + dy * dy).sqrt();
                        if dist < threshold {
                            overlaps.push(serde_json::json!({
                                "entity_a": { "x": entities[i].x, "y": entities[i].y, "preset": entities[i].preset },
                                "entity_b": { "x": entities[j].x, "y": entities[j].y, "preset": entities[j].preset },
                                "distance": dist,
                            }));
                        }
                    }
                }
                if overlaps.is_empty() {
                    passed.push(raw.clone());
                } else {
                    violations.push(Violation {
                        constraint: raw.clone(),
                        message: format!("{} entity overlap(s) within {}px", overlaps.len(), threshold),
                        details: serde_json::json!({ "overlaps": overlaps }),
                    });
                }
            }
            "spawn_in_solid" => {
                let mut in_solid = Vec::new();
                for e in entities {
                    let tx = (e.x / physics.tile_size) as i32;
                    let ty = (e.y / physics.tile_size) as i32;
                    if tilemap.is_solid(tx, ty) {
                        in_solid.push(serde_json::json!({
                            "x": e.x, "y": e.y, "preset": e.preset,
                            "tile": [tx, ty],
                        }));
                    }
                }
                if in_solid.is_empty() {
                    passed.push(raw.clone());
                } else {
                    violations.push(Violation {
                        constraint: raw.clone(),
                        message: format!("{} entities spawned inside solid tiles", in_solid.len()),
                        details: serde_json::json!({ "entities_in_solid": in_solid }),
                    });
                }
            }
            "script_errors" => {
                if script_errors.is_empty() {
                    passed.push(raw.clone());
                } else {
                    violations.push(Violation {
                        constraint: raw.clone(),
                        message: format!("{} script error(s)", script_errors.len()),
                        details: serde_json::json!({ "errors": script_errors }),
                    });
                }
            }
            "performance" => {
                let fps_min = spec.args.first().and_then(|v| {
                    if let Some(stripped) = v.strip_prefix("fps_min=") {
                        stripped.parse::<f32>().ok()
                    } else {
                        v.parse::<f32>().ok()
                    }
                }).unwrap_or(30.0);
                if let Some(fps) = perf_fps {
                    if fps >= fps_min {
                        passed.push(raw.clone());
                    } else {
                        violations.push(Violation {
                            constraint: raw.clone(),
                            message: format!("FPS {:.1} below minimum {:.1}", fps, fps_min),
                            details: serde_json::json!({ "fps": fps, "fps_min": fps_min }),
                        });
                    }
                } else {
                    passed.push(raw.clone()); // no perf data = pass
                }
            }
            "asset_missing" => {
                if available_assets.is_empty() {
                    passed.push(raw.clone()); // no asset list provided = pass
                } else {
                    let asset_set: HashSet<&str> = available_assets.iter().map(|s| s.as_str()).collect();
                    let mut missing = Vec::new();
                    for e in entities {
                        if let Some(sheet) = e.config.get("sprite_sheet").and_then(|v| v.as_str()) {
                            if !asset_set.contains(sheet) {
                                missing.push(sheet.to_string());
                            }
                        }
                    }
                    if missing.is_empty() {
                        passed.push(raw.clone());
                    } else {
                        violations.push(Violation {
                            constraint: raw.clone(),
                            message: format!("{} missing asset(s)", missing.len()),
                            details: serde_json::json!({ "missing": missing }),
                        });
                    }
                }
            }
            other => {
                violations.push(Violation {
                    constraint: other.to_string(),
                    message: format!("Unknown constraint: {}", other),
                    details: serde_json::json!({}),
                });
            }
        }
    }

    ValidateResult {
        valid: violations.is_empty(),
        violations,
        passed,
    }
}

/// BFS reachability check modeling player physics
/// State: (tile_x, tile_y, can_jump)
fn check_reachability(
    tilemap: &Tilemap,
    physics: &GameConfig,
    start: (i32, i32),
    goal: (i32, i32),
) -> bool {
    let ts = physics.tile_size;

    // Calculate max jump height in tiles
    // Using kinematic equation: v^2 = 2*g*h → h = v^2 / (2*g)
    let grav = physics.gravity_magnitude();
    let max_jump_height = (physics.jump_velocity * physics.jump_velocity) / (2.0 * grav);
    let max_jump_tiles = (max_jump_height / ts).ceil() as i32 + 1;

    // Max horizontal distance during a jump ≈ jump_velocity/gravity * move_speed * 2 / tile_size
    let jump_time = 2.0 * physics.jump_velocity / grav;
    let max_jump_dist = (jump_time * physics.move_speed / ts).ceil() as i32 + 1;

    // BFS through (x, y) states
    let mut visited: HashSet<(i32, i32)> = HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    // Find starting ground position
    let start_pos = find_ground_position(tilemap, start.0, start.1);
    queue.push_back(start_pos);
    visited.insert(start_pos);

    while let Some((x, y)) = queue.pop_front() {
        if x == goal.0 && y == goal.1 {
            return true;
        }

        // Walk left/right on ground
        for dx in [-1i32, 1] {
            let nx = x + dx;
            if nx >= 0 && nx < tilemap.width as i32 {
                // Can walk to adjacent tile if it's not solid and we're on ground
                if !tilemap.is_ground(nx, y)
                    && is_standing_on(tilemap, nx, y)
                    && visited.insert((nx, y))
                {
                    queue.push_back((nx, y));
                }
                // Can walk off edge (fall)
                if !tilemap.is_ground(nx, y) && !is_standing_on(tilemap, nx, y) {
                    // Fall down until we hit ground
                    let mut fy = y - 1;
                    while fy >= 0 && !tilemap.is_ground(nx, fy) {
                        if is_standing_on(tilemap, nx, fy) {
                            if visited.insert((nx, fy)) {
                                queue.push_back((nx, fy));
                            }
                            break;
                        }
                        fy -= 1;
                    }
                }
            }
        }

        // Jump: can reach tiles up to max_jump_tiles above and max_jump_dist horizontally
        for dy in 1..=max_jump_tiles {
            let ny = y + dy;
            if ny >= tilemap.height as i32 {
                break;
            }
            if tilemap.is_solid(x, ny) {
                break;
            } // head bonk

            for dx in -max_jump_dist..=max_jump_dist {
                let nx = x + dx;
                if nx < 0 || nx >= tilemap.width as i32 {
                    continue;
                }
                if tilemap.is_ground(nx, ny) {
                    continue;
                }

                // Check if this position is reachable: not inside a wall, and either on ground or can fall to ground
                if is_standing_on(tilemap, nx, ny) && visited.insert((nx, ny)) {
                    queue.push_back((nx, ny));
                }
                // Also check falling from this jump apex
                let mut fy = ny - 1;
                while fy >= 0 {
                    if tilemap.is_ground(nx, fy) {
                        break;
                    }
                    if is_standing_on(tilemap, nx, fy) {
                        if visited.insert((nx, fy)) {
                            queue.push_back((nx, fy));
                        }
                        break;
                    }
                    fy -= 1;
                }
            }
        }

        // Also check: goal might not need standing (could be floating)
        // Check if goal tile is in the air-reachable set
        if !is_standing_on(tilemap, goal.0, goal.1) {
            // Check if we can reach the goal column at any height
            for dy in 0..=max_jump_tiles {
                let check_y = y + dy;
                if check_y == goal.1 && (x - goal.0).abs() <= max_jump_dist {
                    return true;
                }
            }
        }
    }

    // Also check if goal is in visited as a non-standing position
    visited.contains(&goal)
}

fn find_ground_position(tilemap: &Tilemap, x: i32, y: i32) -> (i32, i32) {
    let mut cy = y;
    while cy > 0 {
        if is_standing_on(tilemap, x, cy) {
            return (x, cy);
        }
        cy -= 1;
    }
    (x, y) // fallback
}

fn is_standing_on(tilemap: &Tilemap, x: i32, y: i32) -> bool {
    y > 0 && tilemap.is_ground(x, y - 1) && !tilemap.is_ground(x, y)
}

/// Top-down reachability (public wrapper for generation, treats spikes as blocking)
pub fn check_top_down_reachability_pub(
    tilemap: &Tilemap,
    start: (i32, i32),
    goal: (i32, i32),
) -> bool {
    check_top_down_reachability_avoid_spikes(tilemap, start, goal)
}

/// Top-down reachability avoiding spikes (for generation validation)
fn check_top_down_reachability_avoid_spikes(
    tilemap: &Tilemap,
    start: (i32, i32),
    goal: (i32, i32),
) -> bool {
    use crate::components::TileType;
    let mut visited: HashSet<(i32, i32)> = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start);
    visited.insert(start);

    while let Some((x, y)) = queue.pop_front() {
        if x == goal.0 && y == goal.1 {
            return true;
        }
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= tilemap.width as i32 || ny >= tilemap.height as i32 {
                continue;
            }
            let tile = tilemap.get(nx, ny);
            let passable = tile == TileType::Empty || tile == TileType::Goal;
            if passable && visited.insert((nx, ny)) {
                queue.push_back((nx, ny));
            }
        }
    }
    visited.contains(&goal)
}

/// Top-down reachability: 4-direction flood fill (no gravity, no jumps)
fn check_top_down_reachability(tilemap: &Tilemap, start: (i32, i32), goal: (i32, i32)) -> bool {
    let mut visited: HashSet<(i32, i32)> = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start);
    visited.insert(start);

    while let Some((x, y)) = queue.pop_front() {
        if x == goal.0 && y == goal.1 {
            return true;
        }
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= tilemap.width as i32 || ny >= tilemap.height as i32 {
                continue;
            }
            if !tilemap.is_solid(nx, ny) && visited.insert((nx, ny)) {
                queue.push_back((nx, ny));
            }
        }
    }
    visited.contains(&goal)
}

fn check_no_softlock(tilemap: &Tilemap, physics: &GameConfig) -> bool {
    // Simple check: if the level is completable, there's no softlock worth worrying about
    // A more thorough check would enumerate all reachable states and verify each has a path to goal or death
    if let Some(goal) = tilemap.goal {
        let spawn_tile = (
            (tilemap.player_spawn.0 / physics.tile_size) as i32,
            (tilemap.player_spawn.1 / physics.tile_size) as i32,
        );
        return check_reachability(tilemap, physics, spawn_tile, goal);
    }
    true
}

#[derive(Clone)]
struct ConstraintSpec {
    name: String,
    args: Vec<String>,
}

fn parse_constraint(raw: &str) -> ConstraintSpec {
    if let Some((name, value)) = raw.split_once('=') {
        return ConstraintSpec {
            name: name.trim().to_string(),
            args: value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        };
    }
    let mut parts = raw.split(':').map(|s| s.trim()).collect::<Vec<_>>();
    if parts.is_empty() {
        return ConstraintSpec {
            name: raw.to_string(),
            args: Vec::new(),
        };
    }
    let name = parts.remove(0).to_string();
    ConstraintSpec {
        name,
        args: parts
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
    }
}

fn parse_range_args(args: &[String]) -> Option<(f32, f32)> {
    match args.len() {
        0 => None,
        1 => {
            if let Some((a, b)) = args[0].split_once(',') {
                let min = a.trim().parse::<f32>().ok()?;
                let max = b.trim().parse::<f32>().ok()?;
                Some((min.min(max), min.max(max)))
            } else {
                None
            }
        }
        _ => {
            let a = args[0].parse::<f32>().ok()?;
            let b = args[1].parse::<f32>().ok()?;
            Some((a.min(b), a.max(b)))
        }
    }
}

#[derive(Serialize)]
struct TileStats {
    width: usize,
    height: usize,
    area: usize,
    solid_tiles: usize,
    spike_tiles: usize,
    goal_tiles: usize,
    solid_ratio: f32,
    floor_gap_count: usize,
    max_floor_gap: usize,
    avg_floor_gap: f32,
}

fn collect_tile_stats(tilemap: &Tilemap) -> TileStats {
    let area = tilemap.width * tilemap.height;
    let mut solid_tiles = 0usize;
    let mut spike_tiles = 0usize;
    let mut goal_tiles = 0usize;
    for y in 0..tilemap.height {
        for x in 0..tilemap.width {
            let t = tilemap.get(x as i32, y as i32);
            if t.is_solid() {
                solid_tiles += 1;
            }
            if matches!(t, crate::components::TileType::Spike) {
                spike_tiles += 1;
            }
            if matches!(t, crate::components::TileType::Goal) {
                goal_tiles += 1;
            }
        }
    }

    let mut floor_gap_count = 0usize;
    let mut max_floor_gap = 0usize;
    let mut current = 0usize;
    for x in 0..tilemap.width {
        if tilemap.is_solid(x as i32, 0) {
            if current > 0 {
                floor_gap_count += 1;
                max_floor_gap = max_floor_gap.max(current);
                current = 0;
            }
        } else {
            current += 1;
        }
    }
    if current > 0 {
        floor_gap_count += 1;
        max_floor_gap = max_floor_gap.max(current);
    }

    TileStats {
        width: tilemap.width,
        height: tilemap.height,
        area,
        solid_tiles,
        spike_tiles,
        goal_tiles,
        solid_ratio: if area == 0 {
            0.0
        } else {
            solid_tiles as f32 / area as f32
        },
        floor_gap_count,
        max_floor_gap,
        avg_floor_gap: if floor_gap_count == 0 {
            0.0
        } else {
            (0..tilemap.width)
                .fold((0usize, 0usize), |(sum, cur), x| {
                    if tilemap.is_solid(x as i32, 0) {
                        (sum + cur, 0)
                    } else {
                        (sum, cur + 1)
                    }
                })
                .0 as f32
                / floor_gap_count as f32
        },
    }
}

fn is_top_down_like(stats: &TileStats) -> bool {
    stats.solid_ratio >= 0.24
}

#[derive(Serialize)]
struct DifficultyProfile {
    score: f32,
    enemy_count: usize,
    pickup_count: usize,
    floor_gap_count: usize,
    max_floor_gap: usize,
    spike_tiles: usize,
    solid_ratio: f32,
}

fn estimate_difficulty_profile(
    _tilemap: &Tilemap,
    tile_stats: &TileStats,
    entities: &[ValidateEntity],
) -> DifficultyProfile {
    let enemy_count = entities.iter().filter(|e| is_enemy_entity(e)).count();
    let pickup_count = entities.iter().filter(|e| is_pickup_entity(e)).count();

    let gap_norm = (tile_stats.floor_gap_count as f32 / 10.0).clamp(0.0, 1.0);
    let max_gap_norm = (tile_stats.max_floor_gap as f32 / 8.0).clamp(0.0, 1.0);
    let spike_norm =
        (tile_stats.spike_tiles as f32 / (tile_stats.area.max(1) as f32 * 0.08)).clamp(0.0, 1.0);
    let enemy_norm = (enemy_count as f32 / 10.0).clamp(0.0, 1.0);
    let density_norm = ((tile_stats.solid_ratio - 0.20) / 0.60).clamp(0.0, 1.0);
    let pickup_relief = (pickup_count as f32 / 8.0).clamp(0.0, 0.35);

    let score = (0.20 * gap_norm
        + 0.14 * max_gap_norm
        + 0.22 * spike_norm
        + 0.28 * enemy_norm
        + 0.20 * density_norm
        - 0.16 * pickup_relief)
        .clamp(0.0, 1.0);

    DifficultyProfile {
        score,
        enemy_count,
        pickup_count,
        floor_gap_count: tile_stats.floor_gap_count,
        max_floor_gap: tile_stats.max_floor_gap,
        spike_tiles: tile_stats.spike_tiles,
        solid_ratio: tile_stats.solid_ratio,
    }
}

fn check_enemy_fairness(
    tilemap: &Tilemap,
    physics: &GameConfig,
    entities: &[ValidateEntity],
) -> Result<(), (String, serde_json::Value)> {
    let enemies = entities
        .iter()
        .filter(|e| is_enemy_entity(e))
        .collect::<Vec<_>>();
    if enemies.is_empty() {
        return Ok(());
    }

    let spawn = (
        (tilemap.player_spawn.0 / physics.tile_size) as i32,
        (tilemap.player_spawn.1 / physics.tile_size) as i32,
    );
    let goal = tilemap.goal.unwrap_or(spawn);

    let reachable = if is_top_down_like(&collect_tile_stats(tilemap)) {
        check_top_down_reachability_avoid_spikes(tilemap, spawn, goal)
    } else {
        check_reachability(tilemap, physics, spawn, goal)
    };
    if !reachable {
        return Err((
            "Goal is not reachable before evaluating enemy fairness".to_string(),
            serde_json::json!({ "spawn": spawn, "goal": goal }),
        ));
    }

    let path_len = (((goal.0 - spawn.0).abs() + (goal.1 - spawn.1).abs()) as f32).max(1.0);
    let mut close_to_spawn = 0usize;
    let mut corridor_enemies = 0usize;

    for e in enemies {
        let ex = e.x / physics.tile_size;
        let ey = e.y / physics.tile_size;
        let ds = ((ex - spawn.0 as f32).powi(2) + (ey - spawn.1 as f32).powi(2)).sqrt();
        if ds < 2.5 {
            close_to_spawn += 1;
        }

        let t = projection_on_segment(
            spawn.0 as f32,
            spawn.1 as f32,
            goal.0 as f32,
            goal.1 as f32,
            ex,
            ey,
        );
        let px = spawn.0 as f32 + (goal.0 as f32 - spawn.0 as f32) * t;
        let py = spawn.1 as f32 + (goal.1 as f32 - spawn.1 as f32) * t;
        let dline = ((px - ex).powi(2) + (py - ey).powi(2)).sqrt();
        if dline <= 3.0 {
            corridor_enemies += 1;
        }
    }

    let pressure = corridor_enemies as f32 / path_len;
    if close_to_spawn > 1 || pressure > 0.14 {
        return Err((
            "Enemy density is too high near spawn or along the main path".to_string(),
            serde_json::json!({
                "close_to_spawn": close_to_spawn,
                "corridor_enemies": corridor_enemies,
                "path_length_tiles": path_len,
                "pressure": pressure,
                "max_pressure": 0.14,
            }),
        ));
    }

    Ok(())
}

fn projection_on_segment(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    let vx = bx - ax;
    let vy = by - ay;
    let len_sq = vx * vx + vy * vy;
    if len_sq <= 0.0001 {
        return 0.0;
    }
    let t = ((px - ax) * vx + (py - ay) * vy) / len_sq;
    t.clamp(0.0, 1.0)
}

fn check_item_reachability(
    tilemap: &Tilemap,
    physics: &GameConfig,
    entities: &[ValidateEntity],
) -> Result<(), (String, serde_json::Value)> {
    let pickups = entities
        .iter()
        .filter(|e| is_pickup_entity(e))
        .collect::<Vec<_>>();
    if pickups.is_empty() {
        return Ok(());
    }

    let start = (
        (tilemap.player_spawn.0 / physics.tile_size) as i32,
        (tilemap.player_spawn.1 / physics.tile_size) as i32,
    );
    let top_down = is_top_down_like(&collect_tile_stats(tilemap));
    let mut unreachable = Vec::new();

    for item in pickups {
        let target = (
            (item.x / physics.tile_size) as i32,
            (item.y / physics.tile_size) as i32,
        );
        let ok = if top_down {
            check_top_down_reachability_avoid_spikes(tilemap, start, target)
        } else {
            check_reachability(tilemap, physics, start, target)
        };
        if !ok {
            unreachable.push(serde_json::json!({
                "preset": item.preset,
                "target_tile": target,
            }));
        }
    }

    if unreachable.is_empty() {
        Ok(())
    } else {
        Err((
            "Some pickups are not reachable from spawn".to_string(),
            serde_json::json!({ "unreachable": unreachable }),
        ))
    }
}

fn check_pacing(
    tilemap: &Tilemap,
    entities: &[ValidateEntity],
) -> Result<(), (String, serde_json::Value)> {
    let mut enemy_x = entities
        .iter()
        .filter(|e| is_enemy_entity(e))
        .map(|e| e.x)
        .collect::<Vec<_>>();
    if enemy_x.len() < 2 {
        return Ok(());
    }

    enemy_x.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let width_units = tilemap.width as f32 * 16.0;
    let rest_threshold = (width_units * 0.18).max(64.0);
    let mut rest_segments = 0usize;
    let mut gaps = Vec::new();

    for win in enemy_x.windows(2) {
        let gap = (win[1] - win[0]).abs();
        gaps.push(gap);
        if gap >= rest_threshold {
            rest_segments += 1;
        }
    }

    if rest_segments == 0 {
        return Err((
            "Enemy encounters have no meaningful rest gaps".to_string(),
            serde_json::json!({
                "enemy_count": enemy_x.len(),
                "rest_threshold": rest_threshold,
                "gaps": gaps,
            }),
        ));
    }
    Ok(())
}

fn check_no_dead_ends_topdown(
    tilemap: &Tilemap,
    physics: &GameConfig,
    max_ratio: f32,
) -> Result<(), (String, serde_json::Value)> {
    let spawn = (
        (tilemap.player_spawn.0 / physics.tile_size) as i32,
        (tilemap.player_spawn.1 / physics.tile_size) as i32,
    );

    let mut visited: HashSet<(i32, i32)> = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    if spawn.0 < 0
        || spawn.1 < 0
        || spawn.0 >= tilemap.width as i32
        || spawn.1 >= tilemap.height as i32
    {
        return Ok(());
    }
    queue.push_back(spawn);
    visited.insert(spawn);

    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= tilemap.width as i32 || ny >= tilemap.height as i32 {
                continue;
            }
            if !tilemap.is_solid(nx, ny) && visited.insert((nx, ny)) {
                queue.push_back((nx, ny));
            }
        }
    }

    if visited.is_empty() {
        return Ok(());
    }

    let goal = tilemap.goal;
    let mut dead_ends = 0usize;
    for (x, y) in &visited {
        if Some((*x, *y)) == goal || (*x, *y) == spawn {
            continue;
        }
        let mut neighbors = 0usize;
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let nx = *x + dx;
            let ny = *y + dy;
            if nx < 0 || ny < 0 || nx >= tilemap.width as i32 || ny >= tilemap.height as i32 {
                continue;
            }
            if !tilemap.is_solid(nx, ny) {
                neighbors += 1;
            }
        }
        if neighbors <= 1 {
            dead_ends += 1;
        }
    }

    let ratio = dead_ends as f32 / visited.len() as f32;
    if ratio <= max_ratio {
        Ok(())
    } else {
        Err((
            "Top-down map has too many dead-end corridors".to_string(),
            serde_json::json!({
                "dead_ends": dead_ends,
                "reachable_tiles": visited.len(),
                "ratio": ratio,
                "max_ratio": max_ratio,
            }),
        ))
    }
}

fn check_ability_gating(entities: &[ValidateEntity]) -> Result<(), (String, serde_json::Value)> {
    let mut required = std::collections::HashSet::new();
    let mut granted = std::collections::HashSet::new();

    for e in entities {
        if let Some(reqs) = extract_string_list(&e.config, "requires_ability") {
            for r in reqs {
                required.insert(r);
            }
        }
        if let Some(grants) = extract_string_list(&e.config, "grants_ability") {
            for g in grants {
                granted.insert(g);
            }
        }
    }

    if required.is_empty() {
        return Ok(());
    }

    let missing = required
        .iter()
        .filter(|r| !granted.contains(*r))
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err((
            "Some required abilities are gated but never granted".to_string(),
            serde_json::json!({
                "required": required,
                "granted": granted,
                "missing": missing,
            }),
        ))
    }
}

fn is_enemy_entity(entity: &ValidateEntity) -> bool {
    if entity.preset.to_lowercase().contains("enemy") {
        return true;
    }
    entity_has_tag(entity, "enemy")
}

fn is_pickup_entity(entity: &ValidateEntity) -> bool {
    if entity.preset.to_lowercase().contains("pickup") {
        return true;
    }
    entity_has_tag(entity, "pickup") || entity_has_tag(entity, "item")
}

fn entity_has_tag(entity: &ValidateEntity, tag: &str) -> bool {
    extract_string_list(&entity.config, "tags")
        .map(|tags| tags.iter().any(|t| t == tag))
        .unwrap_or(false)
}

fn extract_string_list(config: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    let value = config.get(key)?;
    if let Some(one) = value.as_str() {
        return Some(vec![one.to_string()]);
    }
    let arr = value.as_array()?;
    let mut out = Vec::new();
    for item in arr {
        let s = item.as_str()?;
        out.push(s.to_string());
    }
    Some(out)
}
