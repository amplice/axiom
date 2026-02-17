use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::components::{GameConfig, NextNetworkId};
use crate::scripting::ScriptBackend;
use crate::tilemap::Tilemap;

const EMBEDDED_GAME_DATA: &str =
    include_str!(concat!(env!("OUT_DIR"), "/axiom_embedded_game_data.json"));

pub struct WebBootstrapPlugin;

impl Plugin for WebBootstrapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EmbeddedEntityRestore>()
            .add_systems(PreStartup, apply_embedded_game_data)
            .add_systems(PostStartup, restore_embedded_entities_after_startup);
    }
}

#[derive(Resource, Default)]
struct EmbeddedEntityRestore {
    entities: Vec<EmbeddedSaveEntity>,
    applied: bool,
}

fn apply_embedded_game_data(ctx: EmbeddedBootstrapCtx<'_>) {
    let EmbeddedBootstrapCtx {
        mut tilemap,
        mut config,
        mut next_network_id,
        mut script_engine,
        mut runtime_state,
        mut animation_library,
        mut sprite_registry,
        mut particle_library,
        mut embedded_restore,
    } = ctx;
    let Some(snapshot) = parse_embedded_snapshot(EMBEDDED_GAME_DATA) else {
        return;
    };
    apply_embedded_runtime_snapshot(
        snapshot,
        EmbeddedRuntimeTargets {
            tilemap: &mut tilemap,
            config: &mut config,
            next_network_id: next_network_id.as_deref_mut(),
            script_engine: script_engine.as_deref_mut(),
            runtime_state: runtime_state.as_deref_mut(),
            animation_library: animation_library.as_deref_mut(),
            sprite_registry: sprite_registry.as_deref_mut(),
            particle_library: particle_library.as_deref_mut(),
            embedded_restore: Some(&mut *embedded_restore),
        },
    );
    info!("[Axiom export] Loaded embedded game data snapshot");
}

#[derive(SystemParam)]
struct EmbeddedBootstrapCtx<'w> {
    tilemap: ResMut<'w, Tilemap>,
    config: ResMut<'w, GameConfig>,
    next_network_id: Option<ResMut<'w, NextNetworkId>>,
    script_engine: Option<ResMut<'w, crate::scripting::ScriptEngine>>,
    runtime_state: Option<ResMut<'w, crate::game_runtime::RuntimeState>>,
    animation_library: Option<ResMut<'w, crate::animation::AnimationLibrary>>,
    sprite_registry: Option<ResMut<'w, crate::sprites::SpriteSheetRegistry>>,
    particle_library: Option<ResMut<'w, crate::particles::ParticlePresetLibrary>>,
    embedded_restore: ResMut<'w, EmbeddedEntityRestore>,
}

#[derive(Clone)]
struct EmbeddedSnapshot {
    config: GameConfig,
    tilemap: Tilemap,
    next_network_id: Option<u64>,
    game_state: Option<String>,
    scripts: std::collections::HashMap<String, String>,
    global_scripts: std::collections::HashSet<String>,
    game_vars: std::collections::HashMap<String, serde_json::Value>,
    animation_graphs: std::collections::HashMap<String, crate::animation::AnimationGraphDef>,
    sprite_sheets: std::collections::HashMap<String, crate::sprites::SpriteSheetDef>,
    particle_presets: std::collections::HashMap<String, crate::particles::ParticlePresetDef>,
    entities: Vec<EmbeddedSaveEntity>,
}

fn default_alive_true() -> bool {
    true
}

#[derive(Clone, serde::Deserialize)]
struct EmbeddedSaveEntity {
    #[serde(default)]
    network_id: Option<u64>,
    x: f32,
    y: f32,
    #[serde(default)]
    vx: f32,
    #[serde(default)]
    vy: f32,
    #[serde(default)]
    is_player: bool,
    #[serde(default)]
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    components: Vec<serde_json::Value>,
    #[serde(default)]
    script: Option<String>,
    #[serde(default)]
    script_state: Option<serde_json::Value>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_alive_true")]
    alive: bool,
    #[serde(default)]
    ai_state: Option<EmbeddedSaveAiState>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EmbeddedSaveAiState {
    Idle,
    Patrolling { waypoint_index: usize },
    Chasing { target_id: u64 },
    Fleeing { threat_id: u64 },
    Attacking { target_id: u64 },
    Returning,
    Wandering { pause_frames: u32 },
}

impl EmbeddedSaveAiState {
    fn into_runtime(self) -> crate::components::AiState {
        match self {
            EmbeddedSaveAiState::Idle => crate::components::AiState::Idle,
            EmbeddedSaveAiState::Patrolling { waypoint_index } => {
                crate::components::AiState::Patrolling { waypoint_index }
            }
            EmbeddedSaveAiState::Chasing { target_id } => {
                crate::components::AiState::Chasing { target_id }
            }
            EmbeddedSaveAiState::Fleeing { threat_id } => {
                crate::components::AiState::Fleeing { threat_id }
            }
            EmbeddedSaveAiState::Attacking { target_id } => {
                crate::components::AiState::Attacking { target_id }
            }
            EmbeddedSaveAiState::Returning => crate::components::AiState::Returning,
            EmbeddedSaveAiState::Wandering { pause_frames } => {
                crate::components::AiState::Wandering { pause_frames }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, serde::Deserialize)]
#[serde(tag = "type")]
enum EmbeddedComponentDef {
    #[serde(rename = "collider")]
    Collider { width: f32, height: f32 },
    #[serde(rename = "gravity_body")]
    GravityBody,
    #[serde(rename = "horizontal_mover")]
    HorizontalMover {
        speed: f32,
        #[serde(default = "default_left")]
        left_action: String,
        #[serde(default = "default_right")]
        right_action: String,
    },
    #[serde(rename = "jumper")]
    Jumper {
        velocity: f32,
        #[serde(default = "default_jump")]
        action: String,
        #[serde(default = "default_fall_mult")]
        fall_multiplier: f32,
        #[serde(default = "default_true")]
        variable_height: bool,
        #[serde(default = "default_coyote")]
        coyote_frames: u32,
        #[serde(default = "default_buffer")]
        buffer_frames: u32,
    },
    #[serde(rename = "top_down_mover")]
    TopDownMover {
        speed: f32,
        #[serde(default = "default_up")]
        up_action: String,
        #[serde(default = "default_down")]
        down_action: String,
        #[serde(default = "default_left")]
        left_action: String,
        #[serde(default = "default_right")]
        right_action: String,
    },
    #[serde(rename = "health")]
    Health { current: f32, max: f32 },
    #[serde(rename = "contact_damage")]
    ContactDamage {
        amount: f32,
        #[serde(default = "default_damage_cooldown")]
        cooldown_frames: u32,
        #[serde(default)]
        knockback: f32,
        damage_tag: String,
    },
    #[serde(rename = "trigger_zone")]
    TriggerZone {
        radius: f32,
        trigger_tag: String,
        event_name: String,
        #[serde(default)]
        one_shot: bool,
    },
    #[serde(rename = "pickup")]
    Pickup {
        pickup_tag: String,
        effect: EmbeddedPickupEffectDef,
    },
    #[serde(rename = "projectile")]
    Projectile {
        speed: f32,
        direction: EmbeddedVec2Def,
        lifetime_frames: u32,
        damage: f32,
        owner_id: u64,
        damage_tag: String,
    },
    #[serde(rename = "hitbox")]
    Hitbox {
        width: f32,
        height: f32,
        #[serde(default)]
        offset: EmbeddedVec2Def,
        #[serde(default)]
        active: bool,
        damage: f32,
        damage_tag: String,
    },
    #[serde(rename = "moving_platform")]
    MovingPlatform {
        waypoints: Vec<EmbeddedVec2Def>,
        speed: f32,
        #[serde(default)]
        loop_mode: EmbeddedPlatformLoopModeDef,
        #[serde(default)]
        pause_frames: u32,
        #[serde(default = "default_true")]
        carry_riders: bool,
        #[serde(default)]
        current_waypoint: usize,
        #[serde(default = "default_platform_direction")]
        direction: i8,
    },
    #[serde(rename = "animation_controller")]
    AnimationController {
        graph: String,
        #[serde(default = "default_animation_state")]
        state: String,
        #[serde(default)]
        frame: usize,
        #[serde(default)]
        timer: f32,
        #[serde(default = "default_f32_one")]
        speed: f32,
        #[serde(default = "default_true")]
        playing: bool,
        #[serde(default = "default_true")]
        facing_right: bool,
        #[serde(default)]
        auto_from_velocity: bool,
        #[serde(default = "default_facing_direction")]
        facing_direction: u8,
    },
    #[serde(rename = "path_follower")]
    PathFollower {
        target: EmbeddedVec2Def,
        #[serde(default = "default_recalculate_interval")]
        recalculate_interval: u32,
        #[serde(default)]
        path_type: EmbeddedPathTypeDef,
        speed: f32,
    },
    #[serde(rename = "ai_behavior")]
    AiBehavior { behavior: EmbeddedAiBehaviorDef },
    #[serde(rename = "particle_emitter")]
    ParticleEmitter {
        #[serde(flatten, default)]
        emitter: crate::particles::ParticleEmitter,
    },
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, serde::Deserialize)]
#[serde(tag = "type")]
enum EmbeddedPickupEffectDef {
    #[serde(rename = "heal")]
    Heal { amount: f32 },
    #[serde(rename = "score_add")]
    ScoreAdd { amount: i32 },
    #[serde(rename = "custom")]
    Custom { name: String },
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, serde::Deserialize, Default)]
struct EmbeddedVec2Def {
    x: f32,
    y: f32,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum EmbeddedPathTypeDef {
    #[default]
    TopDown,
    Platformer,
}

#[cfg(target_arch = "wasm32")]
impl EmbeddedPathTypeDef {
    fn into_runtime(self) -> crate::components::PathType {
        match self {
            EmbeddedPathTypeDef::TopDown => crate::components::PathType::TopDown,
            EmbeddedPathTypeDef::Platformer => crate::components::PathType::Platformer,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum EmbeddedPlatformLoopModeDef {
    #[default]
    Loop,
    PingPong,
}

#[cfg(target_arch = "wasm32")]
impl EmbeddedPlatformLoopModeDef {
    fn into_runtime(self) -> crate::components::PlatformLoopMode {
        match self {
            EmbeddedPlatformLoopModeDef::Loop => crate::components::PlatformLoopMode::Loop,
            EmbeddedPlatformLoopModeDef::PingPong => crate::components::PlatformLoopMode::PingPong,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, serde::Deserialize)]
#[serde(tag = "type")]
enum EmbeddedAiBehaviorDef {
    #[serde(rename = "patrol")]
    Patrol {
        waypoints: Vec<EmbeddedVec2Def>,
        speed: f32,
    },
    #[serde(rename = "chase")]
    Chase {
        #[serde(default = "default_player_tag")]
        target_tag: String,
        speed: f32,
        detection_radius: f32,
        give_up_radius: f32,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    #[serde(rename = "flee")]
    Flee {
        threat_tag: String,
        speed: f32,
        detection_radius: f32,
        give_up_radius: f32,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    #[serde(rename = "guard")]
    Guard {
        position: EmbeddedVec2Def,
        radius: f32,
        chase_radius: f32,
        speed: f32,
        #[serde(default = "default_player_tag")]
        target_tag: String,
        #[serde(default)]
        require_line_of_sight: bool,
    },
    #[serde(rename = "wander")]
    Wander {
        speed: f32,
        radius: f32,
        #[serde(default = "default_pause_frames")]
        pause_frames: u32,
    },
    #[serde(rename = "custom")]
    Custom { script: String },
}

#[cfg(target_arch = "wasm32")]
impl EmbeddedAiBehaviorDef {
    fn into_runtime(self) -> crate::components::BehaviorType {
        match self {
            EmbeddedAiBehaviorDef::Patrol { waypoints, speed } => {
                crate::components::BehaviorType::Patrol {
                    waypoints: waypoints.into_iter().map(Into::into).collect(),
                    speed,
                }
            }
            EmbeddedAiBehaviorDef::Chase {
                target_tag,
                speed,
                detection_radius,
                give_up_radius,
                require_line_of_sight,
            } => crate::components::BehaviorType::Chase {
                target_tag,
                speed,
                detection_radius,
                give_up_radius,
                require_line_of_sight,
            },
            EmbeddedAiBehaviorDef::Flee {
                threat_tag,
                speed,
                detection_radius,
                give_up_radius,
                require_line_of_sight,
            } => crate::components::BehaviorType::Flee {
                threat_tag,
                speed,
                detection_radius,
                give_up_radius,
                require_line_of_sight,
            },
            EmbeddedAiBehaviorDef::Guard {
                position,
                radius,
                chase_radius,
                speed,
                target_tag,
                require_line_of_sight,
            } => crate::components::BehaviorType::Guard {
                position: position.into(),
                radius,
                chase_radius,
                speed,
                target_tag,
                require_line_of_sight,
            },
            EmbeddedAiBehaviorDef::Wander {
                speed,
                radius,
                pause_frames,
            } => crate::components::BehaviorType::Wander {
                speed,
                radius,
                pause_frames,
            },
            EmbeddedAiBehaviorDef::Custom { script } => {
                crate::components::BehaviorType::Custom(script)
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl From<EmbeddedVec2Def> for Vec2 {
    fn from(value: EmbeddedVec2Def) -> Self {
        Vec2::new(value.x, value.y)
    }
}

#[cfg(target_arch = "wasm32")]
impl EmbeddedPickupEffectDef {
    fn into_runtime(self) -> crate::components::PickupEffect {
        match self {
            EmbeddedPickupEffectDef::Heal { amount } => {
                crate::components::PickupEffect::Heal(amount)
            }
            EmbeddedPickupEffectDef::ScoreAdd { amount } => {
                crate::components::PickupEffect::ScoreAdd(amount)
            }
            EmbeddedPickupEffectDef::Custom { name } => {
                crate::components::PickupEffect::Custom(name)
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn default_left() -> String {
    "left".into()
}

#[cfg(target_arch = "wasm32")]
fn default_right() -> String {
    "right".into()
}

#[cfg(target_arch = "wasm32")]
fn default_up() -> String {
    "up".into()
}

#[cfg(target_arch = "wasm32")]
fn default_down() -> String {
    "down".into()
}

#[cfg(target_arch = "wasm32")]
fn default_jump() -> String {
    "jump".into()
}

#[cfg(target_arch = "wasm32")]
fn default_fall_mult() -> f32 {
    1.5
}

#[cfg(target_arch = "wasm32")]
fn default_true() -> bool {
    true
}

#[cfg(target_arch = "wasm32")]
fn default_coyote() -> u32 {
    5
}

#[cfg(target_arch = "wasm32")]
fn default_buffer() -> u32 {
    4
}

#[cfg(target_arch = "wasm32")]
fn default_damage_cooldown() -> u32 {
    12
}

#[cfg(target_arch = "wasm32")]
fn default_recalculate_interval() -> u32 {
    20
}

#[cfg(target_arch = "wasm32")]
fn default_platform_direction() -> i8 {
    1
}

#[cfg(target_arch = "wasm32")]
fn default_animation_state() -> String {
    "idle".to_string()
}

#[cfg(target_arch = "wasm32")]
fn default_f32_one() -> f32 {
    1.0
}

#[cfg(target_arch = "wasm32")]
fn default_facing_direction() -> u8 {
    5
}

#[cfg(target_arch = "wasm32")]
fn default_player_tag() -> String {
    "player".to_string()
}

#[cfg(target_arch = "wasm32")]
fn default_pause_frames() -> u32 {
    30
}

#[cfg(target_arch = "wasm32")]
fn parse_embedded_components_wasm(raw: &[serde_json::Value]) -> Vec<EmbeddedComponentDef> {
    let mut out = Vec::new();
    for value in raw.iter() {
        match serde_json::from_value::<EmbeddedComponentDef>(value.clone()) {
            Ok(component) => out.push(component),
            Err(err) => warn!("[Axiom export] Skipping invalid embedded component: {err}"),
        }
    }
    out
}

#[cfg(target_arch = "wasm32")]
fn apply_embedded_component_wasm(entity: &mut EntityCommands, component: EmbeddedComponentDef) {
    match component {
        EmbeddedComponentDef::Collider { width, height } => {
            entity.insert(crate::components::Collider { width, height });
        }
        EmbeddedComponentDef::GravityBody => {
            entity.insert(crate::components::GravityBody);
        }
        EmbeddedComponentDef::HorizontalMover {
            speed,
            left_action,
            right_action,
        } => {
            entity.insert(crate::components::HorizontalMover {
                speed,
                left_action,
                right_action,
            });
        }
        EmbeddedComponentDef::Jumper {
            velocity,
            action,
            fall_multiplier,
            variable_height,
            coyote_frames,
            buffer_frames,
        } => {
            entity.insert((
                crate::components::Jumper {
                    velocity,
                    action,
                    fall_multiplier,
                    variable_height,
                    coyote_frames,
                    buffer_frames,
                },
                crate::components::CoyoteTimer::default(),
                crate::components::JumpBuffer::default(),
            ));
        }
        EmbeddedComponentDef::TopDownMover {
            speed,
            up_action,
            down_action,
            left_action,
            right_action,
        } => {
            entity.insert(crate::components::TopDownMover {
                speed,
                up_action,
                down_action,
                left_action,
                right_action,
            });
        }
        EmbeddedComponentDef::Health { current, max } => {
            entity.insert(crate::components::Health { current, max });
        }
        EmbeddedComponentDef::ContactDamage {
            amount,
            cooldown_frames,
            knockback,
            damage_tag,
        } => {
            entity.insert(crate::components::ContactDamage {
                amount,
                cooldown_frames,
                knockback,
                damage_tag,
            });
        }
        EmbeddedComponentDef::TriggerZone {
            radius,
            trigger_tag,
            event_name,
            one_shot,
        } => {
            entity.insert(crate::components::TriggerZone {
                radius,
                trigger_tag,
                event_name,
                one_shot,
            });
        }
        EmbeddedComponentDef::Pickup { pickup_tag, effect } => {
            entity.insert(crate::components::Pickup {
                pickup_tag,
                effect: effect.into_runtime(),
            });
        }
        EmbeddedComponentDef::Projectile {
            speed,
            direction,
            lifetime_frames,
            damage,
            owner_id,
            damage_tag,
        } => {
            entity.insert(crate::components::Projectile {
                speed,
                direction: direction.into(),
                lifetime_frames,
                damage,
                owner_id,
                damage_tag,
            });
        }
        EmbeddedComponentDef::Hitbox {
            width,
            height,
            offset,
            active,
            damage,
            damage_tag,
        } => {
            entity.insert(crate::components::Hitbox {
                width,
                height,
                offset: offset.into(),
                active,
                damage,
                damage_tag,
            });
        }
        EmbeddedComponentDef::MovingPlatform {
            waypoints,
            speed,
            loop_mode,
            pause_frames,
            carry_riders,
            current_waypoint,
            direction,
        } => {
            let direction = if direction == 0 { 1 } else { direction };
            entity.insert(crate::components::MovingPlatform {
                waypoints: waypoints.into_iter().map(Into::into).collect(),
                speed,
                loop_mode: loop_mode.into_runtime(),
                current_waypoint,
                direction,
                pause_frames,
                pause_timer: 0,
                carry_riders,
            });
        }
        EmbeddedComponentDef::AnimationController {
            graph,
            state,
            frame,
            timer,
            speed,
            playing,
            facing_right,
            auto_from_velocity,
            facing_direction,
        } => {
            entity.insert(crate::components::AnimationController {
                graph,
                state,
                frame,
                timer,
                speed,
                playing,
                facing_right,
                auto_from_velocity,
                facing_direction,
            });
        }
        EmbeddedComponentDef::PathFollower {
            target,
            recalculate_interval,
            path_type,
            speed,
        } => {
            entity.insert(crate::components::PathFollower::new(
                target.into(),
                path_type.into_runtime(),
                recalculate_interval,
                speed,
            ));
        }
        EmbeddedComponentDef::AiBehavior { behavior } => {
            entity.insert(crate::components::AiBehavior {
                behavior: behavior.into_runtime(),
                state: crate::components::AiState::Idle,
            });
        }
        EmbeddedComponentDef::ParticleEmitter { emitter } => {
            entity.insert(emitter);
        }
    }
}

fn parse_embedded_snapshot(raw: &str) -> Option<EmbeddedSnapshot> {
    let text = raw.trim();
    if text.is_empty() || text == "{}" {
        return None;
    }
    let root: serde_json::Value = serde_json::from_str(text).ok()?;
    let save = root.get("save").unwrap_or(&root);
    let cfg = serde_json::from_value::<GameConfig>(save.get("config")?.clone()).ok()?;
    let tilemap = serde_json::from_value::<Tilemap>(save.get("tilemap")?.clone()).ok()?;
    let next_network_id = save.get("next_network_id").and_then(|v| v.as_u64());
    let game_state = save
        .get("game_state")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let scripts = save
        .get("scripts")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let global_scripts = save
        .get("global_scripts")
        .cloned()
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
        .into_iter()
        .collect();
    let game_vars = save
        .get("game_vars")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let animation_graphs = save
        .get("animation_graphs")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let sprite_sheets = save
        .get("sprite_sheets")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let particle_presets = save
        .get("particle_presets")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let entities = save
        .get("entities")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    Some(EmbeddedSnapshot {
        config: cfg,
        tilemap,
        next_network_id,
        game_state,
        scripts,
        global_scripts,
        game_vars,
        animation_graphs,
        sprite_sheets,
        particle_presets,
        entities,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn apply_embedded_game_data_from_str(
    raw: &str,
    tilemap: &mut Tilemap,
    config: &mut GameConfig,
) -> bool {
    let Some(snapshot) = parse_embedded_snapshot(raw) else {
        return false;
    };
    *config = snapshot.config;
    *tilemap = snapshot.tilemap;
    true
}

struct EmbeddedRuntimeTargets<'a> {
    tilemap: &'a mut Tilemap,
    config: &'a mut GameConfig,
    next_network_id: Option<&'a mut NextNetworkId>,
    script_engine: Option<&'a mut crate::scripting::ScriptEngine>,
    runtime_state: Option<&'a mut crate::game_runtime::RuntimeState>,
    animation_library: Option<&'a mut crate::animation::AnimationLibrary>,
    sprite_registry: Option<&'a mut crate::sprites::SpriteSheetRegistry>,
    particle_library: Option<&'a mut crate::particles::ParticlePresetLibrary>,
    embedded_restore: Option<&'a mut EmbeddedEntityRestore>,
}

fn apply_embedded_runtime_snapshot(
    snapshot: EmbeddedSnapshot,
    targets: EmbeddedRuntimeTargets<'_>,
) {
    let EmbeddedSnapshot {
        config: snapshot_config,
        tilemap: snapshot_tilemap,
        next_network_id: snapshot_next_network_id,
        game_state: snapshot_game_state,
        scripts: snapshot_scripts,
        global_scripts: snapshot_global_scripts,
        game_vars: snapshot_game_vars,
        animation_graphs: snapshot_animation_graphs,
        sprite_sheets: snapshot_sprite_sheets,
        particle_presets: snapshot_particle_presets,
        entities: snapshot_entities,
    } = snapshot;

    *targets.config = snapshot_config;
    *targets.tilemap = snapshot_tilemap;

    if let Some(next_id) = snapshot_next_network_id {
        if let Some(next) = targets.next_network_id {
            next.0 = next_id.max(1);
        }
    }
    if let Some(state) = snapshot_game_state {
        let state = state.trim();
        if !state.is_empty() {
            if let Some(runtime) = targets.runtime_state {
                runtime.set_state(state.to_string(), Some("Instant".to_string()), 0.0);
            }
        }
    }
    if let Some(engine) = targets.script_engine {
        ScriptBackend::restore_snapshot(
            engine,
            crate::scripting::ScriptRuntimeSnapshot {
                scripts: snapshot_scripts,
                global_scripts: snapshot_global_scripts,
                vars: snapshot_game_vars,
            },
        );
    }
    if !snapshot_animation_graphs.is_empty() {
        if let Some(library) = targets.animation_library {
            library.graphs = snapshot_animation_graphs;
        }
    }
    if !snapshot_sprite_sheets.is_empty() {
        if let Some(registry) = targets.sprite_registry {
            registry.sheets = snapshot_sprite_sheets;
        }
    }
    if !snapshot_particle_presets.is_empty() {
        if let Some(library) = targets.particle_library {
            library.presets = snapshot_particle_presets;
        }
    }
    if let Some(restore) = targets.embedded_restore {
        restore.entities = snapshot_entities;
        restore.applied = false;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn restore_embedded_entities_after_startup(
    mut commands: Commands,
    mut restore: ResMut<EmbeddedEntityRestore>,
    mut next_network_id: ResMut<NextNetworkId>,
    existing_entities: Query<Entity, With<crate::components::NetworkId>>,
) {
    if restore.applied || restore.entities.is_empty() {
        restore.applied = true;
        return;
    }

    for entity in existing_entities.iter() {
        commands.entity(entity).despawn();
    }

    for saved in restore.entities.iter() {
        let mut components = Vec::new();
        for raw in saved.components.iter() {
            match serde_json::from_value::<crate::api::types::ComponentDef>(raw.clone()) {
                Ok(comp) => components.push(comp),
                Err(err) => warn!("[Axiom export] Skipping invalid embedded component: {err}"),
            }
        }

        let req = crate::api::types::EntitySpawnRequest {
            x: saved.x,
            y: saved.y,
            components,
            script: saved.script.clone(),
            tags: saved.tags.clone(),
            is_player: saved.is_player,
        };
        let entity = crate::spawn::spawn_entity_with_network_id(
            &mut commands,
            &req,
            &mut next_network_id,
            saved.network_id,
        );
        commands.entity(entity).insert(crate::components::Velocity {
            x: saved.vx,
            y: saved.vy,
        });
        commands
            .entity(entity)
            .insert(crate::components::Alive(saved.alive));
        if let Some(script_state) = saved.script_state.clone() {
            commands.queue(move |world: &mut World| {
                if let Some(mut script) = world.get_mut::<crate::scripting::LuaScript>(entity) {
                    script.state = script_state;
                }
            });
        }
        if let Some(ai_state) = saved.ai_state.clone() {
            commands.queue(move |world: &mut World| {
                if let Some(mut ai) = world.get_mut::<crate::components::AiBehavior>(entity) {
                    ai.state = ai_state.into_runtime();
                }
            });
        }
    }

    restore.applied = true;
}

#[cfg(target_arch = "wasm32")]
fn restore_embedded_entities_after_startup(
    mut commands: Commands,
    mut restore: ResMut<EmbeddedEntityRestore>,
    mut next_network_id: ResMut<NextNetworkId>,
    existing_entities: Query<Entity, With<crate::components::NetworkId>>,
) {
    if restore.applied || restore.entities.is_empty() {
        restore.applied = true;
        return;
    }

    for entity in existing_entities.iter() {
        commands.entity(entity).despawn();
    }

    for saved in restore.entities.iter() {
        let assigned_id = saved.network_id.unwrap_or(next_network_id.0).max(1);
        next_network_id.0 = next_network_id.0.max(assigned_id.saturating_add(1));
        let mut tags = std::collections::HashSet::new();
        for tag in saved.tags.iter() {
            tags.insert(tag.clone());
        }
        if saved.is_player {
            tags.insert("player".to_string());
        }

        let parsed_components = parse_embedded_components_wasm(&saved.components);
        let mut entity = commands.spawn((
            crate::components::NetworkId(assigned_id),
            crate::components::GamePosition {
                x: saved.x,
                y: saved.y,
            },
            crate::components::Velocity {
                x: saved.vx,
                y: saved.vy,
            },
            crate::components::Grounded(false),
            crate::components::Alive(saved.alive),
            crate::components::Tags(tags),
            Transform::from_xyz(saved.x, saved.y, 10.0),
        ));
        if saved.is_player {
            entity.insert((
                crate::components::Player,
                Sprite::from_color(Color::srgb(0.2, 0.4, 0.9), Vec2::new(12.0, 14.0)),
            ));
        }
        for component in parsed_components {
            apply_embedded_component_wasm(&mut entity, component);
        }
        let entity_id = entity.id();
        if let Some(script_name) = saved.script.clone() {
            entity.insert(crate::scripting::LuaScript {
                script_name,
                state: saved.script_state.clone().unwrap_or(serde_json::json!({})),
                enabled: true,
                error_streak: 0,
                disabled_reason: None,
            });
        }
        if let Some(ai_state) = saved.ai_state.clone() {
            commands.queue(move |world: &mut World| {
                if let Some(mut ai) = world.get_mut::<crate::components::AiBehavior>(entity_id) {
                    ai.state = ai_state.into_runtime();
                }
            });
        }
    }

    restore.applied = true;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedDataSanity {
    pub width: usize,
    pub height: usize,
    pub tile_count: usize,
}

pub fn embedded_data_sanity() -> Result<EmbeddedDataSanity, String> {
    let Some(snapshot) = parse_embedded_snapshot(EMBEDDED_GAME_DATA) else {
        return Err("No embedded snapshot found".to_string());
    };
    let tm = snapshot.tilemap;
    if tm.width == 0 || tm.height == 0 {
        return Err("Embedded tilemap dimensions must be > 0".to_string());
    }
    let expected = tm.width.saturating_mul(tm.height);
    if tm.tiles.len() != expected {
        return Err(format!(
            "Embedded tile count mismatch: {} != {}x{} ({})",
            tm.tiles.len(),
            tm.width,
            tm.height,
            expected
        ));
    }
    Ok(EmbeddedDataSanity {
        width: tm.width,
        height: tm.height,
        tile_count: tm.tiles.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_embedded_snapshot_accepts_project_payload() {
        let json = serde_json::json!({
            "version": 1,
            "save": {
                "config": GameConfig::default(),
                "tilemap": Tilemap::test_level(),
            }
        });
        let parsed = parse_embedded_snapshot(&serde_json::to_string(&json).unwrap());
        assert!(parsed.is_some());
    }

    #[test]
    fn parse_embedded_snapshot_accepts_save_payload() {
        let json = serde_json::json!({
            "config": GameConfig::default(),
            "tilemap": Tilemap::test_level(),
        });
        let parsed = parse_embedded_snapshot(&serde_json::to_string(&json).unwrap());
        assert!(parsed.is_some());
    }

    #[test]
    fn parse_embedded_snapshot_extracts_runtime_metadata() {
        let json = serde_json::json!({
            "save": {
                "config": GameConfig::default(),
                "tilemap": Tilemap::test_level(),
                "next_network_id": 99,
                "game_state": "Paused",
                "scripts": { "enemy": "fn update(entity, world, dt) {}" },
                "global_scripts": ["enemy"],
                "game_vars": { "score": 123 },
                "animation_graphs": {},
                "sprite_sheets": {},
                "particle_presets": {},
                "entities": [{
                    "network_id": 7,
                    "x": 12.0,
                    "y": 24.0,
                    "is_player": false,
                    "components": [{
                        "type": "ai_behavior",
                        "behavior": {
                            "type": "chase",
                            "target_tag": "player",
                            "speed": 120.0,
                            "detection_radius": 180.0,
                            "give_up_radius": 240.0
                        }
                    }],
                    "tags": ["enemy"],
                    "alive": true,
                    "ai_state": { "type": "chasing", "target_id": 42 }
                }],
            }
        });
        let parsed = parse_embedded_snapshot(&serde_json::to_string(&json).unwrap())
            .expect("expected embedded snapshot");
        assert_eq!(parsed.next_network_id, Some(99));
        assert_eq!(parsed.game_state.as_deref(), Some("Paused"));
        assert!(parsed.scripts.contains_key("enemy"));
        assert!(parsed.global_scripts.contains("enemy"));
        assert_eq!(
            parsed
                .game_vars
                .get("score")
                .and_then(|v| v.as_i64())
                .unwrap_or_default(),
            123
        );
        assert_eq!(parsed.entities.len(), 1);
        assert_eq!(
            parsed.entities[0].ai_state,
            Some(EmbeddedSaveAiState::Chasing { target_id: 42 })
        );
    }

    #[test]
    fn apply_embedded_game_data_updates_resources() {
        let mut cfg = GameConfig::default();
        let mut tm = Tilemap::test_level();
        let src_tm = Tilemap {
            width: 4,
            height: 3,
            tiles: vec![0u8; 12],
            player_spawn: (8.0, 8.0),
            goal: Some((2, 1)),
            ..Default::default()
        };
        let src_cfg = GameConfig {
            move_speed: 321.0,
            ..GameConfig::default()
        };

        let json = serde_json::json!({
            "save": {
                "config": src_cfg,
                "tilemap": src_tm,
            }
        });
        let ok = apply_embedded_game_data_from_str(
            &serde_json::to_string(&json).unwrap(),
            &mut tm,
            &mut cfg,
        );
        assert!(ok);
        assert_eq!(tm.width, 4);
        assert_eq!(tm.height, 3);
        assert_eq!(tm.tiles.len(), 12);
        assert!((cfg.move_speed - 321.0).abs() < f32::EPSILON);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn restore_embedded_entities_restores_ai_and_script_state() {
        let mut app = App::new();
        app.insert_resource(NextNetworkId(1))
            .insert_resource(EmbeddedEntityRestore {
                entities: vec![EmbeddedSaveEntity {
                    network_id: Some(7),
                    x: 10.0,
                    y: 20.0,
                    vx: 3.0,
                    vy: -2.0,
                    is_player: false,
                    components: vec![
                        serde_json::to_value(crate::api::types::ComponentDef::AiBehavior {
                            behavior: crate::api::types::AiBehaviorDef::Chase {
                                target_tag: "player".to_string(),
                                speed: 100.0,
                                detection_radius: 200.0,
                                give_up_radius: 260.0,
                                require_line_of_sight: false,
                            },
                        })
                        .expect("serialize ai component"),
                        serde_json::to_value(crate::api::types::ComponentDef::Health {
                            current: 5.0,
                            max: 9.0,
                        })
                        .expect("serialize health component"),
                    ],
                    script: Some("enemy_ai".to_string()),
                    script_state: Some(serde_json::json!({"phase":"aggressive"})),
                    tags: vec!["enemy".to_string()],
                    alive: true,
                    ai_state: Some(EmbeddedSaveAiState::Chasing { target_id: 99 }),
                }],
                applied: false,
            })
            .add_systems(Update, restore_embedded_entities_after_startup);

        app.update();
        app.update();

        let mut found = false;
        let mut query = app.world_mut().query::<(
            &crate::components::NetworkId,
            &crate::components::AiBehavior,
            &crate::scripting::LuaScript,
            &crate::components::Health,
            &crate::components::Velocity,
        )>();
        for (nid, ai, script, health, vel) in query.iter(app.world()) {
            if nid.0 == 7 {
                found = true;
                assert!(matches!(
                    ai.state,
                    crate::components::AiState::Chasing { target_id: 99 }
                ));
                assert_eq!(script.script_name, "enemy_ai");
                assert_eq!(script.state["phase"], serde_json::json!("aggressive"));
                assert!((health.current - 5.0).abs() < 0.0001);
                assert!((health.max - 9.0).abs() < 0.0001);
                assert!((vel.x - 3.0).abs() < 0.0001);
                assert!((vel.y - (-2.0)).abs() < 0.0001);
            }
        }
        assert!(found, "expected restored entity with network id 7");
    }
}
