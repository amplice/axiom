use super::*;
use crate::scripting::ScriptBackend;

pub(super) type EntityQueryItem<'a> = (
    Entity,
    &'a GamePosition,
    Option<&'a Velocity>,
    Option<&'a Collider>,
    Option<&'a Player>,
    Option<&'a GravityBody>,
    Option<&'a HorizontalMover>,
    Option<&'a Jumper>,
    Option<&'a TopDownMover>,
    Option<&'a Grounded>,
    Option<&'a Alive>,
    Option<&'a NetworkId>,
    Option<&'a Tags>,
    Option<&'a LuaScript>,
);

pub(super) type ExtrasQueryItem<'a> = (
    Entity,
    Option<&'a Health>,
    Option<&'a ContactDamage>,
    Option<&'a TriggerZone>,
    Option<&'a Pickup>,
    Option<&'a Projectile>,
    Option<&'a Hitbox>,
    Option<&'a MovingPlatform>,
    Option<&'a AnimationController>,
    Option<&'a PathFollower>,
    Option<&'a AiBehavior>,
    Option<&'a crate::particles::ParticleEmitter>,
    Option<&'a Invincibility>,
    (
        Option<&'a RenderLayer>,
        Option<&'a CollisionLayer>,
        Option<&'a crate::state_machine::EntityStateMachine>,
        Option<&'a crate::inventory::Inventory>,
        Option<&'a crate::components::Invisible>,
    ),
    (
        Option<&'a CoyoteTimer>,
        Option<&'a JumpBuffer>,
        Option<&'a Grounded>,
    ),
);

pub(super) fn collect_save_entities(
    entity_query: &Query<EntityQueryItem<'_>>,
    extras_query: &Query<ExtrasQueryItem<'_>>,
) -> Vec<SaveEntity> {
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
        let (
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
            _render_layer,
            collision_layer,
            state_machine,
            inventory,
        ) = match extras_query.get(entity) {
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
                (render_layer, collision_layer, state_machine, inventory, _invisible),
                _physics_diag,
            )) => (
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
                render_layer,
                collision_layer,
                state_machine,
                inventory,
            ),
            Err(_) => (
                None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None,
            ),
        };
        let mut components = Vec::new();
        if let Some(c) = collider {
            components.push(ComponentDef::Collider {
                width: c.width,
                height: c.height,
            });
        }
        if gravity.is_some() {
            components.push(ComponentDef::GravityBody);
        }
        if let Some(h) = hmover {
            components.push(ComponentDef::HorizontalMover {
                speed: h.speed,
                left_action: h.left_action.clone(),
                right_action: h.right_action.clone(),
            });
        }
        if let Some(j) = jumper {
            components.push(ComponentDef::Jumper {
                velocity: j.velocity,
                action: j.action.clone(),
                fall_multiplier: j.fall_multiplier,
                variable_height: j.variable_height,
                coyote_frames: j.coyote_frames,
                buffer_frames: j.buffer_frames,
            });
        }
        if let Some(td) = tdmover {
            components.push(ComponentDef::TopDownMover {
                speed: td.speed,
                up_action: td.up_action.clone(),
                down_action: td.down_action.clone(),
                left_action: td.left_action.clone(),
                right_action: td.right_action.clone(),
            });
        }
        if let Some(health) = health {
            components.push(ComponentDef::Health {
                current: health.current,
                max: health.max,
            });
        }
        if let Some(c) = contact {
            components.push(ComponentDef::ContactDamage {
                amount: c.amount,
                cooldown_frames: c.cooldown_frames,
                knockback: c.knockback,
                damage_tag: c.damage_tag.clone(),
            });
        }
        if let Some(tz) = trigger {
            components.push(ComponentDef::TriggerZone {
                radius: tz.radius,
                trigger_tag: tz.trigger_tag.clone(),
                event_name: tz.event_name.clone(),
                one_shot: tz.one_shot,
            });
        }
        if let Some(p) = pickup {
            components.push(ComponentDef::Pickup {
                pickup_tag: p.pickup_tag.clone(),
                effect: pickup_effect_to_def(&p.effect),
            });
        }
        if let Some(p) = projectile {
            components.push(ComponentDef::Projectile {
                speed: p.speed,
                direction: Vec2Def {
                    x: p.direction.x,
                    y: p.direction.y,
                },
                lifetime_frames: p.lifetime_frames,
                damage: p.damage,
                owner_id: p.owner_id,
                damage_tag: p.damage_tag.clone(),
            });
        }
        if let Some(hb) = hitbox {
            components.push(ComponentDef::Hitbox {
                width: hb.width,
                height: hb.height,
                offset: Vec2Def {
                    x: hb.offset.x,
                    y: hb.offset.y,
                },
                active: hb.active,
                damage: hb.damage,
                damage_tag: hb.damage_tag.clone(),
            });
        }
        if let Some(platform) = moving_platform {
            components.push(ComponentDef::MovingPlatform {
                waypoints: platform
                    .waypoints
                    .iter()
                    .map(|p| Vec2Def { x: p.x, y: p.y })
                    .collect(),
                speed: platform.speed,
                loop_mode: platform_loop_mode_to_def(platform.loop_mode),
                pause_frames: platform.pause_frames,
                carry_riders: platform.carry_riders,
                current_waypoint: platform.current_waypoint,
                direction: platform.direction,
            });
        }
        if let Some(anim) = animation_controller {
            components.push(ComponentDef::AnimationController {
                graph: anim.graph.clone(),
                state: anim.state.clone(),
                frame: anim.frame,
                timer: anim.timer,
                speed: anim.speed,
                playing: anim.playing,
                facing_right: anim.facing_right,
                auto_from_velocity: anim.auto_from_velocity,
                facing_direction: anim.facing_direction,
            });
        }
        if let Some(pf) = path_follower {
            components.push(ComponentDef::PathFollower {
                target: Vec2Def {
                    x: pf.target.x,
                    y: pf.target.y,
                },
                recalculate_interval: pf.recalculate_interval,
                path_type: path_type_to_def(pf.path_type),
                speed: pf.speed,
            });
        }
        if let Some(ai) = ai_behavior {
            components.push(ComponentDef::AiBehavior {
                behavior: ai_behavior_to_def(ai),
            });
        }
        if let Some(emitter) = particle_emitter {
            components.push(ComponentDef::ParticleEmitter {
                emitter: emitter.clone(),
            });
        }
        if let Some(rl) = _render_layer {
            if rl.0 != 0 {
                components.push(ComponentDef::RenderLayer { layer: rl.0 });
            }
        }
        if let Some(cl) = collision_layer {
            if cl.layer != 1 || cl.mask != 0xFFFF {
                components.push(ComponentDef::CollisionLayer {
                    layer: cl.layer,
                    mask: cl.mask,
                });
            }
        }
        if let Some(sm) = state_machine {
            components.push(ComponentDef::StateMachine {
                initial: sm.current.clone(),
                states: sm.states.clone(),
            });
        }
        if let Some(inv) = inventory {
            components.push(ComponentDef::Inventory {
                max_slots: inv.max_slots,
            });
        }
        let req = EntitySpawnRequest {
            x: pos.x,
            y: pos.y,
            components,
            script: script.map(|s| s.script_name.clone()),
            tags: tags
                .map(|t| t.0.iter().cloned().collect())
                .unwrap_or_default(),
            is_player: player.is_some(),
            invisible: false,
        };
        entities.push(SaveEntity {
            network_id: network_id.map(|n| n.0),
            x: req.x,
            y: req.y,
            vx: vel.map_or(0.0, |v| v.x),
            vy: vel.map_or(0.0, |v| v.y),
            is_player: req.is_player,
            components: req.components,
            script: req.script,
            script_state: script.map(|s| s.state.clone()),
            tags: req.tags,
            alive: alive.is_none_or(|a| a.0),
            ai_state: ai_behavior.map(|ai| ai_state_to_save(&ai.state)),
            invincibility_frames: invincibility.map(|inv| inv.frames_remaining),
            path_follower_path: path_follower
                .map(|pf| pf.path.iter().map(|p| (p.x, p.y)).collect())
                .unwrap_or_default(),
            path_follower_frames_until_recalc: path_follower.map(|pf| pf.frames_until_recalc),
            inventory_slots: inventory
                .map(|inv| inv.slots.clone())
                .unwrap_or_default(),
        });
    }
    entities
}

pub(super) fn set_script_vars_from_json(
    script_engine: &mut ScriptEngine,
    vars: serde_json::Value,
) -> Result<(), String> {
    let map = serde_json::from_value::<std::collections::HashMap<String, serde_json::Value>>(vars)
        .map_err(|err| format!("Invalid vars payload: {err}"))?;
    ScriptBackend::set_vars(script_engine, map);
    Ok(())
}

pub(super) fn apply_loaded_save_data(
    save: SaveGameData,
    pending_level: &mut PendingLevelChange,
    pending_physics: &mut PendingPhysicsChange,
    script_engine: &mut ScriptEngine,
    next_network_id: &mut NextNetworkId,
    commands: &mut Commands,
    entity_query: &Query<EntityQueryItem<'_>>,
) -> ImportResult {
    let mut warnings = Vec::new();

    let restore_animation_graphs = save.version >= 2 || !save.animation_graphs.is_empty();
    let restore_sprite_sheets = save.version >= 2 || !save.sprite_sheets.is_empty();
    let restore_particle_presets = save.version >= 2 || !save.particle_presets.is_empty();
    let ag_count = save.animation_graphs.len();
    let ss_count = save.sprite_sheets.len();
    let animation_graphs = save.animation_graphs.clone();
    let sprite_sheets_payload = save.sprite_sheets.clone();
    let particle_presets_payload = save.particle_presets.clone();
    let game_state = save.game_state.clone();

    // Track script counts before restore to detect failures
    let scripts_submitted: std::collections::HashSet<String> =
        save.scripts.keys().cloned().collect();
    let scripts_submitted_count = scripts_submitted.len();

    pending_level.0 = Some(SetLevelRequest {
        width: save.tilemap.width,
        height: save.tilemap.height,
        tiles: save.tilemap.tiles.clone(),
        player_spawn: Some(save.tilemap.player_spawn),
        goal: save.tilemap.goal,
    });
    let tilemap_applied = true;

    pending_physics.0 = Some(save.config.clone());
    let config_applied = true;

    script_engine.restore_snapshot(crate::scripting::ScriptRuntimeSnapshot {
        scripts: save.scripts.clone(),
        global_scripts: save.global_scripts.into_iter().collect(),
        vars: save.game_vars.clone(),
    });

    // Determine which scripts failed to load by comparing submitted vs loaded
    let loaded_scripts: std::collections::HashSet<String> = crate::scripting::ScriptBackend::list_scripts(&*script_engine)
        .iter()
        .map(|info| info.name.clone())
        .collect();
    let mut scripts_failed = Vec::new();
    for name in &scripts_submitted {
        if !loaded_scripts.contains(name) {
            scripts_failed.push(name.clone());
        }
    }
    scripts_failed.sort();
    let scripts_loaded = scripts_submitted_count - scripts_failed.len();
    if !scripts_failed.is_empty() {
        warnings.push(format!(
            "{} script(s) failed to load: {}",
            scripts_failed.len(),
            scripts_failed.join(", ")
        ));
    }

    next_network_id.0 = save.next_network_id.max(1);

    commands.queue(move |world: &mut World| {
        if restore_animation_graphs {
            if let Some(mut library) =
                world.get_resource_mut::<crate::animation::AnimationLibrary>()
            {
                library.graphs = animation_graphs;
            }
        }
        if restore_sprite_sheets {
            if let Some(mut registry) =
                world.get_resource_mut::<crate::sprites::SpriteSheetRegistry>()
            {
                registry.sheets = sprite_sheets_payload;
            }
        }
        if restore_particle_presets {
            if let Some(mut library) =
                world.get_resource_mut::<crate::particles::ParticlePresetLibrary>()
            {
                library.presets = particle_presets_payload;
            }
        }
        if let Some(mut runtime_state) =
            world.get_resource_mut::<crate::game_runtime::RuntimeState>()
        {
            runtime_state.set_state(game_state.clone(), Some("Instant".to_string()), 0.0);
        }
    });

    for (entity, _, _, _, _, _, _, _, _, _, _, _, _, _) in entity_query.iter() {
        commands.entity(entity).despawn();
    }
    let entities_spawned = save.entities.len();
    for se in &save.entities {
        let req = EntitySpawnRequest {
            x: se.x,
            y: se.y,
            components: se.components.clone(),
            script: se.script.clone(),
            tags: se.tags.clone(),
            is_player: se.is_player,
            invisible: false,
        };
        let entity = crate::spawn::spawn_entity_with_network_id(
            commands,
            &req,
            next_network_id,
            se.network_id,
        );
        commands
            .entity(entity)
            .insert(Velocity { x: se.vx, y: se.vy });
        if !se.alive {
            commands.entity(entity).insert(Alive(false));
        }
        if let Some(script_state) = se.script_state.clone() {
            commands.queue(move |world: &mut World| {
                if let Some(mut script) = world.get_mut::<LuaScript>(entity) {
                    script.state = script_state;
                }
            });
        }
        if let Some(ai_state) = se.ai_state.clone() {
            commands.queue(move |world: &mut World| {
                if let Some(mut ai) = world.get_mut::<AiBehavior>(entity) {
                    ai.state = save_ai_state_to_runtime(&ai_state);
                }
            });
        }
        if let Some(frames_remaining) = se.invincibility_frames {
            if frames_remaining > 0 {
                commands
                    .entity(entity)
                    .insert(Invincibility { frames_remaining });
            }
        }
        if !se.path_follower_path.is_empty() || se.path_follower_frames_until_recalc.is_some() {
            let path = se
                .path_follower_path
                .iter()
                .map(|(x, y)| Vec2::new(*x, *y))
                .collect::<Vec<_>>();
            let frames_until_recalc = se.path_follower_frames_until_recalc.unwrap_or(0);
            commands.queue(move |world: &mut World| {
                if let Some(mut follower) = world.get_mut::<PathFollower>(entity) {
                    follower.path = path.clone();
                    follower.frames_until_recalc = frames_until_recalc;
                }
            });
        }
        if !se.inventory_slots.is_empty() {
            let slots = se.inventory_slots.clone();
            commands.queue(move |world: &mut World| {
                if let Some(mut inv) = world.get_mut::<crate::inventory::Inventory>(entity) {
                    inv.slots = slots;
                }
            });
        }
    }
    if next_network_id.0 == 0 {
        next_network_id.0 = 1;
    }

    ImportResult {
        entities_spawned,
        scripts_loaded,
        scripts_failed,
        config_applied,
        tilemap_applied,
        animation_graphs: ag_count,
        sprite_sheets: ss_count,
        warnings,
    }
}

fn ai_state_to_save(state: &AiState) -> SaveAiState {
    match state {
        AiState::Idle => SaveAiState::Idle,
        AiState::Patrolling { waypoint_index } => SaveAiState::Patrolling {
            waypoint_index: *waypoint_index,
        },
        AiState::Chasing { target_id } => SaveAiState::Chasing {
            target_id: *target_id,
        },
        AiState::Fleeing { threat_id } => SaveAiState::Fleeing {
            threat_id: *threat_id,
        },
        AiState::Attacking { target_id } => SaveAiState::Attacking {
            target_id: *target_id,
        },
        AiState::Returning => SaveAiState::Returning,
        AiState::Wandering { pause_frames } => SaveAiState::Wandering {
            pause_frames: *pause_frames,
        },
    }
}

fn save_ai_state_to_runtime(state: &SaveAiState) -> AiState {
    match state {
        SaveAiState::Idle => AiState::Idle,
        SaveAiState::Patrolling { waypoint_index } => AiState::Patrolling {
            waypoint_index: *waypoint_index,
        },
        SaveAiState::Chasing { target_id } => AiState::Chasing {
            target_id: *target_id,
        },
        SaveAiState::Fleeing { threat_id } => AiState::Fleeing {
            threat_id: *threat_id,
        },
        SaveAiState::Attacking { target_id } => AiState::Attacking {
            target_id: *target_id,
        },
        SaveAiState::Returning => AiState::Returning,
        SaveAiState::Wandering { pause_frames } => AiState::Wandering {
            pause_frames: *pause_frames,
        },
    }
}

pub(super) fn pickup_effect_to_def(effect: &PickupEffect) -> PickupEffectDef {
    match effect {
        PickupEffect::Heal(amount) => PickupEffectDef::Heal { amount: *amount },
        PickupEffect::ScoreAdd(amount) => PickupEffectDef::ScoreAdd { amount: *amount },
        PickupEffect::Custom(name) => PickupEffectDef::Custom { name: name.clone() },
    }
}

pub(super) fn path_type_to_def(path_type: PathType) -> PathTypeDef {
    match path_type {
        PathType::TopDown => PathTypeDef::TopDown,
        PathType::Platformer => PathTypeDef::Platformer,
    }
}

pub(super) fn platform_loop_mode_to_def(mode: PlatformLoopMode) -> PlatformLoopModeDef {
    match mode {
        PlatformLoopMode::Loop => PlatformLoopModeDef::Loop,
        PlatformLoopMode::PingPong => PlatformLoopModeDef::PingPong,
    }
}

pub(super) fn ai_behavior_to_def(ai: &AiBehavior) -> AiBehaviorDef {
    match &ai.behavior {
        BehaviorType::Patrol { waypoints, speed } => AiBehaviorDef::Patrol {
            waypoints: waypoints
                .iter()
                .map(|p| Vec2Def { x: p.x, y: p.y })
                .collect(),
            speed: *speed,
        },
        BehaviorType::Chase {
            target_tag,
            speed,
            detection_radius,
            give_up_radius,
            require_line_of_sight,
        } => AiBehaviorDef::Chase {
            target_tag: target_tag.clone(),
            speed: *speed,
            detection_radius: *detection_radius,
            give_up_radius: *give_up_radius,
            require_line_of_sight: *require_line_of_sight,
        },
        BehaviorType::Flee {
            threat_tag,
            speed,
            detection_radius,
            give_up_radius,
            require_line_of_sight,
        } => AiBehaviorDef::Flee {
            threat_tag: threat_tag.clone(),
            speed: *speed,
            detection_radius: *detection_radius,
            give_up_radius: *give_up_radius,
            require_line_of_sight: *require_line_of_sight,
        },
        BehaviorType::Guard {
            position,
            radius,
            chase_radius,
            speed,
            target_tag,
            require_line_of_sight,
        } => AiBehaviorDef::Guard {
            position: Vec2Def {
                x: position.x,
                y: position.y,
            },
            radius: *radius,
            chase_radius: *chase_radius,
            speed: *speed,
            target_tag: target_tag.clone(),
            require_line_of_sight: *require_line_of_sight,
        },
        BehaviorType::Wander {
            speed,
            radius,
            pause_frames,
        } => AiBehaviorDef::Wander {
            speed: *speed,
            radius: *radius,
            pause_frames: *pause_frames,
        },
        BehaviorType::Custom(script) => AiBehaviorDef::Custom {
            script: script.clone(),
        },
    }
}
