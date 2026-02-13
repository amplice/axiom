use serde::Serialize;
use crate::components::PhysicsConfig;
use crate::tilemap::Tilemap;
use crate::simulation::{self, SimulationRequest, SimInput};

#[derive(Serialize, Clone)]
pub struct JumpProfile {
    pub rise_frames: u32,
    pub fall_frames: u32,
    pub max_height_tiles: f32,
    pub gravity_ratio: f32,
    pub horizontal_distance_tiles: f32,
    pub variable_height: bool,
    pub coyote_frames: u32,
    pub jump_buffer_frames: u32,
}

#[derive(Serialize)]
pub struct FeelComparison {
    pub current: JumpProfile,
    pub target: JumpProfile,
    pub target_name: String,
    pub deviations: JumpDeviations,
    pub overall_match_pct: f32,
}

#[derive(Serialize)]
pub struct JumpDeviations {
    pub rise_frames: i32,
    pub fall_frames: i32,
    pub max_height: f32,
    pub gravity_ratio: f32,
}

#[derive(Serialize)]
pub struct TuneResult {
    pub physics: PhysicsConfig,
    pub before: JumpProfile,
    pub after: JumpProfile,
    pub target: JumpProfile,
    pub match_pct: f32,
}

/// Measure jump feel by running a simulation with a jump input
pub fn measure_jump(tilemap: &Tilemap, physics: &PhysicsConfig) -> JumpProfile {
    let ts = physics.tile_size;

    // Create a flat floor tilemap for measurement
    let width = 40;
    let height = 20;
    let mut tiles = vec![0u8; width * height];
    for x in 0..width {
        tiles[x] = 1; // solid ground at y=0
    }

    let measure_map = Tilemap {
        width,
        height,
        tiles,
        player_spawn: (5.0 * ts + ts / 2.0, ts + ts / 2.0 + 1.0),
        goal: None,
    };

    // Full jump: hold jump for entire duration
    let req = SimulationRequest {
        tilemap: None,
        inputs: vec![
            SimInput { frame: 0, action: "jump".to_string(), duration: 60 },
            SimInput { frame: 0, action: "right".to_string(), duration: 120 },
        ],
        max_frames: 180,
        record_interval: 1,
        physics: None,
    };
    let result = simulation::run_simulation(&measure_map, physics, &req);

    // Analyze trace
    let mut rise_frames = 0u32;
    let mut fall_frames = 0u32;
    let mut max_y = f32::MIN;
    let mut start_y = 0.0f32;
    let mut apex_frame = 0u32;
    let mut landed = false;
    let mut land_x = 0.0f32;

    if let Some(first) = result.trace.first() {
        start_y = first.y;
    }

    for (i, frame) in result.trace.iter().enumerate() {
        if frame.y > max_y {
            max_y = frame.y;
            apex_frame = frame.frame;
        }
        if i > 0 && frame.grounded && !result.trace[i - 1].grounded && frame.frame > 5 {
            landed = true;
            land_x = frame.x;
            break;
        }
    }

    if let Some(first) = result.trace.first() {
        rise_frames = apex_frame - first.frame;
    }

    // Count fall frames from apex to landing
    let mut counting_fall = false;
    for frame in &result.trace {
        if frame.frame == apex_frame {
            counting_fall = true;
            continue;
        }
        if counting_fall {
            fall_frames += 1;
            if frame.grounded {
                break;
            }
        }
    }

    let max_height_tiles = (max_y - start_y) / ts;
    let gravity_ratio = if rise_frames > 0 {
        fall_frames as f32 / rise_frames as f32
    } else {
        1.0
    };

    let start_x = result.trace.first().map(|f| f.x).unwrap_or(0.0);
    let horizontal_distance_tiles = (land_x - start_x).abs() / ts;

    // Test variable height: do a short jump (release after 3 frames)
    let short_req = SimulationRequest {
        tilemap: None,
        inputs: vec![
            SimInput { frame: 0, action: "jump".to_string(), duration: 3 },
            SimInput { frame: 0, action: "right".to_string(), duration: 120 },
        ],
        max_frames: 180,
        record_interval: 1,
        physics: None,
    };
    let short_result = simulation::run_simulation(&measure_map, physics, &short_req);
    let short_max_y = short_result.trace.iter().map(|f| f.y).fold(f32::MIN, f32::max);
    let variable_height = (max_y - short_max_y).abs() > ts * 0.5;

    JumpProfile {
        rise_frames,
        fall_frames,
        max_height_tiles,
        gravity_ratio,
        horizontal_distance_tiles,
        variable_height,
        coyote_frames: physics.coyote_frames,
        jump_buffer_frames: physics.jump_buffer_frames,
    }
}

pub fn get_reference_profile(name: &str) -> JumpProfile {
    match name {
        "celeste" => JumpProfile {
            rise_frames: 12,
            fall_frames: 8,
            max_height_tiles: 3.5,
            gravity_ratio: 1.5,
            horizontal_distance_tiles: 6.0,
            variable_height: true,
            coyote_frames: 5,
            jump_buffer_frames: 4,
        },
        "mario" => JumpProfile {
            rise_frames: 18,
            fall_frames: 14,
            max_height_tiles: 4.0,
            gravity_ratio: 1.3,
            horizontal_distance_tiles: 8.0,
            variable_height: true,
            coyote_frames: 3,
            jump_buffer_frames: 3,
        },
        "hollow_knight" => JumpProfile {
            rise_frames: 14,
            fall_frames: 10,
            max_height_tiles: 3.0,
            gravity_ratio: 1.4,
            horizontal_distance_tiles: 5.0,
            variable_height: false,
            coyote_frames: 4,
            jump_buffer_frames: 3,
        },
        _ => get_reference_profile("celeste"), // default
    }
}

pub fn compare(current: &JumpProfile, target: &JumpProfile) -> FeelComparison {
    let devs = JumpDeviations {
        rise_frames: current.rise_frames as i32 - target.rise_frames as i32,
        fall_frames: current.fall_frames as i32 - target.fall_frames as i32,
        max_height: current.max_height_tiles - target.max_height_tiles,
        gravity_ratio: current.gravity_ratio - target.gravity_ratio,
    };

    // Calculate match percentage
    let rise_match = 1.0 - (devs.rise_frames.abs() as f32 / target.rise_frames.max(1) as f32).min(1.0);
    let fall_match = 1.0 - (devs.fall_frames.abs() as f32 / target.fall_frames.max(1) as f32).min(1.0);
    let height_match = 1.0 - (devs.max_height.abs() / target.max_height_tiles.max(0.1)).min(1.0);
    let ratio_match = 1.0 - (devs.gravity_ratio.abs() / target.gravity_ratio.max(0.1)).min(1.0);

    let overall = (rise_match + fall_match + height_match + ratio_match) / 4.0 * 100.0;

    FeelComparison {
        current: current.clone(),
        target: target.clone(),
        target_name: String::new(),
        deviations: devs,
        overall_match_pct: overall,
    }
}

/// Auto-tune physics to match target jump profile
pub fn auto_tune(tilemap: &Tilemap, physics: &PhysicsConfig, target: &JumpProfile) -> TuneResult {
    let before = measure_jump(tilemap, physics);
    let mut best_physics = physics.clone();
    let mut best_match = 0.0f32;

    // Target max height in world units
    let ts = physics.tile_size;
    let target_height = target.max_height_tiles * ts;
    let target_rise_time = target.rise_frames as f32 / 60.0;

    // From kinematics: h = v*t - 0.5*g*t^2, and v = g*t at apex
    // So: h = 0.5 * g * t^2, meaning g = 2*h/t^2, v = g*t
    if target_rise_time > 0.0 {
        let tuned_gravity = 2.0 * target_height / (target_rise_time * target_rise_time);
        let tuned_jump_vel = tuned_gravity * target_rise_time;

        best_physics.gravity = tuned_gravity;
        best_physics.jump_velocity = tuned_jump_vel;

        // Fall multiplier from gravity ratio
        // gravity_ratio = fall_frames / rise_frames
        // Fall is faster → fall_multiplier > 1
        best_physics.fall_multiplier = target.gravity_ratio;

        // Coyote and buffer frames
        best_physics.coyote_frames = target.coyote_frames;
        best_physics.jump_buffer_frames = target.jump_buffer_frames;

        // Adjust move speed for horizontal distance
        // horizontal_distance ≈ move_speed * total_air_time / tile_size
        let total_air_frames = target.rise_frames + target.fall_frames;
        let total_air_time = total_air_frames as f32 / 60.0;
        if total_air_time > 0.0 {
            best_physics.move_speed = target.horizontal_distance_tiles * ts / total_air_time;
        }
    }

    let after = measure_jump(tilemap, &best_physics);
    let comparison = compare(&after, target);

    TuneResult {
        physics: best_physics,
        before,
        after,
        target: target.clone(),
        match_pct: comparison.overall_match_pct,
    }
}
