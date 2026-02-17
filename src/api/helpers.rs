use super::*;

pub(super) fn resolve_simulation_context(
    base_tilemap: &Tilemap,
    base_config: &GameConfig,
    req: &SimulationRequest,
) -> (Tilemap, GameConfig) {
    let tilemap = if let Some(ref tm) = req.tilemap {
        Tilemap {
            width: tm.width,
            height: tm.height,
            tiles: tm.tiles.clone(),
            player_spawn: tm.player_spawn.unwrap_or(base_tilemap.player_spawn),
            goal: tm.goal,
            ..Default::default()
        }
    } else {
        base_tilemap.clone()
    };

    let mut physics = base_config.clone();
    if let Some(ref p) = req.physics {
        if let Some(v) = p.gravity {
            physics.gravity = Vec2::new(0.0, -v);
        }
        if let Some(v) = p.jump_velocity {
            physics.jump_velocity = v;
        }
        if let Some(v) = p.move_speed {
            physics.move_speed = v;
        }
        if let Some(v) = p.fall_multiplier {
            physics.fall_multiplier = v;
        }
        if let Some(v) = p.coyote_frames {
            physics.coyote_frames = v;
        }
        if let Some(v) = p.jump_buffer_frames {
            physics.jump_buffer_frames = v;
        }
    }

    (tilemap, physics)
}

pub(super) fn run_simulation_from_recorded_state(
    base_tilemap: &Tilemap,
    base_config: &GameConfig,
    req: &SimulationRequest,
) -> SimulationResult {
    let (tilemap, physics) = resolve_simulation_context(base_tilemap, base_config, req);
    simulation::run_simulation(&tilemap, &physics, req)
}

pub(super) fn save_dir() -> std::path::PathBuf {
    std::env::var("AXIOM_SAVE_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::PathBuf::from("saves"))
}

pub(super) fn replay_dir() -> std::path::PathBuf {
    std::env::var("AXIOM_REPLAY_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::PathBuf::from("replays"))
}

pub(super) fn export_dir() -> std::path::PathBuf {
    std::env::var("AXIOM_EXPORT_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::PathBuf::from("export"))
}

pub(super) fn screenshot_path() -> std::path::PathBuf {
    std::env::var("AXIOM_SCREENSHOT_PATH")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::PathBuf::from("screenshot.png"))
}

pub(super) fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(super) fn sanitize_replay_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        format!("replay_{}", unix_ms_now())
    } else {
        cleaned
    }
}

pub(super) fn sanitize_slot(slot: &str) -> String {
    let cleaned: String = slot
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        "save".to_string()
    } else {
        cleaned
    }
}

pub(super) fn sanitize_name(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}

#[derive(Clone)]
pub(super) struct ExampleRecipe {
    pub name: &'static str,
    pub description: &'static str,
    pub genre: &'static str,
    pub template: &'static str,
    pub difficulty: f32,
    pub seed: u64,
    pub constraints: Vec<&'static str>,
    pub config_overrides: serde_json::Value,
    pub extra_entities: Vec<crate::generation::EntityPlacement>,
    pub extra_scripts: Vec<crate::generation::ScriptAssignment>,
}

pub(super) fn ai_behavior_kind_name(ai: &AiBehavior) -> String {
    match ai.behavior {
        BehaviorType::Patrol { .. } => "patrol",
        BehaviorType::Chase { .. } => "chase",
        BehaviorType::Flee { .. } => "flee",
        BehaviorType::Guard { .. } => "guard",
        BehaviorType::Wander { .. } => "wander",
        BehaviorType::Custom(..) => "custom",
    }
    .to_string()
}

pub(super) fn ai_state_name(ai: &AiBehavior) -> String {
    match ai.state {
        AiState::Idle => "idle",
        AiState::Patrolling { .. } => "patrolling",
        AiState::Chasing { .. } => "chasing",
        AiState::Fleeing { .. } => "fleeing",
        AiState::Attacking { .. } => "attacking",
        AiState::Returning => "returning",
        AiState::Wandering { .. } => "wandering",
    }
    .to_string()
}

pub(super) fn ai_state_target_id(ai: &AiBehavior) -> Option<u64> {
    match ai.state {
        AiState::Chasing { target_id }
        | AiState::Fleeing {
            threat_id: target_id,
        }
        | AiState::Attacking { target_id } => Some(target_id),
        _ => None,
    }
}

pub(super) fn builtin_examples() -> Vec<ExampleRecipe> {
    vec![
        ExampleRecipe {
            name: "platformer_campaign",
            description: "Mario-like starter with patrol enemies, pickups, and a boss gate.",
            genre: "platformer",
            template: "side_scroller",
            difficulty: 0.30,
            seed: 1001,
            constraints: vec!["reachable", "enemy_fairness", "item_reachability"],
            config_overrides: serde_json::json!({ "move_speed": 220.0 }),
            extra_entities: vec![
                crate::generation::EntityPlacement {
                    preset: "boss".to_string(),
                    x: 118.0 * 16.0,
                    y: 1.5 * 16.0,
                    config: serde_json::json!({
                        "health": 18.0,
                        "speed": 95.0,
                    }),
                },
                crate::generation::EntityPlacement {
                    preset: "health_pickup".to_string(),
                    x: 78.0 * 16.0,
                    y: 5.0 * 16.0,
                    config: serde_json::json!({}),
                },
            ],
            extra_scripts: vec![],
        },
        ExampleRecipe {
            name: "top_down_rpg_dungeon",
            description: "Zelda-like dungeon recipe with chasers, guards, and key-like pickups.",
            genre: "top_down_rpg",
            template: "top_down_dungeon",
            difficulty: 0.4,
            seed: 1,
            constraints: vec![
                "top_down_reachable",
                "enemy_fairness",
                "item_reachability",
                "no_dead_ends:0.45",
            ],
            config_overrides: serde_json::json!({
                "gravity": 0.0,
                "jump_velocity": 0.0,
                "move_speed": 185.0
            }),
            extra_entities: vec![
                crate::generation::EntityPlacement {
                    preset: "guard_enemy".to_string(),
                    x: 22.0 * 16.0,
                    y: 18.0 * 16.0,
                    config: serde_json::json!({
                        "speed": 120.0,
                        "tags": ["enemy", "mini_boss"],
                    }),
                },
                crate::generation::EntityPlacement {
                    preset: "health_pickup".to_string(),
                    x: 12.0 * 16.0,
                    y: 11.0 * 16.0,
                    config: serde_json::json!({
                        "tags": ["pickup", "key_a"],
                        "grants_ability": ["key_a"],
                    }),
                },
            ],
            extra_scripts: vec![],
        },
        ExampleRecipe {
            name: "roguelike_floor_run",
            description: "Procedural floor with escalating density and recovery pickups.",
            genre: "roguelike",
            template: "roguelike_floor",
            difficulty: 0.65,
            seed: 2,
            constraints: vec!["top_down_reachable", "enemy_fairness", "pacing"],
            config_overrides: serde_json::json!({
                "gravity": 0.0,
                "jump_velocity": 0.0,
                "move_speed": 190.0
            }),
            extra_entities: vec![],
            extra_scripts: vec![crate::generation::ScriptAssignment {
                name: "roguelike_score_tick".to_string(),
                source: r#"
function update(world, dt)
    local score = world.get_var("score") or 0
    world.set_var("score", score + 1)
end
"#
                .trim()
                .to_string(),
                global: true,
            }],
        },
        ExampleRecipe {
            name: "bullet_hell_arena",
            description: "Arena-shooter style recipe with flying swarms and fixed turrets.",
            genre: "bullet_hell",
            template: "arena_waves",
            difficulty: 0.75,
            seed: 1337,
            constraints: vec!["top_down_reachable", "enemy_fairness"],
            config_overrides: serde_json::json!({
                "gravity": 0.0,
                "jump_velocity": 0.0,
                "move_speed": 245.0
            }),
            extra_entities: vec![
                crate::generation::EntityPlacement {
                    preset: "turret".to_string(),
                    x: 30.0 * 16.0,
                    y: 7.0 * 16.0,
                    config: serde_json::json!({
                        "detection_radius": 340.0,
                        "health": 5.0,
                    }),
                },
                crate::generation::EntityPlacement {
                    preset: "flying_enemy".to_string(),
                    x: 42.0 * 16.0,
                    y: 12.0 * 16.0,
                    config: serde_json::json!({
                        "speed": 210.0,
                        "health": 2.0,
                    }),
                },
            ],
            extra_scripts: vec![],
        },
        ExampleRecipe {
            name: "puzzle_platformer_trials",
            description: "Puzzle platformer recipe with simple ability-gating metadata.",
            genre: "puzzle_platformer",
            template: "puzzle_platformer",
            difficulty: 0.2,
            seed: 6060,
            constraints: vec![
                "reachable",
                "item_reachability",
                "ability_gating",
                "difficulty_range:0.25:0.95",
            ],
            config_overrides: serde_json::json!({ "move_speed": 220.0 }),
            extra_entities: vec![],
            extra_scripts: vec![],
        },
    ]
}

pub(super) fn apply_config_overrides(
    config: &mut GameConfig,
    overrides: &serde_json::Value,
) -> Result<(), String> {
    if overrides.is_null() {
        return Ok(());
    }
    let Some(obj) = overrides.as_object() else {
        return Err("config_overrides must be an object".into());
    };
    for (key, value) in obj {
        match key.as_str() {
            "gravity" => {
                if let Some(g) = value.as_f64() {
                    config.gravity = Vec2::new(0.0, -(g as f32));
                } else if let Some(arr) = value.as_array() {
                    if arr.len() != 2 {
                        return Err("gravity array must have exactly 2 elements".into());
                    }
                    let gx = arr[0].as_f64().ok_or("gravity[0] must be a number")? as f32;
                    let gy = arr[1].as_f64().ok_or("gravity[1] must be a number")? as f32;
                    config.gravity = Vec2::new(gx, gy);
                } else if let Some(gravity_obj) = value.as_object() {
                    let gx = gravity_obj
                        .get("x")
                        .and_then(|v| v.as_f64())
                        .ok_or("gravity.x must be a number")? as f32;
                    let gy = gravity_obj
                        .get("y")
                        .and_then(|v| v.as_f64())
                        .ok_or("gravity.y must be a number")? as f32;
                    config.gravity = Vec2::new(gx, gy);
                } else {
                    return Err("gravity must be a number, [x,y], or {x,y}".into());
                }
            }
            "tile_size" => {
                config.tile_size = value.as_f64().ok_or("tile_size must be a number")? as f32;
            }
            "move_speed" => {
                config.move_speed = value.as_f64().ok_or("move_speed must be a number")? as f32;
            }
            "jump_velocity" => {
                config.jump_velocity =
                    value.as_f64().ok_or("jump_velocity must be a number")? as f32;
            }
            "fall_multiplier" => {
                config.fall_multiplier =
                    value.as_f64().ok_or("fall_multiplier must be a number")? as f32;
            }
            "coyote_frames" => {
                config.coyote_frames = value
                    .as_u64()
                    .ok_or("coyote_frames must be an unsigned integer")?
                    as u32;
            }
            "jump_buffer_frames" => {
                config.jump_buffer_frames = value
                    .as_u64()
                    .ok_or("jump_buffer_frames must be an unsigned integer")?
                    as u32;
            }
            "tile_types" => {
                config.tile_types = serde_json::from_value(value.clone())
                    .map_err(|e| format!("Invalid tile_types override: {e}"))?;
            }
            other => return Err(format!("Unsupported config override key: {other}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_slot_filters_invalid_chars() {
        assert_eq!(sanitize_slot("../save:one"), "saveone");
        assert_eq!(sanitize_slot(""), "save");
        assert_eq!(sanitize_slot("slot_01-ok"), "slot_01-ok");
    }

    #[test]
    fn sanitize_replay_name_filters_invalid_chars() {
        assert_eq!(sanitize_replay_name("wave#1 replay"), "wave1replay");
    }

    #[test]
    fn sanitize_name_uses_fallback() {
        assert_eq!(sanitize_name("my game!", "game"), "mygame");
        assert_eq!(sanitize_name("???", "game"), "game");
    }

    #[test]
    fn builtin_examples_have_unique_names() {
        let examples = builtin_examples();
        let mut names = std::collections::HashSet::new();
        for ex in examples {
            assert!(!ex.name.trim().is_empty());
            assert!(names.insert(ex.name), "duplicate example name: {}", ex.name);
            assert!(!ex.template.trim().is_empty());
        }
    }

    #[test]
    fn builtin_examples_cover_phase13_targets() {
        let examples = builtin_examples();
        assert!(
            examples.len() >= 5,
            "expected at least 5 builtin examples, got {}",
            examples.len()
        );
        let names: std::collections::HashSet<&str> = examples.iter().map(|e| e.name).collect();
        for required in [
            "platformer_campaign",
            "top_down_rpg_dungeon",
            "roguelike_floor_run",
            "bullet_hell_arena",
            "puzzle_platformer_trials",
        ] {
            assert!(
                names.contains(required),
                "missing builtin example: {required}"
            );
        }
    }

    #[test]
    fn apply_config_overrides_accepts_gravity_vec() {
        let mut cfg = GameConfig::default();
        let overrides = serde_json::json!({
            "gravity": [3.0, -400.0],
            "move_speed": 280.0,
            "tile_size": 20.0
        });
        apply_config_overrides(&mut cfg, &overrides).expect("overrides should apply");
        assert_eq!(cfg.gravity, Vec2::new(3.0, -400.0));
        assert_eq!(cfg.move_speed, 280.0);
        assert_eq!(cfg.tile_size, 20.0);
    }

    #[test]
    fn apply_config_overrides_rejects_unknown_key() {
        let mut cfg = GameConfig::default();
        let err = apply_config_overrides(&mut cfg, &serde_json::json!({ "unknown": 1 }))
            .expect_err("unknown keys should fail");
        assert!(err.contains("Unsupported config override key"));
    }
}
