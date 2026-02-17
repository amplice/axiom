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
    preset_registry: Res<'w, crate::spawn::PresetRegistry>,
    entity_query: Query<'w, 's, EntityQueryItem<'static>>,
    extras_query: Query<'w, 's, ExtrasQueryItem<'static>>,
    pending_real_sim: ResMut<'w, crate::simulation::PendingRealSim>,
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
        preset_registry,
        entity_query,
        extras_query,
        mut pending_real_sim,
    } = ctx;

    // Restore game state after real simulation completes
    if let Some(save) = pending_real_sim.restore_pending.take() {
        apply_loaded_save_data(
            *save,
            &mut pending_level,
            &mut pending_physics,
            &mut script_engine,
            &mut next_network_id,
            &mut commands,
            &entity_query,
        );
    }

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
                invincibility,
                (render_layer, collision_layer, state_machine, inventory),
                (coyote_timer, jump_buffer, grounded),
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
                render_layer: render_layer.map(|r| r.0),
                collision_layer: collision_layer.map(|c| c.layer),
                collision_mask: collision_layer.map(|c| c.mask),
                machine_state: state_machine.map(|sm| sm.current.clone()),
                inventory_slots: inventory.map(|inv| inv.max_slots),
                // Physics diagnostics
                coyote_frames: coyote_timer.map(|c| c.0),
                jump_buffer_frames: jump_buffer.map(|j| j.0),
                invincibility_frames: invincibility.map(|i| i.frames_remaining),
                grounded: grounded.map(|g| g.0),
                // Interaction details
                contact_damage: contact.map(|c| c.amount),
                contact_knockback: contact.map(|c| c.knockback),
                pickup_effect: pickup.map(|p| match &p.effect {
                    crate::components::PickupEffect::Heal(v) => format!("heal:{}", v),
                    crate::components::PickupEffect::ScoreAdd(v) => format!("score:{}", v),
                    crate::components::PickupEffect::Custom(s) => format!("custom:{}", s),
                }),
                trigger_event: trigger.map(|t| t.event_name.clone()),
                projectile_damage: projectile.map(|p| p.damage),
                projectile_speed: projectile.map(|p| p.speed),
                hitbox_active: hitbox.map(|h| h.active),
                hitbox_damage: hitbox.map(|h| h.damage),
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
            // ── Game State & World Queries ──────────────────────────────
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
            // ── Level, Config & Assets ──────────────────────────────────
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
                            path: anim.path.clone(),
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
                    rows: req.rows.max(1),
                    animations: sheet_anims,
                    direction_map: req.direction_map,
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
                        registry.version += 1;
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
            // ── Screenshots ─────────────────────────────────────────────
            ApiCommand::TakeScreenshot(tx) => {
                pending_screenshot.0 = true;
                let _ = tx.send(Ok(()));
            }
            // ── Entity CRUD ─────────────────────────────────────────────
            ApiCommand::SpawnEntity(req, tx) => {
                ensure_entity_id_index!();
                let assigned_id = next_network_id.0;
                let entity = crate::spawn::spawn_entity(&mut commands, &req, &mut next_network_id);
                entity_id_index.insert(assigned_id, entity);
                commands.queue(move |_world: &mut World| {
                    let _ = tx.send(Ok(assigned_id));
                });
            }
            ApiCommand::SpawnPreset(req, tx) => {
                ensure_entity_id_index!();
                let mut spawn_req = crate::spawn::preset_to_request_with_registry(
                    &req.preset, req.x, req.y, Some(&preset_registry),
                );
                if let Err(e) = crate::spawn::apply_preset_config(&mut spawn_req, &req.config) {
                    commands.queue(move |_world: &mut World| {
                        let _ = tx.send(Err(e));
                    });
                    continue;
                }
                if req.script.is_some() {
                    spawn_req.script = req.script;
                }
                if !req.tags.is_empty() {
                    spawn_req.tags = req.tags;
                }
                let assigned_id = next_network_id.0;
                let entity = crate::spawn::spawn_entity(&mut commands, &spawn_req, &mut next_network_id);
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
                            _render_layer,
                            _physics_diag,
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
            ApiCommand::SetEntityPosition(id, x, y, tx) => {
                ensure_entity_id_index!();
                if let Some(entity) = entity_id_index.get(&id).copied() {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut pos) = world.get_mut::<GamePosition>(entity) {
                            pos.x = x;
                            pos.y = y;
                        }
                    });
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("Entity not found".into()));
                }
            }
            ApiCommand::SetEntityVelocity(id, vx, vy, tx) => {
                ensure_entity_id_index!();
                if let Some(entity) = entity_id_index.get(&id).copied() {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut vel) = world.get_mut::<Velocity>(entity) {
                            vel.x = vx;
                            vel.y = vy;
                        }
                    });
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("Entity not found".into()));
                }
            }
            ApiCommand::ModifyEntityTags(id, add_tags, remove_tags, tx) => {
                ensure_entity_id_index!();
                if let Some(entity) = entity_id_index.get(&id).copied() {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut tags) = world.get_mut::<Tags>(entity) {
                            for t in &remove_tags {
                                tags.0.remove(t);
                            }
                            for t in add_tags {
                                tags.0.insert(t);
                            }
                        }
                    });
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("Entity not found".into()));
                }
            }
            ApiCommand::SetEntityHealth(id, current, max, tx) => {
                ensure_entity_id_index!();
                if let Some(entity) = entity_id_index.get(&id).copied() {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut health) = world.get_mut::<Health>(entity) {
                            if let Some(c) = current {
                                health.current = c;
                            }
                            if let Some(m) = max {
                                health.max = m;
                            }
                        }
                    });
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("Entity not found".into()));
                }
            }
            ApiCommand::BulkEntityMutate(req, tx) => {
                ensure_entity_id_index!();
                let filter = &req.filter;
                let mutations = req.mutations;

                // Collect matching entity IDs
                let mut matched_ids: Vec<(u64, Entity)> = Vec::new();
                if let Some(ref ids) = filter.ids {
                    for &id in ids {
                        if let Some(&entity) = entity_id_index.get(&id) {
                            matched_ids.push((id, entity));
                        }
                    }
                } else {
                    for (
                        entity, _pos, _vel, _col, _player, _grav, _hm, _jmp, _tdm,
                        _grounded, alive, network_id, tags, script,
                    ) in entity_query.iter() {
                        let Some(nid) = network_id else { continue; };
                        // Apply filters
                        if let Some(ref tag_filter) = filter.tag {
                            let has = tags.map_or(false, |t| t.0.contains(tag_filter));
                            if !has { continue; }
                        }
                        if let Some(alive_filter) = filter.alive {
                            let is_alive = alive.map_or(true, |a| a.0);
                            if is_alive != alive_filter { continue; }
                        }
                        if let Some(script_filter) = filter.has_script {
                            if script_filter != script.is_some() { continue; }
                        }
                        if let Some(ref comp_filter) = filter.component {
                            let extras = entity_extras(entity);
                            let comps = [
                                extras.has_contact.then_some("ContactDamage"),
                                extras.has_trigger.then_some("TriggerZone"),
                                extras.has_pickup.then_some("Pickup"),
                                extras.has_projectile.then_some("Projectile"),
                                extras.has_hitbox.then_some("Hitbox"),
                                extras.has_moving_platform.then_some("MovingPlatform"),
                                extras.has_ai_behavior.then_some("AiBehavior"),
                            ];
                            if !comps.iter().any(|c| c == &Some(comp_filter.as_str())) { continue; }
                        }
                        if let Some(ref state_filter) = filter.entity_state {
                            let extras = entity_extras(entity);
                            match extras.machine_state {
                                Some(ref s) if s == state_filter => {}
                                _ => continue,
                            }
                        }
                        matched_ids.push((nid.0, entity));
                    }
                }

                let matched = matched_ids.len();
                let mutated = matched;
                commands.queue(move |world: &mut World| {
                    for (_nid, entity) in &matched_ids {
                        if let Some(set_alive) = mutations.alive {
                            if let Some(mut a) = world.get_mut::<Alive>(*entity) {
                                a.0 = set_alive;
                            }
                        }
                        if mutations.health_current.is_some() || mutations.health_max.is_some() {
                            if let Some(mut h) = world.get_mut::<Health>(*entity) {
                                if let Some(c) = mutations.health_current { h.current = c; }
                                if let Some(m) = mutations.health_max { h.max = m; }
                            }
                        }
                        if mutations.add_tags.is_some() || mutations.remove_tags.is_some() {
                            if let Some(mut tags) = world.get_mut::<Tags>(*entity) {
                                if let Some(ref remove) = mutations.remove_tags {
                                    for t in remove { tags.0.remove(t); }
                                }
                                if let Some(ref add) = mutations.add_tags {
                                    for t in add { tags.0.insert(t.clone()); }
                                }
                            }
                        }
                        if mutations.contact_damage.is_some() || mutations.contact_knockback.is_some() {
                            if let Some(mut cd) = world.get_mut::<ContactDamage>(*entity) {
                                if let Some(d) = mutations.contact_damage { cd.amount = d; }
                                if let Some(k) = mutations.contact_knockback { cd.knockback = k; }
                            }
                        }
                        if mutations.hitbox_active.is_some() || mutations.hitbox_damage.is_some() {
                            if let Some(mut hb) = world.get_mut::<Hitbox>(*entity) {
                                if let Some(a) = mutations.hitbox_active { hb.active = a; }
                                if let Some(d) = mutations.hitbox_damage { hb.damage = d; }
                            }
                        }
                    }
                    let _ = tx.send(BulkEntityResult { matched, mutated });
                });
            }
            ApiCommand::SetEntityContactDamage(id, req, tx) => {
                ensure_entity_id_index!();
                if let Some(entity) = entity_id_index.get(&id).copied() {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut cd) = world.get_mut::<ContactDamage>(entity) {
                            if let Some(a) = req.amount { cd.amount = a; }
                            if let Some(c) = req.cooldown_frames { cd.cooldown_frames = c; }
                            if let Some(k) = req.knockback { cd.knockback = k; }
                        }
                    });
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("Entity not found".into()));
                }
            }
            ApiCommand::SetEntityHitbox(id, req, tx) => {
                ensure_entity_id_index!();
                if let Some(entity) = entity_id_index.get(&id).copied() {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut hb) = world.get_mut::<Hitbox>(entity) {
                            if let Some(a) = req.active { hb.active = a; }
                            if let Some(d) = req.damage { hb.damage = d; }
                            if let Some(w) = req.width { hb.width = w; }
                            if let Some(h) = req.height { hb.height = h; }
                        }
                    });
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("Entity not found".into()));
                }
            }
            ApiCommand::QueryTilemap(req, tx) => {
                let ts = physics.tile_size;
                let col1 = (req.x1 / ts).floor() as i32;
                let row1 = (req.y1 / ts).floor() as i32;
                let col2 = (req.x2 / ts).ceil() as i32;
                let row2 = (req.y2 / ts).ceil() as i32;
                let registry = &physics.tile_types;
                let mut solid_tiles = Vec::new();
                let mut total = 0usize;
                for row in row1..=row2 {
                    for col in col1..=col2 {
                        total += 1;
                        let tid = tilemap.tile_id(col, row);
                        if tid == 0 { continue; }
                        let type_name = registry.types.get(tid as usize)
                            .map(|t| t.name.clone())
                            .unwrap_or_else(|| format!("unknown_{}", tid));
                        if registry.is_solid(tid) {
                            solid_tiles.push(TileQueryHit {
                                col,
                                row,
                                tile_id: tid,
                                tile_type: type_name,
                            });
                        }
                    }
                }
                let solid_count = solid_tiles.len();
                let _ = tx.send(TilemapQueryResult { solid_tiles, total_tiles: total, solid_count });
            }
            // ── Events & Performance ────────────────────────────────────
            ApiCommand::GetEvents(tx) => {
                let _ = tx.send(event_bus.recent.iter().cloned().collect());
            }
            ApiCommand::GetPerf(tx) => {
                let _ = tx.send(perf.clone());
            }
            ApiCommand::GetPerfHistory(tx) => {
                let _ = tx.send(perf.history.clone());
            }
            // ── Save/Load ──────────────────────────────────────────────
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
                let result = apply_loaded_save_data(
                    *save,
                    &mut pending_level,
                    &mut pending_physics,
                    &mut script_engine,
                    &mut next_network_id,
                    &mut commands,
                    &entity_query,
                );
                let _ = tx.send(Ok(result));
            }
            // ── Scripting ──────────────────────────────────────────────
            ApiCommand::LoadScript(req, tx) => {
                let script_name = req.name.clone();
                let always_run = req.always_run.unwrap_or(false);
                // Clear old errors for this script on re-upload
                script_errors
                    .entries
                    .retain(|e| e.script_name != script_name);
                let result = crate::scripting::ScriptBackend::load_script(
                    &mut *script_engine,
                    req.name.clone(),
                    req.source,
                    req.global,
                );
                if result.is_ok() {
                    if always_run {
                        script_engine.always_run_scripts.insert(req.name.clone());
                    } else {
                        script_engine.always_run_scripts.remove(&req.name);
                    }
                }
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
            ApiCommand::ClearScriptErrors(tx) => {
                script_errors.entries.clear();
                let _ = tx.send(());
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
            // ── Animation Graphs ────────────────────────────────────────
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
            // ── Debug Overlay ──────────────────────────────────────────
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
            // ── Audio ───────────────────────────────────────────────────
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
            // ── Camera ──────────────────────────────────────────────────
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
                    if let Some(bounds_opt) = req.bounds {
                        match bounds_opt {
                            CameraBoundsOption::Clear(_) => {
                                camera_config.bounds = None;
                            }
                            CameraBoundsOption::Set(bounds) => {
                                camera_config.bounds = Some(crate::camera::CameraBounds {
                                    min_x: bounds.min_x,
                                    max_x: bounds.max_x,
                                    min_y: bounds.min_y,
                                    max_y: bounds.max_y,
                                });
                            }
                        }
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
            // ── UI & Dialogue ──────────────────────────────────────────
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
            // ── Runtime State & Input ──────────────────────────────────
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
            ApiCommand::GetDebugInput(tx) => {
                commands.queue(move |world: &mut World| {
                    let vinput = world
                        .get_resource::<crate::input::VirtualInput>()
                        .cloned()
                        .unwrap_or_default();
                    let mut active: Vec<String> = vinput.active.into_iter().collect();
                    active.sort();
                    let mut just_pressed: Vec<String> = vinput.just_pressed.into_iter().collect();
                    just_pressed.sort();
                    let mut just_released: Vec<String> = vinput.just_released.into_iter().collect();
                    just_released.sort();
                    let mouse = world
                        .get_resource::<crate::input::MouseInput>()
                        .cloned()
                        .unwrap_or_default();
                    let _ = tx.send(serde_json::json!({
                        "active": active,
                        "just_pressed": just_pressed,
                        "just_released": just_released,
                        "mouse": {
                            "screen_x": mouse.screen_x,
                            "screen_y": mouse.screen_y,
                            "world_x": mouse.world_x,
                            "world_y": mouse.world_y,
                            "left": mouse.left,
                            "right": mouse.right,
                            "middle": mouse.middle,
                            "left_just_pressed": mouse.left_just_pressed,
                            "right_just_pressed": mouse.right_just_pressed,
                        }
                    }));
                });
            }
            ApiCommand::GetScriptLogs(tx) => {
                commands.queue(move |world: &mut World| {
                    let entries = world
                        .get_resource::<crate::scripting::ScriptLogBuffer>()
                        .map(|buf| buf.entries.clone())
                        .unwrap_or_default();
                    let _ = tx.send(entries);
                });
            }
            ApiCommand::ClearScriptLogs(tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut buf) =
                        world.get_resource_mut::<crate::scripting::ScriptLogBuffer>()
                    {
                        buf.entries.clear();
                    }
                    let _ = tx.send(());
                });
            }
            ApiCommand::GetGamepadConfig(tx) => {
                commands.queue(move |world: &mut World| {
                    let config = world
                        .get_resource::<crate::input::GamepadConfig>()
                        .cloned()
                        .unwrap_or_default();
                    let connected = world
                        .query::<&bevy::input::gamepad::Gamepad>()
                        .iter(world)
                        .count();
                    let _ = tx.send(GamepadConfigResponse {
                        enabled: config.enabled,
                        deadzone: config.deadzone,
                        connected_count: connected,
                    });
                });
            }
            ApiCommand::SetGamepadConfig(req, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut config) =
                        world.get_resource_mut::<crate::input::GamepadConfig>()
                    {
                        if let Some(enabled) = req.enabled {
                            config.enabled = enabled;
                        }
                        if let Some(deadzone) = req.deadzone {
                            config.deadzone = deadzone.clamp(0.0, 1.0);
                        }
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            // ── Visual Effects (Tweens, Screen, Lighting, Tint, Trail) ─
            ApiCommand::SetEntityTween(entity_id, req, tx) => {
                commands.queue(move |world: &mut World| {
                    let _ = tx.send(crate::tween::apply_tween_command(world, entity_id, req));
                });
            }
            ApiCommand::SetEntityTweenSequence(entity_id, req, tx) => {
                commands.queue(move |world: &mut World| {
                    let steps: Vec<crate::tween::TweenStep> = req.steps.into_iter().map(|s| {
                        crate::tween::TweenStep {
                            property: s.property,
                            to: s.to,
                            from: s.from,
                            duration: s.duration,
                            easing: s.easing,
                        }
                    }).collect();
                    let _ = tx.send(crate::tween::apply_tween_sequence_command(
                        world, entity_id, steps, req.sequence_id,
                    ));
                });
            }
            ApiCommand::TriggerScreenEffect(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let _ = tx.send(crate::screen_effects::trigger_effect_command(world, req));
                });
            }
            ApiCommand::GetScreenState(tx) => {
                commands.queue(move |world: &mut World| {
                    let state = crate::screen_effects::get_screen_state(world);
                    let _ = tx.send(state);
                });
            }
            ApiCommand::SetLightingConfig(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let _ = tx.send(crate::lighting::apply_lighting_config(world, req));
                });
            }
            ApiCommand::GetLightingState(tx) => {
                commands.queue(move |world: &mut World| {
                    let state = crate::lighting::get_lighting_state(world);
                    let _ = tx.send(state);
                });
            }
            ApiCommand::SetEntityTint(entity_id, req, tx) => {
                commands.queue(move |world: &mut World| {
                    let result = apply_tint_to_entity(world, entity_id, req);
                    let _ = tx.send(result);
                });
            }
            ApiCommand::SetEntityTrail(entity_id, req, tx) => {
                commands.queue(move |world: &mut World| {
                    let result = apply_trail_to_entity(world, entity_id, req);
                    let _ = tx.send(result);
                });
            }
            // ── Input Bindings & Day/Night ──────────────────────────────
            ApiCommand::GetInputBindings(tx) => {
                commands.queue(move |world: &mut World| {
                    let bindings = world.get_resource::<crate::input::InputBindings>().cloned().unwrap_or_default();
                    let _ = tx.send(InputBindingsResponse {
                        keyboard: bindings.keyboard,
                        gamepad: bindings.gamepad,
                    });
                });
            }
            ApiCommand::SetInputBindings(req, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut bindings) = world.get_resource_mut::<crate::input::InputBindings>() {
                        if !req.keyboard.is_empty() {
                            bindings.keyboard = req.keyboard;
                        }
                        if !req.gamepad.is_empty() {
                            bindings.gamepad = req.gamepad;
                        }
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::GetDayNight(tx) => {
                commands.queue(move |world: &mut World| {
                    let state = crate::lighting::get_day_night_state(world);
                    let _ = tx.send(state);
                });
            }
            ApiCommand::SetDayNight(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let _ = tx.send(crate::lighting::apply_day_night_config(world, req));
                });
            }
            // ── World Text, State Machine, Tilemap ─────────────────────
            ApiCommand::SpawnWorldText(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let result = spawn_world_text_command(world, req);
                    let _ = tx.send(result);
                });
            }
            ApiCommand::GetEntityState(entity_id, tx) => {
                commands.queue(move |world: &mut World| {
                    let result = get_entity_state_machine(world, entity_id);
                    let _ = tx.send(result);
                });
            }
            ApiCommand::TransitionEntityState(entity_id, req, tx) => {
                commands.queue(move |world: &mut World| {
                    let result = transition_entity_state(world, entity_id, req.state);
                    let _ = tx.send(result);
                });
            }
            ApiCommand::SetAutoTile(req, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut tilemap) = world.get_resource_mut::<Tilemap>() {
                        tilemap.auto_tile_rules = req.rules;
                        tilemap.recalculate_auto_tiles();
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            // ── Parallax, Weather, Inventory, Cutscenes ────────────────
            ApiCommand::GetParallax(tx) => {
                commands.queue(move |world: &mut World| {
                    let config = world.get_resource::<crate::parallax::ParallaxConfig>().cloned().unwrap_or_default();
                    let _ = tx.send(ParallaxResponse { layers: config.layers });
                });
            }
            ApiCommand::SetParallax(req, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut config) = world.get_resource_mut::<crate::parallax::ParallaxConfig>() {
                        config.layers = req.layers;
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::GetWeather(tx) => {
                commands.queue(move |world: &mut World| {
                    let weather = world.get_resource::<crate::weather::WeatherSystem>().cloned().unwrap_or_default();
                    let resp = match weather.active {
                        Some(ref c) => WeatherResponse {
                            active: true,
                            weather_type: Some(match c.weather_type {
                                crate::weather::WeatherType::Rain => "rain".into(),
                                crate::weather::WeatherType::Snow => "snow".into(),
                                crate::weather::WeatherType::Dust => "dust".into(),
                            }),
                            intensity: c.intensity,
                            wind: c.wind,
                        },
                        None => WeatherResponse { active: false, weather_type: None, intensity: 0.0, wind: 0.0 },
                    };
                    let _ = tx.send(resp);
                });
            }
            ApiCommand::SetWeather(req, tx) => {
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut weather: Mut<crate::weather::WeatherSystem>| {
                        world.resource_scope(|_world, mut events: Mut<GameEventBus>| {
                            crate::weather::apply_weather(&mut weather, &mut events, &req.weather_type, req.intensity, req.wind);
                        });
                    });
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::ClearWeather(tx) => {
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut weather: Mut<crate::weather::WeatherSystem>| {
                        world.resource_scope(|_world, mut events: Mut<GameEventBus>| {
                            crate::weather::clear_weather(&mut weather, &mut events);
                        });
                    });
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::DefineItems(req, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut registry) = world.get_resource_mut::<crate::inventory::ItemRegistry>() {
                        for (id, def) in req.items {
                            registry.items.insert(id, def);
                        }
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::GetEntityInventory(entity_id, tx) => {
                commands.queue(move |world: &mut World| {
                    let result = get_entity_inventory(world, entity_id);
                    let _ = tx.send(result);
                });
            }
            ApiCommand::EntityInventoryAction(entity_id, req, tx) => {
                commands.queue(move |world: &mut World| {
                    let result = entity_inventory_action(world, entity_id, req);
                    let _ = tx.send(result);
                });
            }
            ApiCommand::DefineCutscene(req, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut manager) = world.get_resource_mut::<crate::cutscene::CutsceneManager>() {
                        manager.definitions.insert(req.name.clone(), crate::cutscene::CutsceneDef {
                            name: req.name,
                            steps: req.steps,
                        });
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::PlayCutscene(req, tx) => {
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut manager: Mut<crate::cutscene::CutsceneManager>| {
                        world.resource_scope(|world, mut events: Mut<GameEventBus>| {
                            world.resource_scope(|_world, mut runtime: Mut<crate::game_runtime::RuntimeState>| {
                                let result = crate::cutscene::play_cutscene(&mut manager, &mut events, &mut runtime, &req.name);
                                let _ = tx.send(result);
                            });
                        });
                    });
                });
            }
            ApiCommand::StopCutscene(tx) => {
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut manager: Mut<crate::cutscene::CutsceneManager>| {
                        world.resource_scope(|world, mut events: Mut<GameEventBus>| {
                            world.resource_scope(|_world, mut runtime: Mut<crate::game_runtime::RuntimeState>| {
                                crate::cutscene::stop_cutscene(&mut manager, &mut events, &mut runtime);
                            });
                        });
                    });
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::GetCutsceneState(tx) => {
                commands.queue(move |world: &mut World| {
                    let manager = world.get_resource::<crate::cutscene::CutsceneManager>().cloned().unwrap_or_default();
                    let resp = CutsceneStateResponse {
                        playing: manager.active.is_some(),
                        name: manager.active.as_ref().map(|a| a.name.clone()),
                        step_index: manager.active.as_ref().map(|a| a.step_index),
                        total_steps: manager.active.as_ref().and_then(|a| {
                            manager.definitions.get(&a.name).map(|d| d.steps.len())
                        }),
                        defined_cutscenes: manager.definitions.keys().cloned().collect(),
                    };
                    let _ = tx.send(resp);
                });
            }
            // ── Spawn Presets ──────────────────────────────────────────
            ApiCommand::DefinePresets(presets, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut registry) = world.get_resource_mut::<crate::spawn::PresetRegistry>() {
                        for (name, req) in presets {
                            registry.presets.insert(name, req);
                        }
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::ListPresets(tx) => {
                commands.queue(move |world: &mut World| {
                    let presets = world
                        .get_resource::<crate::spawn::PresetRegistry>()
                        .map(|r| r.presets.clone())
                        .unwrap_or_default();
                    let _ = tx.send(presets);
                });
            }
            // ── Tile Layers ───────────────────────────────────────────
            ApiCommand::SetTileLayer(req, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut tilemap) = world.get_resource_mut::<Tilemap>() {
                        let expected_len = tilemap.width * tilemap.height;
                        if req.tiles.len() != expected_len {
                            let _ = tx.send(Err(format!(
                                "Layer tiles length {} doesn't match tilemap {}x{} (expected {})",
                                req.tiles.len(), tilemap.width, tilemap.height, expected_len
                            )));
                            return;
                        }
                        // Update existing or add new
                        if let Some(layer) = tilemap.extra_layers.iter_mut().find(|l| l.name == req.name) {
                            layer.tiles = req.tiles;
                            layer.z_offset = req.z_offset;
                        } else {
                            tilemap.extra_layers.push(crate::tilemap::TileLayer {
                                name: req.name,
                                tiles: req.tiles,
                                z_offset: req.z_offset,
                            });
                        }
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::GetTileLayers(tx) => {
                commands.queue(move |world: &mut World| {
                    let layers = world.get_resource::<Tilemap>()
                        .map(|tm| tm.extra_layers.iter().map(|l| TileLayerInfo {
                            name: l.name.clone(),
                            z_offset: l.z_offset,
                            tile_count: l.tiles.iter().filter(|&&t| t != 0).count(),
                        }).collect())
                        .unwrap_or_default();
                    let _ = tx.send(TileLayersResponse { layers });
                });
            }
            ApiCommand::DeleteTileLayer(name, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut tilemap) = world.get_resource_mut::<Tilemap>() {
                        let before = tilemap.extra_layers.len();
                        tilemap.extra_layers.retain(|l| l.name != name);
                        if tilemap.extra_layers.len() == before {
                            let _ = tx.send(Err(format!("Layer '{}' not found", name)));
                            return;
                        }
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            // ── Entity Pool ──────────────────────────────────────────
            ApiCommand::InitPool(req, tx) => {
                let mut template = crate::spawn::preset_to_request_with_registry(
                    &req.preset, 0.0, 0.0, Some(&preset_registry),
                );
                if let Err(e) = crate::spawn::apply_preset_config(&mut template, &req.config) {
                    commands.queue(move |_world: &mut World| {
                        let _ = tx.send(Err(e));
                    });
                    continue;
                }
                let pool_name = req.pool_name.clone();
                let count = req.count;
                commands.queue(move |world: &mut World| {
                    let mut pool = world.resource_mut::<crate::spawn::EntityPool>();
                    pool.pools.entry(pool_name.clone()).or_insert_with(|| {
                        crate::spawn::PoolBucket {
                            template: template.clone(),
                            available: Vec::new(),
                            active_count: 0,
                        }
                    }).template = template.clone();
                    drop(pool);
                    // Pre-spawn hidden entities using Commands pattern
                    for _ in 0..count {
                        let mut spawn_req = template.clone();
                        spawn_req.x = 0.0;
                        spawn_req.y = -10000.0;
                        let mut next_id = world.resource_mut::<NextNetworkId>();
                        let nid = next_id.0;
                        next_id.0 += 1;
                        let entity = world.spawn((
                            crate::components::GamePosition { x: 0.0, y: -10000.0 },
                            crate::components::NetworkId(crate::components::NetworkId(nid).0),
                            crate::components::Alive(false),
                            crate::components::Pooled { pool_name: pool_name.clone() },
                        )).id();
                        world.resource_mut::<crate::spawn::EntityPool>()
                            .pools.get_mut(&pool_name)
                            .unwrap()
                            .available.push(entity);
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            ApiCommand::AcquireFromPool(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let entity = {
                        let mut pool = world.resource_mut::<crate::spawn::EntityPool>();
                        pool.acquire(&req.pool_name)
                    };
                    match entity {
                        Some(entity) => {
                            if let Some(mut pos) = world.get_mut::<crate::components::GamePosition>(entity) {
                                pos.x = req.x;
                                pos.y = req.y;
                            }
                            if let Some(mut alive) = world.get_mut::<crate::components::Alive>(entity) {
                                alive.0 = true;
                            }
                            if let Some(mut vis) = world.get_mut::<Visibility>(entity) {
                                *vis = Visibility::Inherited;
                            }
                            let nid = world.get::<crate::components::NetworkId>(entity)
                                .map(|n| n.0).unwrap_or(0);
                            let _ = tx.send(Ok(nid));
                        }
                        None => {
                            let _ = tx.send(Err(format!("Pool '{}' is empty", req.pool_name)));
                        }
                    }
                });
            }
            ApiCommand::ReleaseToPool(entity_nid, tx) => {
                commands.queue(move |world: &mut World| {
                    let entity = find_entity_by_network_id(world, entity_nid);
                    match entity {
                        Some(entity) => {
                            let pool_name = world.get::<crate::components::Pooled>(entity)
                                .map(|p| p.pool_name.clone());
                            match pool_name {
                                Some(name) => {
                                    if let Some(mut pos) = world.get_mut::<crate::components::GamePosition>(entity) {
                                        pos.x = 0.0;
                                        pos.y = -10000.0;
                                    }
                                    if let Some(mut alive) = world.get_mut::<crate::components::Alive>(entity) {
                                        alive.0 = false;
                                    }
                                    if let Some(mut vis) = world.get_mut::<Visibility>(entity) {
                                        *vis = Visibility::Hidden;
                                    }
                                    world.resource_mut::<crate::spawn::EntityPool>().release(&name, entity);
                                    let _ = tx.send(Ok(()));
                                }
                                None => {
                                    let _ = tx.send(Err("Entity is not pooled".to_string()));
                                }
                            }
                        }
                        None => {
                            let _ = tx.send(Err(format!("Entity {} not found", entity_nid)));
                        }
                    }
                });
            }
            ApiCommand::GetPoolStatus(tx) => {
                commands.queue(move |world: &mut World| {
                    let pool = world.resource::<crate::spawn::EntityPool>();
                    let pools = pool.pools.iter().map(|(name, bucket)| PoolInfo {
                        name: name.clone(),
                        available: bucket.available.len(),
                        active: bucket.active_count,
                    }).collect();
                    let _ = tx.send(PoolStatusResponse { pools });
                });
            }
            // ── Telemetry ──────────────────────────────────────────────
            ApiCommand::GetTelemetry(tx) => {
                commands.queue(move |world: &mut World| {
                    let telemetry = world.resource::<GameplayTelemetry>();
                    let _ = tx.send(telemetry.clone());
                });
            }
            ApiCommand::ResetTelemetry(tx) => {
                commands.queue(move |world: &mut World| {
                    *world.resource_mut::<GameplayTelemetry>() = GameplayTelemetry::default();
                    let _ = tx.send(());
                });
            }
            // ── World Simulation ───────────────────────────────────────
            ApiCommand::SimulateWorld(req, tx) => {
                let record_interval = req.record_interval.unwrap_or(10).max(1);
                let frames = req.frames.min(3600);
                let inputs = req.inputs.clone();
                let real = req.real;

                if real {
                    // Capture full save state before sim (system-level queries)
                    let save_entities = collect_save_entities(&entity_query, &extras_query);
                    let save_script_snapshot = script_engine.snapshot();
                    let save_config = physics.clone();
                    let save_tilemap = tilemap.clone();
                    let save_next_nid = next_network_id.0;

                    // Real simulation: piggyback on the actual game loop
                    commands.queue(move |world: &mut World| {
                        let saved_runtime_state = world.resource::<crate::game_runtime::RuntimeState>().state.clone();

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

                        let mut pending = world.resource_mut::<crate::simulation::PendingRealSim>();
                        if pending.active.is_some() {
                            let _ = tx.send(Err("A real simulation is already running".to_string()));
                            return;
                        }
                        pending.active = Some(crate::simulation::ActiveRealSim {
                            saved_state: SaveGameData {
                                version: 4,
                                config: save_config,
                                tilemap: save_tilemap,
                                game_state: saved_runtime_state.clone(),
                                next_network_id: save_next_nid,
                                entities: save_entities,
                                scripts: save_script_snapshot.scripts,
                                global_scripts: save_script_snapshot.global_scripts.into_iter().collect(),
                                game_vars: save_script_snapshot.vars,
                                animation_graphs,
                                sprite_sheets,
                                particle_presets,
                            },
                            frames_remaining: frames,
                            frames_total: frames,
                            inputs,
                            record_interval,
                            snapshots: Vec::new(),
                            sender: Some(tx),
                            saved_runtime_state,
                        });
                    });
                } else {
                    // Deterministic simulation (existing behavior)
                    commands.queue(move |world: &mut World| {
                        let tilemap = world.resource::<Tilemap>().clone();
                        let config = world.resource::<GameConfig>().clone();

                        let sim_req = crate::simulation::SimulationRequest {
                            tilemap: None,
                            inputs,
                            max_frames: frames,
                            record_interval,
                            physics: None,
                            goal_position: None,
                            goal_radius: None,
                            initial_game_state: Some("Playing".to_string()),
                            state_transitions: vec![],
                            moving_platforms: vec![],
                            entities: vec![],
                        };

                        let result = crate::simulation::run_simulation(&tilemap, &config, &sim_req);

                        let snapshots: Vec<WorldSimSnapshot> = result.trace.iter().map(|t| {
                            WorldSimSnapshot {
                                frame: t.frame,
                                entities: vec![EntityInfo {
                                    id: 1,
                                    network_id: Some(1),
                                    x: t.x,
                                    y: t.y,
                                    vx: t.vx,
                                    vy: t.vy,
                                    components: vec![],
                                    script: None,
                                    tags: vec![],
                                    health: None,
                                    max_health: None,
                                    alive: Some(true),
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
                                    grounded: Some(t.grounded),
                                    contact_damage: None,
                                    contact_knockback: None,
                                    pickup_effect: None,
                                    trigger_event: None,
                                    projectile_damage: None,
                                    projectile_speed: None,
                                    hitbox_active: None,
                                    hitbox_damage: None,
                                }],
                                vars: serde_json::json!({}),
                            }
                        }).collect();

                        let events: Vec<crate::events::GameEvent> = result.events.iter().map(|e| {
                            crate::events::GameEvent {
                                name: e.event_type.clone(),
                                data: serde_json::json!({
                                    "x": e.x,
                                    "y": e.y,
                                }),
                                frame: e.frame as u64,
                                source_entity: None,
                            }
                        }).collect();

                        let _ = tx.send(Ok(WorldSimResult {
                            frames_run: result.frames_elapsed,
                            snapshots,
                            events,
                            script_errors: vec![],
                            final_vars: serde_json::json!({}),
                        }));
                    });
                }
            }
            // ── Scenario Testing ───────────────────────────────────────
            ApiCommand::RunScenario(req, tx) => {
                commands.queue(move |world: &mut World| {
                    // Apply setup steps
                    for step in &req.setup {
                        match step.action.as_str() {
                            "set_var" => {
                                if let (Some(name), Some(value)) = (
                                    step.params.get("name").and_then(|v| v.as_str()),
                                    step.params.get("value"),
                                ) {
                                    let mut engine = world.resource_mut::<ScriptEngine>();
                                    let mut vars = std::collections::HashMap::new();
                                    vars.insert(name.to_string(), value.clone());
                                    crate::scripting::ScriptBackend::set_vars(
                                        engine.as_mut(),
                                        vars,
                                    );
                                }
                            }
                            "teleport_player" => {
                                let x = step.params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let y = step.params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let mut pq = world.query_filtered::<&mut GamePosition, With<crate::components::Player>>();
                                for mut pos in pq.iter_mut(world) {
                                    pos.x = x;
                                    pos.y = y;
                                }
                            }
                            _ => {}
                        }
                    }

                    // Run simulation frames
                    let tilemap = world.resource::<Tilemap>().clone();
                    let config = world.resource::<GameConfig>().clone();
                    let sim_req = crate::simulation::SimulationRequest {
                        tilemap: None,
                        inputs: req.inputs.clone(),
                        max_frames: req.frames.min(3600),
                        record_interval: req.frames.max(1),
                        physics: None,
                        goal_position: None,
                        goal_radius: None,
                        initial_game_state: None,
                        state_transitions: vec![],
                        moving_platforms: vec![],
                        entities: vec![],
                    };
                    let result = crate::simulation::run_simulation(&tilemap, &config, &sim_req);

                    // Check assertions
                    let engine = world.resource::<ScriptEngine>();
                    let vars_snapshot = engine.snapshot();
                    let final_vars = serde_json::to_value(&vars_snapshot.vars).unwrap_or_default();

                    let mut assertions = Vec::new();
                    let mut all_passed = true;
                    for assertion in &req.assertions {
                        let (passed, actual) = match assertion.check.as_str() {
                            "player_alive" => {
                                let expected = assertion.expected.as_bool().unwrap_or(true);
                                let alive = result.entity_states.first().map(|s| s.alive).unwrap_or(false);
                                (alive == expected, serde_json::json!(alive))
                            }
                            "var_equals" => {
                                let var_name = assertion.expected.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                let expected_val = assertion.expected.get("value").cloned().unwrap_or_default();
                                let actual_val = vars_snapshot.vars.get(var_name).cloned().unwrap_or_default();
                                (actual_val == expected_val, actual_val)
                            }
                            "event_fired" => {
                                let event_name = assertion.expected.as_str().unwrap_or("");
                                let fired = result.events.iter().any(|e| e.event_type == event_name);
                                (fired, serde_json::json!(fired))
                            }
                            "outcome" => {
                                let expected = assertion.expected.as_str().unwrap_or("");
                                (result.outcome == expected, serde_json::json!(result.outcome))
                            }
                            _ => (false, serde_json::json!("unknown assertion")),
                        };
                        if !passed {
                            all_passed = false;
                        }
                        assertions.push(AssertionResult {
                            check: assertion.check.clone(),
                            passed,
                            expected: assertion.expected.clone(),
                            actual,
                        });
                    }

                    let events = result.events.iter().map(|e| crate::events::GameEvent {
                        name: e.event_type.clone(),
                        data: serde_json::json!({"x": e.x, "y": e.y}),
                        frame: e.frame as u64,
                        source_entity: None,
                    }).collect();

                    let _ = tx.send(Ok(ScenarioResult {
                        passed: all_passed,
                        assertions,
                        frames_run: result.frames_elapsed,
                        events,
                        final_vars,
                    }));
                });
            }
            // ── Atomic Build ───────────────────────────────────────────
            ApiCommand::AtomicBuild(req, tx) => {
                let build_req = *req;

                // Optionally validate first
                if let Some(ref constraints) = build_req.validate_first {
                    if !constraints.is_empty() {
                        if let Some(ref tm) = build_req.tilemap {
                            let val_tilemap = Tilemap {
                                width: tm.width,
                                height: tm.height,
                                tiles: tm.tiles.clone(),
                                player_spawn: tm.player_spawn.unwrap_or((16.0, 16.0)),
                                goal: tm.goal,
                                ..Default::default()
                            };
                            let val_config = build_req.config.as_ref().cloned().unwrap_or(physics.clone());
                            let validate_result = crate::constraints::validate(
                                &val_tilemap,
                                &val_config,
                                constraints,
                                &[],
                            );
                            if !validate_result.valid {
                                let _ = tx.send(Ok(BuildResult {
                                    success: false,
                                    import_result: None,
                                    validation: Some(validate_result),
                                    errors: vec!["Validation failed".to_string()],
                                }));
                                continue;
                            }
                        }
                    }
                }

                let config = build_req.config.unwrap_or_else(|| physics.clone());
                let tm = build_req.tilemap.unwrap_or_else(|| SetLevelRequest {
                    width: tilemap.width,
                    height: tilemap.height,
                    tiles: tilemap.tiles.clone(),
                    player_spawn: Some(tilemap.player_spawn),
                    goal: tilemap.goal,
                });

                let mut save_entities = Vec::new();
                let mut entity_count = 0usize;
                for spawn_req in &build_req.entities {
                    entity_count += 1;
                    save_entities.push(SaveEntity {
                        network_id: None,
                        x: spawn_req.x,
                        y: spawn_req.y,
                        vx: 0.0,
                        vy: 0.0,
                        is_player: spawn_req.is_player,
                        components: spawn_req.components.clone(),
                        script: spawn_req.script.clone(),
                        script_state: None,
                        tags: spawn_req.tags.clone(),
                        alive: true,
                        ai_state: None,
                        invincibility_frames: None,
                        path_follower_path: vec![],
                        path_follower_frames_until_recalc: None,
                        inventory_slots: vec![],
                    });
                }

                let scripts_count = build_req.scripts.len();
                let ag_count = build_req.animation_graphs.as_ref().map_or(0, |g| g.len());
                let ss_count = build_req.sprite_sheets.as_ref().map_or(0, |s| s.len());
                let presets_to_register = build_req.presets;

                let save = SaveGameData {
                    version: 4,
                    config,
                    tilemap: Tilemap {
                        width: tm.width,
                        height: tm.height,
                        tiles: tm.tiles.clone(),
                        player_spawn: tm.player_spawn.unwrap_or((16.0, 16.0)),
                        goal: tm.goal,
                        ..Default::default()
                    },
                    game_state: "Playing".to_string(),
                    next_network_id: 1,
                    entities: save_entities,
                    scripts: build_req.scripts,
                    global_scripts: build_req.global_scripts,
                    game_vars: build_req.game_vars.unwrap_or_default(),
                    animation_graphs: build_req.animation_graphs.unwrap_or_default(),
                    sprite_sheets: build_req.sprite_sheets.unwrap_or_default(),
                    particle_presets: std::collections::HashMap::new(),
                };

                apply_loaded_save_data(
                    save,
                    &mut pending_level,
                    &mut pending_physics,
                    &mut script_engine,
                    &mut next_network_id,
                    &mut commands,
                    &entity_query,
                );

                // Register presets after load
                if let Some(presets) = presets_to_register {
                    commands.queue(move |world: &mut World| {
                        let mut preset_reg = world.resource_mut::<crate::spawn::PresetRegistry>();
                        for (name, req) in presets {
                            preset_reg.presets.insert(name, req);
                        }
                    });
                }

                let import_result = ImportResult {
                    entities_spawned: entity_count,
                    scripts_loaded: scripts_count,
                    scripts_failed: vec![],
                    config_applied: true,
                    tilemap_applied: true,
                    animation_graphs: ag_count,
                    sprite_sheets: ss_count,
                    warnings: vec![],
                };

                let _ = tx.send(Ok(BuildResult {
                    success: true,
                    import_result: Some(import_result),
                    validation: None,
                    errors: vec![],
                }));
            }
            // ── Asset Pipeline ─────────────────────────────────────────
            ApiCommand::UploadAsset(req, tx) => {
                let name = req.name.trim().to_string();
                if name.is_empty() {
                    let _ = tx.send(Err("Asset name cannot be empty".into()));
                    continue;
                }
                let data = match base64_decode(&req.data) {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = tx.send(Err(format!("Invalid base64 data: {e}")));
                        continue;
                    }
                };
                let assets_dir = std::env::var("AXIOM_ASSETS_DIR").unwrap_or_else(|_| "assets".to_string());
                let dir_path = std::path::Path::new(&assets_dir);
                let _ = std::fs::create_dir_all(dir_path);
                let file_name = if name.ends_with(".png") { name.clone() } else { format!("{name}.png") };
                let file_path = dir_path.join(&file_name);
                if let Err(e) = std::fs::write(&file_path, &data) {
                    let _ = tx.send(Err(format!("Failed to write asset: {e}")));
                    continue;
                }
                let size_bytes = data.len() as u64;
                let dims = image::image_dimensions(&file_path).ok();
                let _ = tx.send(Ok(AssetInfo {
                    name: file_name,
                    path: file_path.to_string_lossy().to_string(),
                    size_bytes,
                    width: dims.map(|(w, _)| w),
                    height: dims.map(|(_, h)| h),
                }));
            }
            ApiCommand::GenerateAsset(req, tx) => {
                let name = req.name.trim().to_string();
                if name.is_empty() {
                    let _ = tx.send(Err("Asset name cannot be empty".into()));
                    continue;
                }
                let w = req.width.clamp(1, 512);
                let h = req.height.clamp(1, 512);
                let [r, g, b] = req.color;
                let mut img = image::RgbaImage::new(w, h);
                for pixel in img.pixels_mut() {
                    *pixel = image::Rgba([r, g, b, 255]);
                }
                // Draw simple text label if provided (just a centered block for now)
                if let Some(ref label) = req.label {
                    if !label.is_empty() {
                        let text_y = h / 2;
                        let text_w = (label.len() as u32 * 4).min(w);
                        let start_x = (w - text_w) / 2;
                        for x in start_x..(start_x + text_w).min(w) {
                            for dy in 0..4u32 {
                                let y = text_y.saturating_sub(2) + dy;
                                if y < h {
                                    let inv_r = 255u8.wrapping_sub(r);
                                    let inv_g = 255u8.wrapping_sub(g);
                                    let inv_b = 255u8.wrapping_sub(b);
                                    img.put_pixel(x, y, image::Rgba([inv_r, inv_g, inv_b, 255]));
                                }
                            }
                        }
                    }
                }

                let assets_dir = std::env::var("AXIOM_ASSETS_DIR").unwrap_or_else(|_| "assets".to_string());
                let dir_path = std::path::Path::new(&assets_dir);
                let _ = std::fs::create_dir_all(dir_path);
                let file_name = if name.ends_with(".png") { name.clone() } else { format!("{name}.png") };
                let file_path = dir_path.join(&file_name);
                if let Err(e) = img.save(&file_path) {
                    let _ = tx.send(Err(format!("Failed to save generated asset: {e}")));
                    continue;
                }
                let size_bytes = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
                let _ = tx.send(Ok(AssetInfo {
                    name: file_name,
                    path: file_path.to_string_lossy().to_string(),
                    size_bytes,
                    width: Some(w),
                    height: Some(h),
                }));
            }
            ApiCommand::ListAssets(tx) => {
                let assets_dir = std::env::var("AXIOM_ASSETS_DIR").unwrap_or_else(|_| "assets".to_string());
                let dir_path = std::path::Path::new(&assets_dir);
                let mut assets = Vec::new();
                if let Ok(entries) = std::fs::read_dir(dir_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()).map_or(false, |e| {
                            matches!(e.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "bmp" | "gif")
                        }) {
                            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            let dims = image::image_dimensions(&path).ok();
                            assets.push(AssetInfo {
                                name,
                                path: path.to_string_lossy().to_string(),
                                size_bytes,
                                width: dims.map(|(w, _)| w),
                                height: dims.map(|(_, h)| h),
                            });
                        }
                    }
                }
                let _ = tx.send(assets);
            }
            // ── Playtest ─────────────────────────────────────────────────
            ApiCommand::RunPlaytest(req, tx) => {
                let frames = req.frames.min(3600);
                let goal_str = req.goal.clone().unwrap_or_else(|| "survive".to_string());
                let mode_str = req.mode.clone();

                commands.queue(move |world: &mut World| {
                    let saved_runtime_state = world.resource::<crate::game_runtime::RuntimeState>().state.clone();

                    // Auto-detect mode from gravity
                    let config = world.resource::<GameConfig>();
                    let mode = match mode_str.as_deref() {
                        Some("platformer") => crate::simulation::PlaytestMode::Platformer,
                        Some("top_down") => crate::simulation::PlaytestMode::TopDown,
                        _ => {
                            if config.gravity_magnitude() > 0.1 {
                                crate::simulation::PlaytestMode::Platformer
                            } else {
                                crate::simulation::PlaytestMode::TopDown
                            }
                        }
                    };

                    let goal = match goal_str.as_str() {
                        "reach_goal" => crate::simulation::PlaytestGoal::ReachGoal,
                        "explore" => crate::simulation::PlaytestGoal::Explore,
                        _ => crate::simulation::PlaytestGoal::Survive,
                    };

                    // Get player initial state
                    let (px, py, initial_health) = {
                        let mut pq = world.query::<(
                            &GamePosition,
                            Option<&crate::components::Health>,
                            &crate::components::Player,
                        )>();
                        pq.iter(world)
                            .next()
                            .map(|(pos, h, _)| (pos.x, pos.y, h.map_or(10.0, |h| h.current)))
                            .unwrap_or((0.0, 0.0, 10.0))
                    };

                    let mut pending = world.resource_mut::<crate::simulation::PendingPlaytest>();
                    if pending.active.is_some() {
                        let _ = tx.send(Err("A playtest is already running".to_string()));
                        return;
                    }
                    pending.active = Some(crate::simulation::ActivePlaytest {
                        saved_runtime_state,
                        frames_remaining: frames,
                        frames_total: frames,
                        mode,
                        goal,
                        sender: Some(tx),
                        prev_x: px,
                        prev_y: py,
                        stuck_frames: 0,
                        last_jump_frame: 0,
                        direction: 1,
                        explore_phase: 0,
                        visited_cells: std::collections::HashSet::new(),
                        events: Vec::new(),
                        total_damage: 0.0,
                        deaths: Vec::new(),
                        input_counts: std::collections::HashMap::new(),
                        distance_traveled: 0.0,
                        initial_health,
                    });
                });
            }
            // ── Window Config ─────────────────────────────────────────────
            ApiCommand::SetWindowConfig(req, tx) => {
                commands.queue(move |world: &mut World| {
                    if let Some(title) = req.title {
                        let mut window_q = world.query::<&mut bevy::window::Window>();
                        for mut window in window_q.iter_mut(world) {
                            window.title = title.clone();
                        }
                    }
                    if let Some(bg) = req.background {
                        let mut clear_color = world.resource_mut::<ClearColor>();
                        *clear_color = ClearColor(Color::srgb(bg[0], bg[1], bg[2]));
                    }
                    let _ = tx.send(Ok(()));
                });
            }
            // ── Holistic Evaluation ───────────────────────────────────────
            ApiCommand::EvaluateGame(tx) => {
                commands.queue(move |world: &mut World| {
                    let mut issues = Vec::new();

                    // Entity census
                    let mut has_player = false;
                    let mut has_enemies = false;
                    let mut entity_count = 0usize;
                    {
                        let mut q = world.query::<(
                            Option<&crate::components::Player>,
                            Option<&crate::components::Tags>,
                        )>();
                        for (player, tags) in q.iter(world) {
                            entity_count += 1;
                            if player.is_some() { has_player = true; }
                            if let Some(tags) = tags {
                                if tags.0.contains("enemy") { has_enemies = true; }
                            }
                        }
                    }
                    if !has_player { issues.push("No player entity".to_string()); }
                    if !has_enemies { issues.push("No enemy entities".to_string()); }

                    // Script health
                    let script_errors = world.resource::<ScriptErrors>().entries.len();
                    if script_errors > 0 {
                        issues.push(format!("{} script errors", script_errors));
                    }

                    // Script presence
                    let engine = world.resource::<ScriptEngine>();
                    let snapshot = engine.snapshot();
                    let has_scripts = !snapshot.scripts.is_empty();

                    // Game vars
                    let game_vars_count = snapshot.vars.len();

                    // Tilemap quality
                    let tm = world.resource::<Tilemap>();
                    let has_goal = tm.goal.is_some();
                    if !has_goal { issues.push("No goal tile set".to_string()); }
                    let mut tile_types_seen = std::collections::HashSet::new();
                    for &t in &tm.tiles {
                        if t != 0 { tile_types_seen.insert(t); }
                    }
                    let tile_variety = tile_types_seen.len();

                    let overall = if !has_player || script_errors > 2 {
                        "poor"
                    } else if !has_goal || script_errors > 0 || entity_count < 3 {
                        "fair"
                    } else if !has_enemies || tile_variety < 2 {
                        "okay"
                    } else {
                        "good"
                    };

                    let _ = tx.send(EvaluationResult {
                        scores: EvaluationScores {
                            has_player,
                            has_enemies,
                            has_scripts,
                            script_errors,
                            entity_count,
                            tile_variety,
                            has_goal,
                            game_vars_count,
                        },
                        issues,
                        overall: overall.to_string(),
                    });
                });
            }
            ApiCommand::HealthCheck(tx) => {
                commands.queue(move |world: &mut World| {
                    let mut issues = Vec::new();

                    // Player check
                    let mut has_player = false;
                    let mut entity_count = 0usize;
                    {
                        let mut q = world.query::<Option<&crate::components::Player>>();
                        for player in q.iter(world) {
                            entity_count += 1;
                            if player.is_some() { has_player = true; }
                        }
                    }
                    if !has_player { issues.push("No player entity".to_string()); }

                    // Script errors
                    let script_error_count = world.resource::<ScriptErrors>().entries.len();
                    if script_error_count > 0 {
                        issues.push(format!("{} script error(s)", script_error_count));
                    }

                    // Game state
                    let game_state = world
                        .get_resource::<crate::game_runtime::RuntimeState>()
                        .map(|s| s.state.clone())
                        .unwrap_or_else(|| "Unknown".to_string());

                    // Game vars
                    let engine = world.resource::<ScriptEngine>();
                    let snapshot = engine.snapshot();
                    let game_vars_count = snapshot.vars.len();

                    // Tilemap check
                    let tm = world.resource::<Tilemap>();
                    let tilemap_set = tm.width > 0 && tm.height > 0 && !tm.tiles.is_empty();
                    if !tilemap_set { issues.push("No tilemap loaded".to_string()); }

                    let status = if issues.is_empty() {
                        "healthy"
                    } else if script_error_count > 0 || !has_player {
                        "unhealthy"
                    } else {
                        "warning"
                    };

                    let _ = tx.send(HealthCheckResult {
                        status: status.to_string(),
                        has_player,
                        entity_count,
                        script_error_count,
                        game_state,
                        game_vars_count,
                        tilemap_set,
                        issues,
                    });
                });
            }
        }
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    // Simple base64 decoder
    use std::io::Read;
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let table: [u8; 256] = {
        let mut t = [255u8; 256];
        for (i, c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".iter().enumerate() {
            t[*c as usize] = i as u8;
        }
        t[b'=' as usize] = 0;
        t
    };
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = table[bytes[i] as usize];
        let b = table[bytes[i + 1] as usize];
        let c = table[bytes[i + 2] as usize];
        let d = table[bytes[i + 3] as usize];
        if a == 255 || b == 255 {
            return Err("Invalid base64 character".to_string());
        }
        out.push((a << 2) | (b >> 4));
        if bytes[i + 2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if bytes[i + 3] != b'=' {
            out.push((c << 6) | d);
        }
        i += 4;
    }
    let _ = out.as_slice().read(&mut []);
    Ok(out)
}

fn apply_tint_to_entity(world: &mut World, entity_id: u64, req: TintRequest) -> Result<(), String> {
    let entity = find_entity_by_network_id(world, entity_id)
        .ok_or_else(|| format!("Entity {} not found", entity_id))?;
    let tint = crate::components::SpriteColorTint {
        color: req.color,
        flash_color: req.flash_color,
        flash_frames: req.flash_frames,
    };
    world.entity_mut(entity).insert(tint);
    Ok(())
}

fn apply_trail_to_entity(world: &mut World, entity_id: u64, req: Option<TrailRequest>) -> Result<(), String> {
    let entity = find_entity_by_network_id(world, entity_id)
        .ok_or_else(|| format!("Entity {} not found", entity_id))?;
    match req {
        Some(r) => {
            world.entity_mut(entity).insert(crate::trail::TrailEffect {
                interval: r.interval,
                duration: r.duration,
                alpha_start: r.alpha_start,
                alpha_end: r.alpha_end,
                frame_counter: 0,
            });
        }
        None => {
            world.entity_mut(entity).remove::<crate::trail::TrailEffect>();
        }
    }
    Ok(())
}

fn spawn_world_text_command(world: &mut World, req: WorldTextRequest) -> Result<u64, String> {
    let text_id = {
        let mut counter = world.resource_mut::<crate::world_text::WorldTextIdCounter>();
        let id = counter.0;
        counter.0 += 1;
        id
    };
    let z = if req.owner_id.is_some() { 10.5 } else { 10.0 + (-req.y * 0.001) + 0.5 };
    world.spawn((
        crate::world_text::WorldText {
            text_id,
            text: req.text,
            font_size: req.font_size,
            color: req.color,
            offset: Vec2::new(0.0, 0.0),
            owner_entity: req.owner_id,
            duration: req.duration,
            elapsed: 0.0,
            fade: req.fade,
            rise_speed: req.rise_speed,
        },
        Transform::from_xyz(req.x, req.y, z),
    ));
    Ok(text_id)
}

fn get_entity_state_machine(world: &mut World, entity_id: u64) -> Option<StateMachineResponse> {
    let entity = find_entity_by_network_id(world, entity_id)?;
    let sm = world.get::<crate::state_machine::EntityStateMachine>(entity)?;
    Some(StateMachineResponse {
        current: sm.current.clone(),
        previous: sm.previous.clone(),
        entered_at_frame: sm.entered_at_frame,
        states: sm.states.keys().cloned().collect(),
    })
}

fn transition_entity_state(world: &mut World, entity_id: u64, new_state: String) -> Result<(), String> {
    let entity = find_entity_by_network_id(world, entity_id)
        .ok_or_else(|| format!("Entity {} not found", entity_id))?;
    let frame = world.get_resource::<crate::scripting::vm::ScriptFrame>().map(|f| f.frame).unwrap_or(0);
    // Need to get the state machine and event bus
    let sm = world.get_mut::<crate::state_machine::EntityStateMachine>(entity)
        .ok_or_else(|| format!("Entity {} has no state machine", entity_id))?;
    // Clone to avoid borrow issues
    let mut sm_clone = sm.clone();
    let result = {
        let mut events = world.resource_mut::<GameEventBus>();
        sm_clone.transition(&new_state, entity_id, frame, &mut events)
    };
    if result.is_ok() {
        if let Some(mut sm) = world.get_mut::<crate::state_machine::EntityStateMachine>(entity) {
            *sm = sm_clone;
        }
    }
    result
}

fn get_entity_inventory(world: &mut World, entity_id: u64) -> Option<InventoryResponse> {
    let entity = find_entity_by_network_id(world, entity_id)?;
    let inv = world.get::<crate::inventory::Inventory>(entity)?;
    Some(InventoryResponse {
        slots: inv.slots.clone(),
        max_slots: inv.max_slots,
    })
}

fn entity_inventory_action(world: &mut World, entity_id: u64, req: InventoryActionRequest) -> Result<(), String> {
    let entity = find_entity_by_network_id(world, entity_id)
        .ok_or_else(|| format!("Entity {} not found", entity_id))?;

    match req.action.as_str() {
        "add" => {
            let item_id = req.item_id.ok_or("item_id required for add")?;
            let registry = world.get_resource::<crate::inventory::ItemRegistry>().cloned().unwrap_or_default();
            let nid = entity_id;
            let mut inv = world.get_mut::<crate::inventory::Inventory>(entity)
                .ok_or_else(|| format!("Entity {} has no inventory", entity_id))?;
            let added = inv.add_item(&item_id, req.count, &registry);
            let new_total = inv.count_item(&item_id);
            drop(inv);
            let mut events = world.resource_mut::<GameEventBus>();
            events.emit("item_added", serde_json::json!({
                "entity": nid, "item_id": item_id, "count": added, "new_total": new_total
            }), Some(nid));
            Ok(())
        }
        "remove" => {
            let item_id = req.item_id.ok_or("item_id required for remove")?;
            let nid = entity_id;
            let mut inv = world.get_mut::<crate::inventory::Inventory>(entity)
                .ok_or_else(|| format!("Entity {} has no inventory", entity_id))?;
            let removed = inv.remove_item(&item_id, req.count);
            let new_total = inv.count_item(&item_id);
            drop(inv);
            let mut events = world.resource_mut::<GameEventBus>();
            events.emit("item_removed", serde_json::json!({
                "entity": nid, "item_id": item_id, "count": removed, "new_total": new_total
            }), Some(nid));
            Ok(())
        }
        "clear" => {
            let mut inv = world.get_mut::<crate::inventory::Inventory>(entity)
                .ok_or_else(|| format!("Entity {} has no inventory", entity_id))?;
            inv.clear();
            Ok(())
        }
        _ => Err(format!("Unknown inventory action: {}", req.action)),
    }
}

fn find_entity_by_network_id(world: &mut World, network_id: u64) -> Option<Entity> {
    let mut query = world.query::<(Entity, &NetworkId)>();
    query.iter(world).find(|(_, nid)| nid.0 == network_id).map(|(e, _)| e)
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
    render_layer: Option<i32>,
    collision_layer: Option<u16>,
    collision_mask: Option<u16>,
    machine_state: Option<String>,
    inventory_slots: Option<usize>,
    // Physics diagnostics
    coyote_frames: Option<u32>,
    jump_buffer_frames: Option<u32>,
    invincibility_frames: Option<u32>,
    grounded: Option<bool>,
    // Interaction details
    contact_damage: Option<f32>,
    contact_knockback: Option<f32>,
    pickup_effect: Option<String>,
    trigger_event: Option<String>,
    projectile_damage: Option<f32>,
    projectile_speed: Option<f32>,
    hitbox_active: Option<bool>,
    hitbox_damage: Option<f32>,
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
        render_layer: extras.render_layer,
        collision_layer: extras.collision_layer,
        collision_mask: extras.collision_mask,
        machine_state: extras.machine_state,
        inventory_slots: extras.inventory_slots,
        coyote_frames: extras.coyote_frames,
        jump_buffer_frames: extras.jump_buffer_frames,
        invincibility_frames: extras.invincibility_frames,
        grounded: extras.grounded,
        contact_damage: extras.contact_damage,
        contact_knockback: extras.contact_knockback,
        pickup_effect: extras.pickup_effect,
        trigger_event: extras.trigger_event,
        projectile_damage: extras.projectile_damage,
        projectile_speed: extras.projectile_speed,
        hitbox_active: extras.hitbox_active,
        hitbox_damage: extras.hitbox_damage,
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
                let tile_id = tilemap.tile_id(x as i32, y as i32);
                if tile_id == 0 {
                    continue;
                }
                let tile_type = TileType::from_u8(tile_id);

                let sprite = if let Some(ref sa) = ctx.sprite_assets {
                    if let Some(handle) = sa.get_tile(tile_type) {
                        Sprite {
                            image: handle.clone(),
                            custom_size: Some(Vec2::new(ts, ts)),
                            ..default()
                        }
                    } else {
                        crate::tilemap::tile_color_sprite_by_id(tile_id, &ctx.physics.tile_types, ts)
                    }
                } else {
                    crate::tilemap::tile_color_sprite_by_id(tile_id, &ctx.physics.tile_types, ts)
                };

                let (wx, wy) = ctx.physics.tile_mode.grid_to_world(x as f32, y as f32, ts);
                ctx.commands.spawn((
                    TileEntity,
                    Tile { tile_type },
                    GridPosition {
                        x: x as i32,
                        y: y as i32,
                    },
                    sprite,
                    Transform::from_xyz(wx, wy, 0.0),
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
                        facing_direction: 5,
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
                rows: 1,
                animations: HashMap::new(),
                direction_map: None,
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
