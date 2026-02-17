use bevy::prelude::*;

use crate::components::{GamePosition, HeadlessMode, NetworkId};

pub struct WorldTextPlugin;

impl Plugin for WorldTextPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldTextIdCounter(1))
            .add_systems(
                Update,
                (spawn_world_text_visuals, update_world_text, sync_world_text_position)
                    .chain(),
            );
    }
}

#[derive(Resource)]
pub struct WorldTextIdCounter(pub u64);

/// Component for world-space floating text.
#[derive(Component, Clone)]
pub struct WorldText {
    pub text_id: u64,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub offset: Vec2,
    pub owner_entity: Option<u64>,
    pub duration: Option<f32>,
    pub elapsed: f32,
    pub fade: bool,
    pub rise_speed: f32,
}

/// Marker indicating the text2d bundle has been spawned.
#[derive(Component)]
pub struct WorldTextSpawned;

fn spawn_world_text_visuals(
    mut commands: Commands,
    headless: Res<HeadlessMode>,
    query: Query<(Entity, &WorldText, &Transform), Without<WorldTextSpawned>>,
) {
    if headless.0 {
        return;
    }
    for (entity, wt, _transform) in query.iter() {
        let color = Color::srgba(wt.color[0], wt.color[1], wt.color[2], wt.color[3]);
        commands.entity(entity).insert((
            Text2d::new(wt.text.clone()),
            TextFont {
                font_size: wt.font_size,
                ..default()
            },
            TextColor(color),
            WorldTextSpawned,
        ));
    }
}

fn update_world_text(
    mut commands: Commands,
    time: Res<Time>,
    mut events: ResMut<crate::events::GameEventBus>,
    mut query: Query<(Entity, &mut WorldText, &mut Transform, Option<&mut TextColor>)>,
) {
    let dt = time.delta_secs();
    for (entity, mut wt, mut transform, text_color) in query.iter_mut() {
        wt.elapsed += dt;

        // Apply rise (only for non-owner texts; owner texts get rise in sync_world_text_position)
        if wt.owner_entity.is_none() && wt.rise_speed.abs() > 0.001 {
            transform.translation.y += wt.rise_speed * dt;
        }

        // Apply fade
        if wt.fade {
            if let Some(duration) = wt.duration {
                if duration > 0.0 {
                    let t = (wt.elapsed / duration).clamp(0.0, 1.0);
                    let alpha = wt.color[3] * (1.0 - t);
                    if let Some(mut tc) = text_color {
                        tc.0 = Color::srgba(wt.color[0], wt.color[1], wt.color[2], alpha);
                    }
                }
            }
        }

        // Check expiry
        if let Some(duration) = wt.duration {
            if wt.elapsed >= duration {
                events.emit(
                    "world_text_expired",
                    serde_json::json!({ "id": wt.text_id }),
                    None,
                );
                commands.entity(entity).despawn();
            }
        }
    }
}

fn sync_world_text_position(
    mut text_query: Query<(&WorldText, &mut Transform), With<WorldTextSpawned>>,
    entity_query: Query<(&NetworkId, &GamePosition)>,
) {
    // Build lookup once, then iterate texts — O(N+M) instead of O(N*M)
    let owner_positions: std::collections::HashMap<u64, (f32, f32)> = entity_query
        .iter()
        .map(|(nid, pos)| (nid.0, (pos.x, pos.y)))
        .collect();

    for (wt, mut transform) in text_query.iter_mut() {
        if let Some(owner_id) = wt.owner_entity {
            if let Some(&(ox, oy)) = owner_positions.get(&owner_id) {
                transform.translation.x = ox + wt.offset.x;
                // Base Y tracks owner; rise_speed is applied additively in update_world_text
                transform.translation.y = oy + wt.offset.y + wt.rise_speed * wt.elapsed;
            }
        }
    }
}
