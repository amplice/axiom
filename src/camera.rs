use bevy::prelude::*;
use crate::components::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, camera_follow);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Transform::from_xyz(0.0, 0.0, 100.0),
    ));
}

fn camera_follow(
    player_query: Query<&GamePosition, With<Player>>,
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<Player>)>,
) {
    let Ok(player_pos) = player_query.get_single() else { return };
    let Ok(mut cam_transform) = camera_query.get_single_mut() else { return };

    // Smooth follow
    let target = Vec3::new(player_pos.x, player_pos.y, cam_transform.translation.z);
    let lerp_speed = 0.1;
    cam_transform.translation = cam_transform.translation.lerp(target, lerp_speed);
}
