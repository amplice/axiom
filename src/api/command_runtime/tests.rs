use super::*;
use std::collections::HashMap;

fn setup_runtime_app(receiver: Receiver<ApiCommand>) -> App {
    let mut app = App::new();
    app.insert_resource(ApiChannels { receiver })
        .insert_resource(Tilemap::test_level())
        .insert_resource(GameConfig::default())
        .insert_resource(NextNetworkId::default())
        .insert_resource(PendingLevelChange::default())
        .insert_resource(PendingPhysicsChange::default())
        .insert_resource(PendingScreenshot::default())
        .insert_resource(GameEventBus::default())
        .insert_resource(PerfStats::default())
        .insert_resource(DebugOverlayConfig::default())
        .insert_resource(ScriptEngine::default())
        .insert_resource(ScriptErrors::default())
        .insert_resource(crate::game_runtime::RuntimeState::default())
        .insert_resource(crate::animation::AnimationLibrary::default())
        .insert_resource(crate::sprites::SpriteSheetRegistry::default())
        .insert_resource(crate::particles::ParticlePresetLibrary::default())
        .insert_resource(crate::spawn::PresetRegistry::default())
        .insert_resource(crate::spawn::EntityPool::default())
        .insert_resource(crate::simulation::PendingRealSim::default())
        .add_systems(Update, process_api_commands);
    app
}

#[test]
fn load_script_rejects_invalid_or_missing_update() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (bad_tx, bad_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::LoadScript(
            ScriptUpsertRequest {
                name: "broken".to_string(),
                source: "function update(".to_string(),
                global: false,
                always_run: None,
            },
            bad_tx,
        ))
        .expect("send bad script");
    app.update();
    let bad_result = bad_rx.blocking_recv().expect("bad script response");
    assert!(bad_result.is_err());

    let (missing_update_tx, missing_update_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::LoadScript(
            ScriptUpsertRequest {
                name: "missing_update".to_string(),
                source: "local x = 1".to_string(),
                global: false,
                always_run: None,
            },
            missing_update_tx,
        ))
        .expect("send missing update script");
    app.update();
    let missing_update_result = missing_update_rx
        .blocking_recv()
        .expect("missing update response");
    assert!(missing_update_result.is_err());
    assert!(missing_update_result
        .expect_err("expected missing update error")
        .contains("update"));

    let (errs_tx, errs_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetScriptErrors(errs_tx))
        .expect("send get script errors");
    app.update();
    let errors = errs_rx.blocking_recv().expect("script errors response");
    assert!(errors
        .iter()
        .any(|err| err.script_name == "missing_update" && err.entity_id.is_none()));

    let (ok_tx, ok_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::LoadScript(
            ScriptUpsertRequest {
                name: "ok".to_string(),
                source: "function update(entity, world, dt) return end".to_string(),
                global: false,
                always_run: None,
            },
            ok_tx,
        ))
        .expect("send valid script");
    app.update();
    assert!(ok_rx
        .blocking_recv()
        .expect("valid script response")
        .is_ok());
}

#[test]
fn script_stats_include_runtime_budget_limits() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (stats_tx, stats_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetScriptStats(stats_tx))
        .expect("send get script stats");
    app.update();
    let stats = stats_rx.blocking_recv().expect("stats response");

    let env_u64 = |name: &str, default: u64| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(default)
    };
    let env_usize = |name: &str, default: usize| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(default)
    };

    assert_eq!(
        stats.entity_budget_ms,
        env_u64(
            "AXIOM_SCRIPT_ENTITY_BUDGET_MS",
            crate::scripting::DEFAULT_ENTITY_SCRIPT_BUDGET_MS
        )
    );
    assert_eq!(
        stats.global_budget_ms,
        env_u64(
            "AXIOM_SCRIPT_GLOBAL_BUDGET_MS",
            crate::scripting::DEFAULT_GLOBAL_SCRIPT_BUDGET_MS
        )
    );
    assert_eq!(
        stats.hook_instruction_interval,
        env_u64(
            "AXIOM_SCRIPT_HOOK_INSTRUCTION_INTERVAL",
            crate::scripting::DEFAULT_SCRIPT_HOOK_INSTRUCTION_INTERVAL as u64
        ) as u32
    );
    assert_eq!(
        stats.rhai_max_operations,
        env_u64(
            "AXIOM_RHAI_MAX_OPERATIONS",
            crate::scripting::DEFAULT_RHAI_MAX_OPERATIONS
        )
    );
    assert_eq!(
        stats.rhai_max_call_levels,
        env_usize(
            "AXIOM_RHAI_MAX_CALL_LEVELS",
            crate::scripting::DEFAULT_RHAI_MAX_CALL_LEVELS
        )
    );
}

#[test]
fn get_perf_history_returns_ring_buffer_metadata() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (tx, rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetPerfHistory(tx))
        .expect("send get perf history");
    app.update();
    let history = rx.blocking_recv().expect("perf history response");
    assert!(history.capacity >= 60);
    assert!(history.samples.is_empty());
}

#[test]
fn entity_commands_use_stable_network_ids() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (spawn_tx, spawn_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SpawnEntity(
            EntitySpawnRequest {
                x: 32.0,
                y: 48.0,
                components: vec![],
                script: None,
                tags: vec!["enemy".to_string()],
                is_player: false,
                invisible: false,
            },
            spawn_tx,
        ))
        .expect("send spawn");
    app.update();
    let spawned_id = spawn_rx
        .blocking_recv()
        .expect("spawn response")
        .expect("spawn ok");
    assert_eq!(spawned_id, 1);

    let (list_tx, list_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::ListEntities(list_tx))
        .expect("send list");
    app.update();
    let entities = list_rx.blocking_recv().expect("list response");
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].id, spawned_id);
    assert_eq!(entities[0].network_id, Some(spawned_id));

    let (get_tx, get_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetEntity(spawned_id, get_tx))
        .expect("send get");
    app.update();
    let entity = get_rx
        .blocking_recv()
        .expect("get response")
        .expect("entity exists");
    assert_eq!(entity.id, spawned_id);
    assert_eq!(entity.network_id, Some(spawned_id));

    let (del_tx, del_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::DeleteEntity(spawned_id, del_tx))
        .expect("send delete");
    app.update();
    assert!(del_rx.blocking_recv().expect("delete response").is_ok());

    let (get2_tx, get2_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetEntity(spawned_id, get2_tx))
        .expect("send get after delete");
    app.update();
    assert!(get2_rx.blocking_recv().expect("get2 response").is_none());
}

#[test]
fn reset_non_player_entities_keeps_players_only() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (spawn_player_tx, spawn_player_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SpawnEntity(
            EntitySpawnRequest {
                x: 16.0,
                y: 24.0,
                components: vec![],
                script: None,
                tags: vec![],
                is_player: true,
                invisible: false,
            },
            spawn_player_tx,
        ))
        .expect("send spawn player");

    let (spawn_enemy_tx, spawn_enemy_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SpawnEntity(
            EntitySpawnRequest {
                x: 48.0,
                y: 24.0,
                components: vec![],
                script: None,
                tags: vec!["enemy".to_string()],
                is_player: false,
                invisible: false,
            },
            spawn_enemy_tx,
        ))
        .expect("send spawn enemy");

    app.update();
    let player_id = spawn_player_rx
        .blocking_recv()
        .expect("spawn player response")
        .expect("spawn player ok");
    let enemy_id = spawn_enemy_rx
        .blocking_recv()
        .expect("spawn enemy response")
        .expect("spawn enemy ok");
    assert_ne!(player_id, enemy_id);

    let (reset_tx, reset_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::ResetNonPlayerEntities(reset_tx))
        .expect("send reset non-player");
    app.update();
    assert!(reset_rx.blocking_recv().expect("reset response").is_ok());

    let (list_tx, list_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::ListEntities(list_tx))
        .expect("send list");
    app.update();
    let entities = list_rx.blocking_recv().expect("list response");
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].id, player_id);
    assert!(entities[0].components.iter().any(|c| c == "Player"));
}

#[test]
fn entity_animation_commands_set_and_get_state() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (set_graph_tx, set_graph_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SetAnimationGraph(
            "test_graph".to_string(),
            crate::animation::AnimationGraphDef {
                default_state: "idle".to_string(),
                states: HashMap::from([
                    (
                        "idle".to_string(),
                        crate::animation::AnimationClipDef {
                            frame_count: 1,
                            frames: Vec::new(),
                            fps: 8.0,
                            looping: true,
                            next: None,
                            events: Vec::new(),
                        },
                    ),
                    (
                        "run".to_string(),
                        crate::animation::AnimationClipDef {
                            frame_count: 4,
                            frames: Vec::new(),
                            fps: 12.0,
                            looping: true,
                            next: None,
                            events: Vec::new(),
                        },
                    ),
                ]),
            },
            set_graph_tx,
        ))
        .expect("send set graph");
    app.update();
    assert!(set_graph_rx
        .blocking_recv()
        .expect("set graph response")
        .is_ok());

    let (spawn_tx, spawn_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SpawnEntity(
            EntitySpawnRequest {
                x: 16.0,
                y: 16.0,
                components: vec![ComponentDef::AnimationController {
                    graph: "test_graph".to_string(),
                    state: "idle".to_string(),
                    frame: 0,
                    timer: 0.0,
                    speed: 1.0,
                    playing: true,
                    facing_right: true,
                    auto_from_velocity: false,
                    facing_direction: 5,
                }],
                script: None,
                tags: vec![],
                is_player: false,
                invisible: false,
            },
            spawn_tx,
        ))
        .expect("send spawn");
    app.update();
    let entity_id = spawn_rx
        .blocking_recv()
        .expect("spawn response")
        .expect("spawn ok");

    let (set_anim_tx, set_anim_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SetEntityAnimation(
            entity_id,
            "run".to_string(),
            set_anim_tx,
        ))
        .expect("send set entity animation");
    app.update();
    assert!(set_anim_rx
        .blocking_recv()
        .expect("set anim response")
        .is_ok());

    let (get_anim_tx, get_anim_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetEntityAnimation(entity_id, get_anim_tx))
        .expect("send get entity animation");
    app.update();
    let anim = get_anim_rx
        .blocking_recv()
        .expect("get anim response")
        .expect("animation exists");
    assert_eq!(anim.id, entity_id);
    assert_eq!(anim.state, "run");
    assert_eq!(anim.frame, 0);
    assert!(anim.playing);
}

#[test]
fn upsert_sprite_sheet_registers_sheet_and_animation_graph() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (upsert_tx, upsert_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::UpsertSpriteSheet(
            SpriteSheetUpsertRequest {
                name: "hero".to_string(),
                path: "assets/hero.png".to_string(),
                frame_width: 32,
                frame_height: 32,
                columns: 8,
                rows: 1,
                animations: HashMap::from([(
                    "run".to_string(),
                    SpriteSheetAnimationRequest {
                        path: None,
                        frames: vec![4, 5, 6, 7],
                        fps: 12.0,
                        looping: true,
                        next: None,
                        events: Vec::new(),
                    },
                )]),
                direction_map: None,
                anchor_y: -0.15,
            },
            upsert_tx,
        ))
        .expect("send upsert sheet");
    app.update();
    assert!(upsert_rx
        .blocking_recv()
        .expect("upsert sheet response")
        .is_ok());

    let (sheets_tx, sheets_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetSpriteSheets(sheets_tx))
        .expect("send get sheets");
    app.update();
    let sheets = sheets_rx.blocking_recv().expect("sheets response");
    let hero = sheets.get("hero").expect("hero sheet");
    assert_eq!(hero.path, "assets/hero.png");
    assert_eq!(hero.frame_width, 32);
    assert!(hero.animations.contains_key("run"));

    let (graph_tx, graph_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetAnimationGraph("hero".to_string(), graph_tx))
        .expect("send get graph");
    app.update();
    let graph = graph_rx
        .blocking_recv()
        .expect("graph response")
        .expect("graph exists");
    let run = graph.states.get("run").expect("run state");
    assert_eq!(run.frames, vec![4, 5, 6, 7]);
    assert_eq!(run.frame_count, 4);
}

#[test]
fn particle_commands_attach_emitter_component() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (set_presets_tx, set_presets_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SetParticlePresets(
            HashMap::from([(
                "dust".to_string(),
                crate::particles::ParticlePresetDef {
                    emit_rate: 18.0,
                    one_shot: false,
                    ..Default::default()
                },
            )]),
            set_presets_tx,
        ))
        .expect("send set presets");
    app.update();
    assert!(set_presets_rx
        .blocking_recv()
        .expect("set presets response")
        .is_ok());

    let (spawn_tx, spawn_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SpawnEntity(
            EntitySpawnRequest {
                x: 4.0,
                y: 5.0,
                components: vec![],
                script: None,
                tags: vec![],
                is_player: false,
                invisible: false,
            },
            spawn_tx,
        ))
        .expect("send spawn");
    app.update();
    let id = spawn_rx
        .blocking_recv()
        .expect("spawn response")
        .expect("spawn ok");

    let (set_particles_tx, set_particles_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SetEntityParticles(
            id,
            EntityParticlesRequest {
                preset: Some("dust".to_string()),
                emitter: None,
            },
            set_particles_tx,
        ))
        .expect("send set entity particles");
    app.update();
    assert!(set_particles_rx
        .blocking_recv()
        .expect("set particles response")
        .is_ok());

    let (get_tx, get_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetEntity(id, get_tx))
        .expect("send get");
    app.update();
    let entity = get_rx
        .blocking_recv()
        .expect("get response")
        .expect("entity exists");
    assert!(entity.components.iter().any(|c| c == "ParticleEmitter"));
}

#[test]
fn save_load_preserves_runtime_entity_state() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (spawn_tx, spawn_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SpawnEntity(
            EntitySpawnRequest {
                x: 8.0,
                y: 12.0,
                components: vec![ComponentDef::AiBehavior {
                    behavior: AiBehaviorDef::Chase {
                        target_tag: "player".to_string(),
                        speed: 120.0,
                        detection_radius: 260.0,
                        give_up_radius: 420.0,
                        require_line_of_sight: false,
                    },
                }],
                script: Some("enemy_logic".to_string()),
                tags: vec!["enemy".to_string()],
                is_player: false,
                invisible: false,
            },
            spawn_tx,
        ))
        .expect("send spawn");
    app.update();
    let spawned_id = spawn_rx
        .blocking_recv()
        .expect("spawn response")
        .expect("spawn ok");

    {
        let mut runtime = app
            .world_mut()
            .resource_mut::<crate::game_runtime::RuntimeState>();
        runtime.set_state("Paused".to_string(), Some("Instant".to_string()), 0.0);
    }

    {
        let world = app.world_mut();
        let mut q =
            world.query::<(&NetworkId, &mut Velocity, &mut LuaScript, &mut AiBehavior)>();
        let mut found = false;
        for (network_id, mut velocity, mut script, mut ai) in q.iter_mut(world) {
            if network_id.0 == spawned_id {
                velocity.x = 17.0;
                velocity.y = -6.5;
                script.state = serde_json::json!({"counter": 7, "mode": "alert"});
                ai.state = AiState::Chasing { target_id: 42 };
                found = true;
            }
        }
        assert!(found, "spawned entity not found");
    }

    let (save_tx, save_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetSaveData(save_tx))
        .expect("send save");
    app.update();
    let mut save = save_rx.blocking_recv().expect("save response");

    assert_eq!(save.game_state, "Paused");
    let saved_entity = save
        .entities
        .iter()
        .find(|e| e.network_id == Some(spawned_id))
        .expect("saved entity exists");
    assert!((saved_entity.vx - 17.0).abs() < 0.0001);
    assert!((saved_entity.vy - (-6.5)).abs() < 0.0001);
    assert_eq!(
        saved_entity
            .script_state
            .as_ref()
            .and_then(|v| v.get("counter"))
            .and_then(|v| v.as_i64()),
        Some(7)
    );
    assert!(matches!(
        saved_entity.ai_state,
        Some(SaveAiState::Chasing { target_id: 42 })
    ));

    save.game_state = "Menu".to_string();
    let (load_tx, load_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::LoadSaveData(Box::new(save), load_tx))
        .expect("send load");
    app.update();
    app.update();
    assert!(load_rx.blocking_recv().expect("load response").is_ok());

    let runtime = app.world().resource::<crate::game_runtime::RuntimeState>();
    assert_eq!(runtime.state, "Menu");

    let world = app.world_mut();
    let mut q = world.query::<(&NetworkId, &Velocity, &LuaScript, &AiBehavior)>();
    let mut restored = false;
    for (network_id, velocity, script, ai) in q.iter(world) {
        if network_id.0 == spawned_id {
            assert!((velocity.x - 17.0).abs() < 0.0001);
            assert!((velocity.y - (-6.5)).abs() < 0.0001);
            assert_eq!(
                script.state.get("counter").and_then(|v| v.as_i64()),
                Some(7)
            );
            assert!(matches!(ai.state, AiState::Chasing { target_id: 42 }));
            restored = true;
        }
    }
    assert!(restored, "restored entity not found");
}

#[test]
fn save_load_restores_network_ids_and_content_resources() {
    let (sender, receiver) = crossbeam_channel::unbounded::<ApiCommand>();
    let mut app = setup_runtime_app(receiver);

    let (spawn_tx, spawn_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SpawnEntity(
            EntitySpawnRequest {
                x: 8.0,
                y: 12.0,
                components: vec![],
                script: None,
                tags: vec!["enemy".to_string()],
                is_player: false,
                invisible: false,
            },
            spawn_tx,
        ))
        .expect("send spawn");
    app.update();
    let spawned_id = spawn_rx
        .blocking_recv()
        .expect("spawn response")
        .expect("spawn ok");
    assert_eq!(spawned_id, 1);

    let (save_tx, save_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::GetSaveData(save_tx))
        .expect("send save");
    app.update();
    let mut save = save_rx.blocking_recv().expect("save response");
    assert_eq!(save.entities.len(), 1);

    save.version = 2;
    save.next_network_id = 10;
    save.entities[0].network_id = Some(77);
    save.animation_graphs.insert(
        "custom_actor".to_string(),
        crate::animation::AnimationGraphDef {
            default_state: "idle".to_string(),
            states: HashMap::from([(
                "idle".to_string(),
                crate::animation::AnimationClipDef {
                    frame_count: 2,
                    frames: vec![0, 1],
                    fps: 10.0,
                    looping: true,
                    next: None,
                    events: Vec::new(),
                },
            )]),
        },
    );
    save.sprite_sheets.insert(
        "custom_actor".to_string(),
        crate::sprites::SpriteSheetDef {
            path: "assets/custom_actor.png".to_string(),
            frame_width: 32,
            frame_height: 32,
            columns: 4,
            rows: 1,
            animations: HashMap::new(),
            direction_map: None,
            anchor_y: -0.15,
        },
    );
    save.particle_presets.insert(
        "burst".to_string(),
        crate::particles::ParticlePresetDef {
            one_shot: true,
            burst_count: 24,
            ..Default::default()
        },
    );

    let (load_tx, load_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::LoadSaveData(Box::new(save), load_tx))
        .expect("send load");
    app.update();
    assert!(load_rx.blocking_recv().expect("load response").is_ok());

    let (list_tx, list_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::ListEntities(list_tx))
        .expect("send list");
    app.update();
    let entities = list_rx.blocking_recv().expect("list response");
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].network_id, Some(77));
    assert_eq!(entities[0].id, 77);

    let (spawn2_tx, spawn2_rx) = tokio::sync::oneshot::channel();
    sender
        .send(ApiCommand::SpawnEntity(
            EntitySpawnRequest {
                x: 9.0,
                y: 12.0,
                components: vec![],
                script: None,
                tags: vec![],
                is_player: false,
                invisible: false,
            },
            spawn2_tx,
        ))
        .expect("send spawn2");
    app.update();
    let spawned_id_2 = spawn2_rx
        .blocking_recv()
        .expect("spawn2 response")
        .expect("spawn2 ok");
    assert_eq!(spawned_id_2, 78);

    let anim = app
        .world()
        .resource::<crate::animation::AnimationLibrary>()
        .graphs
        .clone();
    assert!(anim.contains_key("custom_actor"));
    let sheets = app
        .world()
        .resource::<crate::sprites::SpriteSheetRegistry>()
        .sheets
        .clone();
    assert!(sheets.contains_key("custom_actor"));
    let presets = app
        .world()
        .resource::<crate::particles::ParticlePresetLibrary>()
        .presets
        .clone();
    assert!(presets.contains_key("burst"));
}
