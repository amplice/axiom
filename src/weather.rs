use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::components::HeadlessMode;
use crate::events::GameEventBus;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WeatherSystem::default())
            .add_systems(Update, tick_weather);
    }
}

/// Global weather resource.
#[derive(Resource, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WeatherSystem {
    pub active: Option<WeatherConfig>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct WeatherConfig {
    pub weather_type: WeatherType,
    /// 0.0-1.0 intensity (controls spawn rate).
    pub intensity: f32,
    /// Horizontal wind offset.
    #[serde(default)]
    pub wind: f32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherType {
    Rain,
    Snow,
    Dust,
}

/// Marker for weather particle entities.
#[derive(Component)]
pub struct WeatherParticle {
    pub vx: f32,
    pub vy: f32,
    pub lifetime: f32,
    pub elapsed: f32,
}

fn tick_weather(
    mut commands: Commands,
    headless: Res<HeadlessMode>,
    time: Res<Time>,
    weather: Res<WeatherSystem>,
    camera_q: Query<&Transform, With<MainCamera>>,
    mut particles: Query<(Entity, &mut WeatherParticle, &mut Transform), Without<MainCamera>>,
) {
    if headless.0 {
        return;
    }
    let dt = time.delta_secs();

    // Update existing particles
    for (entity, mut wp, mut transform) in particles.iter_mut() {
        wp.elapsed += dt;
        if wp.elapsed >= wp.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.x += wp.vx * dt;
        transform.translation.y += wp.vy * dt;
    }

    // Spawn new particles if weather is active
    let Some(config) = &weather.active else {
        return;
    };

    let cam_pos = camera_q
        .get_single()
        .map(|t| Vec2::new(t.translation.x, t.translation.y))
        .unwrap_or(Vec2::ZERO);

    // Spawn rate based on intensity
    let spawn_count = (config.intensity * 5.0).ceil() as usize;
    let half_width = 520.0;
    let half_height = 320.0;

    for _ in 0..spawn_count {
        let rx = rand::random::<f32>() * half_width * 2.0 - half_width;
        let (color, vx, vy, size, lifetime) = match config.weather_type {
            WeatherType::Rain => {
                let c = Color::srgba(0.6, 0.7, 0.95, 0.7);
                (c, config.wind, -400.0 - rand::random::<f32>() * 100.0, Vec2::new(1.5, 8.0), 2.0)
            }
            WeatherType::Snow => {
                let drift = (rand::random::<f32>() - 0.5) * 40.0;
                let c = Color::srgba(0.95, 0.95, 1.0, 0.8);
                (c, config.wind + drift, -50.0 - rand::random::<f32>() * 30.0, Vec2::new(3.0, 3.0), 6.0)
            }
            WeatherType::Dust => {
                let c = Color::srgba(0.7, 0.6, 0.4, 0.5);
                (c, config.wind + 60.0 + rand::random::<f32>() * 30.0, (rand::random::<f32>() - 0.5) * 20.0, Vec2::new(2.0, 2.0), 4.0)
            }
        };

        let spawn_x = cam_pos.x + rx;
        let spawn_y = cam_pos.y + half_height + rand::random::<f32>() * 20.0;

        commands.spawn((
            WeatherParticle {
                vx,
                vy,
                lifetime,
                elapsed: 0.0,
            },
            Sprite::from_color(color, size),
            Transform::from_xyz(spawn_x, spawn_y, 200.0),
        ));
    }
}

pub fn apply_weather(weather: &mut WeatherSystem, events: &mut GameEventBus, weather_type: &str, intensity: f32, wind: f32) {
    let wt = match weather_type {
        "rain" => WeatherType::Rain,
        "snow" => WeatherType::Snow,
        "dust" => WeatherType::Dust,
        _ => return,
    };
    weather.active = Some(WeatherConfig {
        weather_type: wt,
        intensity: intensity.clamp(0.0, 1.0),
        wind,
    });
    events.emit(
        "weather_changed",
        serde_json::json!({ "type": weather_type, "intensity": intensity }),
        None,
    );
}

pub fn clear_weather(weather: &mut WeatherSystem, events: &mut GameEventBus) {
    weather.active = None;
    events.emit(
        "weather_changed",
        serde_json::json!({ "type": null, "intensity": 0 }),
        None,
    );
}
