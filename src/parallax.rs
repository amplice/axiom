use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::components::HeadlessMode;

pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ParallaxConfig::default())
            .add_systems(Update, sync_parallax_layers);
    }
}

/// Global parallax background configuration.
#[derive(Resource, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ParallaxConfig {
    pub layers: Vec<ParallaxLayerDef>,
}

/// Definition for a single parallax layer.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ParallaxLayerDef {
    #[serde(default)]
    pub texture: Option<String>,
    #[serde(default)]
    pub color: Option<[f32; 4]>,
    /// 0.0 = static background, 1.0 = moves at camera speed.
    #[serde(default)]
    pub scroll_factor: f32,
    /// Z depth (negative = behind tiles at z=0). E.g. -10.
    #[serde(default = "default_z_depth")]
    pub z_depth: f32,
    #[serde(default = "default_true")]
    pub repeat_x: bool,
    #[serde(default)]
    pub repeat_y: bool,
    #[serde(default)]
    pub scale: Option<f32>,
}

fn default_z_depth() -> f32 {
    -10.0
}
fn default_true() -> bool {
    true
}

/// Marker for parallax layer entities.
#[derive(Component)]
pub struct ParallaxLayerEntity {
    pub index: usize,
}

fn sync_parallax_layers(
    mut commands: Commands,
    headless: Res<HeadlessMode>,
    config: Res<ParallaxConfig>,
    camera_q: Query<&Transform, With<MainCamera>>,
    mut existing: Query<(Entity, &ParallaxLayerEntity, &mut Transform), Without<MainCamera>>,
) {
    if headless.0 {
        return;
    }

    let cam_pos = camera_q
        .get_single()
        .map(|t| Vec2::new(t.translation.x, t.translation.y))
        .unwrap_or(Vec2::ZERO);

    // Remove layers that no longer exist in config
    for (entity, layer, _) in existing.iter() {
        if layer.index >= config.layers.len() {
            commands.entity(entity).despawn();
        }
    }

    for (i, layer_def) in config.layers.iter().enumerate() {
        let parallax_x = cam_pos.x * layer_def.scroll_factor;
        let parallax_y = cam_pos.y * layer_def.scroll_factor;

        // Find existing entity for this index
        let found = existing.iter_mut().find(|(_, pl, _)| pl.index == i);

        if let Some((_entity, _, mut transform)) = found {
            transform.translation.x = parallax_x;
            transform.translation.y = parallax_y;
            transform.translation.z = layer_def.z_depth;
        } else {
            // Spawn new layer entity
            let color = layer_def.color.unwrap_or([0.1, 0.1, 0.2, 1.0]);
            let scale = layer_def.scale.unwrap_or(1.0);
            commands.spawn((
                ParallaxLayerEntity { index: i },
                Sprite::from_color(
                    Color::srgba(color[0], color[1], color[2], color[3]),
                    Vec2::new(4096.0 * scale, 4096.0 * scale),
                ),
                Transform::from_xyz(parallax_x, parallax_y, layer_def.z_depth),
            ));
        }
    }
}
