use crate::components::{GameConfig, TileType};
use crate::physics_core::{self, PhysicsCounters};
use crate::tilemap::Tilemap;
use bevy::prelude::Vec2;
use serde::{Deserialize, Serialize};

const PLAYER_WIDTH: f32 = 12.0;
const PLAYER_HEIGHT: f32 = 14.0;

#[derive(Serialize, Deserialize, Clone)]
pub struct SimulationRequest {
    pub tilemap: Option<SimTilemap>,
    pub inputs: Vec<SimInput>,
    pub max_frames: u32,
    #[serde(default = "default_record_interval")]
    pub record_interval: u32,
    pub physics: Option<SimPhysics>,
    #[serde(default)]
    pub goal_position: Option<(f32, f32)>,
    #[serde(default)]
    pub goal_radius: Option<f32>,
    #[serde(default)]
    pub initial_game_state: Option<String>,
    #[serde(default)]
    pub state_transitions: Vec<SimStateTransition>,
    #[serde(default)]
    pub moving_platforms: Vec<SimMovingPlatform>,
}

fn default_record_interval() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimTilemap {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub player_spawn: Option<(f32, f32)>,
    pub goal: Option<(i32, i32)>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimPhysics {
    pub gravity: Option<f32>,
    pub jump_velocity: Option<f32>,
    pub move_speed: Option<f32>,
    pub fall_multiplier: Option<f32>,
    pub coyote_frames: Option<u32>,
    pub jump_buffer_frames: Option<u32>,
    pub variable_height: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SimPlatformLoopMode {
    #[default]
    Loop,
    PingPong,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimMovingPlatform {
    pub waypoints: Vec<(f32, f32)>,
    pub speed: f32,
    #[serde(default)]
    pub loop_mode: SimPlatformLoopMode,
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
    #[serde(default = "default_platform_width")]
    pub width: f32,
    #[serde(default = "default_platform_height")]
    pub height: f32,
    #[serde(default)]
    pub position: Option<(f32, f32)>,
}

fn default_platform_direction() -> i8 {
    1
}

fn default_platform_carry_riders() -> bool {
    true
}

fn default_platform_width() -> f32 {
    16.0
}

fn default_platform_height() -> f32 {
    8.0
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimInput {
    pub frame: u32,
    pub action: String,
    #[serde(default)]
    pub duration: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimStateTransition {
    pub frame: u32,
    pub to: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimulationResult {
    pub outcome: String,
    pub frames_elapsed: u32,
    pub trace: Vec<TraceFrame>,
    pub events: Vec<SimEvent>,
    pub entity_events: Vec<SimEntityEvent>,
    pub entity_states: Vec<SimEntityState>,
    pub game_state: String,
    pub game_state_trace: Vec<SimGameStateFrame>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TraceFrame {
    pub frame: u32,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimEvent {
    pub frame: u32,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimEntityEvent {
    pub frame: u32,
    #[serde(rename = "type")]
    pub event_type: String,
    pub entities: Vec<u64>,
    pub data: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimEntityState {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    pub alive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SimGameStateFrame {
    pub frame: u32,
    pub state: String,
}

struct SimState {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    grounded: bool,
    alive: bool,
    coyote: u32,
    jump_buffer: u32,
    was_grounded: bool,
    was_rising: bool,
    jump_start_y: f32,
    max_jump_y: f32,
}

struct SimPlatformState {
    x: f32,
    y: f32,
    waypoints: Vec<Vec2>,
    speed: f32,
    loop_mode: SimPlatformLoopMode,
    current_waypoint: usize,
    direction: i8,
    pause_frames: u32,
    pause_timer: u32,
    carry_riders: bool,
    width: f32,
    height: f32,
}

const SIM_PLAYER_ID: u64 = 1;

struct InputState {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    jump_pressed: bool,
    jump_just_pressed: bool,
}

pub fn run_simulation(
    tilemap: &Tilemap,
    config: &GameConfig,
    request: &SimulationRequest,
) -> SimulationResult {
    let dt = 1.0 / 60.0f32;
    let gravity = config.gravity_magnitude();
    let ts = config.tile_size;
    let top_down_mode = gravity <= f32::EPSILON || config.jump_velocity <= 0.0;
    let variable_height = request
        .physics
        .as_ref()
        .and_then(|p| p.variable_height)
        .unwrap_or(true);

    let spawn = tilemap.player_spawn;
    let mut state = SimState {
        x: spawn.0,
        y: spawn.1,
        vx: 0.0,
        vy: 0.0,
        grounded: false,
        alive: true,
        coyote: 0,
        jump_buffer: 0,
        was_grounded: false,
        was_rising: false,
        jump_start_y: spawn.1,
        max_jump_y: spawn.1,
    };
    let mut moving_platforms = build_sim_platforms(&request.moving_platforms);

    let mut trace = Vec::new();
    let mut events = Vec::new();
    let mut outcome = "timeout".to_string();
    let mut current_state = request
        .initial_game_state
        .as_deref()
        .unwrap_or("Playing")
        .trim()
        .to_string();
    if current_state.is_empty() {
        current_state = "Playing".to_string();
    }
    let mut game_state_trace = vec![SimGameStateFrame {
        frame: 0,
        state: current_state.clone(),
    }];

    // Pre-process inputs into per-frame active actions
    let mut active_inputs: Vec<Vec<String>> = vec![Vec::new(); request.max_frames as usize + 1];
    for input in &request.inputs {
        let duration = if input.duration == 0 {
            1
        } else {
            input.duration
        };
        for f in input.frame..(input.frame + duration).min(request.max_frames) {
            active_inputs[f as usize].push(input.action.clone());
        }
    }

    let mut prev_jump = false;
    let mut state_transitions = request.state_transitions.clone();
    state_transitions.sort_by_key(|t| t.frame);
    let mut transition_idx = 0usize;

    for frame in 0..request.max_frames {
        while transition_idx < state_transitions.len()
            && state_transitions[transition_idx].frame == frame
        {
            let to = state_transitions[transition_idx].to.trim();
            if !to.is_empty() && to != current_state {
                current_state = to.to_string();
                events.push(SimEvent {
                    frame,
                    event_type: "game_state_changed".to_string(),
                    x: None,
                    y: None,
                    height: None,
                });
                game_state_trace.push(SimGameStateFrame {
                    frame,
                    state: current_state.clone(),
                });
            }
            transition_idx += 1;
        }

        // Decode inputs for this frame
        let actions = &active_inputs[frame as usize];
        let jump_now = actions.iter().any(|a| a == "jump" || a == "up");
        let input = InputState {
            left: actions.iter().any(|a| a == "left"),
            right: actions.iter().any(|a| a == "right"),
            up: actions.iter().any(|a| a == "up"),
            down: actions.iter().any(|a| a == "down"),
            jump_pressed: jump_now,
            jump_just_pressed: jump_now && !prev_jump,
        };
        prev_jump = jump_now;

        if !current_state.eq_ignore_ascii_case("playing") {
            if request.record_interval > 0 && frame % request.record_interval == 0 {
                trace.push(TraceFrame {
                    frame,
                    x: state.x,
                    y: state.y,
                    vx: state.vx,
                    vy: state.vy,
                    grounded: state.grounded,
                });
            }
            continue;
        }

        let platform_motions = step_moving_platforms(&mut moving_platforms, dt);
        for motion in &platform_motions {
            if !physics_core::rider_on_platform_top(
                state.x,
                state.y,
                PLAYER_WIDTH,
                PLAYER_HEIGHT,
                motion,
            ) {
                continue;
            }
            state.x += motion.delta_x;
            state.y += motion.delta_y;
            if motion.delta_y > 0.0 {
                state.vy = state.vy.max(0.0);
            }
            state.grounded = true;
            break;
        }

        if top_down_mode {
            let mut dx = 0.0f32;
            let mut dy = 0.0f32;
            if input.left {
                dx -= 1.0;
            }
            if input.right {
                dx += 1.0;
            }
            if input.up {
                dy += 1.0;
            }
            if input.down {
                dy -= 1.0;
            }
            let dir = Vec2::new(dx, dy);
            let dir = if dir.length_squared() > 0.0 {
                dir.normalize()
            } else {
                dir
            };
            state.vx = dir.x * config.move_speed;
            state.vy = dir.y * config.move_speed;
            state.grounded = true;
            state.was_grounded = true;
            state.was_rising = false;
            state.coyote = 0;
            state.jump_buffer = 0;
        } else {
            // Track pre-step state
            state.was_grounded = state.grounded;
            state.was_rising = state.vy > 0.0;

            physics_core::apply_gravity(
                &mut state.vy,
                state.grounded,
                gravity,
                config.fall_multiplier,
                dt,
            );

            if input.left || input.right {
                state.vx = physics_core::horizontal_velocity(
                    input.left,
                    input.right,
                    config.move_speed,
                );
            } else if state.grounded {
                let friction = physics_core::surface_friction(
                    tilemap,
                    config,
                    state.x,
                    state.y,
                    PLAYER_WIDTH,
                    PLAYER_HEIGHT,
                );
                physics_core::apply_surface_friction(&mut state.vx, friction);
            }

            physics_core::update_jump_buffer(
                input.jump_just_pressed,
                &mut state.jump_buffer,
                config.jump_buffer_frames,
            );

            // Jump
            if physics_core::try_jump(
                state.grounded,
                &mut state.coyote,
                &mut state.jump_buffer,
                input.jump_just_pressed,
                config.jump_velocity,
                &mut state.vy,
            ) {
                state.jump_start_y = state.y;
                state.max_jump_y = state.y;
                events.push(SimEvent {
                    frame,
                    event_type: "jump_start".to_string(),
                    x: Some(state.x),
                    y: Some(state.y),
                    height: None,
                });
            }

            physics_core::apply_variable_jump(&mut state.vy, input.jump_pressed, variable_height);
        }

        let mut counters = PhysicsCounters::default();
        let motion = physics_core::resolve_motion(
            tilemap,
            physics_core::MotionParams {
                tile_size: ts,
                dt,
                x: state.x,
                y: state.y,
                vx: state.vx,
                vy: state.vy,
                width: PLAYER_WIDTH,
                height: PLAYER_HEIGHT,
            },
            &mut counters,
        );
        state.x = motion.x;
        state.y = motion.y;
        state.vx = motion.vx;
        state.vy = motion.vy;

        // Track max jump height
        if state.y > state.max_jump_y {
            state.max_jump_y = state.y;
        }

        if top_down_mode {
            state.grounded = true;
        } else {
            state.grounded = physics_core::compute_grounded(
                tilemap,
                ts,
                state.x,
                state.y,
                PLAYER_WIDTH,
                PLAYER_HEIGHT,
                &mut counters,
            );
            physics_core::update_coyote_timer(
                state.grounded,
                &mut state.coyote,
                config.coyote_frames,
            );

            // Events: jump apex
            if state.was_rising && state.vy <= 0.0 {
                events.push(SimEvent {
                    frame,
                    event_type: "jump_apex".to_string(),
                    x: Some(state.x),
                    y: Some(state.y),
                    height: Some(state.max_jump_y - state.jump_start_y),
                });
            }

            // Events: landing
            if state.grounded && !state.was_grounded {
                events.push(SimEvent {
                    frame,
                    event_type: "land".to_string(),
                    x: Some(state.x),
                    y: Some(state.y),
                    height: None,
                });
            }
        }

        // Check spikes
        if physics_core::collides_type(
            tilemap,
            physics_core::CollisionQuery {
                x: state.x,
                y: state.y,
                width: PLAYER_WIDTH,
                height: PLAYER_HEIGHT,
                tile_size: ts,
                target: TileType::Spike,
            },
            &mut counters,
        ) {
            state.alive = false;
            events.push(SimEvent {
                frame,
                event_type: "death".to_string(),
                x: Some(state.x),
                y: Some(state.y),
                height: None,
            });
            outcome = "death".to_string();
            trace.push(TraceFrame {
                frame,
                x: state.x,
                y: state.y,
                vx: state.vx,
                vy: state.vy,
                grounded: state.grounded,
            });
            break;
        }

        // Optional world-space goal target (for modes that don't rely on Goal tiles).
        if let Some((goal_x, goal_y)) = request.goal_position {
            let radius = request.goal_radius.unwrap_or(ts * 0.5).max(0.1);
            let dist2 = (state.x - goal_x).powi(2) + (state.y - goal_y).powi(2);
            if dist2 <= radius * radius {
                events.push(SimEvent {
                    frame,
                    event_type: "goal_reached".to_string(),
                    x: Some(state.x),
                    y: Some(state.y),
                    height: None,
                });
                outcome = "goal_reached".to_string();
                trace.push(TraceFrame {
                    frame,
                    x: state.x,
                    y: state.y,
                    vx: state.vx,
                    vy: state.vy,
                    grounded: state.grounded,
                });
                break;
            }
        }

        // Check goal tiles
        if physics_core::collides_type(
            tilemap,
            physics_core::CollisionQuery {
                x: state.x,
                y: state.y,
                width: PLAYER_WIDTH,
                height: PLAYER_HEIGHT,
                tile_size: ts,
                target: TileType::Goal,
            },
            &mut counters,
        ) {
            events.push(SimEvent {
                frame,
                event_type: "goal_reached".to_string(),
                x: Some(state.x),
                y: Some(state.y),
                height: None,
            });
            outcome = "goal_reached".to_string();
            trace.push(TraceFrame {
                frame,
                x: state.x,
                y: state.y,
                vx: state.vx,
                vy: state.vy,
                grounded: state.grounded,
            });
            break;
        }

        // Fall out of world
        if state.y < -100.0 {
            state.alive = false;
            events.push(SimEvent {
                frame,
                event_type: "fell_off".to_string(),
                x: Some(state.x),
                y: Some(state.y),
                height: None,
            });
            outcome = "death".to_string();
            trace.push(TraceFrame {
                frame,
                x: state.x,
                y: state.y,
                vx: state.vx,
                vy: state.vy,
                grounded: state.grounded,
            });
            break;
        }

        // Record trace
        if request.record_interval > 0 && frame % request.record_interval == 0 {
            trace.push(TraceFrame {
                frame,
                x: state.x,
                y: state.y,
                vx: state.vx,
                vy: state.vy,
                grounded: state.grounded,
            });
        }

        // Stuck detection: if player hasn't moved significantly in 300 frames
        if frame > 300 && trace.len() >= 2 {
            let recent = &trace[trace.len() - 1];
            let old_idx = if trace.len() > 300 {
                trace.len() - 300
            } else {
                0
            };
            let old = &trace[old_idx];
            let dx = (recent.x - old.x).abs();
            let dy = (recent.y - old.y).abs();
            if dx < 1.0 && dy < 1.0 {
                outcome = "stuck".to_string();
                break;
            }
        }
    }

    let entity_events: Vec<SimEntityEvent> = events
        .iter()
        .filter_map(|e| {
            let mapped = match e.event_type.as_str() {
                "death" | "fell_off" => Some(("died".to_string(), vec![SIM_PLAYER_ID])),
                "goal_reached" => Some(("goal_reached".to_string(), vec![SIM_PLAYER_ID])),
                "jump_start" => Some(("jumped".to_string(), vec![SIM_PLAYER_ID])),
                "land" => Some(("landed".to_string(), vec![SIM_PLAYER_ID])),
                _ => None,
            }?;
            Some(SimEntityEvent {
                frame: e.frame,
                event_type: mapped.0,
                entities: mapped.1,
                data: serde_json::json!({
                    "x": e.x,
                    "y": e.y,
                    "height": e.height,
                }),
            })
        })
        .collect();

    let entity_states = vec![SimEntityState {
        id: SIM_PLAYER_ID,
        x: state.x,
        y: state.y,
        vx: state.vx,
        vy: state.vy,
        grounded: state.grounded,
        alive: state.alive,
        health: None,
    }];

    SimulationResult {
        outcome,
        frames_elapsed: trace.last().map(|t| t.frame).unwrap_or(0),
        trace,
        events,
        entity_events,
        entity_states,
        game_state: current_state,
        game_state_trace,
    }
}

fn build_sim_platforms(defs: &[SimMovingPlatform]) -> Vec<SimPlatformState> {
    defs.iter()
        .filter_map(|def| {
            if def.waypoints.is_empty() {
                return None;
            }
            let waypoints = def
                .waypoints
                .iter()
                .map(|(x, y)| Vec2::new(*x, *y))
                .collect::<Vec<_>>();
            let start = def
                .position
                .map(|(x, y)| Vec2::new(x, y))
                .unwrap_or(waypoints[0]);
            Some(SimPlatformState {
                x: start.x,
                y: start.y,
                waypoints,
                speed: def.speed.max(0.0),
                loop_mode: def.loop_mode,
                current_waypoint: def.current_waypoint,
                direction: if def.direction == 0 { 1 } else { def.direction },
                pause_frames: def.pause_frames,
                pause_timer: def.pause_timer,
                carry_riders: def.carry_riders,
                width: def.width.max(0.1),
                height: def.height.max(0.1),
            })
        })
        .collect()
}

fn step_moving_platforms(
    platforms: &mut [SimPlatformState],
    dt: f32,
) -> Vec<physics_core::PlatformMotion> {
    let mut motions = Vec::new();
    for platform in platforms.iter_mut() {
        if platform.waypoints.len() < 2 || platform.speed <= 0.0 || dt <= 0.0 {
            continue;
        }
        if platform.current_waypoint >= platform.waypoints.len() {
            platform.current_waypoint = 0;
        }
        if platform.direction == 0 {
            platform.direction = 1;
        }
        if platform.pause_timer > 0 {
            platform.pause_timer -= 1;
            continue;
        }

        let prev = Vec2::new(platform.x, platform.y);
        let target = platform.waypoints[platform.current_waypoint];
        let to_target = target - prev;
        let dist = to_target.length();
        let max_step = platform.speed * dt;
        if max_step <= 0.0 {
            continue;
        }

        let reached = dist <= max_step + 0.001;
        let next = if reached {
            target
        } else {
            prev + to_target.normalize_or_zero() * max_step
        };
        let delta = next - prev;
        platform.x = next.x;
        platform.y = next.y;
        if platform.carry_riders && delta.length_squared() > 0.000001 {
            motions.push(physics_core::PlatformMotion {
                prev_x: prev.x,
                prev_y: prev.y,
                delta_x: delta.x,
                delta_y: delta.y,
                width: platform.width,
                height: platform.height,
            });
        }
        if reached {
            advance_sim_platform_waypoint(platform);
        }
    }
    motions
}

fn advance_sim_platform_waypoint(platform: &mut SimPlatformState) {
    let len = platform.waypoints.len();
    if len <= 1 {
        platform.current_waypoint = 0;
        return;
    }
    match platform.loop_mode {
        SimPlatformLoopMode::Loop => {
            platform.current_waypoint = (platform.current_waypoint + 1) % len;
        }
        SimPlatformLoopMode::PingPong => {
            let dir = if platform.direction >= 0 {
                1isize
            } else {
                -1isize
            };
            let next = platform.current_waypoint as isize + dir;
            if next < 0 || next >= len as isize {
                platform.direction = -platform.direction;
                let new_dir = if platform.direction >= 0 {
                    1isize
                } else {
                    -1isize
                };
                let bounced = platform.current_waypoint as isize + new_dir;
                platform.current_waypoint = bounced.clamp(0, len as isize - 1) as usize;
            } else {
                platform.current_waypoint = next as usize;
            }
        }
    }
    if platform.pause_frames > 0 {
        platform.pause_timer = platform.pause_frames;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::GameConfig;
    use bevy::prelude::Vec2;

    fn flat_tilemap() -> Tilemap {
        let width = 20usize;
        let height = 8usize;
        let mut tiles = vec![0u8; width * height];
        for tile in tiles.iter_mut().take(width) {
            *tile = TileType::Solid as u8;
        }
        Tilemap {
            width,
            height,
            tiles,
            player_spawn: (24.0, 24.0),
            goal: None,
        }
    }

    #[test]
    fn simulate_respects_paused_state_until_transition_to_playing() {
        let map = flat_tilemap();
        let cfg = GameConfig::default();
        let req = SimulationRequest {
            tilemap: None,
            inputs: vec![SimInput {
                frame: 0,
                action: "right".to_string(),
                duration: 60,
            }],
            max_frames: 40,
            record_interval: 1,
            physics: None,
            goal_position: None,
            goal_radius: None,
            initial_game_state: Some("Paused".to_string()),
            state_transitions: vec![SimStateTransition {
                frame: 10,
                to: "Playing".to_string(),
            }],
            moving_platforms: Vec::new(),
        };
        let result = run_simulation(&map, &cfg, &req);
        let start_x = result.trace.first().map(|f| f.x).unwrap_or(0.0);
        let x_at_9 = result
            .trace
            .iter()
            .find(|f| f.frame == 9)
            .map(|f| f.x)
            .unwrap_or(start_x);
        let x_at_20 = result
            .trace
            .iter()
            .find(|f| f.frame == 20)
            .map(|f| f.x)
            .unwrap_or(start_x);
        assert!((x_at_9 - start_x).abs() < 0.001);
        assert!(x_at_20 > x_at_9);
        assert_eq!(result.game_state, "Playing");
        assert!(result
            .game_state_trace
            .iter()
            .any(|s| s.frame == 10 && s.state == "Playing"));
    }

    #[test]
    fn simulate_top_down_mode_moves_with_up_down_actions() {
        let mut map = flat_tilemap();
        map.goal = Some((1, 5));
        let cfg = GameConfig {
            gravity: Vec2::ZERO,
            move_speed: 180.0,
            ..GameConfig::default()
        };

        let req = SimulationRequest {
            tilemap: None,
            inputs: vec![SimInput {
                frame: 0,
                action: "up".to_string(),
                duration: 30,
            }],
            max_frames: 40,
            record_interval: 1,
            physics: None,
            goal_position: None,
            goal_radius: None,
            initial_game_state: None,
            state_transitions: Vec::new(),
            moving_platforms: Vec::new(),
        };
        let result = run_simulation(&map, &cfg, &req);
        let start_y = result.trace.first().map(|f| f.y).unwrap_or(0.0);
        let end_y = result.trace.last().map(|f| f.y).unwrap_or(start_y);
        assert!(
            end_y > start_y + 1.0,
            "expected top-down up action to increase y"
        );
        assert!(
            !result.events.iter().any(|e| e.event_type == "jump_start"),
            "top-down mode should not emit jump events"
        );
    }

    #[test]
    fn simulate_respects_goal_position_without_goal_tile() {
        let map = flat_tilemap();
        let cfg = GameConfig {
            gravity: Vec2::ZERO,
            move_speed: 180.0,
            jump_velocity: 0.0,
            ..GameConfig::default()
        };
        let req = SimulationRequest {
            tilemap: None,
            inputs: vec![SimInput {
                frame: 0,
                action: "right".to_string(),
                duration: 40,
            }],
            max_frames: 80,
            record_interval: 5,
            physics: None,
            goal_position: Some((140.0, 24.0)),
            goal_radius: Some(20.0),
            initial_game_state: None,
            state_transitions: Vec::new(),
            moving_platforms: Vec::new(),
        };
        let result = run_simulation(&map, &cfg, &req);
        assert_eq!(result.outcome, "goal_reached");
        assert!(result.events.iter().any(|e| e.event_type == "goal_reached"));
    }
}
