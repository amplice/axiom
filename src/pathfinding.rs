use crate::ai;
use crate::components::GameConfig;
use crate::physics_core::{self, PhysicsCounters};
use crate::simulation::{self, SimInput, SimulationRequest};
use crate::tilemap::Tilemap;
use bevy::prelude::Vec2;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{hash_map::Entry, BinaryHeap, HashMap, HashSet, VecDeque};

/// Result of pathfinding + simulation
#[derive(Serialize)]
pub struct SolveResult {
    pub solved: bool,
    pub inputs: Vec<SimInput>,
    pub simulation: Option<crate::simulation::SimulationResult>,
    pub path_tiles: Vec<(i32, i32)>,
}

pub fn find_platformer_path_tiles(
    tilemap: &Tilemap,
    physics: &GameConfig,
    from: Vec2,
    to: Vec2,
) -> Option<Vec<(i32, i32)>> {
    let ts = physics.tile_size;
    let start = ((from.x / ts).floor() as i32, (from.y / ts).floor() as i32);
    let goal = ((to.x / ts).floor() as i32, (to.y / ts).floor() as i32);
    find_tile_path(tilemap, physics, start, goal)
}

pub fn find_platformer_path_points(
    tilemap: &Tilemap,
    physics: &GameConfig,
    from: Vec2,
    to: Vec2,
) -> Option<Vec<Vec2>> {
    let ts = physics.tile_size;
    let tiles = find_platformer_path_tiles(tilemap, physics, from, to)?;
    if tiles.is_empty() {
        return None;
    }
    Some(
        tiles
            .into_iter()
            .map(|(x, y)| Vec2::new((x as f32 + 0.5) * ts, (y as f32 + 0.5) * ts))
            .collect(),
    )
}

/// Find a path from spawn to goal and generate simulation inputs
pub fn solve(tilemap: &Tilemap, physics: &GameConfig) -> SolveResult {
    let ts = physics.tile_size;
    let goal = match tilemap.goal {
        Some(g) => g,
        None => {
            return SolveResult {
                solved: false,
                inputs: vec![],
                simulation: None,
                path_tiles: vec![],
            }
        }
    };

    let spawn_tile = (
        (tilemap.player_spawn.0 / ts) as i32,
        (tilemap.player_spawn.1 / ts) as i32,
    );

    if physics.gravity_magnitude() <= f32::EPSILON || physics.jump_velocity <= 0.0 {
        return solve_top_down(tilemap, physics, goal, spawn_tile);
    }

    // Find tile path via BFS
    let path = match find_tile_path(tilemap, physics, spawn_tile, goal) {
        Some(p) => p,
        None => {
            let goal_center = Vec2::new(
                (goal.0 as f32 + 0.5) * physics.tile_size,
                (goal.1 as f32 + 0.5) * physics.tile_size,
            );
            let goal_radius = (physics.tile_size * 0.75).max(2.0);
            if let Some((inputs, sim)) = adaptive_input_search(
                tilemap,
                physics,
                goal_center,
                goal_radius,
                false,
                &[Vec::new()],
            ) {
                return SolveResult {
                    solved: sim.outcome == "goal_reached",
                    inputs,
                    simulation: Some(sim),
                    path_tiles: vec![],
                };
            }
            return SolveResult {
                solved: false,
                inputs: vec![],
                simulation: None,
                path_tiles: vec![],
            }
        }
    };

    // Convert tile path to simulation inputs
    let inputs = path_to_inputs(&path, tilemap, physics);
    let goal_position = tilemap.goal.map(|(gx, gy)| {
        (
            (gx as f32 + 0.5) * physics.tile_size,
            (gy as f32 + 0.5) * physics.tile_size,
        )
    });
    let goal_radius = Some((physics.tile_size * 0.75).max(2.0));

    // Run the simulation to verify
    let sim_request = SimulationRequest {
        tilemap: None,
        inputs: inputs.clone(),
        max_frames: 1200,
        record_interval: 10,
        physics: None,
        goal_position,
        goal_radius,
        initial_game_state: None,
        state_transitions: Vec::new(),
        moving_platforms: Vec::new(),
    };
    let sim_result = simulation::run_simulation(tilemap, physics, &sim_request);
    if sim_result.outcome == "goal_reached" {
        return SolveResult {
            solved: true,
            inputs,
            simulation: Some(sim_result),
            path_tiles: path,
        };
    }

    // Fallback: try simple directional/jump patterns to recover from
    // path->input conversion misses on specific layouts.
    let mut best_sim = sim_result;
    let mut best_inputs = inputs.clone();
    for candidate in fallback_input_candidates(tilemap, physics) {
        let req = SimulationRequest {
            tilemap: None,
            inputs: candidate.clone(),
            max_frames: 1200,
            record_interval: 10,
            physics: None,
            goal_position,
            goal_radius,
            initial_game_state: None,
            state_transitions: Vec::new(),
            moving_platforms: Vec::new(),
        };
        let res = simulation::run_simulation(tilemap, physics, &req);
        if res.outcome == "goal_reached" {
            return SolveResult {
                solved: true,
                inputs: candidate,
                simulation: Some(res),
                path_tiles: path,
            };
        }
        if res.frames_elapsed > best_sim.frames_elapsed {
            best_sim = res;
            best_inputs = candidate;
        }
    }

    let seed_candidates = vec![Vec::new(), inputs.clone(), best_inputs.clone()];
    if let Some((inputs, sim)) = adaptive_input_search(
        tilemap,
        physics,
        Vec2::new(goal_position.unwrap().0, goal_position.unwrap().1),
        goal_radius.unwrap_or((physics.tile_size * 0.75).max(2.0)),
        false,
        &seed_candidates,
    ) {
        if sim.outcome == "goal_reached" {
            return SolveResult {
                solved: true,
                inputs,
                simulation: Some(sim),
                path_tiles: path,
            };
        }
        if simulation_score(&sim, Vec2::new(goal_position.unwrap().0, goal_position.unwrap().1))
            > simulation_score(&best_sim, Vec2::new(goal_position.unwrap().0, goal_position.unwrap().1))
        {
            best_inputs = inputs;
            best_sim = sim;
        }
    }

    if let Some((inputs, sim)) = brute_force_platformer_pulses(
        tilemap,
        physics,
        Vec2::new(goal_position.unwrap().0, goal_position.unwrap().1),
        goal_radius.unwrap_or((physics.tile_size * 0.75).max(2.0)),
        if goal.0 as f32 * physics.tile_size >= tilemap.player_spawn.0 {
            "right"
        } else {
            "left"
        },
        1200,
    ) {
        if sim.outcome == "goal_reached" {
            return SolveResult {
                solved: true,
                inputs,
                simulation: Some(sim),
                path_tiles: path,
            };
        }
        if simulation_score(&sim, Vec2::new(goal_position.unwrap().0, goal_position.unwrap().1))
            > simulation_score(&best_sim, Vec2::new(goal_position.unwrap().0, goal_position.unwrap().1))
        {
            best_inputs = inputs;
            best_sim = sim;
        }
    }

    SolveResult {
        solved: false,
        inputs: best_inputs,
        simulation: Some(best_sim),
        path_tiles: path,
    }
}

fn solve_top_down(
    tilemap: &Tilemap,
    physics: &GameConfig,
    goal: (i32, i32),
    spawn_tile: (i32, i32),
) -> SolveResult {
    let ts = physics.tile_size;
    let spawn = Vec2::new(tilemap.player_spawn.0, tilemap.player_spawn.1);
    let goal_center = Vec2::new((goal.0 as f32 + 0.5) * ts, (goal.1 as f32 + 0.5) * ts);
    let points = ai::find_top_down_path_points(tilemap, ts, spawn, goal_center).unwrap_or_default();

    let mut path_tiles = Vec::new();
    if !points.is_empty() {
        path_tiles.reserve(points.len() + 1);
        path_tiles.push(spawn_tile);
        for point in &points {
            path_tiles.push(((point.x / ts).floor() as i32, (point.y / ts).floor() as i32));
        }
        path_tiles.dedup();
    }

    let goal_position = Some((goal_center.x, goal_center.y));
    let goal_radius = Some((ts * 0.75).max(2.0));
    let run_candidate = |inputs: &Vec<SimInput>| {
        let estimated_end = inputs
            .iter()
            .map(|i| i.frame.saturating_add(i.duration))
            .max()
            .unwrap_or(0);
        let max_frames = estimated_end.saturating_add(2500).clamp(1000, 7000);
        simulation::run_simulation(
            tilemap,
            physics,
            &SimulationRequest {
                tilemap: None,
                inputs: inputs.clone(),
                max_frames,
                record_interval: 10,
                physics: None,
                goal_position,
                goal_radius,
                initial_game_state: None,
                state_transitions: Vec::new(),
                moving_platforms: Vec::new(),
            },
        )
    };

    let mut best_inputs = top_down_guided_inputs(tilemap, physics, &points, spawn, 3000);
    if best_inputs.is_empty() && !points.is_empty() {
        best_inputs = top_down_points_to_inputs(&points, physics.move_speed, spawn, "yx");
    }
    let mut best_sim = run_candidate(&best_inputs);
    if best_sim.outcome == "goal_reached" {
        return SolveResult {
            solved: true,
            inputs: best_inputs,
            simulation: Some(best_sim),
            path_tiles,
        };
    }

    if !points.is_empty() {
        let candidates = [
            top_down_points_to_inputs(&points, physics.move_speed, spawn, "yx"),
            top_down_points_to_inputs(&points, physics.move_speed, spawn, "xy"),
        ];
        for alt_inputs in candidates {
            let alt_sim = run_candidate(&alt_inputs);
            if alt_sim.outcome == "goal_reached" {
                return SolveResult {
                    solved: true,
                    inputs: alt_inputs,
                    simulation: Some(alt_sim),
                    path_tiles,
                };
            }
            if alt_sim.frames_elapsed > best_sim.frames_elapsed {
                best_sim = alt_sim;
                best_inputs = alt_inputs;
            }
        }
    }

    if let Some(planned_inputs) =
        top_down_motion_plan_inputs(tilemap, physics, spawn, goal_center, goal_radius.unwrap_or(8.0))
    {
        let planned_sim = run_candidate(&planned_inputs);
        if planned_sim.outcome == "goal_reached" {
            return SolveResult {
                solved: true,
                inputs: planned_inputs,
                simulation: Some(planned_sim),
                path_tiles,
            };
        }
        if simulation_score(&planned_sim, goal_center) > simulation_score(&best_sim, goal_center) {
            best_sim = planned_sim;
            best_inputs = planned_inputs;
        }
    }

    let mut seeds = vec![Vec::new(), best_inputs.clone()];
    if !points.is_empty() {
        seeds.push(top_down_points_to_inputs(
            &points,
            physics.move_speed,
            spawn,
            "xy",
        ));
    }
    if let Some((inputs, sim)) =
        adaptive_input_search(tilemap, physics, goal_center, goal_radius.unwrap_or(8.0), true, &seeds)
    {
        if sim.outcome == "goal_reached" {
            return SolveResult {
                solved: true,
                inputs,
                simulation: Some(sim),
                path_tiles,
            };
        }
        if simulation_score(&sim, goal_center) > simulation_score(&best_sim, goal_center) {
            best_sim = sim;
            best_inputs = inputs;
        }
    }

    SolveResult {
        solved: false,
        inputs: best_inputs,
        simulation: Some(best_sim),
        path_tiles,
    }
}

fn top_down_guided_inputs(
    tilemap: &Tilemap,
    physics: &GameConfig,
    path_points: &[Vec2],
    start_point: Vec2,
    max_frames: u32,
) -> Vec<SimInput> {
    if path_points.is_empty() || max_frames == 0 {
        return Vec::new();
    }

    let dt = 1.0 / 60.0f32;
    let ts = physics.tile_size;
    let mut x = start_point.x;
    let mut y = start_point.y;
    let goal_point = *path_points.last().unwrap_or(&start_point);
    let mut current_path = path_points.to_vec();
    let mut waypoint_idx = 0usize;
    let mut stagnant_frames = 0u32;
    let mut inputs = Vec::<SimInput>::new();
    let mut counters = PhysicsCounters::default();
    let arrive_radius = (ts * 0.35).max(2.0);

    for frame in 0..max_frames {
        let current_pos = Vec2::new(x, y);
        let current_tile = ((x / ts).floor() as i32, (y / ts).floor() as i32);
        if current_pos.distance(goal_point) <= arrive_radius {
            break;
        }

        let needs_replan =
            frame % 8 == 0 || waypoint_idx >= current_path.len() || stagnant_frames >= 20;
        if needs_replan {
            if let Some(new_path) =
                ai::find_top_down_path_points(tilemap, ts, current_pos, goal_point)
            {
                current_path = new_path;
                waypoint_idx = 0;
            }
        }

        while waypoint_idx < current_path.len() && {
            let wp = current_path[waypoint_idx];
            current_pos.distance(wp) <= arrive_radius
        } {
            waypoint_idx += 1;
            stagnant_frames = 0;
        }
        if waypoint_idx >= current_path.len() {
            break;
        }

        let target = current_path[waypoint_idx];
        let target_tile = (
            (target.x / ts).floor() as i32,
            (target.y / ts).floor() as i32,
        );
        let delta = target - current_pos;
        let mut move_x: Option<&str> = None;
        let mut move_y: Option<&str> = None;
        if current_tile.0 != target_tile.0 {
            move_x = Some(if target_tile.0 > current_tile.0 {
                "right"
            } else {
                "left"
            });
        }
        if current_tile.1 != target_tile.1 {
            move_y = Some(if target_tile.1 > current_tile.1 {
                "up"
            } else {
                "down"
            });
        }
        if move_x.is_none() && move_y.is_none() {
            if delta.x.abs() >= delta.y.abs() {
                if delta.x > 0.5 {
                    move_x = Some("right");
                } else if delta.x < -0.5 {
                    move_x = Some("left");
                } else if delta.y > 0.5 {
                    move_y = Some("up");
                } else if delta.y < -0.5 {
                    move_y = Some("down");
                }
            } else if delta.y > 0.5 {
                move_y = Some("up");
            } else if delta.y < -0.5 {
                move_y = Some("down");
            } else if delta.x > 0.5 {
                move_x = Some("right");
            } else if delta.x < -0.5 {
                move_x = Some("left");
            }
        }

        if move_x.is_some() && move_y.is_some() {
            // Prefer axis-aligned correction to avoid corner clipping on tight corridors.
            if delta.x.abs() >= delta.y.abs() {
                move_y = None;
            } else {
                move_x = None;
            }
        }

        if let Some(act) = move_x {
            inputs.push(SimInput {
                frame,
                action: act.to_string(),
                duration: 1,
            });
        }
        if let Some(act) = move_y {
            inputs.push(SimInput {
                frame,
                action: act.to_string(),
                duration: 1,
            });
        }

        let mut input_dx = 0.0f32;
        if let Some(act) = move_x {
            input_dx = if act == "right" { 1.0 } else { -1.0 };
        }
        let mut input_dy = 0.0f32;
        if let Some(act) = move_y {
            input_dy = if act == "up" { 1.0 } else { -1.0 };
        }
        let input_dir = Vec2::new(input_dx, input_dy);
        let input_dir = if input_dir.length_squared() > 0.0 {
            input_dir.normalize()
        } else {
            input_dir
        };
        let vx = input_dir.x * physics.move_speed;
        let vy = input_dir.y * physics.move_speed;
        let prev = Vec2::new(x, y);
        let motion = physics_core::resolve_motion(
            tilemap,
            physics_core::MotionParams {
                tile_size: ts,
                dt,
                x,
                y,
                vx,
                vy,
                width: 12.0,
                height: 14.0,
            },
            &mut counters,
        );
        x = motion.x;
        y = motion.y;

        if prev.distance(Vec2::new(x, y)) <= 0.01 {
            stagnant_frames = stagnant_frames.saturating_add(1);
            if stagnant_frames >= 90 {
                break;
            }
        } else {
            stagnant_frames = 0;
        }
    }

    inputs
}

fn top_down_points_to_inputs(
    path_points: &[Vec2],
    move_speed: f32,
    start_point: Vec2,
    axis_order: &str,
) -> Vec<SimInput> {
    if path_points.is_empty() {
        return Vec::new();
    }

    let units_per_frame = (move_speed / 60.0).max(1.0);
    let mut frame = 0u32;
    let mut residual_x = 0.0f32;
    let mut residual_y = 0.0f32;
    let mut x0 = start_point.x;
    let mut y0 = start_point.y;
    let mut inputs = Vec::new();

    for point in path_points {
        let dx = point.x - x0;
        let dy = point.y - y0;
        let axis_steps = if axis_order == "xy" {
            [("x", dx), ("y", dy)]
        } else {
            [("y", dy), ("x", dx)]
        };

        for (axis, delta) in axis_steps {
            if delta.abs() <= 1.0 {
                continue;
            }
            let action = if axis == "x" {
                if delta > 0.0 {
                    "right"
                } else {
                    "left"
                }
            } else if delta > 0.0 {
                "up"
            } else {
                "down"
            };
            let mut exact = delta.abs() / units_per_frame;
            if axis == "x" {
                exact += residual_x;
            } else {
                exact += residual_y;
            }
            let duration = exact.round().max(1.0) as u32;
            if axis == "x" {
                residual_x = exact - duration as f32;
            } else {
                residual_y = exact - duration as f32;
            }
            inputs.push(SimInput {
                frame,
                action: action.to_string(),
                duration,
            });
            frame = frame.saturating_add(duration);
        }

        x0 = point.x;
        y0 = point.y;
    }

    inputs
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TopDownAction {
    dx: i8,
    dy: i8,
}

#[derive(Clone)]
struct TopDownPlanNode {
    pos: Vec2,
    cost: u32,
    parent: Option<usize>,
    action: TopDownAction,
}

fn top_down_motion_plan_inputs(
    tilemap: &Tilemap,
    physics: &GameConfig,
    start: Vec2,
    goal_center: Vec2,
    goal_radius: f32,
) -> Option<Vec<SimInput>> {
    let chunk_frames = 8u32;
    let step = (physics.tile_size * 0.5).max(2.0);
    let mut nodes = Vec::<TopDownPlanNode>::new();
    let start_key = top_down_quantize(start, step);
    nodes.push(TopDownPlanNode {
        pos: start,
        cost: 0,
        parent: None,
        action: TopDownAction { dx: 0, dy: 0 },
    });

    let mut open = BinaryHeap::<(Reverse<u32>, usize)>::new();
    let start_h = top_down_heuristic(start, goal_center, physics.move_speed, chunk_frames);
    open.push((Reverse(start_h), 0));

    let mut best_cost = HashMap::<(i32, i32), u32>::new();
    best_cost.insert(start_key, 0);
    let mut expansions = 0usize;
    const MAX_EXPANSIONS: usize = 20_000;

    let actions = [
        TopDownAction { dx: 1, dy: 0 },
        TopDownAction { dx: -1, dy: 0 },
        TopDownAction { dx: 0, dy: 1 },
        TopDownAction { dx: 0, dy: -1 },
        TopDownAction { dx: 1, dy: 1 },
        TopDownAction { dx: 1, dy: -1 },
        TopDownAction { dx: -1, dy: 1 },
        TopDownAction { dx: -1, dy: -1 },
    ];

    while let Some((_, node_idx)) = open.pop() {
        if expansions >= MAX_EXPANSIONS {
            break;
        }
        expansions += 1;
        let node = nodes[node_idx].clone();
        if node.pos.distance(goal_center) <= goal_radius.max(0.1) {
            return Some(top_down_plan_to_inputs(&nodes, node_idx, chunk_frames));
        }
        for action in actions {
            let next_pos = simulate_top_down_chunk(
                tilemap,
                physics,
                node.pos,
                action,
                chunk_frames,
            );
            if next_pos.distance(node.pos) <= 0.01 {
                continue;
            }
            let key = top_down_quantize(next_pos, step);
            let cost = node.cost.saturating_add(chunk_frames);
            if best_cost.get(&key).is_some_and(|seen| *seen <= cost) {
                continue;
            }
            best_cost.insert(key, cost);
            let h = top_down_heuristic(next_pos, goal_center, physics.move_speed, chunk_frames);
            let score = cost.saturating_add(h);
            nodes.push(TopDownPlanNode {
                pos: next_pos,
                cost,
                parent: Some(node_idx),
                action,
            });
            open.push((Reverse(score), nodes.len() - 1));
        }
    }

    None
}

fn top_down_quantize(pos: Vec2, step: f32) -> (i32, i32) {
    (
        (pos.x / step).round() as i32,
        (pos.y / step).round() as i32,
    )
}

fn top_down_heuristic(pos: Vec2, goal: Vec2, move_speed: f32, chunk_frames: u32) -> u32 {
    let units_per_chunk = ((move_speed / 60.0).max(1.0) * chunk_frames as f32).max(0.1);
    ((pos.distance(goal) / units_per_chunk).ceil() as u32).saturating_mul(chunk_frames)
}

fn simulate_top_down_chunk(
    tilemap: &Tilemap,
    physics: &GameConfig,
    start: Vec2,
    action: TopDownAction,
    chunk_frames: u32,
) -> Vec2 {
    let dt = 1.0 / 60.0f32;
    let mut x = start.x;
    let mut y = start.y;
    let dir = Vec2::new(action.dx as f32, action.dy as f32).normalize_or_zero();
    let vx = dir.x * physics.move_speed;
    let vy = dir.y * physics.move_speed;
    let mut counters = PhysicsCounters::default();
    for _ in 0..chunk_frames {
        let motion = physics_core::resolve_motion(
            tilemap,
            physics_core::MotionParams {
                tile_size: physics.tile_size,
                dt,
                x,
                y,
                vx,
                vy,
                width: 12.0,
                height: 14.0,
            },
            &mut counters,
        );
        x = motion.x;
        y = motion.y;
    }
    Vec2::new(x, y)
}

fn top_down_plan_to_inputs(
    nodes: &[TopDownPlanNode],
    goal_idx: usize,
    chunk_frames: u32,
) -> Vec<SimInput> {
    let mut actions = Vec::<TopDownAction>::new();
    let mut idx = goal_idx;
    while let Some(parent) = nodes[idx].parent {
        actions.push(nodes[idx].action);
        idx = parent;
    }
    actions.reverse();

    let mut inputs = Vec::<SimInput>::new();
    let mut frame = 0u32;
    for action in actions {
        if action.dx > 0 {
            push_or_extend_input(&mut inputs, frame, "right", chunk_frames);
        } else if action.dx < 0 {
            push_or_extend_input(&mut inputs, frame, "left", chunk_frames);
        }
        if action.dy > 0 {
            push_or_extend_input(&mut inputs, frame, "up", chunk_frames);
        } else if action.dy < 0 {
            push_or_extend_input(&mut inputs, frame, "down", chunk_frames);
        }
        frame = frame.saturating_add(chunk_frames);
    }
    inputs
}

fn push_or_extend_input(inputs: &mut Vec<SimInput>, frame: u32, action: &str, duration: u32) {
    if let Some(last) = inputs.last_mut() {
        if last.action == action && last.frame.saturating_add(last.duration) == frame {
            last.duration = last.duration.saturating_add(duration);
            return;
        }
    }
    inputs.push(SimInput {
        frame,
        action: action.to_string(),
        duration,
    });
}

fn fallback_input_candidates(tilemap: &Tilemap, physics: &GameConfig) -> Vec<Vec<SimInput>> {
    let mut out = Vec::new();
    let goal_x = tilemap.goal.map(|g| g.0 as f32).unwrap_or(0.0) * physics.tile_size;
    let dir = if goal_x >= tilemap.player_spawn.0 {
        "right"
    } else {
        "left"
    };
    let rise_frames = ((physics.jump_velocity / physics.gravity_magnitude()) * 60.0)
        .ceil()
        .max(4.0) as u32;

    // Pure run.
    out.push(vec![SimInput {
        frame: 0,
        action: dir.to_string(),
        duration: 1000,
    }]);

    // Run + periodic jump pulses with multiple offsets/hold variants.
    for offset in [0u32, 4, 8, 12] {
        for interval in [16u32, 18, 20, 22, 24, 26, 30, 36, 45, 60] {
            let hold_primary = rise_frames.min(interval.saturating_sub(2)).max(3);
            let hold_secondary = (interval / 2).max(3);
            let hold_tertiary = ((rise_frames * 2) / 3).max(3);
            let hold_quaternary = 16u32.min(interval.saturating_sub(1)).max(3);
            let mut holds = vec![hold_primary, hold_secondary, hold_tertiary, hold_quaternary];
            holds.sort_unstable();
            holds.dedup();
            for hold in holds {
                let mut inputs = vec![SimInput {
                    frame: 0,
                    action: dir.to_string(),
                    duration: 1000,
                }];
                let mut f = offset;
                while f < 1000 {
                    inputs.push(SimInput {
                        frame: f,
                        action: "jump".to_string(),
                        duration: hold,
                    });
                    f = f.saturating_add(interval);
                }
                out.push(inputs);
            }
        }
    }
    out
}

fn brute_force_platformer_pulses(
    tilemap: &Tilemap,
    physics: &GameConfig,
    goal_center: Vec2,
    goal_radius: f32,
    run_dir: &str,
    max_attempts: usize,
) -> Option<(Vec<SimInput>, crate::simulation::SimulationResult)> {
    let rise_frames = ((physics.jump_velocity / physics.gravity_magnitude()) * 60.0)
        .ceil()
        .max(4.0) as u32;
    let mut best: Option<(Vec<SimInput>, crate::simulation::SimulationResult, f32)> = None;
    let mut attempts = 0usize;

    for offset in [0u32, 2, 4, 6, 8, 10, 12, 14] {
        for interval in [14u32, 16, 18, 20, 22, 24, 26, 28, 30, 34, 40, 48, 60] {
            let mut holds = vec![
                4u32,
                6,
                8,
                10,
                12,
                14,
                16,
                18,
                20,
                24,
                28,
                (interval / 2).max(3),
                rise_frames.min(interval.saturating_sub(2)).max(3),
            ];
            holds.sort_unstable();
            holds.dedup();
            for hold in holds {
                attempts += 1;
                if attempts > max_attempts {
                    break;
                }
                let mut inputs = vec![SimInput {
                    frame: 0,
                    action: run_dir.to_string(),
                    duration: 1500,
                }];
                let mut f = offset;
                while f < 1400 {
                    inputs.push(SimInput {
                        frame: f,
                        action: "jump".to_string(),
                        duration: hold,
                    });
                    f = f.saturating_add(interval);
                }

                let sim = simulation::run_simulation(
                    tilemap,
                    physics,
                    &SimulationRequest {
                        tilemap: None,
                        inputs: inputs.clone(),
                        max_frames: 1800,
                        record_interval: 10,
                        physics: None,
                        goal_position: Some((goal_center.x, goal_center.y)),
                        goal_radius: Some(goal_radius),
                        initial_game_state: None,
                        state_transitions: Vec::new(),
                        moving_platforms: Vec::new(),
                    },
                );
                if sim.outcome == "goal_reached" {
                    return Some((inputs, sim));
                }
                let score = simulation_score(&sim, goal_center);
                if best.as_ref().is_none_or(|(_, _, s)| score > *s) {
                    best = Some((inputs, sim, score));
                }
            }
            if attempts > max_attempts {
                break;
            }
        }
        if attempts > max_attempts {
            break;
        }
    }

    best.map(|(inputs, sim, _)| (inputs, sim))
}

#[derive(Clone)]
struct SearchPrimitive {
    actions: Vec<&'static str>,
    duration: u32,
    jump_duration: Option<u32>,
}

#[derive(Clone)]
struct SearchCandidate {
    inputs: Vec<SimInput>,
    next_frame: u32,
    score: f32,
    sim: crate::simulation::SimulationResult,
}

fn adaptive_input_search(
    tilemap: &Tilemap,
    physics: &GameConfig,
    goal_center: Vec2,
    goal_radius: f32,
    top_down: bool,
    seed_inputs: &[Vec<SimInput>],
) -> Option<(Vec<SimInput>, crate::simulation::SimulationResult)> {
    let prefer_right = goal_center.x >= tilemap.player_spawn.0;
    let primitives = if top_down {
        top_down_primitives()
    } else {
        platformer_primitives(prefer_right)
    };
    if primitives.is_empty() {
        return None;
    }

    let run = |inputs: &Vec<SimInput>, next_frame: u32| {
        let (buffer, min_frames, max_frames_cap) = if top_down {
            (2500u32, 1000u32, 7000u32)
        } else {
            (450u32, 300u32, 3500u32)
        };
        let max_frames = next_frame
            .saturating_add(buffer)
            .clamp(min_frames, max_frames_cap);
        simulation::run_simulation(
            tilemap,
            physics,
            &SimulationRequest {
                tilemap: None,
                inputs: inputs.clone(),
                max_frames,
                record_interval: 10,
                physics: None,
                goal_position: Some((goal_center.x, goal_center.y)),
                goal_radius: Some(goal_radius),
                initial_game_state: None,
                state_transitions: Vec::new(),
                moving_platforms: Vec::new(),
            },
        )
    };

    let mut beam = Vec::<SearchCandidate>::new();
    for seed in seed_inputs {
        let end = candidate_end_frame(seed);
        let sim = run(seed, end);
        let effective_inputs =
            truncate_inputs_at(seed, sim.frames_elapsed.saturating_add(2));
        let effective_end = candidate_end_frame(&effective_inputs);
        let score = simulation_score(&sim, goal_center);
        if sim.outcome == "goal_reached" {
            return Some((effective_inputs, sim));
        }
        beam.push(SearchCandidate {
            inputs: effective_inputs,
            next_frame: effective_end,
            score,
            sim,
        });
    }
    if beam.is_empty() {
        let empty = Vec::new();
        let sim = run(&empty, 0);
        let score = simulation_score(&sim, goal_center);
        beam.push(SearchCandidate {
            inputs: empty,
            next_frame: 0,
            score,
            sim,
        });
    }
    beam.sort_by(|a, b| b.score.total_cmp(&a.score));
    beam.truncate(8);

    let mut evaluations = beam.len();
    let mut best = beam[0].clone();
    let mut step = 0usize;
    let max_evaluations = if top_down { 460usize } else { 700usize };
    let max_steps = if top_down { 56usize } else { 90usize };
    while evaluations < max_evaluations && step < max_steps {
        step += 1;
        let mut expanded = Vec::<SearchCandidate>::new();
        for cand in &beam {
            for primitive in &primitives {
                if evaluations >= max_evaluations {
                    break;
                }
                let (inputs, next_frame) = append_primitive(
                    &cand.inputs,
                    cand.next_frame,
                    primitive,
                );
                let sim = run(&inputs, next_frame);
                evaluations += 1;
                let effective_inputs =
                    truncate_inputs_at(&inputs, sim.frames_elapsed.saturating_add(2));
                let effective_next = candidate_end_frame(&effective_inputs);
                let score = simulation_score(&sim, goal_center);
                if sim.outcome == "goal_reached" {
                    return Some((effective_inputs, sim));
                }
                expanded.push(SearchCandidate {
                    inputs: effective_inputs,
                    next_frame: effective_next,
                    score,
                    sim,
                });
            }
        }

        if expanded.is_empty() {
            break;
        }
        expanded.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut seen = HashSet::<(i32, i32, u32, &'static str)>::new();
        let mut next_beam = Vec::<SearchCandidate>::new();
        for cand in expanded {
            let terminal = simulation_terminal_position(&cand.sim).unwrap_or(Vec2::ZERO);
            let key = (
                (terminal.x / 8.0).round() as i32,
                (terminal.y / 8.0).round() as i32,
                (cand.next_frame / 6) * 6,
                outcome_bucket(&cand.sim.outcome),
            );
            if !seen.insert(key) {
                continue;
            }
            if cand.score > best.score {
                best = cand.clone();
            }
            next_beam.push(cand);
            if next_beam.len() >= 8 {
                break;
            }
        }
        beam = next_beam;
        if beam.is_empty() {
            break;
        }
    }
    Some((best.inputs, best.sim))
}

fn outcome_bucket(outcome: &str) -> &'static str {
    match outcome {
        "death" => "death",
        "stuck" => "stuck",
        "goal_reached" => "goal",
        _ => "other",
    }
}

fn simulation_terminal_position(sim: &crate::simulation::SimulationResult) -> Option<Vec2> {
    if let Some(state) = sim.entity_states.first() {
        return Some(Vec2::new(state.x, state.y));
    }
    sim.trace.last().map(|f| Vec2::new(f.x, f.y))
}

fn simulation_score(sim: &crate::simulation::SimulationResult, goal_center: Vec2) -> f32 {
    let pos = simulation_terminal_position(sim).unwrap_or(Vec2::ZERO);
    let dist = pos.distance(goal_center);
    let mut score = -dist;
    if sim.outcome == "goal_reached" {
        score += 10_000.0 - sim.frames_elapsed as f32;
    } else if sim.outcome == "death" {
        score -= 1200.0;
    } else if sim.outcome == "stuck" {
        score -= 160.0;
    }
    score
}

fn candidate_end_frame(inputs: &[SimInput]) -> u32 {
    inputs
        .iter()
        .map(|i| i.frame.saturating_add(i.duration.max(1)))
        .max()
        .unwrap_or(0)
}

fn truncate_inputs_at(inputs: &[SimInput], frame_limit: u32) -> Vec<SimInput> {
    inputs
        .iter()
        .filter_map(|input| {
            if input.frame >= frame_limit {
                return None;
            }
            let end = input.frame.saturating_add(input.duration.max(1));
            let clipped_end = end.min(frame_limit);
            if clipped_end <= input.frame {
                return None;
            }
            Some(SimInput {
                frame: input.frame,
                action: input.action.clone(),
                duration: clipped_end.saturating_sub(input.frame),
            })
        })
        .collect()
}

fn append_primitive(
    base: &[SimInput],
    start_frame: u32,
    primitive: &SearchPrimitive,
) -> (Vec<SimInput>, u32) {
    let mut out = base.to_vec();
    if let Some(jump_duration) = primitive.jump_duration {
        out.push(SimInput {
            frame: start_frame,
            action: "jump".to_string(),
            duration: jump_duration.max(1),
        });
    }
    for action in &primitive.actions {
        out.push(SimInput {
            frame: start_frame,
            action: (*action).to_string(),
            duration: primitive.duration.max(1),
        });
    }
    (
        out,
        start_frame.saturating_add(primitive.duration.max(1)).saturating_add(1),
    )
}

fn platformer_primitives(prefer_right: bool) -> Vec<SearchPrimitive> {
    let (primary, secondary) = if prefer_right {
        ("right", "left")
    } else {
        ("left", "right")
    };
    vec![
        SearchPrimitive {
            actions: vec![primary],
            duration: 10,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec![primary],
            duration: 18,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec![secondary],
            duration: 10,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec![primary],
            duration: 14,
            jump_duration: Some(4),
        },
        SearchPrimitive {
            actions: vec![primary],
            duration: 20,
            jump_duration: Some(8),
        },
        SearchPrimitive {
            actions: vec![secondary],
            duration: 14,
            jump_duration: Some(4),
        },
        SearchPrimitive {
            actions: Vec::new(),
            duration: 6,
            jump_duration: Some(6),
        },
        SearchPrimitive {
            actions: Vec::new(),
            duration: 8,
            jump_duration: None,
        },
    ]
}

fn top_down_primitives() -> Vec<SearchPrimitive> {
    vec![
        SearchPrimitive {
            actions: vec!["right"],
            duration: 8,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["left"],
            duration: 8,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["up"],
            duration: 8,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["down"],
            duration: 8,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["right", "up"],
            duration: 10,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["right", "down"],
            duration: 10,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["left", "up"],
            duration: 10,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["left", "down"],
            duration: 10,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["right"],
            duration: 16,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["left"],
            duration: 16,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["up"],
            duration: 16,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: vec!["down"],
            duration: 16,
            jump_duration: None,
        },
        SearchPrimitive {
            actions: Vec::new(),
            duration: 6,
            jump_duration: None,
        },
    ]
}

/// BFS to find a tile-coordinate path from start to goal
/// Returns sequence of (tile_x, tile_y) positions to visit
fn find_tile_path(
    tilemap: &Tilemap,
    physics: &GameConfig,
    start: (i32, i32),
    goal: (i32, i32),
) -> Option<Vec<(i32, i32)>> {
    let ts = physics.tile_size;

    // Max jump height in tiles
    let grav = physics.gravity_magnitude();
    let max_jump_height = (physics.jump_velocity * physics.jump_velocity) / (2.0 * grav);
    let max_jump_tiles = (max_jump_height / ts).ceil() as i32 + 1;

    // Max horizontal distance during a jump
    let jump_time = 2.0 * physics.jump_velocity / grav;
    let max_jump_dist = (jump_time * physics.move_speed / ts).ceil() as i32 + 1;

    // BFS: state = (tile_x, tile_y), parent map for path reconstruction
    let mut visited: HashMap<(i32, i32), Option<(i32, i32)>> = HashMap::new();
    let mut queue = VecDeque::new();

    // Find starting ground position
    let start_pos = find_ground(tilemap, start.0, start.1);
    visited.insert(start_pos, None);
    queue.push_back(start_pos);

    while let Some((x, y)) = queue.pop_front() {
        // Check if we reached the goal
        if x == goal.0 && y == goal.1 {
            return Some(reconstruct_path(&visited, goal));
        }

        // Also check if goal is reachable from current position without standing
        // (goal tile might be floating)
        if !is_standing_on(tilemap, goal.0, goal.1) {
            let dx = (x - goal.0).abs();
            let dy = goal.1 - y;
            if dx <= max_jump_dist
                && dy >= 0
                && dy <= max_jump_tiles
                && insert_parent_if_new(&mut visited, goal, (x, y))
            {
                return Some(reconstruct_path(&visited, goal));
            }
        }

        // Walk left/right on ground
        for dx in [-1i32, 1] {
            let nx = x + dx;
            if nx < 0 || nx >= tilemap.width as i32 {
                continue;
            }

            // Walk on solid ground
            if !tilemap.is_ground(nx, y)
                && is_standing_on(tilemap, nx, y)
                && enqueue_if_new(&mut visited, &mut queue, (nx, y), (x, y))
            {
                // Enqueued by helper.
            }

            // Walk off edge and fall
            if !tilemap.is_ground(nx, y) && !is_standing_on(tilemap, nx, y) {
                let mut fy = y - 1;
                while fy >= 0 {
                    if tilemap.is_ground(nx, fy) {
                        break;
                    }
                    if is_standing_on(tilemap, nx, fy)
                        && enqueue_if_new(&mut visited, &mut queue, (nx, fy), (x, y))
                    {
                        break;
                    }
                    fy -= 1;
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

            for dx_sign in [-1i32, 0, 1] {
                for dist in 0..=max_jump_dist {
                    let nx = x + dx_sign * dist;
                    if nx < 0 || nx >= tilemap.width as i32 {
                        continue;
                    }
                    if tilemap.is_ground(nx, ny) {
                        continue;
                    }

                    // Land on platform at this height
                    if is_standing_on(tilemap, nx, ny) {
                        enqueue_if_new(&mut visited, &mut queue, (nx, ny), (x, y));
                    }

                    // Fall from this apex to ground below
                    let mut fy = ny - 1;
                    while fy >= 0 {
                        if tilemap.is_ground(nx, fy) {
                            break;
                        }
                        if is_standing_on(tilemap, nx, fy)
                            && enqueue_if_new(&mut visited, &mut queue, (nx, fy), (x, y))
                        {
                            break;
                        }
                        fy -= 1;
                    }
                }
            }
        }
    }

    None
}

fn is_standing_on(tilemap: &Tilemap, x: i32, y: i32) -> bool {
    y > 0 && tilemap.is_ground(x, y - 1) && !tilemap.is_ground(x, y)
}

fn insert_parent_if_new(
    visited: &mut HashMap<(i32, i32), Option<(i32, i32)>>,
    node: (i32, i32),
    parent: (i32, i32),
) -> bool {
    match visited.entry(node) {
        Entry::Vacant(entry) => {
            entry.insert(Some(parent));
            true
        }
        Entry::Occupied(_) => false,
    }
}

fn enqueue_if_new(
    visited: &mut HashMap<(i32, i32), Option<(i32, i32)>>,
    queue: &mut VecDeque<(i32, i32)>,
    node: (i32, i32),
    parent: (i32, i32),
) -> bool {
    if insert_parent_if_new(visited, node, parent) {
        queue.push_back(node);
        return true;
    }
    false
}

fn find_ground(tilemap: &Tilemap, x: i32, y: i32) -> (i32, i32) {
    let mut cy = y;
    while cy > 0 {
        if is_standing_on(tilemap, x, cy) {
            return (x, cy);
        }
        cy -= 1;
    }
    (x, y)
}

fn reconstruct_path(
    visited: &HashMap<(i32, i32), Option<(i32, i32)>>,
    goal: (i32, i32),
) -> Vec<(i32, i32)> {
    let mut path = vec![goal];
    let mut current = goal;
    while let Some(Some(parent)) = visited.get(&current) {
        path.push(*parent);
        current = *parent;
    }
    path.reverse();
    path
}

/// Convert a tile-coordinate path into simulation inputs.
/// Tracks expected world position precisely (spawn_x + sum of movement frames * upf)
/// to prevent position drift between expected and actual simulation state.
fn path_to_inputs(path: &[(i32, i32)], tilemap: &Tilemap, physics: &GameConfig) -> Vec<SimInput> {
    if path.len() < 2 {
        return vec![];
    }

    let ts = physics.tile_size;
    let upf = physics.move_speed / 60.0; // units per frame

    let grav = physics.gravity_magnitude();
    // Frames to hold jump for full-height jump (variable jump height mechanic)
    let rise_frames = (physics.jump_velocity / grav * 60.0).ceil() as u32;
    // Conservative airtime estimate: rise + fall (fall is faster due to fall_multiplier)
    let fall_frames_est = (rise_frames as f32 / physics.fall_multiplier.sqrt()).ceil() as u32;
    let air_frames = rise_frames + fall_frames_est + 10; // total airtime + landing buffer

    let mut inputs: Vec<SimInput> = Vec::new();
    let mut frame: u32 = 3; // settle period
    let mut cur_x = tilemap.player_spawn.0; // precise world X tracking

    for seg in 0..path.len() - 1 {
        let (_cx, cy) = path[seg];
        let (nx, ny) = path[seg + 1];
        let dy = ny - cy;

        let target_x = (nx as f32 + 0.5) * ts;
        let dir = if target_x >= cur_x { 1.0f32 } else { -1.0 };
        let dir_str = if dir > 0.0 { "right" } else { "left" };
        let dir_sign: i32 = if dir > 0.0 { 1 } else { -1 };

        if dy > 0 {
            // Jump UP to a higher platform
            let horiz_dist = (target_x - cur_x).abs();
            let walk_frames = ((horiz_dist / upf) + 2.0).ceil() as u32;
            inputs.push(SimInput {
                frame,
                action: "jump".to_string(),
                duration: rise_frames,
            });
            inputs.push(SimInput {
                frame,
                action: dir_str.to_string(),
                duration: walk_frames,
            });
            frame += air_frames.max(walk_frames) + 5;
            cur_x += dir * walk_frames as f32 * upf;
        } else if dy < 0 {
            // Fall DOWN: walk off edge
            let horiz_dist = (target_x - cur_x).abs();
            let walk_frames = ((horiz_dist / upf) + 2.0).ceil() as u32;
            inputs.push(SimInput {
                frame,
                action: dir_str.to_string(),
                duration: walk_frames,
            });
            let fall_height = (-dy) as f32 * ts;
            let fall_time = (2.0 * fall_height / (grav * physics.fall_multiplier)).sqrt();
            let fall_f = (fall_time * 60.0).ceil() as u32;
            frame += walk_frames.max(fall_f) + 5;
            cur_x += dir * walk_frames as f32 * upf;
        } else {
            // Same height: walk with gap detection
            let start_tile = (cur_x / ts).floor() as i32;
            let mut scan = start_tile;

            while (dir_sign > 0 && scan < nx) || (dir_sign < 0 && scan > nx) {
                let next_scan = scan + dir_sign;
                if next_scan < 0 || next_scan >= tilemap.width as i32 {
                    break;
                }

                let has_ground = cy > 0 && tilemap.is_ground(next_scan, cy - 1);
                if !has_ground {
                    // Gap detected! Walk to edge of current solid ground
                    let edge_x = if dir_sign > 0 {
                        (scan as f32 + 1.0) * ts - 2.0
                    } else {
                        scan as f32 * ts + 2.0
                    };
                    let walk_dist = (edge_x - cur_x) * dir;
                    if walk_dist > 1.0 {
                        let walk_frames = (walk_dist / upf).ceil() as u32;
                        inputs.push(SimInput {
                            frame,
                            action: dir_str.to_string(),
                            duration: walk_frames,
                        });
                        frame += walk_frames;
                        cur_x += dir * walk_frames as f32 * upf;
                    }

                    // Find where solid ground resumes
                    let mut gap_end = next_scan;
                    loop {
                        if (dir_sign > 0 && gap_end > nx) || (dir_sign < 0 && gap_end < nx) {
                            break;
                        }
                        if gap_end < 0 || gap_end >= tilemap.width as i32 {
                            break;
                        }
                        if cy > 0 && tilemap.is_ground(gap_end, cy - 1) {
                            break;
                        }
                        gap_end += dir_sign;
                    }

                    // Jump across gap — HOLD jump for full height
                    // cross_frames = exact frames to clear gap + 2 safety margin
                    let land_x = if dir_sign > 0 {
                        gap_end as f32 * ts + 4.0
                    } else {
                        (gap_end as f32 + 1.0) * ts - 4.0
                    };
                    let cross_dist = (land_x - cur_x) * dir;
                    let cross_frames = (cross_dist / upf).ceil() as u32 + 1;

                    inputs.push(SimInput {
                        frame,
                        action: "jump".to_string(),
                        duration: rise_frames,
                    });
                    inputs.push(SimInput {
                        frame,
                        action: dir_str.to_string(),
                        duration: cross_frames,
                    });

                    frame += air_frames.max(cross_frames) + 5;
                    cur_x += dir * cross_frames as f32 * upf;
                    scan = gap_end;
                } else {
                    scan = next_scan;
                }
            }

            // Walk remaining distance to target tile center
            let remaining = (target_x - cur_x) * dir;
            if remaining > 1.0 {
                let walk_frames = (remaining / upf).ceil() as u32;
                inputs.push(SimInput {
                    frame,
                    action: dir_str.to_string(),
                    duration: walk_frames,
                });
                frame += walk_frames;
                cur_x += dir * walk_frames as f32 * upf;
            }
        }
    }

    // Final: walk toward goal
    if let Some(goal) = tilemap.goal {
        let goal_x = (goal.0 as f32 + 0.5) * ts;
        let dir = if goal_x >= cur_x { "right" } else { "left" };
        inputs.push(SimInput {
            frame,
            action: dir.to_string(),
            duration: 120,
        });
    }

    inputs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{GameConfig, TileType};

    #[test]
    fn finds_platformer_path_on_flat_ground() {
        let width = 12usize;
        let height = 6usize;
        let mut tiles = vec![TileType::Empty as u8; width * height];
        for tile in tiles.iter_mut().take(width) {
            *tile = TileType::Solid as u8;
        }
        let map = Tilemap {
            width,
            height,
            tiles,
            player_spawn: (24.0, 24.0),
            goal: Some((9, 1)),
        };
        let cfg = GameConfig::default();
        let path = find_platformer_path_points(
            &map,
            &cfg,
            Vec2::new(24.0, 24.0),
            Vec2::new(9.5 * cfg.tile_size, 1.5 * cfg.tile_size),
        );
        assert!(path.is_some());
        assert!(path.unwrap().len() >= 2);
    }

    #[test]
    fn fallback_candidates_are_generated() {
        let map = Tilemap::test_level();
        let cfg = GameConfig::default();
        let cands = fallback_input_candidates(&map, &cfg);
        assert!(cands.len() >= 2);
        assert!(cands.iter().all(|c| !c.is_empty()));
    }

    #[test]
    fn solve_supports_top_down_levels() {
        let width = 14usize;
        let height = 10usize;
        let tiles = vec![TileType::Empty as u8; width * height];
        let map = Tilemap {
            width,
            height,
            tiles,
            player_spawn: (1.5 * 16.0, 1.5 * 16.0),
            goal: Some((10, 7)),
        };
        let cfg = GameConfig {
            gravity: Vec2::ZERO,
            jump_velocity: 0.0,
            move_speed: 180.0,
            ..GameConfig::default()
        };

        let result = solve(&map, &cfg);
        assert!(result.solved, "top-down solver should reach goal");
        assert!(!result.inputs.is_empty());
        assert!(!result.path_tiles.is_empty());
        let sim = result.simulation.expect("simulation should be present");
        assert_eq!(sim.outcome, "goal_reached");
    }
}
