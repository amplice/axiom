use crate::components::HeadlessMode;
use crate::components::{Collider, CollisionLayer, GamePosition, Hitbox, PathFollower, TriggerZone};
use crate::perf::PerfStats;
use crate::spatial_hash::SpatialHash;
use bevy::gizmos::config::GizmoConfigStore;
use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct DebugOverlayConfig {
    pub show: bool,
    pub features: HashSet<String>,
}

pub struct DebugPlugin;
#[derive(Component)]
struct DebugOverlayText;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DebugOverlayConfig::default())
            .add_systems(Startup, setup_debug_overlay_text)
            .add_systems(Update, update_debug_overlay_text)
            .add_systems(
                Update,
                draw_debug_overlay.run_if(resource_exists::<GizmoConfigStore>),
            );
    }
}

fn setup_debug_overlay_text(mut commands: Commands, headless: Res<HeadlessMode>) {
    if headless.0 {
        return;
    }
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgba(0.95, 1.0, 0.98, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(8.0),
            top: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
        DebugOverlayText,
    ));
}

fn update_debug_overlay_text(
    config: Res<DebugOverlayConfig>,
    perf: Res<PerfStats>,
    script_errors: Res<crate::scripting::vm::ScriptErrors>,
    mut query: Query<(&mut Text, &mut Visibility), With<DebugOverlayText>>,
) {
    let Ok((mut text, mut visibility)) = query.get_single_mut() else {
        return;
    };
    let show_all = config.features.is_empty();
    let show_stats = show_all
        || config.features.contains("stats")
        || config.features.contains("fps")
        || config.features.contains("entity_count");
    let show_script_errors = config.features.contains("script_errors");
    if config.show && (show_stats || show_script_errors) {
        *visibility = Visibility::Visible;
        let mut output = String::new();
        if show_stats {
            output = format!(
                "FPS: {:.0}\nEntities: {}\nPhysics: {:.2} ms\nScripts: {:.2} ms\nRender: {:.2} ms",
                perf.fps,
                perf.entity_count,
                perf.physics_time_ms,
                perf.script_time_ms,
                perf.render_time_ms
            );
        }
        if show_script_errors && !script_errors.entries.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str("Script Errors:");
            let start = script_errors.entries.len().saturating_sub(5);
            for err in &script_errors.entries[start..] {
                let entity_str = err
                    .entity_id
                    .map(|id| format!(" #{}", id))
                    .unwrap_or_default();
                output.push_str(&format!("\n[{}]{}: {}", err.script_name, entity_str, err.error_message));
            }
        }
        text.0 = output;
    } else {
        *visibility = Visibility::Hidden;
    }
}

fn draw_debug_overlay(
    config: Res<DebugOverlayConfig>,
    mut gizmos: Gizmos,
    colliders: Query<(&GamePosition, &Collider, Option<&CollisionLayer>)>,
    triggers: Query<(&GamePosition, &TriggerZone)>,
    paths: Query<(&GamePosition, &PathFollower)>,
    hitboxes: Query<(&GamePosition, &Hitbox)>,
    spatial: Option<Res<SpatialHash>>,
) {
    if !config.show {
        return;
    }

    let show_all = config.features.is_empty();
    let show_colliders = show_all || config.features.contains("colliders");
    let show_triggers = show_all || config.features.contains("triggers");
    let show_paths = show_all || config.features.contains("paths");
    let show_spatial_hash = show_all || config.features.contains("spatial_hash");
    let show_hitboxes = show_all || config.features.contains("hitboxes");

    if show_colliders {
        for (pos, collider, cl) in colliders.iter() {
            // Tint collider color by collision layer for visual distinction
            let color = if let Some(cl) = cl {
                let hue = (cl.layer as f32 * 0.618) % 1.0;
                Color::hsla(hue * 360.0, 0.8, 0.55, 0.9)
            } else {
                Color::srgba(0.15, 1.0, 0.2, 0.9)
            };
            gizmos.rect_2d(
                Vec2::new(pos.x, pos.y),
                Vec2::new(collider.width, collider.height),
                color,
            );
        }
    }

    if show_triggers {
        for (pos, trigger) in triggers.iter() {
            gizmos.circle_2d(
                Vec2::new(pos.x, pos.y),
                trigger.radius,
                Color::srgba(0.25, 0.55, 1.0, 0.85),
            );
        }
    }

    if show_hitboxes {
        for (pos, hitbox) in hitboxes.iter() {
            let color = if hitbox.active {
                Color::srgba(1.0, 0.2, 0.15, 0.9)
            } else {
                Color::srgba(0.6, 0.3, 0.15, 0.4)
            };
            gizmos.rect_2d(
                Vec2::new(pos.x + hitbox.offset.x, pos.y + hitbox.offset.y),
                Vec2::new(hitbox.width, hitbox.height),
                color,
            );
        }
    }

    if show_paths {
        for (pos, follower) in paths.iter() {
            let mut from = Vec2::new(pos.x, pos.y);
            for p in &follower.path {
                let to = Vec2::new(p.x, p.y);
                gizmos.line_2d(from, to, Color::srgba(1.0, 0.9, 0.1, 0.95));
                from = to;
            }
            gizmos.circle_2d(
                Vec2::new(follower.target.x, follower.target.y),
                2.0,
                Color::srgba(1.0, 0.85, 0.2, 1.0),
            );
        }
    }

    if show_spatial_hash {
        if let Some(hash) = spatial {
            let cs = hash.cell_size;
            for &(cx, cy) in hash.cells.keys() {
                let center = Vec2::new((cx as f32 + 0.5) * cs, (cy as f32 + 0.5) * cs);
                gizmos.rect_2d(center, Vec2::splat(cs), Color::srgba(0.0, 0.95, 1.0, 0.22));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_plugin_does_not_panic_in_headless_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(HeadlessMode(true))
            .insert_resource(PerfStats::default())
            .insert_resource(SpatialHash::new(64.0))
            .insert_resource(crate::scripting::vm::ScriptErrors::default())
            .add_plugins(DebugPlugin);
        app.update();
    }
}
