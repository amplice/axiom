use crate::components::*;
use crate::tilemap::Tilemap;
use bevy::prelude::*;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player);
    }
}

fn spawn_player(
    mut commands: Commands,
    tilemap: Res<Tilemap>,
    config: Res<GameConfig>,
    headless: Res<HeadlessMode>,
    mut next_network_id: ResMut<NextNetworkId>,
) {
    let (sx, sy) = tilemap.player_spawn;
    let network_id = NetworkId(next_network_id.0);
    next_network_id.0 = next_network_id.0.saturating_add(1);

    let mut entity = commands.spawn((
        network_id,
        Player,
        GamePosition { x: sx, y: sy },
        Velocity::default(),
        Grounded(false),
        CoyoteTimer::default(),
        JumpBuffer::default(),
        Alive(true),
        Tags(std::collections::HashSet::from(["player".to_string()])),
        Collider {
            width: 12.0,
            height: 14.0,
        },
        GravityBody,
        HorizontalMover {
            speed: config.move_speed,
            left_action: "left".into(),
            right_action: "right".into(),
        },
        Jumper {
            velocity: config.jump_velocity,
            action: "jump".into(),
            fall_multiplier: config.fall_multiplier,
            variable_height: true,
            coyote_frames: config.coyote_frames,
            buffer_frames: config.jump_buffer_frames,
        },
        AnimationController {
            graph: "samurai_player".to_string(),
            state: "idle".to_string(),
            frame: 0,
            timer: 0.0,
            speed: 1.0,
            playing: true,
            facing_right: true,
            auto_from_velocity: true,
        },
        Transform::from_xyz(sx, sy, 10.0),
    ));

    if !headless.0 {
        entity.insert(Sprite::from_color(
            Color::srgb(0.2, 0.4, 0.9),
            Vec2::new(12.0, 14.0),
        ));
    }
}
