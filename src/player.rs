use bevy::prelude::*;
use crate::components::*;
use crate::tilemap::Tilemap;
use crate::sprites::{SpriteAssets, PlayerAnimations, AnimationState};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player);
    }
}

fn spawn_player(
    mut commands: Commands,
    tilemap: Res<Tilemap>,
    sprite_assets: Res<SpriteAssets>,
    player_anims: Res<PlayerAnimations>,
) {
    let (sx, sy) = tilemap.player_spawn;

    // Use animated sprite sheet if available, otherwise fallback
    let sprite = if let Some(ref idle_data) = player_anims.idle {
        Sprite::from_atlas_image(
            idle_data.texture.clone(),
            TextureAtlas {
                layout: idle_data.layout.clone(),
                index: 0,
            },
        )
    } else if let Some(handle) = sprite_assets.get("player") {
        Sprite {
            image: handle.clone(),
            custom_size: Some(Vec2::new(12.0, 14.0)),
            ..default()
        }
    } else {
        Sprite::from_color(Color::srgb(0.2, 0.4, 0.9), Vec2::new(12.0, 14.0))
    };

    commands.spawn((
        Player,
        GamePosition { x: sx, y: sy },
        Velocity::default(),
        Grounded(false),
        CoyoteTimer::default(),
        JumpBuffer::default(),
        Alive(true),
        AnimationState::default(),
        sprite,
        Transform::from_xyz(sx, sy, 10.0),
    ));
}
