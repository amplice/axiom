use crate::components::*;
use crate::perf::PerfAccum;
use bevy::prelude::*;
use bevy::utils::Instant;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_game_position_to_transform);
    }
}

/// Sync GamePosition → Transform for all entities that have both
fn sync_game_position_to_transform(
    mut perf: ResMut<PerfAccum>,
    mut query: Query<(&GamePosition, &mut Transform), Changed<GamePosition>>,
) {
    let start = Instant::now();
    for (pos, mut transform) in query.iter_mut() {
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
    }
    perf.render_time_ms += start.elapsed().as_secs_f32() * 1000.0;
}
