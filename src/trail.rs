use bevy::prelude::*;

use crate::components::{Alive, GamePosition, HeadlessMode, RenderLayer};

pub struct TrailPlugin;

impl Plugin for TrailPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_trail_ghosts, tick_trail_ghosts)
                .chain()
                .run_if(crate::game_runtime::gameplay_systems_enabled),
        );
    }
}

/// Attached to an entity to produce afterimage/trail effects.
#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrailEffect {
    /// Frames between ghost spawns.
    pub interval: u32,
    /// Ghost lifetime in seconds.
    pub duration: f32,
    /// Starting alpha of the ghost.
    pub alpha_start: f32,
    /// Ending alpha of the ghost (faded out).
    pub alpha_end: f32,
    /// Internal frame counter.
    #[serde(default)]
    pub frame_counter: u32,
}

/// Marker on spawned ghost entities.
#[derive(Component)]
pub struct TrailGhost {
    pub lifetime: f32,
    pub elapsed: f32,
    pub alpha_start: f32,
    pub alpha_end: f32,
}

fn spawn_trail_ghosts(
    mut commands: Commands,
    headless: Res<HeadlessMode>,
    mut query: Query<(
        &GamePosition,
        &mut TrailEffect,
        Option<&Sprite>,
        Option<&RenderLayer>,
        Option<&Alive>,
    )>,
) {
    if headless.0 {
        return;
    }
    for (pos, mut trail, sprite, render_layer, alive) in query.iter_mut() {
        if alive.is_some_and(|a| !a.0) {
            continue;
        }
        trail.frame_counter += 1;
        if trail.interval == 0 || trail.frame_counter < trail.interval {
            continue;
        }
        trail.frame_counter = 0;

        let layer = render_layer.map_or(0, |r| r.0);
        let base_z = 10.0 + layer as f32 * 5.0 + (-pos.y * 0.001) - 0.01;

        let ghost_sprite = if let Some(s) = sprite {
            Sprite {
                image: s.image.clone(),
                color: s.color.with_alpha(trail.alpha_start),
                custom_size: s.custom_size,
                texture_atlas: s.texture_atlas.clone(),
                ..default()
            }
        } else {
            Sprite::from_color(
                Color::srgba(1.0, 1.0, 1.0, trail.alpha_start),
                Vec2::new(12.0, 14.0),
            )
        };

        commands.spawn((
            TrailGhost {
                lifetime: trail.duration.max(0.01),
                elapsed: 0.0,
                alpha_start: trail.alpha_start,
                alpha_end: trail.alpha_end,
            },
            ghost_sprite,
            Transform::from_xyz(pos.x, pos.y, base_z),
        ));
    }
}

fn tick_trail_ghosts(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut TrailGhost, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (entity, mut ghost, mut sprite) in query.iter_mut() {
        ghost.elapsed += dt;
        if ghost.elapsed >= ghost.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        let t = (ghost.elapsed / ghost.lifetime).clamp(0.0, 1.0);
        let alpha = ghost.alpha_start + (ghost.alpha_end - ghost.alpha_start) * t;
        sprite.color = sprite.color.with_alpha(alpha.clamp(0.0, 1.0));
    }
}
