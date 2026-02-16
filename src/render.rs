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
            Update,
            (sync_game_position_to_transform, add_fallback_sprites),
        );
    }
}

/// Sync GamePosition -> Transform for all entities that have both.
/// Computes z from RenderLayer + y-position for proper depth sorting.
fn sync_game_position_to_transform(
    mut perf: ResMut<PerfAccum>,
    mut query: Query<
        (&GamePosition, &mut Transform, Option<&RenderLayer>),
        Or<(Changed<GamePosition>, Changed<RenderLayer>)>,
    >,
) {
    let start = Instant::now();
    for (pos, mut transform, render_layer) in query.iter_mut() {
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
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
        (Without<Sprite>, Without<Player>, Without<FallbackSprite>),
    >,
) {
    if headless.0 {
        return;
    }
    for (entity, collider, tags) in query.iter() {
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
