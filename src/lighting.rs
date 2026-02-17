use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::api::types::{LightingConfigRequest, LightingStateResponse};
use crate::camera::MainCamera;
use crate::components::HeadlessMode;

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LightingConfig::default())
            .insert_resource(DayNightCycle::default())
            .add_systems(PreStartup, generate_light_texture)
            .add_systems(Startup, spawn_darkness_overlay)
            .add_systems(
                Update,
                (tick_day_night, sync_darkness_overlay, sync_light_sprites).chain(),
            );
    }
}

/// Day/night cycle resource.
#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct DayNightCycle {
    pub enabled: bool,
    /// 0.0-24.0 hours.
    pub time_of_day: f32,
    /// Real seconds per game hour.
    pub speed: f32,
    pub phases: Vec<DayPhase>,
    #[serde(skip)]
    pub current_phase: String,
}

impl Default for DayNightCycle {
    fn default() -> Self {
        Self {
            enabled: false,
            time_of_day: 12.0,
            speed: 60.0,
            phases: vec![
                DayPhase { name: "night".into(), start_hour: 0.0, ambient_intensity: 0.15, ambient_color: [0.1, 0.1, 0.3] },
                DayPhase { name: "dawn".into(), start_hour: 6.0, ambient_intensity: 0.6, ambient_color: [0.9, 0.6, 0.4] },
                DayPhase { name: "day".into(), start_hour: 8.0, ambient_intensity: 1.0, ambient_color: [1.0, 1.0, 1.0] },
                DayPhase { name: "dusk".into(), start_hour: 18.0, ambient_intensity: 0.5, ambient_color: [0.9, 0.5, 0.3] },
                DayPhase { name: "night".into(), start_hour: 20.0, ambient_intensity: 0.15, ambient_color: [0.1, 0.1, 0.3] },
            ],
            current_phase: "day".into(),
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DayPhase {
    pub name: String,
    pub start_hour: f32,
    pub ambient_intensity: f32,
    pub ambient_color: [f32; 3],
}

fn tick_day_night(
    time: Res<Time>,
    mut cycle: ResMut<DayNightCycle>,
    mut config: ResMut<LightingConfig>,
    mut events: ResMut<crate::events::GameEventBus>,
) {
    if !cycle.enabled || cycle.phases.is_empty() {
        return;
    }

    let dt = time.delta_secs();
    let hours_per_second = if cycle.speed > 0.0 { 1.0 / cycle.speed } else { 0.0 };
    cycle.time_of_day += hours_per_second * dt;
    if cycle.time_of_day >= 24.0 {
        cycle.time_of_day -= 24.0;
    }

    let t = cycle.time_of_day;

    // Find current and next phase
    let mut current_idx = 0;
    for (i, phase) in cycle.phases.iter().enumerate() {
        if t >= phase.start_hour {
            current_idx = i;
        }
    }
    let next_idx = (current_idx + 1) % cycle.phases.len();
    let current = &cycle.phases[current_idx];
    let next = &cycle.phases[next_idx];

    let span = if next.start_hour > current.start_hour {
        next.start_hour - current.start_hour
    } else {
        (24.0 - current.start_hour) + next.start_hour
    };
    let progress = if span > 0.0 {
        let elapsed = if t >= current.start_hour {
            t - current.start_hour
        } else {
            (24.0 - current.start_hour) + t
        };
        (elapsed / span).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Lerp ambient
    config.ambient_intensity = current.ambient_intensity + (next.ambient_intensity - current.ambient_intensity) * progress;
    for i in 0..3 {
        config.ambient_color[i] = current.ambient_color[i] + (next.ambient_color[i] - current.ambient_color[i]) * progress;
    }

    // Check phase change (clone to release borrow on cycle.phases)
    let new_phase = cycle.phases[current_idx].name.clone();
    if new_phase != cycle.current_phase {
        let old = cycle.current_phase.clone();
        cycle.current_phase = new_phase.clone();
        events.emit(
            "day_phase_changed",
            serde_json::json!({ "phase": new_phase, "hour": t, "previous": old }),
            None,
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
    let cycle = world
        .get_resource::<DayNightCycle>()
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
        time_of_day: if cycle.enabled { Some(cycle.time_of_day) } else { None },
        day_phase: if cycle.enabled { Some(cycle.current_phase.clone()) } else { None },
    }
}

pub fn apply_day_night_config(
    world: &mut World,
    req: crate::api::types::DayNightRequest,
) -> Result<(), String> {
    let mut should_enable_lighting = false;
    if let Some(mut cycle) = world.get_resource_mut::<DayNightCycle>() {
        if let Some(enabled) = req.enabled {
            cycle.enabled = enabled;
            if enabled {
                should_enable_lighting = true;
            }
        }
        if let Some(time) = req.time_of_day {
            cycle.time_of_day = time % 24.0;
        }
        if let Some(speed) = req.speed {
            cycle.speed = speed.max(0.01);
        }
        if let Some(phases) = req.phases {
            cycle.phases = phases
                .into_iter()
                .map(|p| DayPhase {
                    name: p.name,
                    start_hour: p.start_hour,
                    ambient_intensity: p.ambient_intensity,
                    ambient_color: p.ambient_color,
                })
                .collect();
        }
    }
    if should_enable_lighting {
        if let Some(mut config) = world.get_resource_mut::<LightingConfig>() {
            config.enabled = true;
        }
    }
    Ok(())
}

pub fn get_day_night_state(world: &mut World) -> crate::api::types::DayNightResponse {
    let cycle = world
        .get_resource::<DayNightCycle>()
        .cloned()
        .unwrap_or_default();
    crate::api::types::DayNightResponse {
        enabled: cycle.enabled,
        time_of_day: cycle.time_of_day,
        speed: cycle.speed,
        current_phase: cycle.current_phase.clone(),
        phases: cycle
            .phases
            .iter()
            .map(|p| crate::api::types::DayPhaseRequest {
                name: p.name.clone(),
                start_hour: p.start_hour,
                ambient_intensity: p.ambient_intensity,
                ambient_color: p.ambient_color,
            })
            .collect(),
    }
}
