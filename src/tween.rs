use bevy::prelude::*;

use crate::api::types::TweenRequest;
use crate::components::{GamePosition, NetworkId};
use crate::events::{GameEvent, GameEventBus};

pub struct TweenPlugin;

impl Plugin for TweenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tick_tweens, advance_tween_sequences).chain());
    }
}

#[derive(Clone)]
pub struct TweenStep {
    pub property: String,
    pub to: f32,
    pub from: Option<f32>,
    pub duration: f32,
    pub easing: Option<String>,
}

/// Holds a queue of tween steps to apply sequentially.
#[derive(Component, Clone, Default)]
pub struct TweenSequence {
    pub steps: Vec<TweenStep>,
    pub current_index: usize,
    pub sequence_id: Option<String>,
    pub active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TweenProperty {
    X,
    Y,
    ScaleX,
    ScaleY,
    Alpha,
    Rotation,
}

impl TweenProperty {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "scale_x" | "scaleX" | "scale" => Some(Self::ScaleX),
            "scale_y" | "scaleY" => Some(Self::ScaleY),
            "alpha" | "opacity" => Some(Self::Alpha),
            "rotation" | "angle" => Some(Self::Rotation),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EasingFunction {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bounce,
    Elastic,
}

impl EasingFunction {
    fn from_str(s: &str) -> Self {
        match s {
            "ease_in" | "easeIn" => Self::EaseIn,
            "ease_out" | "easeOut" => Self::EaseOut,
            "ease_in_out" | "easeInOut" => Self::EaseInOut,
            "bounce" => Self::Bounce,
            "elastic" => Self::Elastic,
            _ => Self::Linear,
        }
    }

    fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Self::Bounce => {
                let t = 1.0 - t;
                let v = if t < 1.0 / 2.75 {
                    7.5625 * t * t
                } else if t < 2.0 / 2.75 {
                    let t = t - 1.5 / 2.75;
                    7.5625 * t * t + 0.75
                } else if t < 2.5 / 2.75 {
                    let t = t - 2.25 / 2.75;
                    7.5625 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / 2.75;
                    7.5625 * t * t + 0.984375
                };
                1.0 - v
            }
            Self::Elastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    let p = 0.3;
                    let s = p / 4.0;
                    (2.0f32).powf(-10.0 * t)
                        * ((t - s) * std::f32::consts::TAU / p).sin()
                        + 1.0
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct TweenInstance {
    pub property: TweenProperty,
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub easing: EasingFunction,
    pub tween_id: Option<String>,
}

#[derive(Component, Clone, Default)]
pub struct TweenSet(pub Vec<TweenInstance>);

fn tick_tweens(
    mut commands: Commands,
    time: Res<Time>,
    mut event_bus: ResMut<GameEventBus>,
    mut query: Query<(
        Entity,
        &mut TweenSet,
        &mut GamePosition,
        &NetworkId,
        Option<&mut Sprite>,
        Option<&mut Transform>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut tween_set, mut pos, network_id, mut sprite, mut transform) in query.iter_mut() {
        let mut completed = Vec::new();
        for (i, tween) in tween_set.0.iter_mut().enumerate() {
            tween.elapsed += dt;
            let t = (tween.elapsed / tween.duration.max(0.001)).clamp(0.0, 1.0);
            let eased = tween.easing.apply(t);
            let value = tween.from + (tween.to - tween.from) * eased;

            match tween.property {
                TweenProperty::X => pos.x = value,
                TweenProperty::Y => pos.y = value,
                TweenProperty::ScaleX => {
                    if let Some(ref mut transform) = transform {
                        transform.scale.x = value;
                    }
                }
                TweenProperty::ScaleY => {
                    if let Some(ref mut transform) = transform {
                        transform.scale.y = value;
                    }
                }
                TweenProperty::Alpha => {
                    if let Some(ref mut sprite) = sprite {
                        sprite.color = sprite.color.with_alpha(value.clamp(0.0, 1.0));
                    }
                }
                TweenProperty::Rotation => {
                    if let Some(ref mut transform) = transform {
                        transform.rotation =
                            Quat::from_rotation_z(value.to_radians());
                    }
                }
            }

            if tween.elapsed >= tween.duration {
                completed.push(i);
            }
        }
        // Remove completed tweens in reverse order and emit events
        for &i in completed.iter().rev() {
            let tween = tween_set.0.remove(i);
            let mut data = serde_json::json!({
                "entity_id": network_id.0,
                "property": format!("{:?}", tween.property).to_lowercase(),
            });
            if let Some(tween_id) = &tween.tween_id {
                data["tween_id"] = serde_json::json!(tween_id);
            }
            event_bus.recent.push_back(GameEvent {
                name: "tween_complete".to_string(),
                data,
                frame: 0,
                source_entity: Some(network_id.0),
            });
        }
        if tween_set.0.is_empty() {
            commands.entity(entity).remove::<TweenSet>();
        }
    }
}

/// Get the current value of a tweened property for an entity
fn get_current_value(
    pos: &GamePosition,
    sprite: Option<&Sprite>,
    transform: Option<&Transform>,
    property: TweenProperty,
) -> f32 {
    match property {
        TweenProperty::X => pos.x,
        TweenProperty::Y => pos.y,
        TweenProperty::ScaleX => transform.map_or(1.0, |t| t.scale.x),
        TweenProperty::ScaleY => transform.map_or(1.0, |t| t.scale.y),
        TweenProperty::Alpha => sprite.map_or(1.0, |s| s.color.alpha()),
        TweenProperty::Rotation => transform.map_or(0.0, |t| {
            let (_, angle) = t.rotation.to_axis_angle();
            angle.to_degrees()
        }),
    }
}

/// Apply a tween command from the API
pub fn apply_tween_command(
    world: &mut World,
    entity_network_id: u64,
    req: TweenRequest,
) -> Result<(), String> {
    let property = TweenProperty::from_str(&req.property)
        .ok_or_else(|| format!("Unknown tween property: {}", req.property))?;
    let easing = req
        .easing
        .as_deref()
        .map(EasingFunction::from_str)
        .unwrap_or(EasingFunction::Linear);

    // Find entity by network id
    let mut entity_id = None;
    let mut query = world.query::<(Entity, &NetworkId)>();
    for (entity, nid) in query.iter(world) {
        if nid.0 == entity_network_id {
            entity_id = Some(entity);
            break;
        }
    }
    let entity = entity_id
        .ok_or_else(|| format!("Entity with network_id {} not found", entity_network_id))?;

    // Get current value for 'from' default
    let from = req.from.unwrap_or_else(|| {
        let pos = world.get::<GamePosition>(entity);
        let sprite = world.get::<Sprite>(entity);
        let transform = world.get::<Transform>(entity);
        get_current_value(
            pos.unwrap_or(&GamePosition::default()),
            sprite,
            transform,
            property,
        )
    });

    let instance = TweenInstance {
        property,
        from,
        to: req.to,
        duration: req.duration.max(0.001),
        elapsed: 0.0,
        easing,
        tween_id: req.tween_id,
    };

    // Add or update TweenSet
    if let Some(mut tween_set) = world.get_mut::<TweenSet>(entity) {
        // Replace existing tween on same property
        tween_set.0.retain(|t| t.property != property);
        tween_set.0.push(instance);
    } else {
        world.entity_mut(entity).insert(TweenSet(vec![instance]));
    }

    Ok(())
}

/// System that advances tween sequences when the current step finishes.
fn advance_tween_sequences(
    mut commands: Commands,
    mut event_bus: ResMut<GameEventBus>,
    mut query: Query<(
        Entity,
        &mut TweenSequence,
        &mut GamePosition,
        &NetworkId,
        Option<&Sprite>,
        Option<&Transform>,
        Option<&TweenSet>,
    )>,
) {
    for (entity, mut seq, pos, network_id, sprite, transform, tween_set) in query.iter_mut() {
        if !seq.active {
            continue;
        }
        // Check if the entity still has an active tween — if so, wait
        let has_active_tween = tween_set.is_some_and(|ts| !ts.0.is_empty());
        if has_active_tween {
            continue;
        }

        // Current step completed (or sequence just started), advance to next
        if seq.current_index >= seq.steps.len() {
            // Sequence complete
            let mut data = serde_json::json!({ "entity_id": network_id.0 });
            if let Some(ref sid) = seq.sequence_id {
                data["sequence_id"] = serde_json::json!(sid);
            }
            event_bus.recent.push_back(GameEvent {
                name: "tween_sequence_complete".to_string(),
                data,
                frame: 0,
                source_entity: Some(network_id.0),
            });
            commands.entity(entity).remove::<TweenSequence>();
            continue;
        }

        let step = &seq.steps[seq.current_index];
        let Some(property) = TweenProperty::from_str(&step.property) else {
            // Skip invalid step
            seq.current_index += 1;
            continue;
        };
        let easing = step.easing.as_deref().map(EasingFunction::from_str).unwrap_or(EasingFunction::Linear);
        let from = step.from.unwrap_or_else(|| {
            get_current_value(
                &pos,
                sprite,
                transform,
                property,
            )
        });
        let instance = TweenInstance {
            property,
            from,
            to: step.to,
            duration: step.duration.max(0.001),
            elapsed: 0.0,
            easing,
            tween_id: seq.sequence_id.clone().map(|s| format!("{}_{}", s, seq.current_index)),
        };
        seq.current_index += 1;

        commands.entity(entity).insert(TweenSet(vec![instance]));
    }
}

/// Apply a tween sequence command from the API
pub fn apply_tween_sequence_command(
    world: &mut World,
    entity_network_id: u64,
    steps: Vec<TweenStep>,
    sequence_id: Option<String>,
) -> Result<(), String> {
    if steps.is_empty() {
        return Err("Tween sequence must have at least one step".to_string());
    }

    let mut entity_id = None;
    let mut query = world.query::<(Entity, &NetworkId)>();
    for (entity, nid) in query.iter(world) {
        if nid.0 == entity_network_id {
            entity_id = Some(entity);
            break;
        }
    }
    let entity = entity_id
        .ok_or_else(|| format!("Entity with network_id {} not found", entity_network_id))?;

    let sequence = TweenSequence {
        steps,
        current_index: 0,
        sequence_id,
        active: true,
    };

    world.entity_mut(entity).insert(sequence);
    Ok(())
}
