use crate::components::*;
use crate::perf::PerfAccum;
use bevy::prelude::*;
use bevy::utils::Instant;

/// Marker for entities that received an auto-generated colored sprite.
#[derive(Component)]
pub struct FallbackSprite;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedFirst,
            save_previous_positions,
        )
        .add_systems(
            Update,
            (sync_game_position_to_transform, add_fallback_sprites),
        );
    }
}

/// At the start of each FixedUpdate tick, snapshot current positions so we can
/// interpolate between them during rendering.
fn save_previous_positions(
    mut commands: Commands,
    mut query: Query<(Entity, &GamePosition, Option<&mut PreviousGamePosition>)>,
) {
    for (entity, pos, prev) in query.iter_mut() {
        if let Some(mut prev) = prev {
            prev.x = pos.x;
            prev.y = pos.y;
        } else {
            commands.entity(entity).insert(PreviousGamePosition {
                x: pos.x,
                y: pos.y,
            });
        }
    }
}

/// Sync GamePosition -> Transform for all entities that have both.
/// Computes z from RenderLayer + y-position for proper depth sorting.
/// Supports pixel_snap (rounds to whole pixels) and interpolate_transforms
/// (smooth blending between FixedUpdate ticks).
pub(crate) fn sync_game_position_to_transform(
    mut perf: ResMut<PerfAccum>,
    config: Res<GameConfig>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(
        &GamePosition,
        &mut Transform,
        Option<&RenderLayer>,
        Option<&PreviousGamePosition>,
    )>,
) {
    let start = Instant::now();
    let alpha = if config.interpolate_transforms {
        fixed_time.overstep_fraction()
    } else {
        1.0
    };

    for (pos, mut transform, render_layer, prev_pos) in query.iter_mut() {
        let (mut x, mut y) = if alpha < 1.0 {
            if let Some(prev) = prev_pos {
                (
                    prev.x + (pos.x - prev.x) * alpha,
                    prev.y + (pos.y - prev.y) * alpha,
                )
            } else {
                (pos.x, pos.y)
            }
        } else {
            (pos.x, pos.y)
        };

        if config.pixel_snap {
            x = x.round();
            y = y.round();
        }

        transform.translation.x = x;
        transform.translation.y = y;
        let layer = render_layer.map_or(0, |r| r.0);
        let base_z = 10.0 + layer as f32 * 5.0;
        transform.translation.z = base_z + (-pos.y * 0.001);
    }
    perf.render_time_ms += start.elapsed().as_secs_f32() * 1000.0;
}

/// Give every entity that has a Collider + Tags but no Sprite (and is not a Player)
/// a colored rectangle so it is visible in windowed mode.
fn add_fallback_sprites(
    mut commands: Commands,
    headless: Res<HeadlessMode>,
    query: Query<
        (Entity, &Collider, &Tags),
        (Without<Sprite>, Without<Player>, Without<FallbackSprite>, Without<Invisible>),
    >,
) {
    if headless.0 {
        return;
    }
    for (entity, collider, tags) in query.iter() {
        if tags.0.contains("projectile") {
            continue;
        }
        let color = tag_to_color(&tags.0);
        commands.entity(entity).insert((
            Sprite::from_color(color, Vec2::new(collider.width, collider.height)),
            FallbackSprite,
        ));
    }
}

fn tag_to_color(tags: &std::collections::HashSet<String>) -> Color {
    if tags.contains("enemy") {
        Color::srgb(0.9, 0.2, 0.2)
    } else if tags.iter().any(|t| t.contains("pickup") || t.contains("health")) {
        Color::srgb(0.2, 0.9, 0.2)
    } else if tags.contains("projectile") {
        Color::srgb(0.9, 0.9, 0.2)
    } else if tags.contains("npc") {
        Color::srgb(0.2, 0.8, 0.9)
    } else if tags.contains("platform") {
        Color::srgb(0.5, 0.4, 0.3)
    } else {
        Color::srgb(0.7, 0.7, 0.7)
    }
}
