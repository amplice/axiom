use crate::components::{GameConfig, TileType};
use crate::constraints;
use crate::tilemap::Tilemap;
use rand::rngs::SmallRng;
use rand::{Rng as _, SeedableRng};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone)]
pub struct GenerateRequest {
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub difficulty: f32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub feel_target: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
}

fn default_seed() -> u64 {
    42
}

#[derive(Serialize, Clone)]
pub struct GenerateResult {
    pub tilemap: GeneratedTilemap,
    pub player_spawn: (f32, f32),
    pub goal: (i32, i32),
    pub entities: Vec<EntityPlacement>,
    pub scripts: Vec<ScriptAssignment>,
    pub validation: serde_json::Value,
    pub difficulty_metrics: DifficultyMetrics,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EntityPlacement {
    pub preset: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScriptAssignment {
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub global: bool,
}

#[derive(Serialize, Clone)]
pub struct GeneratedTilemap {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
}

#[derive(Serialize, Clone)]
pub struct DifficultyMetrics {
    pub required_jumps: u32,
    pub max_gap: u32,
    pub precision_sections: u32,
    pub spike_count: u32,
}

struct Rng(SmallRng);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(SmallRng::seed_from_u64(seed))
    }

    fn float(&mut self) -> f32 {
        self.0.gen()
    }

    fn range(&mut self, min: u32, max: u32) -> u32 {
        if max <= min {
            return min;
        }
        self.0.gen_range(min..max)
    }
}

pub fn generate(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    match req.template.as_deref() {
        Some("top_down_dungeon") => generate_top_down_dungeon(req, physics),
        Some("rts_arena") => generate_rts_arena(req, physics),
        Some("fighting_arena") => generate_fighting_arena(req, physics),
        Some("metroidvania") => generate_metroidvania(&req_with_dims(req, 120, 28), physics),
        Some("roguelike_floor") => generate_roguelike_floor(&req_with_dims(req, 52, 34), physics),
        Some("puzzle_platformer") => {
            generate_puzzle_platformer(&req_with_dims(req, 96, 24), physics)
        }
        Some("arena_waves") => generate_arena_waves(&req_with_dims(req, 68, 44), physics),
        Some("side_scroller") => generate_side_scroller(&req_with_dims(req, 130, 20), physics),
        Some("tower_defense_map") => {
            generate_tower_defense_map(&req_with_dims(req, 72, 40), physics)
        }
        Some("boss_arena") => generate_boss_arena(&req_with_dims(req, 42, 18), physics),
        _ => generate_platformer(req, physics),
    }
}

fn req_with_dims(req: &GenerateRequest, width: usize, height: usize) -> GenerateRequest {
    let mut out = req.clone();
    if out.width.is_none() {
        out.width = Some(width);
    }
    if out.height.is_none() {
        out.height = Some(height);
    }
    out
}

fn run_validation(
    tilemap: &Tilemap,
    physics: &GameConfig,
    constraints_list: &[String],
    entities: &[EntityPlacement],
) -> serde_json::Value {
    if constraints_list.is_empty() {
        return serde_json::json!({ "skipped": true });
    }
    let validate_entities = entities
        .iter()
        .map(|e| crate::constraints::ValidateEntity {
            x: e.x,
            y: e.y,
            preset: e.preset.clone(),
            config: e.config.clone(),
        })
        .collect::<Vec<_>>();
    let result = constraints::validate(tilemap, physics, constraints_list, &validate_entities);
    serde_json::to_value(&result).unwrap_or(serde_json::json!({ "error": "serialize failed" }))
}

fn generate_platformer(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
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
    let grav = physics.gravity_magnitude();
    let max_jump_height = (physics.jump_velocity * physics.jump_velocity) / (2.0 * grav);
    let max_jump_tiles = (max_jump_height / ts).floor() as u32;
    let jump_time = 2.0 * physics.jump_velocity / grav;
    let max_jump_dist = (jump_time * physics.move_speed / ts).floor() as u32;

    // Scale parameters by difficulty
    let gap_chance = 0.05 + req.difficulty * 0.25;
    let max_gap_width = 2 + (req.difficulty * (max_jump_dist as f32 - 2.0)).floor() as u32;
    let platform_chance = 0.1 + req.difficulty * 0.3;
    let spike_chance = req.difficulty * 0.15;
    let platform_height_range =
        2 + (req.difficulty * (max_jump_tiles as f32 - 2.0).max(0.0)).floor() as u32;

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
                let gap_end = (x + gap_width).min(width);
                for tile in tiles.iter_mut().take(gap_end).skip(x) {
                    *tile = TileType::Spike as u8;
                    metrics.spike_count += 1;
                }
            }

            x += gap_width;
        } else {
            tiles[x] = TileType::Solid as u8;
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
    for tile in tiles.iter_mut().take(4) {
        *tile = TileType::Solid as u8;
    }
    for tile in tiles.iter_mut().take(width).skip(width - 5) {
        *tile = TileType::Solid as u8;
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
        ..Default::default()
    };

    let mut entities = Vec::new();
    let enemy_count = (1.0 + req.difficulty * 4.0).round() as usize;
    for i in 0..enemy_count {
        let x = ((10 + i * 14).min(width.saturating_sub(4)) as f32) * ts + ts / 2.0;
        entities.push(EntityPlacement {
            preset: "patrol_enemy".to_string(),
            x,
            y: ts + ts / 2.0 + 1.0,
            config: serde_json::json!({
                "script": "enemy_patrol",
                "tags": ["enemy", "patrol"]
            }),
        });
    }
    let pickup_count = (1.0 + (1.0 - req.difficulty.clamp(0.0, 1.0)) * 2.0).round() as usize;
    for i in 0..pickup_count {
        let x = ((8 + i * 20).min(width.saturating_sub(5)) as f32) * ts + ts / 2.0;
        entities.push(EntityPlacement {
            preset: "health_pickup".to_string(),
            x,
            y: 4.0 * ts + ts / 2.0,
            config: serde_json::json!({}),
        });
    }
    let validation = run_validation(&tilemap, physics, &req.constraints, &entities);

    GenerateResult {
        tilemap: GeneratedTilemap {
            width,
            height,
            tiles,
        },
        player_spawn: spawn,
        goal: (goal_x, goal_y),
        entities,
        scripts: vec![ScriptAssignment {
            name: "enemy_patrol".to_string(),
            source: r#"
function update(entity, world, dt)
    if entity.state.dir == nil then entity.state.dir = 1 end
    if entity.state.t == nil then entity.state.t = 0 end
    entity.state.t = entity.state.t + 1
    if entity.state.t > 90 then
        entity.state.dir = -entity.state.dir
        entity.state.t = 0
    end
    entity.vx = 90 * entity.state.dir
end
"#
            .trim()
            .to_string(),
            global: false,
        }],
        validation,
        difficulty_metrics: metrics,
    }
}

/// Generate a top-down dungeon: rooms connected by corridors, walls are solid
fn generate_top_down_dungeon(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let width = req.width.unwrap_or(40);
    let height = req.height.unwrap_or(30);
    let ts = physics.tile_size;
    let mut rng = Rng::new(req.seed);

    // Start with all solid walls
    let mut tiles = vec![TileType::Solid as u8; width * height];

    let num_rooms = 4 + (req.difficulty * 6.0) as u32;
    let mut rooms: Vec<(usize, usize, usize, usize)> = Vec::new(); // (x, y, w, h)

    // Carve rooms
    for _ in 0..num_rooms * 3 {
        if rooms.len() >= num_rooms as usize {
            break;
        }
        let rw = rng.range(3, 8) as usize;
        let rh = rng.range(3, 6) as usize;
        let rx = rng.range(1, (width - rw - 1) as u32) as usize;
        let ry = rng.range(1, (height - rh - 1) as u32) as usize;

        // Check overlap
        let overlaps = rooms.iter().any(|&(ox, oy, ow, oh)| {
            rx < ox + ow + 1 && rx + rw + 1 > ox && ry < oy + oh + 1 && ry + rh + 1 > oy
        });
        if overlaps {
            continue;
        }

        // Carve room
        for dy in 0..rh {
            for dx in 0..rw {
                tiles[(ry + dy) * width + rx + dx] = TileType::Empty as u8;
            }
        }
        rooms.push((rx, ry, rw, rh));
    }

    // Connect rooms with corridors (L-shaped)
    for i in 1..rooms.len() {
        let (ax, ay, aw, ah) = rooms[i - 1];
        let (bx, by, bw, bh) = rooms[i];
        let cx1 = ax + aw / 2;
        let cy1 = ay + ah / 2;
        let cx2 = bx + bw / 2;
        let cy2 = by + bh / 2;

        // Horizontal then vertical
        let (start_x, end_x) = if cx1 < cx2 { (cx1, cx2) } else { (cx2, cx1) };
        for x in start_x..=end_x {
            if x < width {
                tiles[cy1 * width + x] = TileType::Empty as u8;
            }
        }
        let (start_y, end_y) = if cy1 < cy2 { (cy1, cy2) } else { (cy2, cy1) };
        for y in start_y..=end_y {
            if y < height {
                tiles[y * width + cx2] = TileType::Empty as u8;
            }
        }
    }

    // Spawn in first room, goal in last room (set these BEFORE placing spikes)
    let (sx, sy, sw, sh) = rooms[0];
    let spawn_tx = sx + sw / 2;
    let spawn_ty = sy + sh / 2;
    let spawn = ((spawn_tx as f32 + 0.5) * ts, (spawn_ty as f32 + 0.5) * ts);
    let (gx, gy, gw, gh) = rooms[rooms.len() - 1];
    let goal_x = (gx + gw / 2) as i32;
    let goal_y = (gy + gh / 2) as i32;
    tiles[goal_y as usize * width + goal_x as usize] = TileType::Goal as u8;

    // Add spikes based on difficulty, but only in rooms (not corridors) and never blocking reachability
    let spike_count = (req.difficulty * 8.0) as u32;
    let mut spikes_placed = 0u32;
    let spawn_tile = (spawn_tx as i32, spawn_ty as i32);
    for _ in 0..spike_count * 10 {
        if spikes_placed >= spike_count {
            break;
        }
        let spike_x = rng.range(1, (width - 1) as u32) as usize;
        let spike_y = rng.range(1, (height - 1) as u32) as usize;
        if tiles[spike_y * width + spike_x] != TileType::Empty as u8 {
            continue;
        }
        // Don't place on spawn or goal tile
        if (spike_x as i32, spike_y as i32) == spawn_tile {
            continue;
        }
        if (spike_x as i32, spike_y as i32) == (goal_x, goal_y) {
            continue;
        }
        // Only place in rooms, not corridors (tile must have at least 2 empty orthogonal neighbors)
        let mut empty_neighbors = 0;
        if spike_x > 0 && tiles[spike_y * width + spike_x - 1] != TileType::Solid as u8 {
            empty_neighbors += 1;
        }
        if spike_x + 1 < width && tiles[spike_y * width + spike_x + 1] != TileType::Solid as u8 {
            empty_neighbors += 1;
        }
        if spike_y > 0 && tiles[(spike_y - 1) * width + spike_x] != TileType::Solid as u8 {
            empty_neighbors += 1;
        }
        if spike_y + 1 < height && tiles[(spike_y + 1) * width + spike_x] != TileType::Solid as u8 {
            empty_neighbors += 1;
        }
        if empty_neighbors < 3 {
            continue;
        } // skip narrow corridors
          // Place spike and verify reachability
        tiles[spike_y * width + spike_x] = TileType::Spike as u8;
        let test_tilemap = Tilemap {
            width,
            height,
            tiles: tiles.clone(),
            player_spawn: spawn,
            goal: Some((goal_x, goal_y)),
            ..Default::default()
        };
        if constraints::check_top_down_reachability_pub(&test_tilemap, spawn_tile, (goal_x, goal_y))
        {
            spikes_placed += 1;
        } else {
            tiles[spike_y * width + spike_x] = TileType::Empty as u8; // revert
        }
    }

    let tilemap = Tilemap {
        width,
        height,
        tiles: tiles.clone(),
        player_spawn: spawn,
        goal: Some((goal_x, goal_y)),
        ..Default::default()
    };

    let mut entities = Vec::new();
    let enemy_count = (1.0 + req.difficulty * 4.0).round() as usize;
    for (idx, room) in rooms.iter().enumerate().skip(1).take(enemy_count) {
        if idx >= rooms.len().saturating_sub(1) {
            break;
        }
        let (rx, ry, rw, rh) = *room;
        let x = (rx as f32 + rw as f32 / 2.0 + 0.5) * ts;
        let y = (ry as f32 + rh as f32 / 2.0 + 0.5) * ts;
        entities.push(EntityPlacement {
            preset: "chase_enemy".to_string(),
            x,
            y,
            config: serde_json::json!({
                "script": "enemy_chase_player",
                "tags": ["enemy", "chaser"]
            }),
        });
    }
    for room in rooms.iter().skip(1).take(2) {
        let (rx, ry, rw, rh) = *room;
        entities.push(EntityPlacement {
            preset: "health_pickup".to_string(),
            x: (rx as f32 + rw as f32 * 0.35 + 0.5) * ts,
            y: (ry as f32 + rh as f32 * 0.35 + 0.5) * ts,
            config: serde_json::json!({}),
        });
    }
    let validation = run_validation(&tilemap, physics, &req.constraints, &entities);

    GenerateResult {
        tilemap: GeneratedTilemap {
            width,
            height,
            tiles,
        },
        player_spawn: spawn,
        goal: (goal_x, goal_y),
        entities,
        scripts: vec![ScriptAssignment {
            name: "enemy_chase_player".to_string(),
            source: r#"
function update(entity, world, dt)
    local player = world.player()
    if player == nil then return end
    local dx = player.x - entity.x
    local dy = player.y - entity.y
    local d = math.sqrt(dx * dx + dy * dy)
    if d > 1 then
        entity.vx = (dx / d) * 85
        entity.vy = (dy / d) * 85
    else
        entity.vx = 0
        entity.vy = 0
    end
end
"#
            .trim()
            .to_string(),
            global: false,
        }],
        validation,
        difficulty_metrics: DifficultyMetrics {
            required_jumps: 0,
            max_gap: 0,
            precision_sections: rooms.len() as u32,
            spike_count: spikes_placed,
        },
    }
}

/// Generate an RTS arena: open field with resource nodes and obstacles
fn generate_rts_arena(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let width = req.width.unwrap_or(60);
    let height = req.height.unwrap_or(40);
    let ts = physics.tile_size;
    let mut rng = Rng::new(req.seed);

    // Start with all empty (open field)
    let mut tiles = vec![TileType::Empty as u8; width * height];

    // Border walls
    for x in 0..width {
        tiles[x] = TileType::Solid as u8;
        tiles[(height - 1) * width + x] = TileType::Solid as u8;
    }
    for y in 0..height {
        tiles[y * width] = TileType::Solid as u8;
        tiles[y * width + width - 1] = TileType::Solid as u8;
    }

    // Scatter obstacle clusters
    let num_obstacles = 5 + (req.difficulty * 10.0) as u32;
    for _ in 0..num_obstacles {
        let ox = rng.range(3, (width - 5) as u32) as usize;
        let oy = rng.range(3, (height - 5) as u32) as usize;
        let ow = rng.range(1, 4) as usize;
        let oh = rng.range(1, 4) as usize;
        for dy in 0..oh {
            for dx in 0..ow {
                if ox + dx < width && oy + dy < height {
                    tiles[(oy + dy) * width + ox + dx] = TileType::Solid as u8;
                }
            }
        }
    }

    // Place "resource" spots as goal tiles (triggers)
    let num_resources = 3 + (req.difficulty * 4.0) as u32;
    for _ in 0..num_resources {
        let rx = rng.range(2, (width - 2) as u32) as usize;
        let ry = rng.range(2, (height - 2) as u32) as usize;
        if tiles[ry * width + rx] == TileType::Empty as u8 {
            tiles[ry * width + rx] = TileType::Goal as u8;
        }
    }

    // Spawn base at bottom-left, objective at top-right
    let spawn = (3.5 * ts, 3.5 * ts);
    let goal_x = (width - 4) as i32;
    let goal_y = (height - 4) as i32;
    // Ensure goal tile is set
    tiles[goal_y as usize * width + goal_x as usize] = TileType::Goal as u8;

    let tilemap = Tilemap {
        width,
        height,
        tiles: tiles.clone(),
        player_spawn: spawn,
        goal: Some((goal_x, goal_y)),
        ..Default::default()
    };

    let mut entities = Vec::new();
    let enemy_count = (2.0 + req.difficulty * 6.0).round() as usize;
    for i in 0..enemy_count {
        let frac = (i + 1) as f32 / (enemy_count + 1) as f32;
        let x = (2.5 + frac * (width as f32 - 4.0)) * ts;
        let y = (2.5 + ((i as u32 % 3) as f32) * 4.0) * ts;
        entities.push(EntityPlacement {
            preset: "chase_enemy".to_string(),
            x,
            y,
            config: serde_json::json!({
                "script": "enemy_chase_player",
                "tags": ["enemy", "wave"]
            }),
        });
    }
    entities.push(EntityPlacement {
        preset: "health_pickup".to_string(),
        x: (width as f32 * 0.5 + 0.5) * ts,
        y: (height as f32 * 0.5 + 0.5) * ts,
        config: serde_json::json!({}),
    });
    let validation = run_validation(&tilemap, physics, &req.constraints, &entities);

    GenerateResult {
        tilemap: GeneratedTilemap {
            width,
            height,
            tiles,
        },
        player_spawn: spawn,
        goal: (goal_x, goal_y),
        entities,
        scripts: vec![ScriptAssignment {
            name: "enemy_chase_player".to_string(),
            source: r#"
function update(entity, world, dt)
    local player = world.player()
    if player == nil then return end
    local dx = player.x - entity.x
    local dy = player.y - entity.y
    local d = math.sqrt(dx * dx + dy * dy)
    if d > 1 then
        entity.vx = (dx / d) * 95
        entity.vy = (dy / d) * 95
    end
end
"#
            .trim()
            .to_string(),
            global: false,
        }],
        validation,
        difficulty_metrics: DifficultyMetrics {
            required_jumps: 0,
            max_gap: 0,
            precision_sections: num_resources,
            spike_count: 0,
        },
    }
}

/// Generate a fighting game arena: flat floor with walls
fn generate_fighting_arena(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let width = req.width.unwrap_or(30);
    let height = req.height.unwrap_or(15);
    let ts = physics.tile_size;
    let mut rng = Rng::new(req.seed);

    let mut tiles = vec![TileType::Empty as u8; width * height];

    // Floor
    for tile in tiles.iter_mut().take(width) {
        *tile = TileType::Solid as u8;
    }

    // Side walls
    for y in 0..height {
        tiles[y * width] = TileType::Solid as u8;
        tiles[y * width + width - 1] = TileType::Solid as u8;
    }

    // Platforms based on difficulty
    let num_platforms = (req.difficulty * 3.0) as u32;
    for _ in 0..num_platforms {
        let px = rng.range(4, (width - 6) as u32) as usize;
        let py = rng.range(3, (height / 2) as u32) as usize;
        let pw = rng.range(3, 7) as usize;
        for dx in 0..pw {
            if px + dx < width && py < height {
                tiles[py * width + px + dx] = TileType::Solid as u8;
            }
        }
    }

    // Player 1 spawn left, Player 2 spawn right (goal marks P2 start)
    let spawn = (4.0 * ts, ts + ts / 2.0 + 1.0);
    let goal_x = (width - 5) as i32;
    let goal_y = 1i32;
    tiles[goal_y as usize * width + goal_x as usize] = TileType::Goal as u8;

    let tilemap = Tilemap {
        width,
        height,
        tiles: tiles.clone(),
        player_spawn: spawn,
        goal: Some((goal_x, goal_y)),
        ..Default::default()
    };

    let entities = vec![
        EntityPlacement {
            preset: "patrol_enemy".to_string(),
            x: (width as f32 - 7.0) * ts,
            y: ts + ts / 2.0 + 1.0,
            config: serde_json::json!({
                "script": "enemy_patrol",
                "tags": ["enemy", "fighter"]
            }),
        },
        EntityPlacement {
            preset: "health_pickup".to_string(),
            x: (width as f32 * 0.5) * ts,
            y: (3.0 * ts),
            config: serde_json::json!({}),
        },
    ];
    let validation = run_validation(&tilemap, physics, &req.constraints, &entities);

    GenerateResult {
        tilemap: GeneratedTilemap {
            width,
            height,
            tiles,
        },
        player_spawn: spawn,
        goal: (goal_x, goal_y),
        entities,
        scripts: vec![ScriptAssignment {
            name: "enemy_patrol".to_string(),
            source: r#"
function update(entity, world, dt)
    if entity.state.dir == nil then entity.state.dir = -1 end
    if entity.state.t == nil then entity.state.t = 0 end
    entity.state.t = entity.state.t + 1
    if entity.state.t > 75 then
        entity.state.dir = -entity.state.dir
        entity.state.t = 0
    end
    entity.vx = 120 * entity.state.dir
end
"#
            .trim()
            .to_string(),
            global: false,
        }],
        validation,
        difficulty_metrics: DifficultyMetrics {
            required_jumps: 0,
            max_gap: 0,
            precision_sections: num_platforms,
            spike_count: 0,
        },
    }
}

fn generate_arena_waves(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let mut result = generate_rts_arena(req, physics);
    let ts = physics.tile_size;
    let width = result.tilemap.width as f32;
    let height = result.tilemap.height as f32;

    result.entities.clear();
    let wave_count = (2.0 + req.difficulty.clamp(0.0, 1.0) * 4.0).round() as usize;
    for i in 0..wave_count.max(2) {
        let t = i as f32 / wave_count.max(1) as f32;
        let edge_x = if i % 2 == 0 { 2.5 } else { width - 2.5 };
        let edge_y = 3.0 + t * (height - 6.0);
        let preset = if i % 3 == 0 {
            "flying_enemy"
        } else {
            "chase_enemy"
        };
        let speed = if preset == "flying_enemy" {
            155.0
        } else {
            130.0
        } + req.difficulty * 45.0;
        result.entities.push(EntityPlacement {
            preset: preset.to_string(),
            x: edge_x * ts,
            y: edge_y * ts,
            config: serde_json::json!({
                "speed": speed,
                "health": 1.0 + req.difficulty * 2.5,
                "contact_damage": 1.0 + req.difficulty * 0.8,
            }),
        });
    }

    let turret_count = (req.difficulty * 4.0).floor() as usize;
    for i in 0..turret_count {
        let frac = (i + 1) as f32 / (turret_count + 1) as f32;
        result.entities.push(EntityPlacement {
            preset: "turret".to_string(),
            x: (4.0 + frac * (width - 8.0)) * ts,
            y: 3.0 * ts,
            config: serde_json::json!({
                "health": 2.0 + req.difficulty * 3.0,
                "detection_radius": 220.0 + req.difficulty * 120.0,
            }),
        });
    }

    let tilemap = Tilemap {
        width: result.tilemap.width,
        height: result.tilemap.height,
        tiles: result.tilemap.tiles.clone(),
        player_spawn: result.player_spawn,
        goal: Some(result.goal),
        ..Default::default()
    };
    result.validation = run_validation(&tilemap, physics, &req.constraints, &result.entities);
    result
}

fn generate_tower_defense_map(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let width = req.width.unwrap_or(72);
    let height = req.height.unwrap_or(40);
    let ts = physics.tile_size;

    let mut tiles = vec![TileType::Solid as u8; width * height];
    for x in 1..(width - 1) {
        for y in 1..(height - 1) {
            tiles[y * width + x] = TileType::Empty as u8;
        }
    }

    // Build a main lane with choke blocks suitable for tower placements.
    let lane_y = height / 2;
    for x in 1..(width - 1) {
        for y in lane_y.saturating_sub(1)..=(lane_y + 1).min(height - 2) {
            tiles[y * width + x] = TileType::Empty as u8;
        }
    }
    for gate_x in [width / 4, width / 2, (width * 3) / 4] {
        for y in 2..(height - 2) {
            if (y as i32 - lane_y as i32).abs() <= 2 {
                continue;
            }
            tiles[y * width + gate_x] = TileType::Solid as u8;
        }
    }

    let spawn = (2.5 * ts, (lane_y as f32 + 0.5) * ts);
    let goal_x = (width - 3) as i32;
    let goal_y = lane_y as i32;
    tiles[goal_y as usize * width + goal_x as usize] = TileType::Goal as u8;

    let mut entities = Vec::new();
    let turret_count = (3.0 + req.difficulty.clamp(0.0, 1.0) * 5.0).round() as usize;
    for i in 0..turret_count {
        let frac = (i + 1) as f32 / (turret_count + 1) as f32;
        let x = (4.0 + frac * (width as f32 - 8.0)) * ts;
        let y = if i % 2 == 0 {
            (lane_y as f32 - 4.0).max(2.0) * ts
        } else {
            (lane_y as f32 + 4.0).min(height as f32 - 2.0) * ts
        };
        entities.push(EntityPlacement {
            preset: "turret".to_string(),
            x,
            y,
            config: serde_json::json!({
                "health": 2.0 + req.difficulty * 4.0,
                "detection_radius": 210.0 + req.difficulty * 140.0,
            }),
        });
    }

    let tilemap = Tilemap {
        width,
        height,
        tiles: tiles.clone(),
        player_spawn: spawn,
        goal: Some((goal_x, goal_y)),
        ..Default::default()
    };
    let validation = run_validation(&tilemap, physics, &req.constraints, &entities);
    GenerateResult {
        tilemap: GeneratedTilemap {
            width,
            height,
            tiles,
        },
        player_spawn: spawn,
        goal: (goal_x, goal_y),
        entities,
        scripts: vec![],
        validation,
        difficulty_metrics: DifficultyMetrics {
            required_jumps: 0,
            max_gap: 0,
            precision_sections: turret_count as u32,
            spike_count: 0,
        },
    }
}

fn generate_boss_arena(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let mut result = generate_fighting_arena(req, physics);
    let ts = physics.tile_size;
    let width = result.tilemap.width as f32;

    result.entities = vec![
        EntityPlacement {
            preset: "boss".to_string(),
            x: (width - 6.0) * ts,
            y: ts + ts / 2.0 + 1.0,
            config: serde_json::json!({
                "health": 12.0 + req.difficulty * 18.0,
                "contact_damage": 1.5 + req.difficulty * 1.5,
                "speed": 80.0 + req.difficulty * 40.0,
            }),
        },
        EntityPlacement {
            preset: "health_pickup".to_string(),
            x: (width * 0.32) * ts,
            y: 3.0 * ts,
            config: serde_json::json!({}),
        },
    ];

    let tilemap = Tilemap {
        width: result.tilemap.width,
        height: result.tilemap.height,
        tiles: result.tilemap.tiles.clone(),
        player_spawn: result.player_spawn,
        goal: Some(result.goal),
        ..Default::default()
    };
    result.validation = run_validation(&tilemap, physics, &req.constraints, &result.entities);
    result
}

fn generate_metroidvania(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let mut result = generate_platformer(req, physics);
    let ts = physics.tile_size;
    let width = result.tilemap.width;

    // Add an ability gate and a matching grant pickup for ability_gating constraints.
    let gate_x = (width as f32 * 0.62) * ts;
    let grant_x = (width as f32 * 0.28) * ts;
    result.entities.push(EntityPlacement {
        preset: "guard_enemy".to_string(),
        x: gate_x,
        y: ts + ts / 2.0 + 1.0,
        config: serde_json::json!({
            "speed": 120.0 + req.difficulty * 30.0,
            "requires_ability": ["dash"],
            "tags": ["enemy", "gate_keeper"],
        }),
    });
    result.entities.push(EntityPlacement {
        preset: "health_pickup".to_string(),
        x: grant_x,
        y: 4.0 * ts,
        config: serde_json::json!({
            "grants_ability": ["dash"],
            "tags": ["pickup", "ability_dash"],
        }),
    });

    let tilemap = Tilemap {
        width: result.tilemap.width,
        height: result.tilemap.height,
        tiles: result.tilemap.tiles.clone(),
        player_spawn: result.player_spawn,
        goal: Some(result.goal),
        ..Default::default()
    };
    result.validation = run_validation(&tilemap, physics, &req.constraints, &result.entities);
    result
}

fn generate_roguelike_floor(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let mut result = generate_top_down_dungeon(req, physics);
    let ts = physics.tile_size;
    let width = result.tilemap.width as f32;
    let height = result.tilemap.height as f32;
    let enemy_count = (3.0 + req.difficulty.clamp(0.0, 1.0) * 8.0).round() as usize;

    result.entities.clear();
    for i in 0..enemy_count {
        let frac = (i + 1) as f32 / (enemy_count + 1) as f32;
        let x = (2.0 + frac * (width - 4.0)) * ts;
        let y = (2.0 + ((i * 7) % ((height as usize).saturating_sub(4)).max(1)) as f32) * ts;
        let preset = if i % 4 == 0 {
            "flying_enemy"
        } else {
            "chase_enemy"
        };
        result.entities.push(EntityPlacement {
            preset: preset.to_string(),
            x,
            y,
            config: serde_json::json!({
                "health": 1.0 + req.difficulty * 2.0,
                "speed": 115.0 + req.difficulty * 55.0,
                "tags": ["enemy", "roguelike_floor"],
            }),
        });
    }
    result.entities.push(EntityPlacement {
        preset: "health_pickup".to_string(),
        x: width * 0.5 * ts,
        y: height * 0.5 * ts,
        config: serde_json::json!({}),
    });

    let tilemap = Tilemap {
        width: result.tilemap.width,
        height: result.tilemap.height,
        tiles: result.tilemap.tiles.clone(),
        player_spawn: result.player_spawn,
        goal: Some(result.goal),
        ..Default::default()
    };
    result.validation = run_validation(&tilemap, physics, &req.constraints, &result.entities);
    result
}

fn generate_puzzle_platformer(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let mut result = generate_platformer(req, physics);
    let ts = physics.tile_size;
    let width = result.tilemap.width as f32;

    // Puzzle proxy: trigger + guard around a choke and a required ability metadata key.
    result.entities.push(EntityPlacement {
        preset: "guard_enemy".to_string(),
        x: width * 0.55 * ts,
        y: ts + ts / 2.0 + 1.0,
        config: serde_json::json!({
            "speed": 110.0,
            "requires_ability": ["switch_a"],
            "tags": ["enemy", "puzzle_guard"],
        }),
    });
    result.entities.push(EntityPlacement {
        preset: "health_pickup".to_string(),
        x: width * 0.35 * ts,
        y: 4.5 * ts,
        config: serde_json::json!({
            "grants_ability": ["switch_a"],
            "tags": ["pickup", "switch_key"],
        }),
    });

    let tilemap = Tilemap {
        width: result.tilemap.width,
        height: result.tilemap.height,
        tiles: result.tilemap.tiles.clone(),
        player_spawn: result.player_spawn,
        goal: Some(result.goal),
        ..Default::default()
    };
    result.validation = run_validation(&tilemap, physics, &req.constraints, &result.entities);
    result
}

fn generate_side_scroller(req: &GenerateRequest, physics: &GameConfig) -> GenerateResult {
    let mut result = generate_platformer(req, physics);
    let ts = physics.tile_size;
    let width = result.tilemap.width as f32;
    let count = (2.0 + req.difficulty * 5.0).round() as usize;

    for i in 0..count {
        let frac = (i + 1) as f32 / (count + 1) as f32;
        let x = (4.0 + frac * (width - 8.0)) * ts;
        let preset = if i % 2 == 0 {
            "patrol_enemy"
        } else {
            "flying_enemy"
        };
        result.entities.push(EntityPlacement {
            preset: preset.to_string(),
            x,
            y: ts
                + ts / 2.0
                + 1.0
                + if preset == "flying_enemy" {
                    ts * 2.0
                } else {
                    0.0
                },
            config: serde_json::json!({
                "speed": 120.0 + req.difficulty * 40.0,
                "tags": ["enemy", "side_scroller"],
            }),
        });
    }

    let tilemap = Tilemap {
        width: result.tilemap.width,
        height: result.tilemap.height,
        tiles: result.tilemap.tiles.clone(),
        player_spawn: result.player_spawn,
        goal: Some(result.goal),
        ..Default::default()
    };
    result.validation = run_validation(&tilemap, physics, &req.constraints, &result.entities);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::{run_simulation, SimInput, SimulationRequest};
    use crate::tilemap::Tilemap;
    use bevy::prelude::Vec2;

    fn req(template: &str, difficulty: f32) -> GenerateRequest {
        GenerateRequest {
            width: None,
            height: None,
            difficulty,
            seed: 42,
            constraints: vec![],
            feel_target: None,
            template: Some(template.to_string()),
        }
    }

    #[test]
    fn arena_waves_contains_wave_enemy_presets() {
        let out = generate(&req("arena_waves", 0.6), &GameConfig::default());
        assert!(out.entities.iter().any(|e| e.preset == "chase_enemy"));
        assert!(out.entities.iter().any(|e| e.preset == "flying_enemy"));
    }

    #[test]
    fn tower_defense_contains_turrets() {
        let out = generate(&req("tower_defense_map", 0.5), &GameConfig::default());
        assert!(out.entities.iter().any(|e| e.preset == "turret"));
    }

    #[test]
    fn boss_arena_contains_boss_entity() {
        let out = generate(&req("boss_arena", 0.8), &GameConfig::default());
        assert!(out.entities.iter().any(|e| e.preset == "boss"));
    }

    #[test]
    fn top_down_dungeon_spawn_is_tile_centered() {
        let cfg = GameConfig::default();
        let out = generate(&req("top_down_dungeon", 0.4), &cfg);
        let ts = cfg.tile_size;
        let rem_x = out.player_spawn.0.rem_euclid(ts);
        let rem_y = out.player_spawn.1.rem_euclid(ts);
        assert!(
            (rem_x - ts * 0.5).abs() <= 0.001,
            "spawn.x={}",
            out.player_spawn.0
        );
        assert!(
            (rem_y - ts * 0.5).abs() <= 0.001,
            "spawn.y={}",
            out.player_spawn.1
        );
    }

    #[test]
    fn rts_arena_spawn_is_tile_centered() {
        let cfg = GameConfig::default();
        let out = generate(&req("rts_arena", 0.4), &cfg);
        let ts = cfg.tile_size;
        let rem_x = out.player_spawn.0.rem_euclid(ts);
        let rem_y = out.player_spawn.1.rem_euclid(ts);
        assert!(
            (rem_x - ts * 0.5).abs() <= 0.001,
            "spawn.x={}",
            out.player_spawn.0
        );
        assert!(
            (rem_y - ts * 0.5).abs() <= 0.001,
            "spawn.y={}",
            out.player_spawn.1
        );
    }

    fn build_inputs_from_path(start: (f32, f32), path: &[Vec2], move_speed: f32) -> Vec<SimInput> {
        let units_per_frame = (move_speed / 60.0).max(1.0);
        let mut frame = 0u32;
        let mut residual_x = 0.0f32;
        let mut residual_y = 0.0f32;
        let mut out = Vec::new();
        let mut x0 = start.0;
        let mut y0 = start.1;

        for point in path {
            let dx = point.x - x0;
            let dy = point.y - y0;
            for (axis, delta) in [("y", dy), ("x", dx)] {
                if delta.abs() <= 1.0 {
                    continue;
                }
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
                out.push(SimInput {
                    frame,
                    action: if axis == "x" {
                        if delta > 0.0 { "right" } else { "left" }.to_string()
                    } else if delta > 0.0 {
                        "up".to_string()
                    } else {
                        "down".to_string()
                    },
                    duration,
                });
                frame = frame.saturating_add(duration);
            }
            x0 = point.x;
            y0 = point.y;
        }

        out
    }

    #[test]
    fn top_down_dungeon_path_reaches_goal_in_simulation() {
        let cfg = GameConfig {
            gravity: Vec2::ZERO,
            jump_velocity: 0.0,
            move_speed: 200.0,
            ..GameConfig::default()
        };
        let mut request = req("top_down_dungeon", 0.3);
        request.seed = 777;
        request.width = Some(30);
        request.height = Some(25);
        let out = generate(&request, &cfg);
        let tilemap = Tilemap {
            width: out.tilemap.width,
            height: out.tilemap.height,
            tiles: out.tilemap.tiles.clone(),
            player_spawn: out.player_spawn,
            goal: Some(out.goal),
            ..Default::default()
        };
        let from = Vec2::new(out.player_spawn.0, out.player_spawn.1);
        let to = Vec2::new(
            (out.goal.0 as f32 + 0.5) * cfg.tile_size,
            (out.goal.1 as f32 + 0.5) * cfg.tile_size,
        );
        let path = crate::ai::find_top_down_path_points(&tilemap, cfg.tile_size, from, to)
            .expect("expected top-down path to exist");
        let inputs = build_inputs_from_path(out.player_spawn, &path, cfg.move_speed);
        let max_frames = inputs
            .iter()
            .map(|i| i.frame.saturating_add(i.duration))
            .max()
            .unwrap_or(0)
            .saturating_add(180);
        let req = SimulationRequest {
            tilemap: None,
            inputs,
            max_frames,
            record_interval: 10,
            physics: None,
            goal_position: None,
            goal_radius: None,
            initial_game_state: None,
            state_transitions: Vec::new(),
            moving_platforms: Vec::new(),
            entities: Vec::new(),
        };
        let sim = run_simulation(&tilemap, &cfg, &req);
        assert_eq!(sim.outcome, "goal_reached");
    }

    #[test]
    fn rts_arena_path_reaches_goal_in_simulation() {
        let cfg = GameConfig {
            gravity: Vec2::ZERO,
            jump_velocity: 0.0,
            move_speed: 150.0,
            ..GameConfig::default()
        };
        let mut request = req("rts_arena", 0.4);
        request.seed = 1337;
        request.width = Some(50);
        request.height = Some(35);
        let out = generate(&request, &cfg);
        let tilemap = Tilemap {
            width: out.tilemap.width,
            height: out.tilemap.height,
            tiles: out.tilemap.tiles.clone(),
            player_spawn: out.player_spawn,
            goal: Some(out.goal),
            ..Default::default()
        };
        let from = Vec2::new(out.player_spawn.0, out.player_spawn.1);
        let to = Vec2::new(
            (out.goal.0 as f32 + 0.5) * cfg.tile_size,
            (out.goal.1 as f32 + 0.5) * cfg.tile_size,
        );
        let path = crate::ai::find_top_down_path_points(&tilemap, cfg.tile_size, from, to)
            .expect("expected top-down path to exist");
        let inputs = build_inputs_from_path(out.player_spawn, &path, cfg.move_speed);
        let max_frames = inputs
            .iter()
            .map(|i| i.frame.saturating_add(i.duration))
            .max()
            .unwrap_or(0)
            .saturating_add(240);
        let req = SimulationRequest {
            tilemap: None,
            inputs,
            max_frames,
            record_interval: 10,
            physics: None,
            goal_position: None,
            goal_radius: None,
            initial_game_state: None,
            state_transitions: Vec::new(),
            moving_platforms: Vec::new(),
            entities: Vec::new(),
        };
        let sim = run_simulation(&tilemap, &cfg, &req);
        assert_eq!(sim.outcome, "goal_reached");
    }
}
