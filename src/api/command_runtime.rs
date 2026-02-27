mod entity_helpers;
mod systems;
#[cfg(test)]
mod tests;

use entity_helpers::{
    apply_tint_to_entity, apply_trail_to_entity, build_entity_info,
    entity_inventory_action as entity_inventory_action_world,
    find_entity_by_network_id,
    get_entity_inventory as get_entity_inventory_world,
    get_entity_state_machine, spawn_world_text_command,
    transition_entity_state as transition_entity_state_world,
    EntityInfoExtras, EntityInfoSource,
};
pub(super) use systems::{
    apply_level_change, apply_physics_change, screenshot_captured_observer,
    sync_runtime_store_from_ecs, take_screenshot,
};

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
                (render_layer, collision_layer, state_machine, inventory, invisible),
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
                visible: Some(invisible.is_none()),
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
                    anchor_y: req.anchor_y,
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
                let dir = screenshot_dir(physics.screenshot_path.as_deref());
                let path = next_screenshot_path(&dir);
                let path_str = path.to_string_lossy().to_string();
                pending_screenshot.requested = true;
                pending_screenshot.path = Some(path);
                let _ = tx.send(Ok(path_str));
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
                    let result = transition_entity_state_world(world, entity_id, req.state);
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
            ApiCommand::RegisterTerrainMaterial(req, tx) => {
                commands.queue(move |world: &mut World| {
                    let do_autotile = req.autotile;
                    // Register tileset for this tile_id in TileTypeRegistry
                    if let Some(mut config) = world.get_resource_mut::<crate::components::GameConfig>() {
                        let registry = &mut config.tile_types;
                        // Grow registry if needed
                        while registry.types.len() <= req.tile_id as usize {
                            registry.types.push(crate::components::TileTypeDef {
                                name: format!("auto_{}", registry.types.len()),
                                flags: 0,
                                friction: 1.0,
                                color: None,
                                tileset: None,
                            });
                        }
                        let def = &mut registry.types[req.tile_id as usize];
                        def.name = req.name.clone();
                        let columns = if do_autotile {
                            req.columns.unwrap_or(13)
                        } else {
                            1
                        };
                        def.tileset = Some(crate::components::TilesetDef {
                            path: req.atlas.clone(),
                            tile_width: req.frame_width,
                            tile_height: req.frame_height,
                            columns,
                            rows: 1,
                            variant_map: None,
                        });
                    }

                    // Register the autotile rule (only if autotile is enabled)
                    if do_autotile {
                        let max_cols = req.columns.unwrap_or(13) as u16;
                        let table = crate::tilemap::build_mask_to_frame_table(
                            req.slots.as_deref(),
                            max_cols,
                        );
                        let rule = crate::api::types::MaterialAutoTileRule {
                            name: req.name.clone(),
                            base_tile_id: req.tile_id,
                            mask_to_frame: table,
                        };
                        if let Some(mut tilemap) = world.get_resource_mut::<Tilemap>() {
                            tilemap.material_auto_tile_rules.insert(req.name.clone(), rule);
                            tilemap.recalculate_auto_tiles();
                        }
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
                    let result = get_entity_inventory_world(world, entity_id);
                    let _ = tx.send(result);
                });
            }
            ApiCommand::EntityInventoryAction(entity_id, req, tx) => {
                commands.queue(move |world: &mut World| {
                    let result = entity_inventory_action_world(world, entity_id, req);
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
                                    visible: None,
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
                    extra_layers: tilemap.extra_layers.clone(),
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

                // Asset validation warnings
                let mut build_warnings: Vec<String> = Vec::new();
                let assets_dir = resolve_assets_dir(config.asset_path.as_deref());
                let assets_path = std::path::Path::new(&assets_dir);

                // Validate sprite sheet paths
                if let Some(ref sheets) = build_req.sprite_sheets {
                    for (name, sheet) in sheets {
                        let resolved = crate::sprites::resolve_sprite_asset_path(&sheet.path, config.asset_path.as_deref());
                        let full_path = if std::path::Path::new(&resolved).is_absolute() {
                            std::path::PathBuf::from(&resolved)
                        } else {
                            assets_path.join(&resolved)
                        };
                        if !full_path.exists() {
                            build_warnings.push(format!("Sprite sheet '{}': file not found at '{}'", name, sheet.path));
                        }
                        // Check animation path overrides
                        for (anim_name, anim) in &sheet.animations {
                            if let Some(ref anim_path) = anim.path {
                                let resolved_anim = crate::sprites::resolve_sprite_asset_path(anim_path, config.asset_path.as_deref());
                                let full_anim_path = if std::path::Path::new(&resolved_anim).is_absolute() {
                                    std::path::PathBuf::from(&resolved_anim)
                                } else {
                                    assets_path.join(&resolved_anim)
                                };
                                if !full_anim_path.exists() {
                                    build_warnings.push(format!("Sprite sheet '{}' animation '{}': file not found at '{}'", name, anim_name, anim_path));
                                }
                            }
                        }
                    }
                }

                // Validate animation graph references in entities
                if let Some(ref graphs) = build_req.animation_graphs {
                    for spawn_req in &build_req.entities {
                        for comp in &spawn_req.components {
                            if let ComponentDef::AnimationController { ref graph, .. } = comp {
                                if !graphs.contains_key(graph) {
                                    let tag_hint = if spawn_req.tags.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" (tags: {})", spawn_req.tags.join(", "))
                                    };
                                    build_warnings.push(format!("Entity at ({}, {}){}: animation graph '{}' not found in build request", spawn_req.x, spawn_req.y, tag_hint, graph));
                                }
                            }
                        }
                    }
                }

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
                    warnings: build_warnings,
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
                let assets_dir = resolve_assets_dir(physics.asset_path.as_deref());
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

                let assets_dir = resolve_assets_dir(physics.asset_path.as_deref());
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
                let assets_dir = resolve_assets_dir(physics.asset_path.as_deref());
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
            // ── Diagnose ─────────────────────────────────────────────
            ApiCommand::Diagnose(tx) => {
                commands.queue(move |world: &mut World| {
                    let mut entity_count = 0usize;
                    let mut diagnosed: Vec<EntityDiagnosis> = Vec::new();

                    let mut q = world.query::<(
                        Entity,
                        &NetworkId,
                        Option<&Tags>,
                        Option<&Collider>,
                        Option<&CircleCollider>,
                        Option<&ContactDamage>,
                        Option<&Pickup>,
                        Option<&SolidBody>,
                        Option<&CollisionLayer>,
                        Option<&HorizontalMover>,
                        Option<&TopDownMover>,
                        Option<&Jumper>,
                        Option<&GravityBody>,
                        Option<&Projectile>,
                    )>();

                    for (_entity, network_id, tags, collider, circle_collider, contact_damage, pickup, solid_body, collision_layer, horizontal_mover, top_down_mover, jumper, gravity_body, projectile) in q.iter(world) {
                        entity_count += 1;
                        let has_collider = collider.is_some();
                        let has_circle = circle_collider.is_some();
                        let has_either = has_collider || has_circle;
                        let mut issues = Vec::new();

                        if contact_damage.is_some() && !has_either {
                            issues.push(ComponentIssue {
                                component: "ContactDamage".to_string(),
                                severity: "error".to_string(),
                                message: "requires Collider or CircleCollider for overlap detection".to_string(),
                                missing: vec!["Collider".to_string(), "CircleCollider".to_string()],
                            });
                        }
                        if pickup.is_some() && !has_either {
                            issues.push(ComponentIssue {
                                component: "Pickup".to_string(),
                                severity: "error".to_string(),
                                message: "requires Collider or CircleCollider to be collected".to_string(),
                                missing: vec!["Collider".to_string(), "CircleCollider".to_string()],
                            });
                        }
                        if solid_body.is_some() && !has_either {
                            issues.push(ComponentIssue {
                                component: "SolidBody".to_string(),
                                severity: "error".to_string(),
                                message: "requires Collider or CircleCollider for push-back".to_string(),
                                missing: vec!["Collider".to_string(), "CircleCollider".to_string()],
                            });
                        }
                        if collision_layer.is_some() && !has_either {
                            issues.push(ComponentIssue {
                                component: "CollisionLayer".to_string(),
                                severity: "error".to_string(),
                                message: "requires Collider or CircleCollider to filter".to_string(),
                                missing: vec!["Collider".to_string(), "CircleCollider".to_string()],
                            });
                        }
                        if horizontal_mover.is_some() && !has_collider {
                            issues.push(ComponentIssue {
                                component: "HorizontalMover".to_string(),
                                severity: "error".to_string(),
                                message: "requires Collider for tilemap collision".to_string(),
                                missing: vec!["Collider".to_string()],
                            });
                        }
                        if top_down_mover.is_some() && !has_collider {
                            issues.push(ComponentIssue {
                                component: "TopDownMover".to_string(),
                                severity: "error".to_string(),
                                message: "requires Collider for tilemap collision".to_string(),
                                missing: vec!["Collider".to_string()],
                            });
                        }
                        if jumper.is_some() {
                            if gravity_body.is_none() {
                                issues.push(ComponentIssue {
                                    component: "Jumper".to_string(),
                                    severity: "error".to_string(),
                                    message: "requires GravityBody for falling".to_string(),
                                    missing: vec!["GravityBody".to_string()],
                                });
                            }
                            if !has_collider {
                                issues.push(ComponentIssue {
                                    component: "Jumper".to_string(),
                                    severity: "error".to_string(),
                                    message: "requires Collider for ground detection".to_string(),
                                    missing: vec!["Collider".to_string()],
                                });
                            }
                        }
                        if projectile.is_some() && !has_either {
                            issues.push(ComponentIssue {
                                component: "Projectile".to_string(),
                                severity: "warning".to_string(),
                                message: "has no collider; will use default 4x4 hitbox".to_string(),
                                missing: vec!["Collider".to_string(), "CircleCollider".to_string()],
                            });
                        }

                        if !issues.is_empty() {
                            let tag_list = tags.map(|t| t.0.iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
                            diagnosed.push(EntityDiagnosis {
                                id: network_id.0,
                                tags: tag_list,
                                issues,
                            });
                        }
                    }

                    let issues_count = diagnosed.iter().map(|d| d.issues.len()).sum();
                    let _ = tx.send(DiagnoseResult {
                        entity_count,
                        issues_count,
                        entities: diagnosed,
                    });
                });
            }
        }
    }
}
