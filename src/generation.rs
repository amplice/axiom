use serde::{Deserialize, Serialize};
use crate::components::{PhysicsConfig, TileType};
use crate::tilemap::Tilemap;
use crate::constraints;

#[derive(Deserialize)]
pub struct GenerateRequest {
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub difficulty: f32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub feel_target: Option<String>,
}

fn default_seed() -> u64 { 42 }

#[derive(Serialize)]
pub struct GenerateResult {
    pub tilemap: GeneratedTilemap,
    pub player_spawn: (f32, f32),
    pub goal: (i32, i32),
    pub validation: serde_json::Value,
    pub difficulty_metrics: DifficultyMetrics,
}

#[derive(Serialize)]
pub struct GeneratedTilemap {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
}

#[derive(Serialize)]
pub struct DifficultyMetrics {
    pub required_jumps: u32,
    pub max_gap: u32,
    pub precision_sections: u32,
    pub spike_count: u32,
}

/// Simple seeded RNG (xorshift64)
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn float(&mut self) -> f32 {
        (self.next() % 10000) as f32 / 10000.0
    }

    fn range(&mut self, min: u32, max: u32) -> u32 {
        if max <= min { return min; }
        min + (self.next() % (max - min) as u64) as u32
    }
}

pub fn generate(req: &GenerateRequest, physics: &PhysicsConfig) -> GenerateResult {
    let width = req.width.unwrap_or(80);
    let height = req.height.unwrap_or(20);
    let ts = physics.tile_size;
    let mut rng = Rng::new(req.seed);

    let mut tiles = vec![0u8; width * height];
    let mut metrics = DifficultyMetrics {
        required_jumps: 0,
        max_gap: 0,
        precision_sections: 0,
        spike_count: 0,
    };

    // Calculate jump capabilities
    let max_jump_height = (physics.jump_velocity * physics.jump_velocity) / (2.0 * physics.gravity);
    let max_jump_tiles = (max_jump_height / ts).floor() as u32;
    let jump_time = 2.0 * physics.jump_velocity / physics.gravity;
    let max_jump_dist = (jump_time * physics.move_speed / ts).floor() as u32;

    // Scale parameters by difficulty
    let gap_chance = 0.05 + req.difficulty * 0.25;
    let max_gap_width = 2 + (req.difficulty * (max_jump_dist as f32 - 2.0)).floor() as u32;
    let platform_chance = 0.1 + req.difficulty * 0.3;
    let spike_chance = req.difficulty * 0.15;
    let platform_height_range = 2 + (req.difficulty * (max_jump_tiles as f32 - 2.0).max(0.0)).floor() as u32;

    // Step 1: Generate ground floor
    let mut x = 0usize;
    while x < width {
        // Decide: ground or gap
        if x > 3 && x < width - 5 && rng.float() < gap_chance {
            let gap_width = rng.range(2, max_gap_width.min(width as u32 - x as u32 - 3)) as usize;
            metrics.max_gap = metrics.max_gap.max(gap_width as u32);
            metrics.required_jumps += 1;

            // Add spikes in gap?
            if rng.float() < spike_chance {
                for gx in x..x + gap_width {
                    if gx < width {
                        tiles[0 * width + gx] = TileType::Spike as u8;
                        metrics.spike_count += 1;
                    }
                }
            }

            x += gap_width;
        } else {
            tiles[0 * width + x] = TileType::Solid as u8;
            x += 1;
        }
    }

    // Step 2: Add platforms
    let num_platforms = (3.0 + req.difficulty * 10.0) as u32;
    for _ in 0..num_platforms {
        if rng.float() < platform_chance || req.difficulty > 0.3 {
            let px = rng.range(5, (width - 5) as u32) as usize;
            let py = rng.range(2, platform_height_range.min(height as u32 - 2)) as usize;
            let pw = rng.range(2, 5) as usize;

            for dx in 0..pw {
                if px + dx < width && py < height {
                    tiles[py * width + px + dx] = TileType::Solid as u8;
                }
            }

            // Harder difficulty → sometimes require platform use
            if req.difficulty > 0.5 && rng.float() < 0.3 {
                metrics.precision_sections += 1;
            }
        }
    }

    // Step 3: Ensure ground at start and end
    for x in 0..4 {
        tiles[0 * width + x] = TileType::Solid as u8;
    }
    for x in (width - 5)..width {
        tiles[0 * width + x] = TileType::Solid as u8;
    }

    // Step 4: Place goal
    let goal_x = (width - 3) as i32;
    let goal_y = 1i32;
    tiles[goal_y as usize * width + goal_x as usize] = TileType::Goal as u8;

    // Step 5: Player spawn
    let spawn = (2.0 * ts + ts / 2.0, ts + ts / 2.0 + 1.0);

    // Build tilemap for validation
    let tilemap = Tilemap {
        width,
        height,
        tiles: tiles.clone(),
        player_spawn: spawn,
        goal: Some((goal_x, goal_y)),
    };

    // Step 6: Validate if requested
    let validation = if !req.constraints.is_empty() {
        let result = constraints::validate(&tilemap, physics, &req.constraints);
        serde_json::to_value(&result).unwrap_or(serde_json::json!({"error": "serialize failed"}))
    } else {
        serde_json::json!({"skipped": true})
    };

    GenerateResult {
        tilemap: GeneratedTilemap { width, height, tiles },
        player_spawn: spawn,
        goal: (goal_x, goal_y),
        validation,
        difficulty_metrics: metrics,
    }
}
