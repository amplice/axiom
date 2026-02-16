use crate::api::types::{
    AiBehaviorDef, ComponentDef, EntitySpawnRequest, PathTypeDef, PickupEffectDef,
    PlatformLoopModeDef, Vec2Def,
};
use crate::components::*;
use crate::scripting::LuaScript;
use bevy::prelude::*;

/// Spawn an entity from an API request, returns the Entity id
pub fn spawn_entity(
    commands: &mut Commands,
    req: &EntitySpawnRequest,
    next_network_id: &mut NextNetworkId,
) -> Entity {
    spawn_entity_with_network_id(commands, req, next_network_id, None)
}

pub fn spawn_entity_with_network_id(
    commands: &mut Commands,
    req: &EntitySpawnRequest,
    next_network_id: &mut NextNetworkId,
    forced_network_id: Option<u64>,
) -> Entity {
    let is_player = req.is_player;
    let mut tags: std::collections::HashSet<String> = req.tags.iter().cloned().collect();
    if is_player {
        tags.insert("player".to_string());
    }
    let assigned_network_id = forced_network_id.unwrap_or(next_network_id.0).max(1);
    let network_id = NetworkId(assigned_network_id);
    if forced_network_id.is_some() {
        next_network_id.0 = next_network_id.0.max(assigned_network_id.saturating_add(1));
    } else {
        next_network_id.0 = assigned_network_id.saturating_add(1);
    }

    let mut entity = commands.spawn((
        network_id,
        GamePosition { x: req.x, y: req.y },
        Velocity::default(),
        Grounded(false),
        Alive(true),
        Tags(tags),
        Transform::from_xyz(req.x, req.y, 10.0),
    ));

    if is_player {
        entity.insert((
            Player,
            Sprite::from_color(Color::srgb(0.2, 0.4, 0.9), Vec2::new(12.0, 14.0)),
        ));
    }

    let mut has_path_follower = false;
    let mut has_ai_behavior = false;

    for comp in &req.components {
        match comp {
            ComponentDef::Collider { width, height } => {
                entity.insert(Collider {
                    width: *width,
                    height: *height,
                });
            }
            ComponentDef::GravityBody => {
                entity.insert(GravityBody);
            }
            ComponentDef::HorizontalMover {
                speed,
                left_action,
                right_action,
            } => {
                entity.insert(HorizontalMover {
                    speed: *speed,
                    left_action: left_action.clone(),
                    right_action: right_action.clone(),
                });
            }
            ComponentDef::Jumper {
                velocity,
                action,
                fall_multiplier,
                variable_height,
                coyote_frames,
                buffer_frames,
            } => {
                entity.insert((
                    Jumper {
                        velocity: *velocity,
                        action: action.clone(),
                        fall_multiplier: *fall_multiplier,
                        variable_height: *variable_height,
                        coyote_frames: *coyote_frames,
                        buffer_frames: *buffer_frames,
                    },
                    CoyoteTimer::default(),
                    JumpBuffer::default(),
                ));
            }
            ComponentDef::TopDownMover {
                speed,
                up_action,
                down_action,
                left_action,
                right_action,
            } => {
                entity.insert(TopDownMover {
                    speed: *speed,
                    up_action: up_action.clone(),
                    down_action: down_action.clone(),
                    left_action: left_action.clone(),
                    right_action: right_action.clone(),
                });
            }
            ComponentDef::Health { current, max } => {
                entity.insert(Health {
                    current: *current,
                    max: *max,
                });
            }
            ComponentDef::ContactDamage {
                amount,
                cooldown_frames,
                knockback,
                damage_tag,
            } => {
                entity.insert(ContactDamage {
                    amount: *amount,
                    cooldown_frames: *cooldown_frames,
                    knockback: *knockback,
                    damage_tag: damage_tag.clone(),
                });
            }
            ComponentDef::TriggerZone {
                radius,
                trigger_tag,
                event_name,
                one_shot,
            } => {
                entity.insert(TriggerZone {
                    radius: *radius,
                    trigger_tag: trigger_tag.clone(),
                    event_name: event_name.clone(),
                    one_shot: *one_shot,
                });
            }
            ComponentDef::Pickup { pickup_tag, effect } => {
                let effect = match effect {
                    PickupEffectDef::Heal { amount } => PickupEffect::Heal(*amount),
                    PickupEffectDef::ScoreAdd { amount } => PickupEffect::ScoreAdd(*amount),
                    PickupEffectDef::Custom { name } => PickupEffect::Custom(name.clone()),
                };
                entity.insert(Pickup {
                    pickup_tag: pickup_tag.clone(),
                    effect,
                });
            }
            ComponentDef::Projectile {
                speed,
                direction,
                lifetime_frames,
                damage,
                owner_id,
                damage_tag,
            } => {
                entity.insert(Projectile {
                    speed: *speed,
                    direction: Vec2::new(direction.x, direction.y),
                    lifetime_frames: *lifetime_frames,
                    damage: *damage,
                    owner_id: *owner_id,
                    damage_tag: damage_tag.clone(),
                });
            }
            ComponentDef::Hitbox {
                width,
                height,
                offset,
                active,
                damage,
                damage_tag,
            } => {
                entity.insert(Hitbox {
                    width: *width,
                    height: *height,
                    offset: Vec2::new(offset.x, offset.y),
                    active: *active,
                    damage: *damage,
                    damage_tag: damage_tag.clone(),
                });
            }
            ComponentDef::MovingPlatform {
                waypoints,
                speed,
                loop_mode,
                pause_frames,
                carry_riders,
                current_waypoint,
                direction,
            } => {
                let loop_mode = match loop_mode {
                    PlatformLoopModeDef::Loop => PlatformLoopMode::Loop,
                    PlatformLoopModeDef::PingPong => PlatformLoopMode::PingPong,
                };
                let mut direction = *direction;
                if direction == 0 {
                    direction = 1;
                }
                entity.insert(MovingPlatform {
                    waypoints: waypoints.iter().map(|p| Vec2::new(p.x, p.y)).collect(),
                    speed: *speed,
                    loop_mode,
                    current_waypoint: *current_waypoint,
                    direction,
                    pause_frames: *pause_frames,
                    pause_timer: 0,
                    carry_riders: *carry_riders,
                });
            }
            ComponentDef::AnimationController {
                graph,
                state,
                frame,
                timer,
                speed,
                playing,
                facing_right,
                auto_from_velocity,
            } => {
                entity.insert(AnimationController {
                    graph: graph.clone(),
                    state: state.clone(),
                    frame: *frame,
                    timer: *timer,
                    speed: *speed,
                    playing: *playing,
                    facing_right: *facing_right,
                    auto_from_velocity: *auto_from_velocity,
                });
            }
            ComponentDef::PathFollower {
                target,
                recalculate_interval,
                path_type,
                speed,
            } => {
                let path_type = match path_type {
                    PathTypeDef::TopDown => PathType::TopDown,
                    PathTypeDef::Platformer => PathType::Platformer,
                };
                entity.insert(PathFollower::new(
                    Vec2::new(target.x, target.y),
                    path_type,
                    *recalculate_interval,
                    *speed,
                ));
                has_path_follower = true;
            }
            ComponentDef::AiBehavior { behavior } => {
                let behavior = match behavior {
                    AiBehaviorDef::Patrol { waypoints, speed } => BehaviorType::Patrol {
                        waypoints: waypoints.iter().map(|p| Vec2::new(p.x, p.y)).collect(),
                        speed: *speed,
                    },
                    AiBehaviorDef::Chase {
                        target_tag,
                        speed,
                        detection_radius,
                        give_up_radius,
                        require_line_of_sight,
                    } => BehaviorType::Chase {
                        target_tag: target_tag.clone(),
                        speed: *speed,
                        detection_radius: *detection_radius,
                        give_up_radius: *give_up_radius,
                        require_line_of_sight: *require_line_of_sight,
                    },
                    AiBehaviorDef::Flee {
                        threat_tag,
                        speed,
                        detection_radius,
                        give_up_radius,
                        require_line_of_sight,
                    } => BehaviorType::Flee {
                        threat_tag: threat_tag.clone(),
                        speed: *speed,
                        detection_radius: *detection_radius,
                        give_up_radius: *give_up_radius,
                        require_line_of_sight: *require_line_of_sight,
                    },
                    AiBehaviorDef::Guard {
                        position,
                        radius,
                        chase_radius,
                        speed,
                        target_tag,
                        require_line_of_sight,
                    } => BehaviorType::Guard {
                        position: Vec2::new(position.x, position.y),
                        radius: *radius,
                        chase_radius: *chase_radius,
                        speed: *speed,
                        target_tag: target_tag.clone(),
                        require_line_of_sight: *require_line_of_sight,
                    },
                    AiBehaviorDef::Wander {
                        speed,
                        radius,
                        pause_frames,
                    } => BehaviorType::Wander {
                        speed: *speed,
                        radius: *radius,
                        pause_frames: *pause_frames,
                    },
                    AiBehaviorDef::Custom { script } => BehaviorType::Custom(script.clone()),
                };
                entity.insert(AiBehavior {
                    behavior,
                    state: AiState::Idle,
                });
                has_ai_behavior = true;
            }
            ComponentDef::ParticleEmitter { emitter } => {
                entity.insert(emitter.clone());
            }
        }
    }

    // Backward compatibility for older tag-based enemy spawns.
    let has_patrol_tag = req.tags.iter().any(|t| t == "patrol");
    let has_chaser_tag = req.tags.iter().any(|t| t == "chaser");

    if has_patrol_tag && !(has_path_follower || has_ai_behavior) {
        entity.insert(PathFollower::new(
            Vec2::new(req.x + 48.0, req.y),
            PathType::Platformer,
            20,
            120.0,
        ));
        entity.insert(AiBehavior {
            behavior: BehaviorType::Patrol {
                waypoints: vec![
                    Vec2::new(req.x - 48.0, req.y),
                    Vec2::new(req.x + 48.0, req.y),
                ],
                speed: 120.0,
            },
            state: AiState::Patrolling { waypoint_index: 0 },
        });
    } else if has_chaser_tag && !(has_path_follower || has_ai_behavior) {
        entity.insert(PathFollower::new(
            Vec2::new(req.x, req.y),
            PathType::TopDown,
            10,
            140.0,
        ));
        entity.insert(AiBehavior {
            behavior: BehaviorType::Chase {
                target_tag: "player".into(),
                speed: 140.0,
                detection_radius: 260.0,
                give_up_radius: 420.0,
                require_line_of_sight: false,
            },
            state: AiState::Idle,
        });
    }

    if let Some(script_name) = &req.script {
        entity.insert(LuaScript {
            script_name: script_name.clone(),
            state: serde_json::json!({}),
            enabled: true,
            error_streak: 0,
            disabled_reason: None,
        });
    }

    entity.id()
}

pub fn apply_preset_config(
    req: &mut EntitySpawnRequest,
    config: &serde_json::Value,
) -> Result<(), String> {
    if config.is_null() {
        return Ok(());
    }
    let Some(obj) = config.as_object() else {
        return Err("preset config must be a JSON object".to_string());
    };

    if let Some(script) = obj.get("script").and_then(|v| v.as_str()) {
        req.script = Some(script.to_string());
    }
    if let Some(tags) = obj.get("tags").and_then(|v| v.as_array()) {
        let mut out = Vec::with_capacity(tags.len());
        for item in tags {
            let Some(tag) = item.as_str() else {
                return Err("preset config tags must be string values".to_string());
            };
            out.push(tag.to_string());
        }
        req.tags = out;
    }
    if let Some(is_player) = obj.get("is_player").and_then(|v| v.as_bool()) {
        req.is_player = is_player;
    }
    if let Some(x) = obj.get("x").and_then(|v| v.as_f64()) {
        req.x = x as f32;
    }
    if let Some(y) = obj.get("y").and_then(|v| v.as_f64()) {
        req.y = y as f32;
    }
    if let Some(components) = obj.get("components") {
        req.components = serde_json::from_value::<Vec<ComponentDef>>(components.clone())
            .map_err(|e| format!("Invalid preset components override: {e}"))?;
    }

    if let Some(speed) = obj.get("speed").and_then(|v| v.as_f64()) {
        apply_speed_override(&mut req.components, speed as f32);
    }
    if let Some(health) = obj.get("health").and_then(|v| v.as_f64()) {
        apply_health_override(&mut req.components, health as f32);
    }
    if let Some(amount) = obj.get("contact_damage").and_then(|v| v.as_f64()) {
        apply_contact_damage_override(&mut req.components, amount as f32);
    }
    if let Some(waypoints_val) = obj.get("waypoints") {
        let waypoints = parse_vec2_list(waypoints_val, "waypoints")?;
        for comp in &mut req.components {
            if let ComponentDef::AiBehavior {
                behavior: AiBehaviorDef::Patrol { waypoints: wp, .. },
            } = comp
            {
                *wp = waypoints.clone();
            }
            if let ComponentDef::PathFollower { target, .. } = comp {
                if let Some(first) = waypoints.first() {
                    *target = first.clone();
                }
            }
            if let ComponentDef::MovingPlatform {
                waypoints: platform_waypoints,
                current_waypoint,
                ..
            } = comp
            {
                *platform_waypoints = waypoints.clone();
                if *current_waypoint >= platform_waypoints.len() {
                    *current_waypoint = 0;
                }
            }
        }
    }
    if let Some(radius) = obj.get("detection_radius").and_then(|v| v.as_f64()) {
        for comp in &mut req.components {
            if let ComponentDef::AiBehavior {
                behavior:
                    AiBehaviorDef::Chase {
                        detection_radius, ..
                    }
                    | AiBehaviorDef::Flee {
                        detection_radius, ..
                    },
            } = comp
            {
                *detection_radius = radius as f32;
            }
        }
    }
    if let Some(radius) = obj.get("give_up_radius").and_then(|v| v.as_f64()) {
        for comp in &mut req.components {
            if let ComponentDef::AiBehavior {
                behavior:
                    AiBehaviorDef::Chase { give_up_radius, .. }
                    | AiBehaviorDef::Flee { give_up_radius, .. },
            } = comp
            {
                *give_up_radius = radius as f32;
            }
        }
    }
    if let Some(radius) = obj.get("chase_radius").and_then(|v| v.as_f64()) {
        for comp in &mut req.components {
            if let ComponentDef::AiBehavior {
                behavior: AiBehaviorDef::Guard { chase_radius, .. },
            } = comp
            {
                *chase_radius = radius as f32;
            }
        }
    }
    if let Some(radius) = obj
        .get("radius")
        .or_else(|| obj.get("guard_radius"))
        .and_then(|v| v.as_f64())
    {
        for comp in &mut req.components {
            match comp {
                ComponentDef::AiBehavior {
                    behavior:
                        AiBehaviorDef::Guard {
                            radius: guard_radius,
                            ..
                        },
                } => *guard_radius = radius as f32,
                ComponentDef::AiBehavior {
                    behavior:
                        AiBehaviorDef::Wander {
                            radius: wander_radius,
                            ..
                        },
                } => *wander_radius = radius as f32,
                _ => {}
            }
        }
    }
    if let Some(target_tag) = obj.get("target_tag").and_then(|v| v.as_str()) {
        for comp in &mut req.components {
            if let ComponentDef::AiBehavior {
                behavior:
                    AiBehaviorDef::Chase {
                        target_tag: tag, ..
                    }
                    | AiBehaviorDef::Guard {
                        target_tag: tag, ..
                    },
            } = comp
            {
                *tag = target_tag.to_string();
            }
        }
    }
    if let Some(threat_tag) = obj.get("threat_tag").and_then(|v| v.as_str()) {
        for comp in &mut req.components {
            if let ComponentDef::AiBehavior {
                behavior:
                    AiBehaviorDef::Flee {
                        threat_tag: tag, ..
                    },
            } = comp
            {
                *tag = threat_tag.to_string();
            }
        }
    }
    if let Some(require_los) = obj.get("require_line_of_sight").and_then(|v| v.as_bool()) {
        for comp in &mut req.components {
            if let ComponentDef::AiBehavior {
                behavior:
                    AiBehaviorDef::Chase {
                        require_line_of_sight,
                        ..
                    }
                    | AiBehaviorDef::Flee {
                        require_line_of_sight,
                        ..
                    }
                    | AiBehaviorDef::Guard {
                        require_line_of_sight,
                        ..
                    },
            } = comp
            {
                *require_line_of_sight = require_los;
            }
        }
    }
    if let Some(pause_frames) = obj.get("pause_frames").and_then(|v| v.as_u64()) {
        for comp in &mut req.components {
            if let ComponentDef::AiBehavior {
                behavior:
                    AiBehaviorDef::Wander {
                        pause_frames: pf, ..
                    },
            } = comp
            {
                *pf = pause_frames as u32;
            }
        }
    }
    if let Some(recalc) = obj
        .get("recalculate_interval")
        .or_else(|| obj.get("path_recalculate_interval"))
        .and_then(|v| v.as_u64())
    {
        for comp in &mut req.components {
            if let ComponentDef::PathFollower {
                recalculate_interval,
                ..
            } = comp
            {
                *recalculate_interval = (recalc as u32).max(1);
            }
        }
    }
    if let Some(path_type) = obj.get("path_type").and_then(|v| v.as_str()) {
        let parsed = match path_type {
            "top_down" => Some(PathTypeDef::TopDown),
            "platformer" => Some(PathTypeDef::Platformer),
            _ => None,
        };
        if let Some(path_type) = parsed {
            for comp in &mut req.components {
                if let ComponentDef::PathFollower { path_type: p, .. } = comp {
                    *p = path_type;
                }
            }
        }
    }
    if let Some(loop_mode) = obj.get("loop_mode").and_then(|v| v.as_str()) {
        let parsed = match loop_mode {
            "loop" => Some(PlatformLoopModeDef::Loop),
            "ping_pong" | "pingpong" => Some(PlatformLoopModeDef::PingPong),
            _ => None,
        };
        if let Some(loop_mode) = parsed {
            for comp in &mut req.components {
                if let ComponentDef::MovingPlatform { loop_mode: lm, .. } = comp {
                    *lm = loop_mode;
                }
            }
        }
    }
    if let Some(carry) = obj.get("carry_riders").and_then(|v| v.as_bool()) {
        for comp in &mut req.components {
            if let ComponentDef::MovingPlatform { carry_riders, .. } = comp {
                *carry_riders = carry;
            }
        }
    }
    if let Some(pause_frames) = obj.get("pause_frames").and_then(|v| v.as_u64()) {
        for comp in &mut req.components {
            if let ComponentDef::MovingPlatform {
                pause_frames: pf, ..
            } = comp
            {
                *pf = pause_frames as u32;
            }
        }
    }
    if let Some(direction) = obj.get("direction").and_then(|v| v.as_i64()) {
        let direction = if direction < 0 { -1 } else { 1 };
        for comp in &mut req.components {
            if let ComponentDef::MovingPlatform { direction: d, .. } = comp {
                *d = direction;
            }
        }
    }

    Ok(())
}

fn apply_speed_override(components: &mut [ComponentDef], speed: f32) {
    for comp in components {
        match comp {
            ComponentDef::HorizontalMover { speed: s, .. }
            | ComponentDef::TopDownMover { speed: s, .. }
            | ComponentDef::PathFollower { speed: s, .. }
            | ComponentDef::MovingPlatform { speed: s, .. } => *s = speed,
            ComponentDef::AnimationController { speed: s, .. } => *s = speed,
            ComponentDef::AiBehavior { behavior } => match behavior {
                AiBehaviorDef::Patrol { speed: s, .. }
                | AiBehaviorDef::Chase { speed: s, .. }
                | AiBehaviorDef::Flee { speed: s, .. }
                | AiBehaviorDef::Guard { speed: s, .. }
                | AiBehaviorDef::Wander { speed: s, .. } => *s = speed,
                AiBehaviorDef::Custom { .. } => {}
            },
            _ => {}
        }
    }
}

fn apply_health_override(components: &mut Vec<ComponentDef>, health: f32) {
    let mut found = false;
    for comp in components.iter_mut() {
        if let ComponentDef::Health { current, max } = comp {
            *current = health;
            *max = (*max).max(health);
            found = true;
        }
    }
    if !found {
        components.push(ComponentDef::Health {
            current: health,
            max: health,
        });
    }
}

fn apply_contact_damage_override(components: &mut Vec<ComponentDef>, amount: f32) {
    let mut found = false;
    for comp in components.iter_mut() {
        if let ComponentDef::ContactDamage { amount: a, .. } = comp {
            *a = amount;
            found = true;
        }
    }
    if !found {
        components.push(ComponentDef::ContactDamage {
            amount,
            cooldown_frames: 20,
            knockback: 80.0,
            damage_tag: "player".into(),
        });
    }
}

fn parse_vec2_list(value: &serde_json::Value, label: &str) -> Result<Vec<Vec2Def>, String> {
    let Some(items) = value.as_array() else {
        return Err(format!("{label} must be an array"));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if let Some(arr) = item.as_array() {
            if arr.len() != 2 {
                return Err(format!("{label} points must be [x, y] or {{x, y}}"));
            }
            let x = arr[0]
                .as_f64()
                .ok_or_else(|| format!("{label}[].x must be a number"))? as f32;
            let y = arr[1]
                .as_f64()
                .ok_or_else(|| format!("{label}[].y must be a number"))? as f32;
            out.push(Vec2Def { x, y });
            continue;
        }
        let Some(obj) = item.as_object() else {
            return Err(format!("{label} points must be [x, y] or {{x, y}}"));
        };
        let x = obj
            .get("x")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("{label}[].x must be a number"))? as f32;
        let y = obj
            .get("y")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("{label}[].y must be a number"))? as f32;
        out.push(Vec2Def { x, y });
    }
    Ok(out)
}

/// Convert a preset name to an EntitySpawnRequest
pub fn preset_to_request(preset: &str, x: f32, y: f32) -> EntitySpawnRequest {
    match preset {
        "platformer_player" => EntitySpawnRequest {
            x,
            y,
            is_player: true,
            script: None,
            tags: vec!["player".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 12.0,
                    height: 14.0,
                },
                ComponentDef::GravityBody,
                ComponentDef::HorizontalMover {
                    speed: 200.0,
                    left_action: "left".into(),
                    right_action: "right".into(),
                },
                ComponentDef::Jumper {
                    velocity: 400.0,
                    action: "jump".into(),
                    fall_multiplier: 1.5,
                    variable_height: true,
                    coyote_frames: 5,
                    buffer_frames: 4,
                },
                ComponentDef::Health {
                    current: 3.0,
                    max: 3.0,
                },
                ComponentDef::AnimationController {
                    graph: "samurai_player".into(),
                    state: "idle".into(),
                    frame: 0,
                    timer: 0.0,
                    speed: 1.0,
                    playing: true,
                    facing_right: true,
                    auto_from_velocity: true,
                },
            ],
        },
        "top_down_player" => EntitySpawnRequest {
            x,
            y,
            is_player: true,
            script: None,
            tags: vec!["player".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 12.0,
                    height: 14.0,
                },
                ComponentDef::TopDownMover {
                    speed: 200.0,
                    up_action: "up".into(),
                    down_action: "down".into(),
                    left_action: "left".into(),
                    right_action: "right".into(),
                },
                ComponentDef::Health {
                    current: 3.0,
                    max: 3.0,
                },
                ComponentDef::AnimationController {
                    graph: "basic_actor".into(),
                    state: "idle".into(),
                    frame: 0,
                    timer: 0.0,
                    speed: 1.0,
                    playing: true,
                    facing_right: true,
                    auto_from_velocity: true,
                },
            ],
        },
        "patrol_enemy" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["enemy".into(), "patrol".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 12.0,
                    height: 14.0,
                },
                ComponentDef::GravityBody,
                ComponentDef::HorizontalMover {
                    speed: 120.0,
                    left_action: "left".into(),
                    right_action: "right".into(),
                },
                ComponentDef::PathFollower {
                    target: Vec2Def { x: x + 48.0, y },
                    recalculate_interval: 20,
                    path_type: PathTypeDef::Platformer,
                    speed: 120.0,
                },
                ComponentDef::AiBehavior {
                    behavior: AiBehaviorDef::Patrol {
                        waypoints: vec![Vec2Def { x: x - 48.0, y }, Vec2Def { x: x + 48.0, y }],
                        speed: 120.0,
                    },
                },
                ComponentDef::Health {
                    current: 2.0,
                    max: 2.0,
                },
                ComponentDef::ContactDamage {
                    amount: 1.0,
                    cooldown_frames: 20,
                    knockback: 110.0,
                    damage_tag: "player".into(),
                },
                ComponentDef::AnimationController {
                    graph: "basic_actor".into(),
                    state: "idle".into(),
                    frame: 0,
                    timer: 0.0,
                    speed: 1.0,
                    playing: true,
                    facing_right: true,
                    auto_from_velocity: true,
                },
            ],
        },
        "chase_enemy" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["enemy".into(), "chaser".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 12.0,
                    height: 14.0,
                },
                ComponentDef::TopDownMover {
                    speed: 140.0,
                    up_action: "up".into(),
                    down_action: "down".into(),
                    left_action: "left".into(),
                    right_action: "right".into(),
                },
                ComponentDef::PathFollower {
                    target: Vec2Def { x, y },
                    recalculate_interval: 10,
                    path_type: PathTypeDef::TopDown,
                    speed: 140.0,
                },
                ComponentDef::AiBehavior {
                    behavior: AiBehaviorDef::Chase {
                        target_tag: "player".into(),
                        speed: 140.0,
                        detection_radius: 260.0,
                        give_up_radius: 420.0,
                        require_line_of_sight: true,
                    },
                },
                ComponentDef::Health {
                    current: 2.0,
                    max: 2.0,
                },
                ComponentDef::ContactDamage {
                    amount: 1.0,
                    cooldown_frames: 18,
                    knockback: 90.0,
                    damage_tag: "player".into(),
                },
                ComponentDef::AnimationController {
                    graph: "basic_actor".into(),
                    state: "idle".into(),
                    frame: 0,
                    timer: 0.0,
                    speed: 1.0,
                    playing: true,
                    facing_right: true,
                    auto_from_velocity: true,
                },
            ],
        },
        "guard_enemy" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["enemy".into(), "guard".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 12.0,
                    height: 14.0,
                },
                ComponentDef::TopDownMover {
                    speed: 130.0,
                    up_action: "up".into(),
                    down_action: "down".into(),
                    left_action: "left".into(),
                    right_action: "right".into(),
                },
                ComponentDef::PathFollower {
                    target: Vec2Def { x, y },
                    recalculate_interval: 10,
                    path_type: PathTypeDef::TopDown,
                    speed: 130.0,
                },
                ComponentDef::AiBehavior {
                    behavior: AiBehaviorDef::Guard {
                        position: Vec2Def { x, y },
                        radius: 48.0,
                        chase_radius: 220.0,
                        speed: 130.0,
                        target_tag: "player".into(),
                        require_line_of_sight: true,
                    },
                },
                ComponentDef::Health {
                    current: 3.0,
                    max: 3.0,
                },
                ComponentDef::ContactDamage {
                    amount: 1.0,
                    cooldown_frames: 20,
                    knockback: 90.0,
                    damage_tag: "player".into(),
                },
            ],
        },
        "turret" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["enemy".into(), "turret".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 14.0,
                    height: 14.0,
                },
                ComponentDef::PathFollower {
                    target: Vec2Def { x, y },
                    recalculate_interval: 20,
                    path_type: PathTypeDef::TopDown,
                    speed: 0.0,
                },
                ComponentDef::AiBehavior {
                    behavior: AiBehaviorDef::Guard {
                        position: Vec2Def { x, y },
                        radius: 8.0,
                        chase_radius: 260.0,
                        speed: 0.0,
                        target_tag: "player".into(),
                        require_line_of_sight: true,
                    },
                },
                ComponentDef::Health {
                    current: 3.0,
                    max: 3.0,
                },
                ComponentDef::ContactDamage {
                    amount: 1.0,
                    cooldown_frames: 30,
                    knockback: 70.0,
                    damage_tag: "player".into(),
                },
            ],
        },
        "flying_enemy" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["enemy".into(), "flying".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 11.0,
                    height: 11.0,
                },
                ComponentDef::PathFollower {
                    target: Vec2Def { x, y },
                    recalculate_interval: 8,
                    path_type: PathTypeDef::TopDown,
                    speed: 165.0,
                },
                ComponentDef::AiBehavior {
                    behavior: AiBehaviorDef::Chase {
                        target_tag: "player".into(),
                        speed: 165.0,
                        detection_radius: 320.0,
                        give_up_radius: 500.0,
                        require_line_of_sight: false,
                    },
                },
                ComponentDef::Health {
                    current: 1.0,
                    max: 1.0,
                },
                ComponentDef::ContactDamage {
                    amount: 1.0,
                    cooldown_frames: 14,
                    knockback: 60.0,
                    damage_tag: "player".into(),
                },
            ],
        },
        "boss" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["enemy".into(), "boss".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 22.0,
                    height: 26.0,
                },
                ComponentDef::GravityBody,
                ComponentDef::HorizontalMover {
                    speed: 95.0,
                    left_action: "left".into(),
                    right_action: "right".into(),
                },
                ComponentDef::PathFollower {
                    target: Vec2Def { x, y },
                    recalculate_interval: 8,
                    path_type: PathTypeDef::TopDown,
                    speed: 95.0,
                },
                ComponentDef::AiBehavior {
                    behavior: AiBehaviorDef::Chase {
                        target_tag: "player".into(),
                        speed: 95.0,
                        detection_radius: 460.0,
                        give_up_radius: 760.0,
                        require_line_of_sight: false,
                    },
                },
                ComponentDef::Health {
                    current: 20.0,
                    max: 20.0,
                },
                ComponentDef::ContactDamage {
                    amount: 2.0,
                    cooldown_frames: 12,
                    knockback: 140.0,
                    damage_tag: "player".into(),
                },
                ComponentDef::Hitbox {
                    width: 28.0,
                    height: 24.0,
                    offset: Vec2Def { x: 0.0, y: 0.0 },
                    active: false,
                    damage: 2.0,
                    damage_tag: "player".into(),
                },
            ],
        },
        "health_pickup" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["pickup".into(), "health".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 10.0,
                    height: 10.0,
                },
                ComponentDef::Pickup {
                    pickup_tag: "player".into(),
                    effect: PickupEffectDef::Heal { amount: 1.0 },
                },
            ],
        },
        "projectile" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["projectile".into()],
            components: vec![ComponentDef::Collider {
                width: 4.0,
                height: 4.0,
            }],
        },
        "moving_platform" => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec!["platform".into(), "moving_platform".into()],
            components: vec![
                ComponentDef::Collider {
                    width: 32.0,
                    height: 8.0,
                },
                ComponentDef::MovingPlatform {
                    waypoints: vec![Vec2Def { x: x - 32.0, y }, Vec2Def { x: x + 32.0, y }],
                    speed: 80.0,
                    loop_mode: PlatformLoopModeDef::PingPong,
                    pause_frames: 0,
                    carry_riders: true,
                    current_waypoint: 0,
                    direction: 1,
                },
            ],
        },
        _ => EntitySpawnRequest {
            x,
            y,
            is_player: false,
            script: None,
            tags: vec![],
            components: vec![ComponentDef::Collider {
                width: 12.0,
                height: 14.0,
            }],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_config_updates_patrol_waypoints_and_speed() {
        let mut req = preset_to_request("patrol_enemy", 100.0, 40.0);
        let cfg = serde_json::json!({
            "speed": 180.0,
            "waypoints": [[90.0, 40.0], [150.0, 40.0]]
        });
        apply_preset_config(&mut req, &cfg).expect("config should apply");

        let mut saw_patrol = false;
        let mut saw_path_target = false;
        for c in &req.components {
            if let ComponentDef::AiBehavior {
                behavior: AiBehaviorDef::Patrol { waypoints, speed },
            } = c
            {
                saw_patrol = true;
                assert_eq!(*speed, 180.0);
                assert_eq!(waypoints.len(), 2);
                assert_eq!(waypoints[0].x, 90.0);
            }
            if let ComponentDef::PathFollower { target, speed, .. } = c {
                saw_path_target = true;
                assert_eq!(*speed, 180.0);
                assert_eq!(target.x, 90.0);
            }
        }
        assert!(saw_patrol);
        assert!(saw_path_target);
    }

    #[test]
    fn preset_config_updates_combat_values() {
        let mut req = preset_to_request("chase_enemy", 32.0, 48.0);
        let cfg = serde_json::json!({
            "health": 7.0,
            "contact_damage": 2.5,
            "detection_radius": 300.0,
            "require_line_of_sight": false
        });
        apply_preset_config(&mut req, &cfg).expect("config should apply");

        let mut health_ok = false;
        let mut damage_ok = false;
        let mut ai_ok = false;
        for c in &req.components {
            match c {
                ComponentDef::Health { current, max } => {
                    health_ok = true;
                    assert_eq!(*current, 7.0);
                    assert!(*max >= 7.0);
                }
                ComponentDef::ContactDamage { amount, .. } => {
                    damage_ok = true;
                    assert_eq!(*amount, 2.5);
                }
                ComponentDef::AiBehavior {
                    behavior:
                        AiBehaviorDef::Chase {
                            detection_radius,
                            require_line_of_sight,
                            ..
                        },
                } => {
                    ai_ok = true;
                    assert_eq!(*detection_radius, 300.0);
                    assert!(!*require_line_of_sight);
                }
                _ => {}
            }
        }
        assert!(health_ok && damage_ok && ai_ok);
    }

    #[test]
    fn moving_platform_preset_supports_speed_and_waypoint_overrides() {
        let mut req = preset_to_request("moving_platform", 100.0, 24.0);
        let cfg = serde_json::json!({
            "speed": 140.0,
            "waypoints": [[80.0, 24.0], [132.0, 24.0]],
            "loop_mode": "ping_pong",
            "pause_frames": 12,
            "carry_riders": false,
            "direction": -1
        });
        apply_preset_config(&mut req, &cfg).expect("config should apply");

        let mut saw_platform = false;
        for c in &req.components {
            if let ComponentDef::MovingPlatform {
                waypoints,
                speed,
                loop_mode,
                pause_frames,
                carry_riders,
                direction,
                ..
            } = c
            {
                saw_platform = true;
                assert_eq!(*speed, 140.0);
                assert_eq!(waypoints.len(), 2);
                assert_eq!(waypoints[0].x, 80.0);
                assert_eq!(waypoints[1].x, 132.0);
                assert!(matches!(loop_mode, PlatformLoopModeDef::PingPong));
                assert_eq!(*pause_frames, 12);
                assert!(!*carry_riders);
                assert_eq!(*direction, -1);
            }
        }
        assert!(saw_platform);
    }
}
