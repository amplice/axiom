mod command_runtime;
mod command_runtime_helpers;
mod commands;
mod docs;
mod helpers;
mod persistence;
mod router;
mod routes_campaign;
mod routes_core;
mod routes_entities;
mod routes_exports;
mod routes_gameplay;
mod routes_generation;
mod routes_misc;
mod security;
mod state;
pub mod types;

use axum::{
    extract::Request,
    extract::State,
    http::StatusCode,
    middleware::{self, Next},
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        Html, IntoResponse,
    },
    routing::{delete, get, post},
    Json, Router,
};
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, RwLock};

use crate::components::*;
use crate::debug::DebugOverlayConfig;
use crate::events::{GameEvent, GameEventBus};
use crate::perf::PerfStats;
use crate::scripting::api::{ScriptSource, ScriptTestRequest, ScriptUpsertRequest};
use crate::scripting::{LuaScript, ScriptEngine, ScriptError, ScriptErrors, ScriptEvent};
use crate::simulation::{self, SimulationRequest, SimulationResult};
use crate::sprites::SpriteAssets;
use crate::tilemap::{TileEntity, Tilemap};
use command_runtime::*;
use commands::*;
use helpers::*;
pub(crate) use persistence::{ProjectExportData, SaveAiState, SaveEntity, SaveGameData};
use router::build_router;
use routes_campaign::*;
use routes_core::*;
use routes_entities::*;
use routes_exports::*;
use routes_gameplay::*;
use routes_generation::*;
use routes_misc::*;
use security::*;
use state::*;
use types::*;

pub struct ApiPlugin;

impl Plugin for ApiPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = crossbeam_channel::unbounded::<ApiCommand>();

        let snapshot = Arc::new(RwLock::new(SnapshotData {
            tilemap: Tilemap::test_level(),
            physics: GameConfig::default(),
        }));
        let level_packs = Arc::new(RwLock::new(LevelPackStore::default()));
        let replay_store = Arc::new(RwLock::new(ReplayStore::default()));
        let game_runtime = Arc::new(RwLock::new(GameRuntimeStore {
            state: "Playing".to_string(),
            entered_at: std::time::Instant::now(),
            last_loaded: None,
            transitions: Vec::new(),
            active_transition: None,
        }));

        app.insert_resource(ApiChannels { receiver: rx })
            .init_resource::<crate::sprites::SpriteSheetRegistry>()
            .insert_resource(PendingLevelChange::default())
            .insert_resource(PendingPhysicsChange::default())
            .insert_resource(PendingScreenshot::default())
            .insert_resource(crate::simulation::PendingRealSim::default())
            .insert_resource(crate::simulation::PendingPlaytest::default())
            .insert_resource(SharedSnapshot {
                data: snapshot.clone(),
            })
            .insert_resource(RuntimeStoreHandle(game_runtime.clone()))
            .add_systems(
                Update,
                (
                    update_snapshot,
                    process_api_commands,
                    sync_runtime_store_from_ecs,
                    apply_level_change,
                    apply_physics_change,
                    take_screenshot,
                )
                    .chain(),
            )
            .add_systems(
                FixedFirst,
                (
                    crate::simulation::tick_real_sim,
                    crate::simulation::tick_playtest_agent,
                ),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    crate::simulation::finalize_real_sim,
                    crate::simulation::finalize_playtest,
                ),
            );

        let state = AppState {
            sender: tx,
            snapshot,
            level_packs,
            game_runtime,
            replay_store,
        };
        let security = ApiSecurity::from_env();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let app = build_router(state, security);

                let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
                    .await
                    .expect("Failed to bind to port 3000");

                println!("[Axiom API] Listening on http://127.0.0.1:3000");

                axum::serve(listener, app).await.unwrap();
            });
        });
    }
}

/// Keep the shared snapshot in sync with current game state
fn update_snapshot(tilemap: Res<Tilemap>, physics: Res<GameConfig>, shared: Res<SharedSnapshot>) {
    if tilemap.is_changed() || physics.is_changed() {
        if let Ok(mut snap) = shared.data.try_write() {
            snap.tilemap = tilemap.clone();
            snap.physics = physics.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_game_data_roundtrip_json() {
        let save = SaveGameData {
            version: 4,
            config: GameConfig::default(),
            tilemap: Tilemap::test_level(),
            game_state: "Paused".to_string(),
            next_network_id: 2,
            entities: vec![SaveEntity {
                network_id: Some(1),
                x: 10.0,
                y: 20.0,
                vx: 1.5,
                vy: -3.0,
                is_player: true,
                components: vec![ComponentDef::Collider {
                    width: 12.0,
                    height: 14.0,
                }],
                script: None,
                script_state: Some(serde_json::json!({"patrol_timer": 42})),
                tags: vec!["player".to_string()],
                alive: true,
                ai_state: Some(SaveAiState::Patrolling { waypoint_index: 2 }),
                invincibility_frames: Some(8),
                path_follower_path: vec![(10.0, 20.0)],
                path_follower_frames_until_recalc: Some(3),
                inventory_slots: vec![],
            }],
            scripts: std::collections::HashMap::new(),
            global_scripts: vec![],
            game_vars: std::collections::HashMap::new(),
            animation_graphs: std::collections::HashMap::new(),
            sprite_sheets: std::collections::HashMap::new(),
            particle_presets: std::collections::HashMap::new(),
        };
        let json = serde_json::to_string(&save).expect("serialize");
        let parsed: SaveGameData = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.version, 4);
        assert_eq!(parsed.game_state, "Paused");
        assert_eq!(parsed.next_network_id, 2);
        assert_eq!(parsed.entities.len(), 1);
        assert!(parsed.entities[0].is_player);
        assert_eq!(parsed.entities[0].network_id, Some(1));
        assert!((parsed.entities[0].vx - 1.5).abs() < 0.0001);
        assert!((parsed.entities[0].vy - (-3.0)).abs() < 0.0001);
        assert!(parsed.entities[0].script_state.is_some());
        assert!(matches!(
            parsed.entities[0].ai_state,
            Some(SaveAiState::Patrolling { waypoint_index: 2 })
        ));
        assert_eq!(parsed.entities[0].invincibility_frames, Some(8));
        assert_eq!(parsed.entities[0].path_follower_path.len(), 1);
        assert_eq!(parsed.entities[0].path_follower_frames_until_recalc, Some(3));
    }

    #[test]
    fn save_game_data_legacy_payload_defaults_new_fields() {
        let legacy = serde_json::json!({
            "version": 1,
            "config": GameConfig::default(),
            "tilemap": Tilemap::test_level(),
            "entities": [{
                "network_id": 1,
                "x": 10.0,
                "y": 20.0,
                "is_player": false,
                "components": [],
                "script": null,
                "tags": [],
                "alive": true
            }],
            "scripts": {},
            "global_scripts": [],
            "game_vars": {}
        });

        let parsed: SaveGameData = serde_json::from_value(legacy).expect("deserialize legacy save");
        assert_eq!(parsed.game_state, "Playing");
        assert_eq!(parsed.next_network_id, 1);
        assert_eq!(parsed.entities.len(), 1);
        assert_eq!(parsed.entities[0].vx, 0.0);
        assert_eq!(parsed.entities[0].vy, 0.0);
        assert!(parsed.entities[0].script_state.is_none());
        assert!(parsed.entities[0].ai_state.is_none());
    }

    #[test]
    fn builtin_examples_generate_sane_levels() {
        for example in builtin_examples() {
            let mut cfg = GameConfig::default();
            apply_config_overrides(&mut cfg, &example.config_overrides)
                .expect("example config overrides should be valid");
            let generated = crate::generation::generate(
                &crate::generation::GenerateRequest {
                    width: None,
                    height: None,
                    difficulty: example.difficulty.clamp(0.0, 1.0),
                    seed: example.seed,
                    constraints: example.constraints.iter().map(|s| s.to_string()).collect(),
                    feel_target: None,
                    template: Some(example.template.to_string()),
                },
                &cfg,
            );

            assert!(generated.tilemap.width > 0, "{}", example.name);
            assert!(generated.tilemap.height > 0, "{}", example.name);
            assert_eq!(
                generated.tilemap.tiles.len(),
                generated.tilemap.width * generated.tilemap.height,
                "{}",
                example.name
            );
            assert!(generated.goal.0 >= 0, "{}", example.name);
            assert!(generated.goal.1 >= 0, "{}", example.name);
            assert!(
                (generated.goal.0 as usize) < generated.tilemap.width,
                "{}",
                example.name
            );
            assert!(
                (generated.goal.1 as usize) < generated.tilemap.height,
                "{}",
                example.name
            );
        }
    }

    #[test]
    fn default_platformer_examples_are_solver_completable() {
        for example in builtin_examples() {
            if !example
                .constraints
                .iter()
                .any(|c| *c == "reachable" || *c == "completable" || *c == "top_down_reachable")
            {
                continue;
            }
            let mut cfg = GameConfig::default();
            apply_config_overrides(&mut cfg, &example.config_overrides)
                .expect("example config overrides should be valid");
            let generated = crate::generation::generate(
                &crate::generation::GenerateRequest {
                    width: None,
                    height: None,
                    difficulty: example.difficulty.clamp(0.0, 1.0),
                    seed: example.seed,
                    constraints: example.constraints.iter().map(|s| s.to_string()).collect(),
                    feel_target: None,
                    template: Some(example.template.to_string()),
                },
                &cfg,
            );
            let tilemap = Tilemap {
                width: generated.tilemap.width,
                height: generated.tilemap.height,
                tiles: generated.tilemap.tiles.clone(),
                player_spawn: generated.player_spawn,
                goal: Some(generated.goal),
                ..Default::default()
            };
            let solved = crate::pathfinding::solve(&tilemap, &cfg);
            assert!(
                solved.solved,
                "expected {} to be solver-completable at default settings",
                example.name
            );
        }
    }
}
