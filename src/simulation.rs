use crate::components::{GameConfig, TileType};
use crate::physics_core::{self, PhysicsCounters};
use crate::scripting::ScriptBackend;
use crate::tilemap::Tilemap;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    #[serde(default)]
    pub entities: Vec<serde_json::Value>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coyote: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_buffer: Option<u32>,
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
                    health: None,
                    coyote: Some(state.coyote),
                    jump_buffer: Some(state.jump_buffer),
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
                health: None,
                coyote: Some(state.coyote),
                jump_buffer: Some(state.jump_buffer),
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
                    health: None,
                    coyote: Some(state.coyote),
                    jump_buffer: Some(state.jump_buffer),
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
                health: None,
                coyote: Some(state.coyote),
                jump_buffer: Some(state.jump_buffer),
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
                health: None,
                coyote: Some(state.coyote),
                jump_buffer: Some(state.jump_buffer),
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
                health: None,
                coyote: Some(state.coyote),
                jump_buffer: Some(state.jump_buffer),
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

// === Real Simulation (PendingRealSim) ===

#[derive(Resource, Default)]
pub struct PendingRealSim {
    pub active: Option<ActiveRealSim>,
    /// When a real sim finishes, the saved state is moved here for restoration
    /// by `process_api_commands` (which has the right system params).
    pub restore_pending: Option<Box<crate::api::SaveGameData>>,
}

pub struct ActiveRealSim {
    pub saved_state: crate::api::SaveGameData,
    pub frames_remaining: u32,
    pub frames_total: u32,
    pub inputs: Vec<SimInput>,
    pub record_interval: u32,
    pub snapshots: Vec<crate::api::types::WorldSimSnapshot>,
    pub sender: Option<tokio::sync::oneshot::Sender<Result<crate::api::types::WorldSimResult, String>>>,
    pub saved_runtime_state: String,
}

/// Runs early in FixedUpdate to inject virtual inputs for real sim.
pub fn tick_real_sim(
    mut pending: ResMut<PendingRealSim>,
    mut vinput: ResMut<crate::input::VirtualInput>,
    mut runtime: ResMut<crate::game_runtime::RuntimeState>,
) {
    let Some(ref mut sim) = pending.active else { return };
    if sim.frames_remaining == 0 { return; }

    // Force gameplay to be active during sim
    if !runtime.is_gameplay_active() {
        runtime.state = "Playing".to_string();
    }

    let current_frame = sim.frames_total - sim.frames_remaining;

    vinput.active.clear();
    vinput.just_pressed.clear();
    for input in &sim.inputs {
        let dur = if input.duration == 0 { 1 } else { input.duration };
        if current_frame >= input.frame && current_frame < input.frame + dur {
            vinput.active.insert(input.action.clone());
        }
        if current_frame == input.frame {
            vinput.just_pressed.insert(input.action.clone());
        }
    }

    sim.frames_remaining -= 1;
}

/// Runs late in FixedUpdate to capture snapshots and finalize real sim.
pub fn finalize_real_sim(
    mut pending: ResMut<PendingRealSim>,
    mut runtime: ResMut<crate::game_runtime::RuntimeState>,
    entity_query: Query<(
        &crate::components::GamePosition,
        Option<&crate::components::Velocity>,
        Option<&crate::components::NetworkId>,
        Option<&crate::components::Player>,
        Option<&crate::components::Alive>,
        Option<&crate::components::Tags>,
        Option<&crate::scripting::LuaScript>,
        Option<&crate::components::Health>,
    )>,
    script_engine: Res<crate::scripting::ScriptEngine>,
    event_bus: Res<crate::events::GameEventBus>,
    script_errors: Res<crate::scripting::ScriptErrors>,
) {
    let Some(ref mut sim) = pending.active else { return };
    let current_frame = sim.frames_total - sim.frames_remaining;

    // Record snapshot at interval
    if sim.record_interval > 0 && current_frame % sim.record_interval == 0 {
        let mut entities = Vec::new();
        for (pos, vel, nid, _player, alive, tags, script, health) in entity_query.iter() {
            entities.push(crate::api::types::EntityInfo {
                id: nid.map_or(0, |n| n.0),
                network_id: nid.map(|n| n.0),
                x: pos.x,
                y: pos.y,
                vx: vel.map_or(0.0, |v| v.x),
                vy: vel.map_or(0.0, |v| v.y),
                components: vec![],
                script: script.map(|s| s.script_name.clone()),
                tags: tags.map(|t| t.0.iter().cloned().collect()).unwrap_or_default(),
                health: health.map(|h| h.current),
                max_health: health.map(|h| h.max),
                alive: Some(alive.map_or(true, |a| a.0)),
                ai_behavior: None,
                ai_state: None,
                ai_target_id: None,
                path_target: None,
                path_len: None,
                animation_graph: None,
                animation_state: None,
                animation_frame: None,
                animation_facing_right: None,
                render_layer: None,
                collision_layer: None,
                collision_mask: None,
                machine_state: None,
                inventory_slots: None,
                coyote_frames: None,
                jump_buffer_frames: None,
                invincibility_frames: None,
                grounded: None,
                contact_damage: None,
                contact_knockback: None,
                pickup_effect: None,
                trigger_event: None,
                projectile_damage: None,
                projectile_speed: None,
                hitbox_active: None,
                hitbox_damage: None,
                visible: None,
            });
        }

        let snapshot = script_engine.snapshot();
        let vars_map: serde_json::Map<String, serde_json::Value> = snapshot.vars.into_iter().collect();

        sim.snapshots.push(crate::api::types::WorldSimSnapshot {
            frame: current_frame,
            entities,
            vars: serde_json::Value::Object(vars_map),
        });
    }

    // If done, send result and restore state
    if sim.frames_remaining == 0 {
        let events: Vec<crate::events::GameEvent> = event_bus.recent.iter().cloned().collect();
        let errors: Vec<crate::scripting::ScriptError> = script_errors.entries.iter().cloned().collect();

        let snapshot = script_engine.snapshot();
        let final_vars_map: serde_json::Map<String, serde_json::Value> = snapshot.vars.into_iter().collect();

        if let Some(sender) = sim.sender.take() {
            let _ = sender.send(Ok(crate::api::types::WorldSimResult {
                frames_run: sim.frames_total,
                snapshots: std::mem::take(&mut sim.snapshots),
                events,
                script_errors: errors,
                final_vars: serde_json::Value::Object(final_vars_map),
            }));
        }

        // Extract what we need before releasing the borrow on sim
        let saved_runtime = sim.saved_runtime_state.clone();
        let _ = sim;

        // Restore runtime state immediately
        runtime.state = saved_runtime;

        // Take the active sim and move saved_state to restore_pending
        if let Some(finished) = pending.active.take() {
            pending.restore_pending = Some(Box::new(finished.saved_state));
        }
    }
}

// === Playtest Agent (PendingPlaytest) ===

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlaytestMode {
    Platformer,
    TopDown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlaytestGoal {
    Survive,
    ReachGoal,
    Explore,
}

#[derive(Resource, Default)]
pub struct PendingPlaytest {
    pub active: Option<ActivePlaytest>,
}

pub struct ActivePlaytest {
    pub saved_runtime_state: String,
    pub frames_remaining: u32,
    pub frames_total: u32,
    pub mode: PlaytestMode,
    pub goal: PlaytestGoal,
    pub sender: Option<tokio::sync::oneshot::Sender<Result<crate::api::types::PlaytestResult, String>>>,
    pub prev_x: f32,
    pub prev_y: f32,
    pub stuck_frames: u32,
    pub last_jump_frame: u32,
    pub direction: i32,
    pub explore_phase: u32,
    pub visited_cells: HashSet<(i32, i32)>,
    pub events: Vec<crate::api::types::PlaytestEvent>,
    pub total_damage: f32,
    pub deaths: Vec<crate::api::types::PlaytestEvent>,
    pub input_counts: HashMap<String, u32>,
    pub distance_traveled: f32,
    pub initial_health: f32,
}

/// Runs first in FixedUpdate to generate AI inputs for playtest.
pub fn tick_playtest_agent(
    mut pending: ResMut<PendingPlaytest>,
    mut vinput: ResMut<crate::input::VirtualInput>,
    mut runtime: ResMut<crate::game_runtime::RuntimeState>,
    player_query: Query<
        (&crate::components::GamePosition, Option<&crate::components::Health>),
        With<crate::components::Player>,
    >,
    enemy_tags_query: Query<
        (&crate::components::GamePosition, &crate::components::Tags),
        Without<crate::components::Player>,
    >,
    tilemap: Res<Tilemap>,
    config: Res<GameConfig>,
) {
    let Some(ref mut pt) = pending.active else { return };
    if pt.frames_remaining == 0 { return; }

    // Force gameplay active
    if !runtime.is_gameplay_active() {
        runtime.state = "Playing".to_string();
    }

    let current_frame = pt.frames_total - pt.frames_remaining;

    // Get player position
    let Ok((player_pos, player_health)) = player_query.get_single() else {
        pt.frames_remaining -= 1;
        return;
    };
    let px = player_pos.x;
    let py = player_pos.y;

    // Track visited cells (32px grid)
    let cell_x = (px / 32.0) as i32;
    let cell_y = (py / 32.0) as i32;
    pt.visited_cells.insert((cell_x, cell_y));

    // Track distance
    if current_frame > 0 {
        let dx = px - pt.prev_x;
        let dy = py - pt.prev_y;
        pt.distance_traveled += (dx * dx + dy * dy).sqrt();
    }

    // Check for damage taken
    if let Some(health) = player_health {
        let expected = pt.initial_health - pt.total_damage;
        if health.current < expected - 0.01 {
            let dmg = expected - health.current;
            pt.total_damage += dmg;
            pt.events.push(crate::api::types::PlaytestEvent {
                frame: current_frame,
                event_type: "damage_taken".to_string(),
                x: px, y: py,
                detail: format!("took {:.1} damage", dmg),
            });
        }
        if health.current <= 0.0 {
            pt.deaths.push(crate::api::types::PlaytestEvent {
                frame: current_frame,
                event_type: "death".to_string(),
                x: px, y: py,
                detail: "player died".to_string(),
            });
        }
    }

    // Stuck detection
    let move_dist = ((px - pt.prev_x).powi(2) + (py - pt.prev_y).powi(2)).sqrt();
    if move_dist < 0.5 { pt.stuck_frames += 1; } else { pt.stuck_frames = 0; }
    pt.prev_x = px;
    pt.prev_y = py;

    // Find nearest enemy
    let mut nearest_enemy_dist = f32::MAX;
    let mut nearest_enemy_dx = 0.0f32;
    let mut nearest_enemy_dy = 0.0f32;
    for (epos, etags) in enemy_tags_query.iter() {
        if !etags.0.contains("enemy") { continue; }
        let edx = epos.x - px;
        let edy = epos.y - py;
        let dist = (edx * edx + edy * edy).sqrt();
        if dist < nearest_enemy_dist {
            nearest_enemy_dist = dist;
            nearest_enemy_dx = edx;
            nearest_enemy_dy = edy;
        }
    }

    // Clear inputs
    vinput.active.clear();
    vinput.just_pressed.clear();

    let ts = config.tile_size;

    // Goal-directed: determine goal position if using ReachGoal
    let goal_pos: Option<(f32, f32)> = if pt.goal == PlaytestGoal::ReachGoal {
        tilemap.goal.map(|(gx, gy)| {
            let (wx, wy) = config.tile_mode.grid_to_world(gx as f32, gy as f32, ts);
            (wx, wy)
        })
    } else {
        None
    };

    match pt.mode {
        PlaytestMode::Platformer => {
            // Goal-aware: set direction toward goal if ReachGoal
            if let Some((goal_x, _goal_y)) = goal_pos {
                if (goal_x - px).abs() > ts {
                    pt.direction = if goal_x > px { 1 } else { -1 };
                }
            }

            let dir_action = if pt.direction > 0 { "right" } else { "left" };
            vinput.active.insert(dir_action.to_string());
            *pt.input_counts.entry(dir_action.to_string()).or_insert(0) += 1;

            // Check for wall ahead
            let check_x = px + (pt.direction as f32) * ts;
            let tile_x = (check_x / ts) as i32;
            let tile_y = (py / ts) as i32;
            let wall_ahead = tilemap.get(tile_x, tile_y) == TileType::Solid;

            // Check for gap ahead
            let gap_tile_x = ((px + (pt.direction as f32) * ts) / ts) as i32;
            let below_y = tile_y - 1;
            let ground_below = tilemap.get(gap_tile_x, below_y);
            let gap_ahead = ground_below != TileType::Solid && ground_below != TileType::Platform;

            if wall_ahead || gap_ahead {
                vinput.active.insert("jump".to_string());
                vinput.just_pressed.insert("jump".to_string());
                *pt.input_counts.entry("jump".to_string()).or_insert(0) += 1;
                pt.last_jump_frame = current_frame;
            }

            if pt.stuck_frames > 60 {
                if pt.stuck_frames < 90 {
                    vinput.active.insert("jump".to_string());
                    vinput.just_pressed.insert("jump".to_string());
                    *pt.input_counts.entry("jump".to_string()).or_insert(0) += 1;
                } else {
                    pt.direction = -pt.direction;
                    pt.stuck_frames = 0;
                    pt.events.push(crate::api::types::PlaytestEvent {
                        frame: current_frame,
                        event_type: "stuck_reverse".to_string(),
                        x: px, y: py,
                        detail: "reversed direction due to being stuck".to_string(),
                    });
                }
            }

            // Survive mode: be more cautious near enemies
            let attack_range = if pt.goal == PlaytestGoal::Survive { 40.0 } else { 60.0 };
            if nearest_enemy_dist < attack_range {
                vinput.active.insert("attack".to_string());
                vinput.just_pressed.insert("attack".to_string());
                *pt.input_counts.entry("attack".to_string()).or_insert(0) += 1;
            }

            // Check if reached goal tile
            if let Some((goal_x, goal_y)) = goal_pos {
                let dist_to_goal = ((px - goal_x).powi(2) + (py - goal_y).powi(2)).sqrt();
                if dist_to_goal < ts * 1.5 {
                    pt.events.push(crate::api::types::PlaytestEvent {
                        frame: current_frame,
                        event_type: "goal_reached".to_string(),
                        x: px, y: py,
                        detail: "player reached goal".to_string(),
                    });
                    pt.frames_remaining = 0;
                    return;
                }
            }
        }
        PlaytestMode::TopDown => {
            // Goal-aware: move toward goal in top-down mode
            let (dir_x, dir_y): (f32, f32) = if let Some((goal_x, goal_y)) = goal_pos {
                let dx = goal_x - px;
                let dy = goal_y - py;
                let len = (dx * dx + dy * dy).sqrt();
                if len > ts * 1.5 {
                    (dx / len, dy / len)
                } else {
                    // Reached goal
                    pt.events.push(crate::api::types::PlaytestEvent {
                        frame: current_frame,
                        event_type: "goal_reached".to_string(),
                        x: px, y: py,
                        detail: "player reached goal".to_string(),
                    });
                    pt.frames_remaining = 0;
                    return;
                }
            } else {
                // Default exploration spiral
                let phase = (current_frame / 120) % 4;
                match phase {
                    0 => (1.0, 0.0),
                    1 => (0.0, 1.0),
                    2 => (-1.0, 0.0),
                    _ => (0.0, -1.0),
                }
            };

            // Survive mode: prioritize fleeing over attacking
            let flee_threshold = if pt.goal == PlaytestGoal::Survive { 150.0 } else { 100.0 };
            let attack_threshold = if pt.goal == PlaytestGoal::Survive { 30.0 } else { 50.0 };

            if nearest_enemy_dist < flee_threshold {
                if nearest_enemy_dx > 0.0 { vinput.active.insert("left".to_string()); }
                else { vinput.active.insert("right".to_string()); }
                if nearest_enemy_dy > 0.0 { vinput.active.insert("down".to_string()); }
                else { vinput.active.insert("up".to_string()); }
                vinput.active.insert("sprint".to_string());
                *pt.input_counts.entry("sprint".to_string()).or_insert(0) += 1;

                if nearest_enemy_dist < attack_threshold {
                    vinput.active.insert("attack".to_string());
                    vinput.just_pressed.insert("attack".to_string());
                    *pt.input_counts.entry("attack".to_string()).or_insert(0) += 1;
                }
            } else {
                if dir_x > 0.5 { vinput.active.insert("right".to_string()); *pt.input_counts.entry("right".to_string()).or_insert(0) += 1; }
                if dir_x < -0.5 { vinput.active.insert("left".to_string()); *pt.input_counts.entry("left".to_string()).or_insert(0) += 1; }
                if dir_y > 0.5 { vinput.active.insert("up".to_string()); *pt.input_counts.entry("up".to_string()).or_insert(0) += 1; }
                if dir_y < -0.5 { vinput.active.insert("down".to_string()); *pt.input_counts.entry("down".to_string()).or_insert(0) += 1; }
            }

            // Wall check
            let check_x = px + dir_x * ts;
            let check_y = py + dir_y * ts;
            let tile_cx = (check_x / ts) as i32;
            let tile_cy = (check_y / ts) as i32;
            if tilemap.get(tile_cx, tile_cy) == TileType::Solid {
                vinput.active.clear();
                if dir_x.abs() > dir_y.abs() {
                    vinput.active.insert("up".to_string());
                    *pt.input_counts.entry("up".to_string()).or_insert(0) += 1;
                } else {
                    vinput.active.insert("right".to_string());
                    *pt.input_counts.entry("right".to_string()).or_insert(0) += 1;
                }
            }

            if pt.stuck_frames > 60 {
                pt.explore_phase = pt.explore_phase.wrapping_add(1);
                pt.stuck_frames = 0;
                pt.events.push(crate::api::types::PlaytestEvent {
                    frame: current_frame,
                    event_type: "stuck_change_dir".to_string(),
                    x: px, y: py,
                    detail: "changed exploration direction due to being stuck".to_string(),
                });
            }
        }
    }

    pt.frames_remaining -= 1;
}

/// Runs late in FixedUpdate to detect playtest completion and send results.
pub fn finalize_playtest(
    mut pending: ResMut<PendingPlaytest>,
    mut runtime: ResMut<crate::game_runtime::RuntimeState>,
    player_query: Query<
        (Option<&crate::components::Alive>, Option<&crate::components::Health>),
        With<crate::components::Player>,
    >,
) {
    let Some(ref mut pt) = pending.active else { return };

    let player_alive = player_query
        .get_single()
        .map(|(alive, _)| alive.map_or(true, |a| a.0))
        .unwrap_or(false);

    let current_frame = pt.frames_total - pt.frames_remaining;
    let done = pt.frames_remaining == 0 || !player_alive;
    if !done { return; }

    let frames_played = current_frame;
    let tiles_explored = pt.visited_cells.len();
    let stuck_count = pt.events.iter().filter(|e| e.event_type.starts_with("stuck_")).count() as u32;
    let death_count = pt.deaths.len();

    let difficulty_rating = if death_count > 2 || (frames_played < 120 && death_count > 0) {
        "impossible"
    } else if death_count > 0 || pt.total_damage > 8.0 {
        "hard"
    } else if pt.total_damage > 4.0 || stuck_count > 3 {
        "medium"
    } else if pt.total_damage > 0.0 || stuck_count > 0 {
        "easy"
    } else {
        "trivial"
    };

    let mut notes = Vec::new();
    if !player_alive { notes.push(format!("Player died at frame {}", current_frame)); }
    if tiles_explored < 5 { notes.push("Very limited exploration - agent may be trapped".to_string()); }
    if stuck_count > 5 { notes.push(format!("Agent got stuck {} times - level may have navigation issues", stuck_count)); }
    if pt.total_damage > 0.0 { notes.push(format!("Total damage taken: {:.1}", pt.total_damage)); }

    let goal_reached = pt.events.iter().any(|e| e.event_type == "goal_reached");

    if let Some(sender) = pt.sender.take() {
        let _ = sender.send(Ok(crate::api::types::PlaytestResult {
            frames_played,
            alive: player_alive,
            goal_reached,
            deaths: std::mem::take(&mut pt.deaths),
            damage_taken: pt.total_damage,
            distance_traveled: pt.distance_traveled,
            tiles_explored,
            events: std::mem::take(&mut pt.events),
            stuck_count,
            input_summary: pt.input_counts.clone(),
            difficulty_rating: difficulty_rating.to_string(),
            notes,
        }));
    }

    runtime.state = pt.saved_runtime_state.clone();
    pending.active = None;
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
            ..Default::default()
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
            entities: vec![],
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
            entities: vec![],
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
            entities: vec![],
        };
        let result = run_simulation(&map, &cfg, &req);
        assert_eq!(result.outcome, "goal_reached");
        assert!(result.events.iter().any(|e| e.event_type == "goal_reached"));
    }
}
