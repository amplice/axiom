use bevy::prelude::*;
use crate::components::*;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_game_position_to_transform);
    }
}

/// Sync GamePosition → Transform for all entities that have both
fn sync_game_position_to_transform(
    mut query: Query<(&GamePosition, &mut Transform), Changed<GamePosition>>,
) {
    for (pos, mut transform) in query.iter_mut() {
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
    }
}
