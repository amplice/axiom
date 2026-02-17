use bevy::prelude::*;

use crate::api::types::ScreenEffectRequest;
use crate::camera::MainCamera;
use crate::components::HeadlessMode;
use crate::events::{GameEvent, GameEventBus};

pub struct ScreenEffectsPlugin;

impl Plugin for ScreenEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ScreenEffects::default())
            .add_systems(Startup, spawn_screen_overlay)
            .add_systems(
                Update,
                (tick_standalone_effects, sync_overlay_visual).chain(),
            );
    }
}

#[derive(Resource, Default)]
pub struct ScreenEffects {
    pub color: [f32; 4], // RGBA
    pub active_effect: Option<ActiveScreenEffect>,
}

pub struct ActiveScreenEffect {
    pub effect_type: ScreenEffectType,
    pub duration: f32,
    pub elapsed: f32,
    pub color: [f32; 3],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScreenEffectType {
    FadeIn,
    FadeOut,
    Flash,
    Tint,
}

/// Marker for the screen effects overlay sprite
#[derive(Component)]
pub struct ScreenOverlay;

fn spawn_screen_overlay(
    mut commands: Commands,
    headless: Res<HeadlessMode>,
) {
    if headless.0 {
        return;
    }
    commands.spawn((
        ScreenOverlay,
        Sprite::from_color(
            Color::srgba(0.0, 0.0, 0.0, 0.0),
            Vec2::new(4096.0, 4096.0),
        ),
        Transform::from_xyz(0.0, 0.0, 300.0),
    ));
}

fn tick_standalone_effects(
    time: Res<Time>,
    mut effects: ResMut<ScreenEffects>,
    mut event_bus: ResMut<GameEventBus>,
) {
    // Extract info we need without holding a mutable borrow on active_effect
    let update = effects.active_effect.as_ref().map(|active| {
        let elapsed = active.elapsed + time.delta_secs();
        let t = (elapsed / active.duration.max(0.001)).clamp(0.0, 1.0);
        let alpha = match active.effect_type {
            ScreenEffectType::FadeIn => 1.0 - t,
            ScreenEffectType::FadeOut => t,
            ScreenEffectType::Flash => {
                let inv = 1.0 - t;
                inv * inv * inv
            }
            ScreenEffectType::Tint => 1.0,
        };
        let color = [active.color[0], active.color[1], active.color[2], alpha];
        let done = elapsed >= active.duration;
        let effect_type = active.effect_type;
        (elapsed, color, done, effect_type)
    });

    if let Some((elapsed, color, done, effect_type)) = update {
        if let Some(ref mut active) = effects.active_effect {
            active.elapsed = elapsed;
        }
        effects.color = color;

        if done {
            let effect_name = match effect_type {
                ScreenEffectType::FadeIn => "fade_in",
                ScreenEffectType::FadeOut => "fade_out",
                ScreenEffectType::Flash => "flash",
                ScreenEffectType::Tint => "tint",
            };
            event_bus.recent.push_back(GameEvent {
                name: "screen_effect_complete".to_string(),
                data: serde_json::json!({ "effect": effect_name }),
                frame: 0,
                source_entity: None,
            });
            if effect_type != ScreenEffectType::FadeOut {
                effects.color = [0.0, 0.0, 0.0, 0.0];
            }
            effects.active_effect = None;
        }
    }
}

fn sync_overlay_visual(
    effects: Res<ScreenEffects>,
    camera_q: Query<&Transform, (With<MainCamera>, Without<ScreenOverlay>)>,
    mut overlay_q: Query<(&mut Sprite, &mut Transform), With<ScreenOverlay>>,
) {
    let Ok((mut sprite, mut transform)) = overlay_q.get_single_mut() else {
        return;
    };
    // Follow camera
    if let Ok(cam_transform) = camera_q.get_single() {
        transform.translation.x = cam_transform.translation.x;
        transform.translation.y = cam_transform.translation.y;
    }
    sprite.color = Color::srgba(
        effects.color[0],
        effects.color[1],
        effects.color[2],
        effects.color[3],
    );
}

pub fn trigger_effect_command(
    world: &mut World,
    req: ScreenEffectRequest,
) -> Result<(), String> {
    let effect_type = match req.effect.as_str() {
        "fade_in" => ScreenEffectType::FadeIn,
        "fade_out" => ScreenEffectType::FadeOut,
        "flash" => ScreenEffectType::Flash,
        "tint" => ScreenEffectType::Tint,
        other => return Err(format!("Unknown screen effect: {other}")),
    };
    let color = req.color.unwrap_or(match effect_type {
        ScreenEffectType::Flash => [1.0, 1.0, 1.0],
        _ => [0.0, 0.0, 0.0],
    });

    if let Some(mut effects) = world.get_resource_mut::<ScreenEffects>() {
        effects.active_effect = Some(ActiveScreenEffect {
            effect_type,
            duration: req.duration.max(0.001),
            elapsed: 0.0,
            color,
        });
        // Set initial color based on effect type
        match effect_type {
            ScreenEffectType::FadeIn => {
                effects.color = [color[0], color[1], color[2], 1.0];
            }
            ScreenEffectType::FadeOut => {
                effects.color = [color[0], color[1], color[2], 0.0];
            }
            ScreenEffectType::Flash => {
                effects.color = [color[0], color[1], color[2], 1.0];
            }
            ScreenEffectType::Tint => {
                let alpha = req.alpha.unwrap_or(0.5);
                effects.color = [color[0], color[1], color[2], alpha];
            }
        }
    }
    Ok(())
}

pub fn get_screen_state(world: &mut World) -> serde_json::Value {
    let effects = world
        .get_resource::<ScreenEffects>()
        .map(|e| {
            let active = e.active_effect.as_ref().map(|a| {
                let effect_name = match a.effect_type {
                    ScreenEffectType::FadeIn => "fade_in",
                    ScreenEffectType::FadeOut => "fade_out",
                    ScreenEffectType::Flash => "flash",
                    ScreenEffectType::Tint => "tint",
                };
                let progress = (a.elapsed / a.duration.max(0.001)).clamp(0.0, 1.0);
                serde_json::json!({
                    "effect": effect_name,
                    "duration": a.duration,
                    "elapsed": a.elapsed,
                    "progress": progress,
                })
            });
            serde_json::json!({
                "color": e.color,
                "active_effect": active,
            })
        })
        .unwrap_or_else(|| serde_json::json!({"color": [0,0,0,0], "active_effect": null}));
    effects
}
