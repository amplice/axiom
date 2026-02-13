use bevy::prelude::*;
use crate::components::*;
use crate::tilemap::Tilemap;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player);
    }
}

fn spawn_player(mut commands: Commands, tilemap: Res<Tilemap>) {
    let (sx, sy) = tilemap.player_spawn;
    commands.spawn((
        Player,
        GamePosition { x: sx, y: sy },
        Velocity::default(),
        Grounded(false),
        CoyoteTimer::default(),
        JumpBuffer::default(),
        Alive(true),
        Sprite::from_color(Color::srgb(0.2, 0.4, 0.9), Vec2::new(12.0, 14.0)),
        Transform::from_xyz(sx, sy, 10.0),
    ));
}
