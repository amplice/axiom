use serde::{Deserialize, Serialize};
use crate::components::{PhysicsConfig, TileType};
use crate::tilemap::Tilemap;

const PLAYER_WIDTH: f32 = 12.0;
const PLAYER_HEIGHT: f32 = 14.0;

#[derive(Deserialize, Clone)]
pub struct SimulationRequest {
    pub tilemap: Option<SimTilemap>,
    pub inputs: Vec<SimInput>,
    pub max_frames: u32,
    #[serde(default = "default_record_interval")]
    pub record_interval: u32,
    pub physics: Option<SimPhysics>,
}

fn default_record_interval() -> u32 { 1 }

#[derive(Deserialize, Clone)]
pub struct SimTilemap {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub player_spawn: Option<(f32, f32)>,
    pub goal: Option<(i32, i32)>,
}

#[derive(Deserialize, Clone)]
pub struct SimPhysics {
    pub gravity: Option<f32>,
    pub jump_velocity: Option<f32>,
    pub move_speed: Option<f32>,
    pub fall_multiplier: Option<f32>,
    pub coyote_frames: Option<u32>,
    pub jump_buffer_frames: Option<u32>,
}

#[derive(Deserialize, Clone)]
pub struct SimInput {
    pub frame: u32,
    pub action: String,
    #[serde(default)]
    pub duration: u32,
}

#[derive(Serialize, Clone)]
pub struct SimulationResult {
    pub outcome: String,
    pub frames_elapsed: u32,
    pub trace: Vec<TraceFrame>,
    pub events: Vec<SimEvent>,
}

#[derive(Serialize, Clone)]
pub struct TraceFrame {
    pub frame: u32,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
}

#[derive(Serialize, Clone)]
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

struct InputState {
    left: bool,
    right: bool,
    jump_pressed: bool,
    jump_just_pressed: bool,
}

pub fn run_simulation(
    tilemap: &Tilemap,
    config: &PhysicsConfig,
    request: &SimulationRequest,
) -> SimulationResult {
    let ts = config.tile_size;
    let dt = 1.0 / 60.0f32;

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

    let mut trace = Vec::new();
    let mut events = Vec::new();
    let mut outcome = "timeout".to_string();

    // Pre-process inputs into per-frame active actions
    let mut active_inputs: Vec<Vec<String>> = vec![Vec::new(); request.max_frames as usize + 1];
    for input in &request.inputs {
        let duration = if input.duration == 0 { 1 } else { input.duration };
        for f in input.frame..(input.frame + duration).min(request.max_frames) {
            active_inputs[f as usize].push(input.action.clone());
        }
    }

    let mut prev_jump = false;

    for frame in 0..request.max_frames {
        // Decode inputs for this frame
        let actions = &active_inputs[frame as usize];
        let jump_now = actions.iter().any(|a| a == "jump" || a == "up");
        let input = InputState {
            left: actions.iter().any(|a| a == "left"),
            right: actions.iter().any(|a| a == "right"),
            jump_pressed: jump_now,
            jump_just_pressed: jump_now && !prev_jump,
        };
        prev_jump = jump_now;

        // Track pre-step state
        state.was_grounded = state.grounded;
        state.was_rising = state.vy > 0.0;

        // Gravity
        if !state.grounded {
            let mult = if state.vy < 0.0 { config.fall_multiplier } else { 1.0 };
            state.vy -= config.gravity * mult * dt;
        }

        // Horizontal movement
        let mut dir = 0.0f32;
        if input.left { dir -= 1.0; }
        if input.right { dir += 1.0; }
        state.vx = dir * config.move_speed;

        // Jump buffer
        if input.jump_just_pressed {
            state.jump_buffer = config.jump_buffer_frames;
        }
        if state.jump_buffer > 0 {
            state.jump_buffer -= 1;
        }

        // Jump
        let can_jump = state.grounded || state.coyote > 0;
        let wants_jump = state.jump_buffer > 0 || input.jump_just_pressed;
        if can_jump && wants_jump {
            state.vy = config.jump_velocity;
            state.coyote = 0;
            state.jump_buffer = 0;
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

        // Variable jump height
        if !input.jump_pressed && state.vy > 0.0 {
            state.vy *= 0.5;
        }

        // Move X
        let new_x = state.x + state.vx * dt;
        if !collides_solid(tilemap, new_x, state.y, ts) {
            state.x = new_x;
        } else {
            if state.vx > 0.0 {
                let tile_x = ((new_x + PLAYER_WIDTH / 2.0) / ts).floor() as i32;
                state.x = tile_x as f32 * ts - PLAYER_WIDTH / 2.0 - 0.01;
            } else if state.vx < 0.0 {
                let tile_x = ((new_x - PLAYER_WIDTH / 2.0) / ts).floor() as i32;
                state.x = (tile_x + 1) as f32 * ts + PLAYER_WIDTH / 2.0 + 0.01;
            }
            state.vx = 0.0;
        }

        // Move Y
        let new_y = state.y + state.vy * dt;
        if !collides_solid(tilemap, state.x, new_y, ts) {
            state.y = new_y;
        } else {
            if state.vy < 0.0 {
                let tile_y = ((new_y - PLAYER_HEIGHT / 2.0) / ts).floor() as i32;
                state.y = (tile_y + 1) as f32 * ts + PLAYER_HEIGHT / 2.0;
            } else if state.vy > 0.0 {
                let tile_y = ((new_y + PLAYER_HEIGHT / 2.0) / ts).floor() as i32;
                state.y = tile_y as f32 * ts - PLAYER_HEIGHT / 2.0 - 0.01;
            }
            state.vy = 0.0;
        }

        // Track max jump height
        if state.y > state.max_jump_y {
            state.max_jump_y = state.y;
        }

        // Check grounded
        let check_y = state.y - PLAYER_HEIGHT / 2.0 - 0.5;
        let left_x = state.x - PLAYER_WIDTH / 2.0 + 1.0;
        let right_x = state.x + PLAYER_WIDTH / 2.0 - 1.0;
        let left_tile = (left_x / ts).floor() as i32;
        let right_tile = (right_x / ts).floor() as i32;
        let ground_tile_y = (check_y / ts).floor() as i32;
        state.grounded = false;
        for tx in left_tile..=right_tile {
            if tilemap.is_solid(tx, ground_tile_y) {
                state.grounded = true;
                break;
            }
        }

        // Coyote timer
        if state.grounded {
            state.coyote = config.coyote_frames;
        } else if state.coyote > 0 {
            state.coyote -= 1;
        }

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

        // Check spikes
        if collides_type(tilemap, state.x, state.y, ts, TileType::Spike) {
            events.push(SimEvent {
                frame,
                event_type: "death".to_string(),
                x: Some(state.x),
                y: Some(state.y),
                height: None,
            });
            outcome = "death".to_string();
            // Record final frame
            trace.push(TraceFrame {
                frame, x: state.x, y: state.y, vx: state.vx, vy: state.vy, grounded: state.grounded,
            });
            break;
        }

        // Check goal
        if collides_type(tilemap, state.x, state.y, ts, TileType::Goal) {
            events.push(SimEvent {
                frame,
                event_type: "goal_reached".to_string(),
                x: Some(state.x),
                y: Some(state.y),
                height: None,
            });
            outcome = "goal_reached".to_string();
            trace.push(TraceFrame {
                frame, x: state.x, y: state.y, vx: state.vx, vy: state.vy, grounded: state.grounded,
            });
            break;
        }

        // Fall out of world
        if state.y < -100.0 {
            events.push(SimEvent {
                frame,
                event_type: "fell_off".to_string(),
                x: Some(state.x),
                y: Some(state.y),
                height: None,
            });
            outcome = "death".to_string();
            trace.push(TraceFrame {
                frame, x: state.x, y: state.y, vx: state.vx, vy: state.vy, grounded: state.grounded,
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
            let old_idx = if trace.len() > 300 { trace.len() - 300 } else { 0 };
            let old = &trace[old_idx];
            let dx = (recent.x - old.x).abs();
            let dy = (recent.y - old.y).abs();
            if dx < 1.0 && dy < 1.0 {
                outcome = "stuck".to_string();
                break;
            }
        }
    }

    SimulationResult {
        outcome,
        frames_elapsed: trace.last().map(|t| t.frame).unwrap_or(0),
        trace,
        events,
    }
}

fn collides_solid(tilemap: &Tilemap, cx: f32, cy: f32, ts: f32) -> bool {
    let hw = PLAYER_WIDTH / 2.0;
    let hh = PLAYER_HEIGHT / 2.0;
    let min_tx = ((cx - hw) / ts).floor() as i32;
    let max_tx = ((cx + hw - 0.01) / ts).floor() as i32;
    let min_ty = ((cy - hh) / ts).floor() as i32;
    let max_ty = ((cy + hh - 0.01) / ts).floor() as i32;

    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            if tilemap.is_solid(tx, ty) {
                let t_min_x = tx as f32 * ts;
                let t_min_y = ty as f32 * ts;
                let t_max_x = t_min_x + ts;
                let t_max_y = t_min_y + ts;
                if cx + hw > t_min_x && cx - hw < t_max_x
                    && cy + hh > t_min_y && cy - hh < t_max_y
                {
                    return true;
                }
            }
        }
    }
    false
}

fn collides_type(tilemap: &Tilemap, cx: f32, cy: f32, ts: f32, target: TileType) -> bool {
    let hw = PLAYER_WIDTH / 2.0;
    let hh = PLAYER_HEIGHT / 2.0;
    let min_tx = ((cx - hw) / ts).floor() as i32;
    let max_tx = ((cx + hw - 0.01) / ts).floor() as i32;
    let min_ty = ((cy - hh) / ts).floor() as i32;
    let max_ty = ((cy + hh - 0.01) / ts).floor() as i32;

    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            if tilemap.get(tx, ty) == target {
                let t_min_x = tx as f32 * ts;
                let t_min_y = ty as f32 * ts;
                let t_max_x = t_min_x + ts;
                let t_max_y = t_min_y + ts;
                if cx + hw > t_min_x && cx - hw < t_max_x
                    && cy + hh > t_min_y && cy - hh < t_max_y
                {
                    return true;
                }
            }
        }
    }
    false
}
