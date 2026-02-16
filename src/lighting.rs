use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::api::types::{LightingConfigRequest, LightingStateResponse};
use crate::camera::MainCamera;
use crate::components::HeadlessMode;

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LightingConfig::default())
            .add_systems(PreStartup, generate_light_texture)
            .add_systems(Startup, spawn_darkness_overlay)
            .add_systems(
                Update,
                (sync_darkness_overlay, sync_light_sprites).chain(),
            );
    }
}

/// Per-entity point light component
#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct PointLight2d {
    pub color: [f32; 3],
    pub intensity: f32,
    pub radius: f32,
}

/// Global lighting configuration
#[derive(Resource, Clone)]
pub struct LightingConfig {
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    pub enabled: bool,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            ambient_color: [1.0, 1.0, 1.0],
            ambient_intensity: 1.0,
            enabled: false,
        }
    }
}

/// Stores the procedural radial gradient texture for light sprites
#[derive(Resource)]
pub struct LightingAssets {
    pub gradient_image: Handle<Image>,
}

/// Marker for the darkness overlay
#[derive(Component)]
pub struct DarknessOverlay;

/// Marker for a light sprite child entity
#[derive(Component)]
pub struct LightSprite {
    pub parent_light: Entity,
}

fn generate_light_texture(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = 64u32;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let center = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center + 0.5;
            let dy = y as f32 - center + 0.5;
            let dist = (dx * dx + dy * dy).sqrt() / center;
            let alpha = (1.0 - dist).clamp(0.0, 1.0);
            // Smooth falloff
            let alpha = alpha * alpha;
            data.push(255); // R
            data.push(255); // G
            data.push(255); // B
            data.push((alpha * 255.0) as u8);
        }
    }
    let image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );
    let handle = images.add(image);
    commands.insert_resource(LightingAssets {
        gradient_image: handle,
    });
}

fn spawn_darkness_overlay(mut commands: Commands, headless: Res<HeadlessMode>) {
    if headless.0 {
        return;
    }
    commands.spawn((
        DarknessOverlay,
        Sprite::from_color(
            Color::srgba(0.0, 0.0, 0.0, 0.0),
            Vec2::new(4096.0, 4096.0),
        ),
        Transform::from_xyz(0.0, 0.0, 250.0),
    ));
}

fn sync_darkness_overlay(
    config: Res<LightingConfig>,
    camera_q: Query<&Transform, (With<MainCamera>, Without<DarknessOverlay>)>,
    mut overlay_q: Query<(&mut Sprite, &mut Transform), With<DarknessOverlay>>,
) {
    let Ok((mut sprite, mut transform)) = overlay_q.get_single_mut() else {
        return;
    };
    // Follow camera
    if let Ok(cam_transform) = camera_q.get_single() {
        transform.translation.x = cam_transform.translation.x;
        transform.translation.y = cam_transform.translation.y;
    }
    if !config.enabled {
        sprite.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        return;
    }
    let alpha = (1.0 - config.ambient_intensity).clamp(0.0, 1.0);
    sprite.color = Color::srgba(
        config.ambient_color[0] * (1.0 - config.ambient_intensity),
        config.ambient_color[1] * (1.0 - config.ambient_intensity),
        config.ambient_color[2] * (1.0 - config.ambient_intensity),
        alpha,
    );
}

fn sync_light_sprites(
    mut commands: Commands,
    config: Res<LightingConfig>,
    lighting_assets: Option<Res<LightingAssets>>,
    lights: Query<(Entity, &PointLight2d, &crate::components::GamePosition)>,
    existing_light_sprites: Query<(Entity, &LightSprite)>,
) {
    let Some(assets) = lighting_assets else {
        return;
    };
    if !config.enabled {
        // Remove all light sprites when disabled
        for (entity, _) in existing_light_sprites.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    // Track which parent entities still have lights
    let mut active_parents = std::collections::HashSet::new();

    for (entity, light, pos) in lights.iter() {
        active_parents.insert(entity);

        // Check if this entity already has a light sprite
        let existing = existing_light_sprites
            .iter()
            .find(|(_, ls)| ls.parent_light == entity);

        let scale = light.radius * 2.0 / 64.0; // 64 = texture size
        let color = Color::srgba(
            light.color[0] * light.intensity,
            light.color[1] * light.intensity,
            light.color[2] * light.intensity,
            light.intensity.clamp(0.0, 1.0),
        );

        if let Some((sprite_entity, _)) = existing {
            // Update existing light sprite
            commands.entity(sprite_entity).insert((
                Sprite {
                    image: assets.gradient_image.clone(),
                    color,
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y, 251.0)
                    .with_scale(Vec3::new(scale, scale, 1.0)),
            ));
        } else {
            // Spawn new light sprite
            commands.spawn((
                LightSprite {
                    parent_light: entity,
                },
                Sprite {
                    image: assets.gradient_image.clone(),
                    color,
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y, 251.0)
                    .with_scale(Vec3::new(scale, scale, 1.0)),
            ));
        }
    }

    // Remove orphaned light sprites
    for (entity, ls) in existing_light_sprites.iter() {
        if !active_parents.contains(&ls.parent_light) {
            commands.entity(entity).despawn();
        }
    }
}

pub fn apply_lighting_config(
    world: &mut World,
    req: LightingConfigRequest,
) -> Result<(), String> {
    if let Some(mut config) = world.get_resource_mut::<LightingConfig>() {
        if let Some(enabled) = req.enabled {
            config.enabled = enabled;
        }
        if let Some(intensity) = req.ambient_intensity {
            config.ambient_intensity = intensity.clamp(0.0, 1.0);
        }
        if let Some(color) = req.ambient_color {
            config.ambient_color = color;
        }
    }
    Ok(())
}

pub fn get_lighting_state(world: &mut World) -> LightingStateResponse {
    let config = world
        .get_resource::<LightingConfig>()
        .cloned()
        .unwrap_or_default();
    let light_count = world
        .query::<&PointLight2d>()
        .iter(world)
        .count();
    LightingStateResponse {
        enabled: config.enabled,
        ambient_intensity: config.ambient_intensity,
        ambient_color: config.ambient_color,
        light_count,
    }
}
