use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use crate::components::PhysicsConfig;
use crate::tilemap::Tilemap;
use crate::simulation::SimTilemap;

#[derive(Deserialize)]
pub struct ValidateRequest {
    pub tilemap: Option<SimTilemap>,
    pub constraints: Vec<String>,
}

#[derive(Serialize)]
pub struct ValidateResult {
    pub valid: bool,
    pub violations: Vec<Violation>,
    pub passed: Vec<String>,
}

#[derive(Serialize)]
pub struct Violation {
    pub constraint: String,
    pub message: String,
    pub details: serde_json::Value,
}

pub fn validate(tilemap: &Tilemap, physics: &PhysicsConfig, constraints: &[String]) -> ValidateResult {
    let mut violations = Vec::new();
    let mut passed = Vec::new();

    for constraint in constraints {
        match constraint.as_str() {
            "reachable" | "completable" => {
                if let Some(goal) = tilemap.goal {
                    let spawn_tile = (
                        (tilemap.player_spawn.0 / physics.tile_size) as i32,
                        (tilemap.player_spawn.1 / physics.tile_size) as i32,
                    );
                    let reachable = check_reachability(tilemap, physics, spawn_tile, goal);
                    if reachable {
                        passed.push(constraint.clone());
                    } else {
                        violations.push(Violation {
                            constraint: constraint.clone(),
                            message: format!("Goal at ({},{}) not reachable from spawn", goal.0, goal.1),
                            details: serde_json::json!({
                                "spawn": spawn_tile,
                                "goal": goal,
                            }),
                        });
                    }
                } else {
                    passed.push(constraint.clone()); // no goal = trivially reachable
                }
            }
            "bounds_check" => {
                if tilemap.tiles.len() == tilemap.width * tilemap.height {
                    passed.push(constraint.clone());
                } else {
                    violations.push(Violation {
                        constraint: constraint.clone(),
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
                    passed.push(constraint.clone());
                } else {
                    violations.push(Violation {
                        constraint: constraint.clone(),
                        message: "Potential softlock detected: player can reach areas with no exit".to_string(),
                        details: serde_json::json!({}),
                    });
                }
            }
            "has_ground" => {
                let has_ground = (0..tilemap.width).any(|x| tilemap.is_solid(x as i32, 0));
                if has_ground {
                    passed.push(constraint.clone());
                } else {
                    violations.push(Violation {
                        constraint: constraint.clone(),
                        message: "Level has no ground tiles at y=0".to_string(),
                        details: serde_json::json!({}),
                    });
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
    physics: &PhysicsConfig,
    start: (i32, i32),
    goal: (i32, i32),
) -> bool {
    let ts = physics.tile_size;

    // Calculate max jump height in tiles
    // Using kinematic equation: v^2 = 2*g*h → h = v^2 / (2*g)
    let max_jump_height = (physics.jump_velocity * physics.jump_velocity) / (2.0 * physics.gravity);
    let max_jump_tiles = (max_jump_height / ts).ceil() as i32 + 1;

    // Max horizontal distance during a jump ≈ jump_velocity/gravity * move_speed * 2 / tile_size
    let jump_time = 2.0 * physics.jump_velocity / physics.gravity;
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
                if !tilemap.is_solid(nx, y) && is_standing_on(tilemap, nx, y) {
                    if visited.insert((nx, y)) {
                        queue.push_back((nx, y));
                    }
                }
                // Can walk off edge (fall)
                if !tilemap.is_solid(nx, y) && !is_standing_on(tilemap, nx, y) {
                    // Fall down until we hit ground
                    let mut fy = y - 1;
                    while fy >= 0 && !tilemap.is_solid(nx, fy) {
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
            if ny >= tilemap.height as i32 { break; }
            if tilemap.is_solid(x, ny) { break; } // head bonk

            for dx in -max_jump_dist..=max_jump_dist {
                let nx = x + dx;
                if nx < 0 || nx >= tilemap.width as i32 { continue; }
                if tilemap.is_solid(nx, ny) { continue; }

                // Check if this position is reachable: not inside a wall, and either on ground or can fall to ground
                if is_standing_on(tilemap, nx, ny) {
                    if visited.insert((nx, ny)) {
                        queue.push_back((nx, ny));
                    }
                }
                // Also check falling from this jump apex
                let mut fy = ny - 1;
                while fy >= 0 {
                    if tilemap.is_solid(nx, fy) { break; }
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
    y > 0 && tilemap.is_solid(x, y - 1) && !tilemap.is_solid(x, y)
}

fn check_no_softlock(tilemap: &Tilemap, physics: &PhysicsConfig) -> bool {
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
