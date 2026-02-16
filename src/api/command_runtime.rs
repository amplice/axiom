use super::command_runtime_helpers::*;
use super::*;
use crate::scripting::ScriptBackend;
use bevy::ecs::system::SystemParam;

#[derive(SystemParam)]
pub(super) struct ApiRuntimeCtx<'w, 's> {
    channels: Res<'w, ApiChannels>,
    commands: Commands<'w, 's>,
    tilemap: Res<'w, Tilemap>,
    physics: Res<'w, GameConfig>,
    next_network_id: ResMut<'w, NextNetworkId>,
    pending_level: ResMut<'w, PendingLevelChange>,
    pending_physics: ResMut<'w, PendingPhysicsChange>,
    event_bus: ResMut<'w, GameEventBus>,
    perf: Res<'w, PerfStats>,
    debug_overlay: ResMut<'w, DebugOverlayConfig>,
    script_engine: ResMut<'w, ScriptEngine>,
    script_errors: ResMut<'w, ScriptErrors>,
    sprite_assets: Option<ResMut<'w, SpriteAssets>>,
    pending_screenshot: ResMut<'w, PendingScreenshot>,
    entity_query: Query<'w, 's, EntityQueryItem<'static>>,
    extras_query: Query<'w, 's, ExtrasQueryItem<'static>>,
}

pub(super) fn process_api_commands(ctx: ApiRuntimeCtx<'_, '_>) {
    let ApiRuntimeCtx {
        channels,
        mut commands,
        tilemap,
        physics,
        mut next_network_id,
        mut pending_level,
        mut pending_physics,
        mut event_bus,
        perf,
        mut debug_overlay,
        mut script_engine,
        mut script_errors,
        sprite_assets,
        mut pending_screenshot,
        entity_query,
        extras_query,
    } = ctx;
    let player_state_snapshot = || -> PlayerState {
        for (
            _entity,
            pos,
            vel,
            _col,
            player,
            _grav,
            _hm,
            _jmp,
            _tdm,
            grounded,
            alive,
            _network_id,
            _tags,
            _script,
        ) in entity_query.iter()
        {
            if player.is_some() {
                return PlayerState {
                    x: pos.x,
                    y: pos.y,
                    vx: vel.map_or(0.0, |v| v.x),
                    vy: vel.map_or(0.0, |v| v.y),
                    grounded: grounded.is_some_and(|g| g.0),
                    alive: alive.is_some_and(|a| a.0),
                };
            }
        }
        PlayerState {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            grounded: false,
            alive: false,
        }
    };

    let player_entity = || -> Option<Entity> {
        entity_query.iter().find_map(
            |(
                entity,
                _pos,
                _vel,
                _col,
                player,
                _grav,
                _hm,
                _jmp,
                _tdm,
                _grounded,
                _alive,
                _network_id,
                _tags,
                _script,
            )| player.map(|_| entity),
        )
    };

    let entity_extras = |entity: Entity| -> EntityInfoExtras {
        match extras_query.get(entity) {
            Ok((
                _e,
                health,
                contact,
                trigger,
                pickup,
                projectile,
                hitbox,
                moving_platform,
                animation_controller,
                path_follower,
                ai_behavior,
                particle_emitter,
                _invincibility,
            )) => EntityInfoExtras {
                health_current: health.map(|h| h.current),
                health_max: health.map(|h| h.max),
                has_contact: contact.is_some(),
                has_trigger: trigger.is_some(),
                has_pickup: pickup.is_some(),
                has_projectile: projectile.is_some(),
                has_hitbox: hitbox.is_some(),
                has_moving_platform: moving_platform.is_some(),
                has_animation_controller: animation_controller.is_some(),
                has_path_follower: path_follower.is_some(),
                has_ai_behavior: ai_behavior.is_some(),
                has_particle_emitter: particle_emitter.is_some(),
                ai_behavior: ai_behavior.map(ai_behavior_kind_name),
                ai_state: ai_behavior.map(ai_state_name),
                ai_target_id: ai_behavior.and_then(ai_state_target_id),
                path_target: path_follower.map(|f| Vec2Def {
                    x: f.target.x,
                    y: f.target.y,
                }),
                path_len: path_follower.map(|f| f.path.len()),
                animation_graph: animation_controller.map(|a| a.graph.clone()),
                animation_state: animation_controller.map(|a| a.state.clone()),
                animation_frame: animation_controller.map(|a| a.frame),
                animation_facing_right: animation_controller.map(|a| a.facing_right),
            },
            Err(_) => EntityInfoExtras::default(),
        }
    };

    let mut entity_id_index = HashMap::<u64, Entity>::new();
    let mut entity_id_index_ready = false;
    macro_rules! ensure_entity_id_index {
        () => {
            if !entity_id_index_ready {
                entity_id_index.clear();
                for (
                    entity,
                    _pos,
                    _vel,
                    _col,
                    _player,
                    _grav,
                    _hm,
                    _jmp,
                    _tdm,
                    _grounded,
                    _alive,
                    network_id,
                    _tags,
                    _script,
                ) in entity_query.iter()
                {
                    if let Some(network_id) = network_id {
                        entity_id_index.insert(network_id.0, entity);
                    }
                }
                entity_id_index_ready = true;
            }
        };
    }

    while let Ok(cmd) = channels.receiver.try_recv() {
        match cmd {
            ApiCommand::GetState(tx) => {
                let player_state = player_state_snapshot();
                let state = GameState {
                    tilemap: TilemapState {
                        width: tilemap.width,
                        height: tilemap.height,
                        tiles: tilemap.tiles.clone(),
                        player_spawn: tilemap.player_spawn,
                        goal: tilemap.goal,
                    },
                    player: player_state,
                };
                let _ = tx.send(state);
            }
            ApiCommand::GetPlayer(tx) => {
                let _ = tx.send(player_state_snapshot());
            }
            ApiCommand::RaycastEntities(req, tx) => {
                let origin = Vec2::new(req.origin[0], req.origin[1]);
                let dir = Vec2::new(req.direction[0], req.direction[1]);
                let max_distance = req.max_distance.max(0.0);
                let tag_filter = req
                    .tag
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(|t| t.to_string());
                let targets = entity_query.iter().filter_map(
                    |(
                        _entity,
                        pos,
                        _vel,
                        collider,
                        _player,
                        _grav,
                        _hm,
                        _jmp,
                        _tdm,
                        _grounded,
                        _alive,
                        network_id,
                        tags,
                        _script,
                    )| {
                        let collider = collider?;
                        let network_id = network_id?;
                        if let Some(tag) = tag_filter.as_deref() {
                            let matches = tags.map(|t| t.0.contains(tag)).unwrap_or(false);
                            if !matches {
                                return None;
                            }
                        }
                        Some(crate::raycast::RaycastAabb {
                            id: network_id.0,
                            min: Vec2::new(
                                pos.x - collider.width * 0.5,
                                pos.y - collider.height * 0.5,
                            ),
                            max: Vec2::new(
                                pos.x + collider.width * 0.5,
                                pos.y + collider.height * 0.5,
                            ),
                        })
                    },
                );
                let hits: Vec<EntityRaycastHit> =
                    crate::raycast::raycast_aabbs(origin, dir, max_distance, targets)
                        .into_iter()
                        .map(|h| EntityRaycastHit {
                            id: h.id,
                            x: h.x,
                            y: h.y,
                            distance: h.distance,
                        })
                        .collect();
                let _ = tx.send(hits);
            }
            ApiCommand::SetLevel(req, tx) => {
                if req.tiles.len() != req.width * req.height {
                    let _ = tx.send(Err(format!(
                        "Tile count {} doesn't match {}x{}={}",
                        req.tiles.len(),
                        req.width,
                        req.height,
                        req.width * req.height
                    )));
                    continue;
                }
                pending_level.0 = Some(req);
                let _ = tx.send(Ok(()));
            }
            ApiCommand::TeleportPlayer(x, y, tx) => {
                if let Some(entity) = player_entity() {
                    commands.entity(entity).insert(GamePosition { x, y });
                    commands.entity(entity).insert(Velocity { x: 0.0, y: 0.0 });
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("No player found".to_string()));
                }
            }
            ApiCommand::GetPhysicsConfig(tx) | ApiCommand::GetConfig(tx) => {
                let _ = tx.send(physics.clone());
            }
            ApiCommand::SetPhysicsConfig(config, tx) | ApiCommand::SetConfig(config, tx) => {
                pending_physics.0 = Some(config);
                let _ = tx.send(Ok(()));
            }
            ApiCommand::GetSprites(tx) => {
                if let Some(ref sa) = sprite_assets {
                    let _ = tx.send(sa.manifest.clone());
                } else {
                    let _ = tx.send(crate::sprites::SpriteManifest::default());
                }
            }
            ApiCommand::SetSprites(manifest, tx) => {
                let manifest_for_world = manifest.clone();
                commands.queue(move |world: &mut World| {
                    let asset_server = world.get_resource::<AssetServer>().cloned();
                    if let (Some(asset_srv), Some(mut sa)) =
                        (asset_server, world.get_resource_mut::<SpriteAssets>())
                    {
                        crate::sprites::reload_from_manifest(
                            &manifest_for_world,
                            &asset_srv,
                            &mut sa,
                        );
                    }
                });
                let _ = tx.send(Ok(()));
            }
            ApiCommand::GetSpriteSheets(tx) => {
                commands.queue(move |world: &mut World| {
                    let items = world
                        .get_resource::<crate::sprites::SpriteSheetRegistry>()
                        .map(|r| r.sheets.clone())
                        .unwrap_or_default();
                    let _ = tx.send(items);
                });
            }
            ApiCommand::UpsertSpriteSheet(req, tx) => {
                let name = req.name.trim().to_string();
                if name.is_empty() {
                    let _ = tx.send(Err("Sprite sheet name cannot be empty".into()));
                    continue;
                }
                if req.frame_width == 0 || req.frame_height == 0 || req.columns == 0 {
                    let _ = tx.send(Err(
                        "Sprite sheet frame_width/frame_height/columns must be > 0".into(),
                    ));
                    continue;
                }

                let mut sheet_anims = std::collections::HashMap::new();
                let mut graph_states = std::collections::HashMap::new();
                for (state_name, anim) in req.animations {
                    let state_name = state_name.trim().to_string();
                    if state_name.is_empty() {
                        continue;
                    }
                    let frames = anim.frames;
                    let graph_frames = frames.clone();
                    let frame_count = graph_frames.len().max(1);
                    sheet_anims.insert(
                        state_name.clone(),
                        crate::sprites::SpriteSheetAnimationDef {
                            frames,
                            fps: anim.fps.max(0.001),
                            looping: anim.looping,
                            next: anim.next.clone(),
                            events: anim.events.clone(),
                        },
                    );
                    graph_states.insert(
                        state_name,
                        crate::animation::AnimationClipDef {
                            frame_count,
                            frames: graph_frames,
                            fps: anim.fps.max(0.001),
                            looping: anim.looping,
                            next: anim.next,
                            events: anim.events,
                        },
                    );
                }

                if graph_states.is_empty() {
                    let _ = tx.send(Err(
                        "Sprite sheet must include at least one named animation".into(),
                    ));
                    continue;
                }

                let default_state = if graph_states.contains_key("idle") {
                    "idle".to_string()
                } else {
                    let mut names: Vec<_> = graph_states.keys().cloned().collect();
                    names.sort();
                    names.first().cloned().unwrap_or_else(|| "idle".to_string())
                };

                let sheet = crate::sprites::SpriteSheetDef {
                    path: req.path,
                    frame_width: req.frame_width,
                    frame_height: req.frame_height,
                    columns: req.columns,
                    animations: sheet_anims,
                };
                let graph = crate::animation::AnimationGraphDef {
                    default_state,
                    states: graph_states,
                };

                commands.queue(move |world: &mut World| {
                    {
                        let Some(mut registry) =
                            world.get_resource_mut::<crate::sprites::SpriteSheetRegistry>()
                        else {
                            let _ = tx.send(Err("Sprite sheet registry unavailable".into()));
                            return;
                        };
                        registry.sheets.insert(name.clone(), sheet);
                    }
                    {
                        let Some(mut library) =
                            world.get_resource_mut::<crate::animation::AnimationLibrary>()
                        else {
                            let _ = tx.send(Err("Animation library unavailable".into()));
                            return;
                        };
                        library.graphs.insert(name, graph);
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::TakeScreenshot(tx) => {
                pending_screenshot.0 = true;
                let _ = tx.send(Ok(()));
            }
            ApiCommand::SpawnEntity(req, tx) => {
                ensure_entity_id_index!();
                let assigned_id = next_network_id.0;
                let entity = crate::spawn::spawn_entity(&mut commands, &req, &mut next_network_id);
                entity_id_index.insert(assigned_id, entity);
                commands.queue(move |_world: &mut World| {
                    let _ = tx.send(Ok(assigned_id));
                });
            }
            ApiCommand::ListEntities(tx) => {
                let mut entities = Vec::new();
                for (
                    entity,
                    pos,
                    vel,
                    collider,
                    player,
                    gravity,
                    hmover,
                    jumper,
                    tdmover,
                    _grounded,
                    alive,
                    network_id,
                    tags,
                    script,
                ) in entity_query.iter()
                {
                    let Some(network_id) = network_id else {
                        continue;
                    };
                    entities.push(build_entity_info(
                        EntityInfoSource {
                            id: network_id.0,
                            pos,
                            vel,
                            collider,
                            player,
                            gravity,
                            hmover,
                            jumper,
                            tdmover,
                            alive,
                            network_id: Some(network_id),
                            tags,
                            script,
                        },
                        entity_extras(entity),
                    ));
                }
                let _ = tx.send(entities);
            }
            ApiCommand::GetEntity(id, tx) => {
                ensure_entity_id_index!();
                let info = match entity_id_index
                    .get(&id)
                    .copied()
                    .and_then(|entity| entity_query.get(entity).ok().map(|v| (entity, v)))
                {
                    Some((
                        entity,
                        (
                            _entity,
                            pos,
                            vel,
                            collider,
                            player,
                            gravity,
                            hmover,
                            jumper,
                            tdmover,
                            _grounded,
                            alive,
                            network_id,
                            tags,
                            script,
                        ),
                    )) => network_id.map(|nid| {
                        build_entity_info(
                            EntityInfoSource {
                                id: nid.0,
                                pos,
                                vel,
                                collider,
                                player,
                                gravity,
                                hmover,
                                jumper,
                                tdmover,
                                alive,
                                network_id: Some(nid),
                                tags,
                                script,
                            },
                            entity_extras(entity),
                        )
                    }),
                    None => None,
                };
                let _ = tx.send(info);
            }
            ApiCommand::GetEntityAnimation(id, tx) => {
                ensure_entity_id_index!();
                let state = entity_id_index.get(&id).copied().and_then(|entity| {
                    let public_id = entity_query.get(entity).ok().and_then(
                        |(
                            _entity,
                            _pos,
                            _vel,
                            _collider,
                            _player,
                            _gravity,
                            _hmover,
                            _jumper,
                            _tdmover,
                            _grounded,
                            _alive,
                            network_id,
                            _tags,
                            _script,
                        )| network_id.map(|n| n.0),
                    )?;
                    extras_query.get(entity).ok().and_then(
                        |(
                            _entity,
                            _health,
                            _contact,
                            _trigger,
                            _pickup,
                            _projectile,
                            _hitbox,
                            _moving_platform,
                            animation_controller,
                            _path_follower,
                            _ai_behavior,
                            _particle_emitter,
                            _invincibility,
                        )| {
                            animation_controller.map(|anim| {
                                crate::animation::AnimationEntityState {
                                    id: public_id,
                                    graph: anim.graph.clone(),
                                    state: anim.state.clone(),
                                    frame: anim.frame,
                                    playing: anim.playing,
                                    speed: anim.speed,
                                    facing_right: anim.facing_right,
                                }
                            })
                        },
                    )
                });
                let _ = tx.send(state);
            }
            ApiCommand::SetEntityAnimation(id, state_name, tx) => {
                ensure_entity_id_index!();
                let Some(entity) = entity_id_index.get(&id).copied() else {
                    let _ = tx.send(Err("Entity not found".into()));
                    continue;
                };
                commands.queue(move |world: &mut World| {
                    let next_state = state_name.trim();
                    if next_state.is_empty() {
                        let _ = tx.send(Err("Animation state cannot be empty".into()));
                        return;
                    }
                    let Some(current_anim) = world.get::<AnimationController>(entity) else {
                        let _ = tx.send(Err("Entity has no AnimationController".into()));
                        return;
                    };
                    if let Some(library) =
                        world.get_resource::<crate::animation::AnimationLibrary>()
                    {
                        if let Some(graph) = library.graphs.get(&current_anim.graph) {
                            if !graph.states.contains_key(next_state) {
                                let _ = tx.send(Err(format!(
                                    "Animation state '{}' not found in graph '{}'",
                                    next_state, current_anim.graph
                                )));
                                return;
                            }
                        }
                    }
                    let Some(mut anim) = world.get_mut::<AnimationController>(entity) else {
                        let _ = tx.send(Err("Entity has no AnimationController".into()));
                        return;
                    };
                    anim.state = next_state.to_string();
                    anim.frame = 0;
                    anim.timer = 0.0;
                    anim.playing = true;
                    anim.auto_from_velocity = false;
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::SetEntityParticles(id, req, tx) => {
                ensure_entity_id_index!();
                let Some(entity) = entity_id_index.get(&id).copied() else {
                    let _ = tx.send(Err("Entity not found".into()));
                    continue;
                };
                commands.queue(move |world: &mut World| {
                    let emitter = if let Some(def) = req.emitter.as_ref() {
                        crate::particles::ParticleEmitter::from_preset(def, None)
                    } else if let Some(name) = req.preset.as_deref() {
                        let trimmed = name.trim();
                        if trimmed.is_empty() {
                            let _ = tx.send(Err("Particle preset name cannot be empty".into()));
                            return;
                        }
                        if let Some(presets) =
                            world.get_resource::<crate::particles::ParticlePresetLibrary>()
                        {
                            if let Some(preset) = presets.presets.get(trimmed) {
                                crate::particles::ParticleEmitter::from_preset(
                                    preset,
                                    Some(trimmed.to_string()),
                                )
                            } else {
                                crate::particles::ParticleEmitter::preset_only(trimmed.to_string())
                            }
                        } else {
                            crate::particles::ParticleEmitter::preset_only(trimmed.to_string())
                        }
                    } else {
                        let _ = tx.send(Err(
                            "Provide either preset or emitter definition for particles".into(),
                        ));
                        return;
                    };
                    world.entity_mut(entity).insert(emitter);
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::DeleteEntity(id, tx) => {
                ensure_entity_id_index!();
                if let Some(entity) = entity_id_index.get(&id).copied() {
                    entity_id_index.retain(|_, mapped| *mapped != entity);
                    commands.queue(move |world: &mut World| {
                        world.despawn(entity);
                        let _ = tx.send(Ok(()));
                    });
                } else {
                    let _ = tx.send(Err("Entity not found".into()));
                }
            }
            ApiCommand::ResetNonPlayerEntities(tx) => {
                ensure_entity_id_index!();
                for (
                    entity,
                    _pos,
                    _vel,
                    _col,
                    player,
                    _grav,
                    _hm,
                    _jmp,
                    _tdm,
                    _grounded,
                    _alive,
                    _network_id,
                    _tags,
                    _script,
                ) in entity_query.iter()
                {
                    if player.is_none() {
                        entity_id_index.retain(|_, mapped| *mapped != entity);
                        commands.entity(entity).despawn();
                    }
                }
                let _ = tx.send(Ok(()));
            }
            ApiCommand::DamageEntity(id, amount, tx) => {
                ensure_entity_id_index!();
                if let Some(entity) = entity_id_index.get(&id).copied() {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut health) = world.get_mut::<Health>(entity) {
                            health.current -= amount.max(0.0);
                            if health.current <= 0.0 {
                                if let Some(mut alive) = world.get_mut::<Alive>(entity) {
                                    alive.0 = false;
                                }
                            }
                        }
                    });
                    event_bus.emit(
                        "entity_damaged_manual",
                        serde_json::json!({"target": id, "amount": amount}),
                        None,
                    );
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("Entity not found".into()));
                }
            }
            ApiCommand::GetEvents(tx) => {
                let _ = tx.send(event_bus.recent.clone());
            }
            ApiCommand::GetPerf(tx) => {
                let _ = tx.send(perf.clone());
            }
            ApiCommand::GetPerfHistory(tx) => {
                let _ = tx.send(perf.history.clone());
            }
            ApiCommand::GetSaveData(tx) => {
                let entities = collect_save_entities(&entity_query, &extras_query);
                let script_snapshot = script_engine.snapshot();
                let config_snapshot = physics.clone();
                let tilemap_snapshot = tilemap.clone();
                let next_network_id_snapshot = next_network_id.0;
                commands.queue(move |world: &mut World| {
                    let animation_graphs = world
                        .get_resource::<crate::animation::AnimationLibrary>()
                        .map(|lib| lib.graphs.clone())
                        .unwrap_or_default();
                    let sprite_sheets = world
                        .get_resource::<crate::sprites::SpriteSheetRegistry>()
                        .map(|registry| registry.sheets.clone())
                        .unwrap_or_default();
                    let particle_presets = world
                        .get_resource::<crate::particles::ParticlePresetLibrary>()
                        .map(|library| library.presets.clone())
                        .unwrap_or_default();
                    let game_state = world
                        .get_resource::<crate::game_runtime::RuntimeState>()
                        .map(|state| state.state.clone())
                        .unwrap_or_else(|| "Playing".to_string());
                    let data = SaveGameData {
                        version: 4,
                        config: config_snapshot,
                        tilemap: tilemap_snapshot,
                        game_state,
                        next_network_id: next_network_id_snapshot,
                        entities,
                        scripts: script_snapshot.scripts,
                        global_scripts: script_snapshot.global_scripts.into_iter().collect(),
                        game_vars: script_snapshot.vars,
                        animation_graphs,
                        sprite_sheets,
                        particle_presets,
                    };
                    let _ = tx.send(data);
                });
            }
            ApiCommand::LoadSaveData(save, tx) => {
                apply_loaded_save_data(
                    *save,
                    &mut pending_level,
                    &mut pending_physics,
                    &mut script_engine,
                    &mut next_network_id,
                    &mut commands,
                    &entity_query,
                );
                let _ = tx.send(Ok(()));
            }
            ApiCommand::LoadScript(req, tx) => {
                let script_name = req.name.clone();
                let result = crate::scripting::ScriptBackend::load_script(
                    &mut *script_engine,
                    req.name,
                    req.source,
                    req.global,
                );
                if let Err(err) = &result {
                    script_errors.push(ScriptError {
                        script_name,
                        entity_id: None,
                        error_message: format!("Script load failed: {err}"),
                        frame: 0,
                    });
                }
                let _ = tx.send(result);
            }
            ApiCommand::ListScripts(tx) => {
                let _ = tx.send(crate::scripting::ScriptBackend::list_scripts(
                    &*script_engine,
                ));
            }
            ApiCommand::GetScript(name, tx) => {
                let source = script_engine.get_script_source(&name);
                let _ = tx.send(source);
            }
            ApiCommand::DeleteScript(name, tx) => {
                crate::scripting::ScriptBackend::remove_script(&mut *script_engine, &name);
                let _ = tx.send(Ok(()));
            }
            ApiCommand::TestScript(req, tx) => {
                let result = crate::scripting::dry_run_script(&req.source);
                let _ = tx.send(result);
            }
            ApiCommand::GetScriptErrors(tx) => {
                let _ = tx.send(script_errors.entries.clone());
            }
            ApiCommand::GetScriptVars(tx) => {
                let _ = tx.send(
                    serde_json::to_value(crate::scripting::ScriptBackend::vars(&*script_engine))
                        .unwrap_or(serde_json::json!({})),
                );
            }
            ApiCommand::SetScriptVars(vars, tx) => {
                let _ = tx.send(set_script_vars_from_json(&mut script_engine, vars));
            }
            ApiCommand::GetScriptEvents(tx) => {
                let _ = tx.send(script_engine.events().to_vec());
            }
            ApiCommand::GetScriptStats(tx) => {
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
                let _ = tx.send(crate::scripting::api::ScriptStats {
                    loaded_scripts: script_engine.scripts.len(),
                    global_scripts: script_engine.global_scripts.len(),
                    disabled_global_scripts: script_engine.disabled_global_scripts.len(),
                    recent_event_buffer_len: script_engine.events.len(),
                    dropped_events: script_engine.dropped_events,
                    recent_error_buffer_len: script_errors.entries.len(),
                    entity_budget_ms: env_u64(
                        "AXIOM_SCRIPT_ENTITY_BUDGET_MS",
                        crate::scripting::DEFAULT_ENTITY_SCRIPT_BUDGET_MS,
                    ),
                    global_budget_ms: env_u64(
                        "AXIOM_SCRIPT_GLOBAL_BUDGET_MS",
                        crate::scripting::DEFAULT_GLOBAL_SCRIPT_BUDGET_MS,
                    ),
                    hook_instruction_interval: env_u64(
                        "AXIOM_SCRIPT_HOOK_INSTRUCTION_INTERVAL",
                        crate::scripting::DEFAULT_SCRIPT_HOOK_INSTRUCTION_INTERVAL as u64,
                    ) as u32,
                    rhai_max_operations: env_u64(
                        "AXIOM_RHAI_MAX_OPERATIONS",
                        crate::scripting::DEFAULT_RHAI_MAX_OPERATIONS,
                    ),
                    rhai_max_call_levels: env_usize(
                        "AXIOM_RHAI_MAX_CALL_LEVELS",
                        crate::scripting::DEFAULT_RHAI_MAX_CALL_LEVELS,
                    ),
                });
            }
            ApiCommand::ListAnimationGraphs(tx) => {
                commands.queue(move |world: &mut World| {
                    let items = world
                        .get_resource::<crate::animation::AnimationLibrary>()
                        .map(crate::animation::list_graph_infos)
                        .unwrap_or_default();
                    let _ = tx.send(items);
                });
            }
            ApiCommand::GetAnimationGraph(name, tx) => {
                commands.queue(move |world: &mut World| {
                    let graph = world
                        .get_resource::<crate::animation::AnimationLibrary>()
                        .and_then(|library| library.graphs.get(&name).cloned());
                    let _ = tx.send(graph);
                });
            }
            ApiCommand::SetAnimationGraph(name, graph, tx) => {
                commands.queue(move |world: &mut World| {
                    if graph.states.is_empty() {
                        let _ =
                            tx.send(Err("Animation graph must define at least one state".into()));
                        return;
                    }
                    let Some(mut library) =
                        world.get_resource_mut::<crate::animation::AnimationLibrary>()
                    else {
                        let _ = tx.send(Err("Animation library unavailable".into()));
                        return;
                    };
                    library.graphs.insert(name, graph);
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::DeleteAnimationGraph(name, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut library) =
                        world.get_resource_mut::<crate::animation::AnimationLibrary>()
                    else {
                        let _ = tx.send(Err("Animation library unavailable".into()));
                        return;
                    };
                    if library.graphs.remove(&name).is_some() {
                        let _ = tx.send(Ok(()));
                    } else {
                        let _ = tx.send(Err("Animation graph not found".into()));
                    }
                });
            }
            ApiCommand::GetAnimationStates(tx) => {
                commands.queue(move |world: &mut World| {
                    let mut query = world.query::<(&AnimationController, &NetworkId)>();
                    let items = crate::animation::collect_animation_states(world, &mut query);
                    let _ = tx.send(items);
                });
            }
            ApiCommand::GetDebugOverlay(tx) => {
                let mut features = debug_overlay.features.iter().cloned().collect::<Vec<_>>();
                features.sort();
                let _ = tx.send(DebugOverlayState {
                    show: debug_overlay.show,
                    features,
                });
            }
            ApiCommand::SetDebugOverlay(req, tx) => {
                debug_overlay.show = req.show;
                debug_overlay.features = req.features.into_iter().collect();
                let _ = tx.send(Ok(()));
            }
            ApiCommand::SetAudioSfx(effects, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut audio) = world.get_resource_mut::<crate::audio::AudioManager>()
                    else {
                        let _ = tx.send(Err("Audio manager unavailable".to_string()));
                        return;
                    };
                    audio.set_sfx(effects);
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::SetAudioMusic(tracks, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut audio) = world.get_resource_mut::<crate::audio::AudioManager>()
                    else {
                        let _ = tx.send(Err("Audio manager unavailable".to_string()));
                        return;
                    };
                    audio.set_music(tracks);
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::PlayAudio(req, tx) => {
                let frame = event_bus.frame;
                commands.queue(move |world: &mut World| {
                    let Some(mut audio) = world.get_resource_mut::<crate::audio::AudioManager>()
                    else {
                        let _ = tx.send(Err("Audio manager unavailable".to_string()));
                        return;
                    };
                    if let Some(sfx) = req.sfx.as_deref() {
                        if let Err(e) = audio.play_sfx(
                            sfx,
                            frame,
                            req.volume,
                            req.pitch,
                            Some("api".to_string()),
                        ) {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    }
                    if let Some(music) = req.music.as_deref() {
                        if let Err(e) =
                            audio.play_music(music, frame, req.fade_in, Some("api".to_string()))
                        {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    }
                    if req.sfx.is_none() && req.music.is_none() {
                        let _ = tx.send(Err("Request must include sfx or music".to_string()));
                    } else {
                        let _ = tx.send(Ok(()));
                    }
                });
            }
            ApiCommand::StopAudio(req, tx) => {
                let frame = event_bus.frame;
                commands.queue(move |world: &mut World| {
                    let Some(mut audio) = world.get_resource_mut::<crate::audio::AudioManager>()
                    else {
                        let _ = tx.send(Err("Audio manager unavailable".to_string()));
                        return;
                    };
                    if req.music.unwrap_or(false) {
                        audio.stop_music(frame, req.fade_out);
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::SetAudioConfig(req, tx) => {
                let frame = event_bus.frame;
                commands.queue(move |world: &mut World| {
                    let Some(mut audio) = world.get_resource_mut::<crate::audio::AudioManager>()
                    else {
                        let _ = tx.send(Err("Audio manager unavailable".to_string()));
                        return;
                    };
                    if let Some(v) = req.master_volume {
                        if let Err(e) = audio.set_volume("master", v, frame) {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    }
                    if let Some(v) = req.sfx_volume {
                        if let Err(e) = audio.set_volume("sfx", v, frame) {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    }
                    if let Some(v) = req.music_volume {
                        if let Err(e) = audio.set_volume("music", v, frame) {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::SetAudioTriggers(mappings, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut audio) = world.get_resource_mut::<crate::audio::AudioManager>()
                    else {
                        let _ = tx.send(Err("Audio manager unavailable".to_string()));
                        return;
                    };
                    audio.set_triggers(mappings);
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::SetParticlePresets(presets, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut library) =
                        world.get_resource_mut::<crate::particles::ParticlePresetLibrary>()
                    else {
                        let _ = tx.send(Err("Particle preset library unavailable".to_string()));
                        return;
                    };
                    library.presets = presets;
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::GetAudioState(tx) => {
                commands.queue(move |world: &mut World| {
                    let snapshot = world
                        .get_resource::<crate::audio::AudioManager>()
                        .map(|a| a.snapshot())
                        .unwrap_or_else(|| crate::audio::AudioManager::default().snapshot());
                    let _ = tx.send(snapshot);
                });
            }
            ApiCommand::SetCameraConfig(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut camera_config) =
                        world.get_resource_mut::<crate::camera::CameraConfig>()
                    else {
                        let _ = tx.send(Err("Camera config unavailable".to_string()));
                        return;
                    };
                    if let Some(id) = req.follow_target {
                        camera_config.follow_target = Some(id);
                    }
                    if let Some(speed) = req.follow_speed {
                        camera_config.follow_speed = speed.clamp(0.0, 1.0);
                    }
                    if let Some(zoom) = req.zoom {
                        camera_config.zoom = zoom.max(0.05);
                    }
                    if let Some(offset) = req.offset {
                        camera_config.offset = Vec2::new(offset[0], offset[1]);
                    }
                    if let Some(deadzone) = req.deadzone {
                        camera_config.deadzone =
                            Vec2::new(deadzone[0].max(0.0), deadzone[1].max(0.0));
                    }
                    if let Some(bounds) = req.bounds {
                        camera_config.bounds = Some(crate::camera::CameraBounds {
                            min_x: bounds.min_x,
                            max_x: bounds.max_x,
                            min_y: bounds.min_y,
                            max_y: bounds.max_y,
                        });
                    }
                    if let Some(look_at) = req.look_at {
                        camera_config.look_at = Some(Vec2::new(look_at[0], look_at[1]));
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::CameraShake(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut camera_shake) =
                        world.get_resource_mut::<crate::camera::CameraShakeState>()
                    else {
                        let _ = tx.send(Err("Camera shake unavailable".to_string()));
                        return;
                    };
                    camera_shake.intensity = req.intensity.max(0.0);
                    camera_shake.duration = req.duration.max(0.0);
                    camera_shake.remaining = req.duration.max(0.0);
                    camera_shake.decay = req.decay.unwrap_or(1.0).max(0.01);
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::CameraLookAt(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut camera_config) =
                        world.get_resource_mut::<crate::camera::CameraConfig>()
                    else {
                        let _ = tx.send(Err("Camera config unavailable".to_string()));
                        return;
                    };
                    if let Some(speed) = req.speed {
                        camera_config.follow_speed = speed.clamp(0.0, 1.0);
                    }
                    camera_config.look_at = Some(Vec2::new(req.x, req.y));
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::GetCameraState(tx) => {
                commands.queue(move |world: &mut World| {
                    let camera_config = world
                        .get_resource::<crate::camera::CameraConfig>()
                        .cloned()
                        .unwrap_or_default();
                    let shake_remaining = world
                        .get_resource::<crate::camera::CameraShakeState>()
                        .map(|s| s.remaining)
                        .unwrap_or(0.0);
                    let mut camera_query =
                        world.query_filtered::<&Transform, With<crate::camera::MainCamera>>();
                    let position = camera_query
                        .iter(world)
                        .next()
                        .map(|t| [t.translation.x, t.translation.y]);
                    let bounds = camera_config.bounds.as_ref().map(|b| CameraBoundsRequest {
                        min_x: b.min_x,
                        max_x: b.max_x,
                        min_y: b.min_y,
                        max_y: b.max_y,
                    });
                    let state = CameraStateResponse {
                        position,
                        zoom: camera_config.zoom,
                        follow_speed: camera_config.follow_speed,
                        follow_target: camera_config.follow_target,
                        look_at: camera_config.look_at.map(|v| [v.x, v.y]),
                        offset: [camera_config.offset.x, camera_config.offset.y],
                        deadzone: [camera_config.deadzone.x, camera_config.deadzone.y],
                        bounds,
                        shake_remaining,
                    };
                    let _ = tx.send(state);
                });
            }
            ApiCommand::SetUiScreen(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut ui) = world.get_resource_mut::<crate::ui::UiManager>() else {
                        let _ = tx.send(Err("UI manager unavailable".to_string()));
                        return;
                    };
                    if req.name.trim().is_empty() {
                        let _ = tx.send(Err("Screen name cannot be empty".to_string()));
                        return;
                    }
                    ui.upsert_screen(crate::ui::UiScreen {
                        name: req.name,
                        layer: req.layer,
                        nodes: req.nodes,
                        visible: true,
                    });
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::ShowUiScreen(name, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut ui) = world.get_resource_mut::<crate::ui::UiManager>() else {
                        let _ = tx.send(Err("UI manager unavailable".to_string()));
                        return;
                    };
                    let _ = tx.send(ui.show_screen(&name));
                });
            }
            ApiCommand::HideUiScreen(name, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut ui) = world.get_resource_mut::<crate::ui::UiManager>() else {
                        let _ = tx.send(Err("UI manager unavailable".to_string()));
                        return;
                    };
                    let _ = tx.send(ui.hide_screen(&name));
                });
            }
            ApiCommand::UpdateUiNode(screen, node_id, update, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut ui) = world.get_resource_mut::<crate::ui::UiManager>() else {
                        let _ = tx.send(Err("UI manager unavailable".to_string()));
                        return;
                    };
                    let result = ui.update_node(
                        &screen,
                        &node_id,
                        crate::ui::UiNodeUpdate {
                            node_type: update.node_type,
                            visible: update.visible,
                            text: update.text,
                            value: update.value,
                            max: update.max,
                        },
                    );
                    let _ = tx.send(result);
                });
            }
            ApiCommand::GetUiState(tx) => {
                commands.queue(move |world: &mut World| {
                    let snapshot = world
                        .get_resource::<crate::ui::UiManager>()
                        .map(|ui| ui.snapshot())
                        .unwrap_or_default();
                    let _ = tx.send(snapshot);
                });
            }
            ApiCommand::SetDialogueConversation(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut dialogue) = world.get_resource_mut::<crate::ui::DialogueManager>()
                    else {
                        let _ = tx.send(Err("Dialogue manager unavailable".to_string()));
                        return;
                    };
                    if req.name.trim().is_empty() {
                        let _ = tx.send(Err("Conversation name cannot be empty".to_string()));
                        return;
                    }
                    dialogue.upsert_conversation(crate::ui::DialogueConversation {
                        name: req.name,
                        nodes: req.nodes,
                    });
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::StartDialogue(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let (result, active) = {
                        let Some(mut dialogue) =
                            world.get_resource_mut::<crate::ui::DialogueManager>()
                        else {
                            let _ = tx.send(Err("Dialogue manager unavailable".to_string()));
                            return;
                        };
                        let result = dialogue.start(&req.conversation);
                        let active = dialogue.snapshot().active;
                        (result, active)
                    };
                    if result.is_ok() {
                        if let Some(mut ui) = world.get_resource_mut::<crate::ui::UiManager>() {
                            ui.dialogue_active = active;
                        }
                    }
                    let _ = tx.send(result);
                });
            }
            ApiCommand::ChooseDialogue(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let choose_result = {
                        let Some(mut dialogue) =
                            world.get_resource_mut::<crate::ui::DialogueManager>()
                        else {
                            let _ = tx.send(Err("Dialogue manager unavailable".to_string()));
                            return;
                        };
                        dialogue
                            .choose(req.choice)
                            .map(|(node_event, choice_event)| {
                                let active = dialogue.snapshot().active;
                                (active, node_event, choice_event)
                            })
                    };
                    match choose_result {
                        Ok((active, node_event, choice_event)) => {
                            if let Some(mut ui) = world.get_resource_mut::<crate::ui::UiManager>() {
                                ui.dialogue_active = active;
                            }
                            if let Some(mut bus) = world.get_resource_mut::<GameEventBus>() {
                                if let Some(name) = node_event {
                                    bus.emit(
                                        name,
                                        serde_json::json!({ "choice": req.choice }),
                                        None,
                                    );
                                }
                                if let Some(name) = choice_event {
                                    bus.emit(
                                        name,
                                        serde_json::json!({ "choice": req.choice }),
                                        None,
                                    );
                                }
                                bus.emit(
                                    "dialogue_choice",
                                    serde_json::json!({ "choice": req.choice }),
                                    None,
                                );
                            }
                            let _ = tx.send(Ok(()));
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e));
                        }
                    }
                });
            }
            ApiCommand::GetDialogueState(tx) => {
                commands.queue(move |world: &mut World| {
                    let snapshot = world
                        .get_resource::<crate::ui::DialogueManager>()
                        .map(|d| d.snapshot())
                        .unwrap_or_default();
                    let _ = tx.send(snapshot);
                });
            }
            ApiCommand::SetRuntimeState(state, effect, duration, tx) => {
                commands.queue(move |world: &mut World| {
                    let Some(mut runtime) =
                        world.get_resource_mut::<crate::game_runtime::RuntimeState>()
                    else {
                        let _ = tx.send(Err("Runtime state unavailable".to_string()));
                        return;
                    };
                    runtime.set_state(state, effect, duration);
                    let _ = tx.send(Ok(()));
                });
            }
        }
    }
}

#[derive(Default)]
struct EntityInfoExtras {
    health_current: Option<f32>,
    health_max: Option<f32>,
    has_contact: bool,
    has_trigger: bool,
    has_pickup: bool,
    has_projectile: bool,
    has_hitbox: bool,
    has_moving_platform: bool,
    has_animation_controller: bool,
    has_path_follower: bool,
    has_ai_behavior: bool,
    has_particle_emitter: bool,
    ai_behavior: Option<String>,
    ai_state: Option<String>,
    ai_target_id: Option<u64>,
    path_target: Option<Vec2Def>,
    path_len: Option<usize>,
    animation_graph: Option<String>,
    animation_state: Option<String>,
    animation_frame: Option<usize>,
    animation_facing_right: Option<bool>,
}

struct EntityInfoSource<'a> {
    id: u64,
    pos: &'a GamePosition,
    vel: Option<&'a Velocity>,
    collider: Option<&'a Collider>,
    player: Option<&'a Player>,
    gravity: Option<&'a GravityBody>,
    hmover: Option<&'a HorizontalMover>,
    jumper: Option<&'a Jumper>,
    tdmover: Option<&'a TopDownMover>,
    alive: Option<&'a Alive>,
    network_id: Option<&'a NetworkId>,
    tags: Option<&'a Tags>,
    script: Option<&'a LuaScript>,
}

fn build_entity_info(source: EntityInfoSource<'_>, extras: EntityInfoExtras) -> EntityInfo {
    let EntityInfoSource {
        id,
        pos,
        vel,
        collider,
        player,
        gravity,
        hmover,
        jumper,
        tdmover,
        alive,
        network_id,
        tags,
        script,
    } = source;
    let mut components = Vec::new();
    if player.is_some() {
        components.push("Player".into());
    }
    if gravity.is_some() {
        components.push("GravityBody".into());
    }
    if collider.is_some() {
        components.push("Collider".into());
    }
    if hmover.is_some() {
        components.push("HorizontalMover".into());
    }
    if jumper.is_some() {
        components.push("Jumper".into());
    }
    if tdmover.is_some() {
        components.push("TopDownMover".into());
    }
    if extras.health_current.is_some() {
        components.push("Health".into());
    }
    if extras.has_contact {
        components.push("ContactDamage".into());
    }
    if extras.has_trigger {
        components.push("TriggerZone".into());
    }
    if extras.has_pickup {
        components.push("Pickup".into());
    }
    if extras.has_projectile {
        components.push("Projectile".into());
    }
    if extras.has_hitbox {
        components.push("Hitbox".into());
    }
    if extras.has_moving_platform {
        components.push("MovingPlatform".into());
    }
    if extras.has_animation_controller {
        components.push("AnimationController".into());
    }
    if extras.has_path_follower {
        components.push("PathFollower".into());
    }
    if extras.has_ai_behavior {
        components.push("AiBehavior".into());
    }
    if extras.has_particle_emitter {
        components.push("ParticleEmitter".into());
    }

    let mut sorted_tags = tags
        .map(|t| t.0.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    sorted_tags.sort();

    EntityInfo {
        id,
        network_id: network_id.map(|n| n.0),
        x: pos.x,
        y: pos.y,
        vx: vel.map_or(0.0, |v| v.x),
        vy: vel.map_or(0.0, |v| v.y),
        components,
        script: script.map(|s| s.script_name.clone()),
        tags: sorted_tags,
        health: extras.health_current,
        max_health: extras.health_max,
        alive: alive.map(|a| a.0),
        ai_behavior: extras.ai_behavior,
        ai_state: extras.ai_state,
        ai_target_id: extras.ai_target_id,
        path_target: extras.path_target,
        path_len: extras.path_len,
        animation_graph: extras.animation_graph,
        animation_state: extras.animation_state,
        animation_frame: extras.animation_frame,
        animation_facing_right: extras.animation_facing_right,
    }
}

#[derive(SystemParam)]
pub(super) struct ApplyLevelCtx<'w, 's> {
    commands: Commands<'w, 's>,
    physics: Res<'w, GameConfig>,
    headless: Res<'w, HeadlessMode>,
    sprite_assets: Option<Res<'w, SpriteAssets>>,
    player_query: Query<'w, 's, (&'static mut GamePosition, &'static mut Velocity), With<Player>>,
}

pub(super) fn apply_level_change(
    mut pending: ResMut<PendingLevelChange>,
    mut tilemap: ResMut<Tilemap>,
    tile_entities: Query<Entity, With<TileEntity>>,
    mut ctx: ApplyLevelCtx,
) {
    let Some(req) = pending.0.take() else { return };

    tilemap.width = req.width;
    tilemap.height = req.height;
    tilemap.tiles = req.tiles;
    if let Some(spawn) = req.player_spawn {
        tilemap.player_spawn = spawn;
    }
    tilemap.goal = req.goal;

    for entity in tile_entities.iter() {
        ctx.commands.entity(entity).despawn();
    }

    // Only spawn visual tile entities in windowed mode
    if !ctx.headless.0 {
        let ts = ctx.physics.tile_size;
        for y in 0..tilemap.height {
            for x in 0..tilemap.width {
                let tile_type = tilemap.get(x as i32, y as i32);
                if tile_type == TileType::Empty {
                    continue;
                }

                let sprite = if let Some(ref sa) = ctx.sprite_assets {
                    if let Some(handle) = sa.get_tile(tile_type) {
                        Sprite {
                            image: handle.clone(),
                            custom_size: Some(Vec2::new(ts, ts)),
                            ..default()
                        }
                    } else {
                        crate::tilemap::tile_color_sprite(tile_type, ts)
                    }
                } else {
                    crate::tilemap::tile_color_sprite(tile_type, ts)
                };

                ctx.commands.spawn((
                    TileEntity,
                    Tile { tile_type },
                    GridPosition {
                        x: x as i32,
                        y: y as i32,
                    },
                    sprite,
                    Transform::from_xyz(x as f32 * ts + ts / 2.0, y as f32 * ts + ts / 2.0, 0.0),
                ));
            }
        }
    }

    if let Ok((mut pos, mut vel)) = ctx.player_query.get_single_mut() {
        pos.x = tilemap.player_spawn.0;
        pos.y = tilemap.player_spawn.1;
        vel.x = 0.0;
        vel.y = 0.0;
    }
}

pub(super) fn sync_runtime_store_from_ecs(
    runtime_state: Option<Res<crate::game_runtime::RuntimeState>>,
    runtime_store: Option<Res<RuntimeStoreHandle>>,
) {
    let (Some(runtime_state), Some(runtime_store)) = (runtime_state, runtime_store) else {
        return;
    };

    let snapshot = runtime_state.snapshot();
    let mut store = runtime_store.0.write().unwrap();

    let now = std::time::Instant::now();
    let elapsed = std::time::Duration::from_secs_f32(snapshot.time_in_state_seconds.max(0.0));
    store.entered_at = now.checked_sub(elapsed).unwrap_or(now);

    if store.state != snapshot.state {
        let from = store.state.clone();
        let to = snapshot.state.clone();
        let effect = snapshot
            .active_transition
            .as_ref()
            .and_then(|t| t.effect.clone())
            .or(Some("Instant".to_string()));
        let duration = snapshot
            .active_transition
            .as_ref()
            .map(|t| t.duration)
            .unwrap_or(0.0);
        store.state = snapshot.state.clone();
        store.transitions.push(RuntimeTransition {
            from,
            to,
            effect,
            duration,
            at_unix_ms: unix_ms_now(),
        });
        if store.transitions.len() > 256 {
            let excess = store.transitions.len() - 256;
            store.transitions.drain(0..excess);
        }
    }

    store.active_transition = snapshot.active_transition.map(|t| {
        let started_at = now
            .checked_sub(std::time::Duration::from_secs_f32(
                t.elapsed_seconds.max(0.0),
            ))
            .unwrap_or(now);
        RuntimeActiveTransition {
            from: t.from,
            to: t.to,
            effect: t.effect,
            duration: t.duration,
            started_at,
        }
    });
}

pub(super) fn apply_physics_change(
    mut pending: ResMut<PendingPhysicsChange>,
    mut physics: ResMut<GameConfig>,
) {
    if let Some(new_config) = pending.0.take() {
        *physics = new_config;
    }
}

pub(super) fn take_screenshot(
    mut pending: ResMut<PendingScreenshot>,
    headless: Res<HeadlessMode>,
    mut commands: Commands,
) {
    if !pending.0 {
        return;
    }
    pending.0 = false;
    if headless.0 {
        return;
    }
    let output_path = screenshot_path();
    commands
        .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
        .observe(bevy::render::view::screenshot::save_to_disk(output_path));
}

// --- HTTP Handlers ---

#[cfg(test)]
mod tests {
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
                    }],
                    script: None,
                    tags: vec![],
                    is_player: false,
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
                    animations: HashMap::from([(
                        "run".to_string(),
                        SpriteSheetAnimationRequest {
                            frames: vec![4, 5, 6, 7],
                            fps: 12.0,
                            looping: true,
                            next: None,
                            events: Vec::new(),
                        },
                    )]),
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
                animations: HashMap::new(),
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
}
