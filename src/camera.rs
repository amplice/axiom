use bevy::prelude::*;

use crate::components::{GamePosition, HeadlessMode, NetworkId, Player};
use crate::events::GameEventBus;

#[derive(Clone)]
pub struct CameraBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

#[derive(Resource, Clone)]
pub struct CameraConfig {
    pub follow_target: Option<u64>,
    pub follow_speed: f32,
    pub zoom: f32,
    pub offset: Vec2,
    pub deadzone: Vec2,
    pub bounds: Option<CameraBounds>,
    pub look_at: Option<Vec2>,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            follow_target: None,
            follow_speed: 0.1,
            zoom: 1.0,
            offset: Vec2::ZERO,
            deadzone: Vec2::new(8.0, 8.0),
            bounds: None,
            look_at: None,
        }
    }
}

#[derive(Resource, Clone)]
pub struct CameraShakeState {
    pub intensity: f32,
    pub remaining: f32,
    pub duration: f32,
    pub decay: f32,
}

impl Default for CameraShakeState {
    fn default() -> Self {
        Self {
            intensity: 0.0,
            remaining: 0.0,
            duration: 0.0,
            decay: 1.0,
        }
    }
}

#[derive(Resource, Default)]
struct CameraRuntimeState {
    base: Vec2,
}

#[derive(Resource, Default)]
struct CameraEventCursor {
    last_frame: u64,
    processed_in_frame: usize,
}

#[derive(Component)]
pub struct MainCamera;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CameraConfig::default())
            .insert_resource(CameraShakeState::default())
            .insert_resource(CameraRuntimeState::default())
            .insert_resource(CameraEventCursor::default())
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    apply_camera_events,
                    camera_follow,
                    camera_zoom,
                    camera_shake,
                )
                    .chain(),
            );
    }
}

fn apply_camera_events(
    bus: Res<GameEventBus>,
    mut config: ResMut<CameraConfig>,
    mut shake: ResMut<CameraShakeState>,
    mut cursor: ResMut<CameraEventCursor>,
) {
    let mut count_in_frame = 0usize;
    for ev in bus.recent.iter() {
        if ev.frame < cursor.last_frame {
            continue;
        }
        if ev.frame == cursor.last_frame {
            count_in_frame = count_in_frame.saturating_add(1);
            if count_in_frame <= cursor.processed_in_frame {
                continue;
            }
        } else {
            count_in_frame = 1;
        }

        match ev.name.as_str() {
            "camera_shake" => {
                if let Some(intensity) = ev.data.get("intensity").and_then(|v| v.as_f64()) {
                    shake.intensity = (intensity as f32).max(0.0);
                }
                if let Some(duration) = ev.data.get("duration").and_then(|v| v.as_f64()) {
                    let duration = (duration as f32).max(0.0);
                    shake.duration = duration;
                    shake.remaining = duration;
                }
                if let Some(decay) = ev.data.get("decay").and_then(|v| v.as_f64()) {
                    shake.decay = (decay as f32).max(0.01);
                }
            }
            "camera_zoom" => {
                if let Some(zoom) = ev.data.get("zoom").and_then(|v| v.as_f64()) {
                    config.zoom = (zoom as f32).max(0.05);
                }
            }
            "camera_look_at" => {
                let x = ev.data.get("x").and_then(|v| v.as_f64()).map(|v| v as f32);
                let y = ev.data.get("y").and_then(|v| v.as_f64()).map(|v| v as f32);
                if let (Some(x), Some(y)) = (x, y) {
                    config.look_at = Some(Vec2::new(x, y));
                }
            }
            _ => {}
        }

        cursor.last_frame = ev.frame;
        cursor.processed_in_frame = count_in_frame;
    }
}

fn spawn_camera(mut commands: Commands, headless: Res<HeadlessMode>) {
    if headless.0 {
        return;
    }
    commands.spawn((MainCamera, Camera2d, Transform::from_xyz(0.0, 0.0, 100.0)));
}

fn camera_follow(
    time: Res<Time>,
    config: Res<CameraConfig>,
    mut runtime: ResMut<CameraRuntimeState>,
    player_query: Query<&GamePosition, With<Player>>,
    id_query: Query<(&GamePosition, &NetworkId)>,
    mut camera_query: Query<&mut Transform, With<MainCamera>>,
) {
    let Ok(mut cam_transform) = camera_query.get_single_mut() else {
        return;
    };

    let explicit_target = config
        .look_at
        .or_else(|| resolve_follow_target(config.follow_target, &id_query))
        .or_else(|| player_query.get_single().ok().map(|p| Vec2::new(p.x, p.y)));
    let Some(mut target) = explicit_target else {
        return;
    };

    target += config.offset;
    let current = runtime.base;
    if (target.x - current.x).abs() < config.deadzone.x {
        target.x = current.x;
    }
    if (target.y - current.y).abs() < config.deadzone.y {
        target.y = current.y;
    }

    if let Some(bounds) = &config.bounds {
        target.x = target.x.clamp(bounds.min_x, bounds.max_x);
        target.y = target.y.clamp(bounds.min_y, bounds.max_y);
    }

    let follow_speed = if config.follow_speed.is_finite() {
        config.follow_speed
    } else {
        1.0
    };
    let alpha = (follow_speed * time.delta_secs() * 60.0).clamp(0.0, 1.0);
    runtime.base = current.lerp(target, alpha);
    cam_transform.translation.x = runtime.base.x;
    cam_transform.translation.y = runtime.base.y;
}

fn resolve_follow_target(
    target_id: Option<u64>,
    id_query: &Query<(&GamePosition, &NetworkId)>,
) -> Option<Vec2> {
    let id = target_id?;
    id_query
        .iter()
        .find(|(_, network_id)| network_id.0 == id)
        .map(|(pos, _)| Vec2::new(pos.x, pos.y))
}

fn camera_zoom(config: Res<CameraConfig>, mut query: Query<&mut Projection, With<MainCamera>>) {
    let Ok(mut projection) = query.get_single_mut() else {
        return;
    };
    let zoom = config.zoom.max(0.05);
    if let Projection::Orthographic(ref mut ortho) = *projection {
        ortho.scale = 1.0 / zoom;
    }
}

fn camera_shake(
    time: Res<Time>,
    mut shake: ResMut<CameraShakeState>,
    runtime: Res<CameraRuntimeState>,
    mut camera_query: Query<&mut Transform, With<MainCamera>>,
) {
    let Ok(mut cam_transform) = camera_query.get_single_mut() else {
        return;
    };

    let mut offset = Vec2::ZERO;
    if shake.remaining > 0.0 && shake.intensity > 0.0 {
        shake.remaining = (shake.remaining - time.delta_secs()).max(0.0);
        let t = time.elapsed_secs();
        let life = if shake.duration > 0.0 {
            (shake.remaining / shake.duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let strength = shake.intensity * life.powf(shake.decay.max(0.01));
        offset.x = (t * 31.0).sin() * strength;
        offset.y = (t * 43.0).cos() * strength;
    }

    cam_transform.translation.x = runtime.base.x + offset.x;
    cam_transform.translation.y = runtime.base.y + offset.y;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_event_system_applies_script_events() {
        let mut app = App::new();
        app.insert_resource(GameEventBus::default())
            .insert_resource(CameraConfig::default())
            .insert_resource(CameraShakeState::default())
            .insert_resource(CameraEventCursor::default())
            .add_systems(Update, apply_camera_events);

        {
            let mut bus = app.world_mut().resource_mut::<GameEventBus>();
            bus.frame = 1;
            bus.emit(
                "camera_shake",
                serde_json::json!({"intensity": 0.8, "duration": 0.4}),
                None,
            );
            bus.emit("camera_zoom", serde_json::json!({"zoom": 1.5}), None);
            bus.emit(
                "camera_look_at",
                serde_json::json!({"x": 42.0, "y": 24.0}),
                None,
            );
        }

        app.update();

        let config = app.world().resource::<CameraConfig>();
        let shake = app.world().resource::<CameraShakeState>();
        assert!((config.zoom - 1.5).abs() < 0.001);
        assert_eq!(config.look_at, Some(Vec2::new(42.0, 24.0)));
        assert!((shake.intensity - 0.8).abs() < 0.001);
        assert!((shake.duration - 0.4).abs() < 0.001);
        assert!((shake.remaining - 0.4).abs() < 0.001);
    }
}
