use std::collections::HashMap;

use bevy::prelude::*;

use crate::components::{GamePosition, HeadlessMode};

fn default_color_start() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

fn default_color_end() -> [f32; 4] {
    [1.0, 1.0, 1.0, 0.0]
}

fn default_size_start() -> f32 {
    4.0
}

fn default_size_end() -> f32 {
    1.0
}

fn default_lifetime() -> f32 {
    0.5
}

fn default_emit_rate() -> f32 {
    24.0
}

fn default_speed_max() -> f32 {
    120.0
}

fn default_true() -> bool {
    true
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ParticlePresetDef {
    #[serde(default = "default_color_start")]
    pub color_start: [f32; 4],
    #[serde(default = "default_color_end")]
    pub color_end: [f32; 4],
    #[serde(default = "default_size_start")]
    pub size_start: f32,
    #[serde(default = "default_size_end")]
    pub size_end: f32,
    #[serde(default = "default_lifetime")]
    pub lifetime: f32,
    #[serde(default = "default_emit_rate")]
    pub emit_rate: f32,
    #[serde(default)]
    pub spread_angle: f32,
    #[serde(default)]
    pub speed_min: f32,
    #[serde(default = "default_speed_max")]
    pub speed_max: f32,
    #[serde(default)]
    pub gravity_multiplier: f32,
    #[serde(default)]
    pub one_shot: bool,
    #[serde(default)]
    pub burst_count: u32,
}

impl Default for ParticlePresetDef {
    fn default() -> Self {
        Self {
            color_start: default_color_start(),
            color_end: default_color_end(),
            size_start: default_size_start(),
            size_end: default_size_end(),
            lifetime: default_lifetime(),
            emit_rate: default_emit_rate(),
            spread_angle: 40.0,
            speed_min: 20.0,
            speed_max: default_speed_max(),
            gravity_multiplier: 0.2,
            one_shot: false,
            burst_count: 16,
        }
    }
}

#[derive(Resource, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ParticlePresetLibrary {
    pub presets: HashMap<String, ParticlePresetDef>,
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParticleEmitter {
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default = "default_color_start")]
    pub color_start: [f32; 4],
    #[serde(default = "default_color_end")]
    pub color_end: [f32; 4],
    #[serde(default = "default_size_start")]
    pub size_start: f32,
    #[serde(default = "default_size_end")]
    pub size_end: f32,
    #[serde(default = "default_lifetime")]
    pub lifetime: f32,
    #[serde(default = "default_emit_rate")]
    pub emit_rate: f32,
    #[serde(default)]
    pub spread_angle: f32,
    #[serde(default)]
    pub speed_min: f32,
    #[serde(default = "default_speed_max")]
    pub speed_max: f32,
    #[serde(default)]
    pub gravity_multiplier: f32,
    #[serde(default)]
    pub one_shot: bool,
    #[serde(default)]
    pub burst_count: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub timer: f32,
    #[serde(default)]
    pub fired_once: bool,
}

#[derive(Component)]
pub struct TransientParticleEmitter;

impl Default for ParticleEmitter {
    fn default() -> Self {
        let preset = ParticlePresetDef::default();
        Self::from_preset(&preset, None)
    }
}

impl ParticleEmitter {
    pub fn from_preset(preset: &ParticlePresetDef, preset_name: Option<String>) -> Self {
        Self {
            preset: preset_name,
            color_start: preset.color_start,
            color_end: preset.color_end,
            size_start: preset.size_start,
            size_end: preset.size_end,
            lifetime: preset.lifetime,
            emit_rate: preset.emit_rate,
            spread_angle: preset.spread_angle,
            speed_min: preset.speed_min,
            speed_max: preset.speed_max,
            gravity_multiplier: preset.gravity_multiplier,
            one_shot: preset.one_shot,
            burst_count: preset.burst_count,
            enabled: true,
            timer: 0.0,
            fired_once: false,
        }
    }

    pub fn preset_only(name: String) -> Self {
        Self {
            preset: Some(name),
            ..Default::default()
        }
    }
}

#[derive(Component)]
struct ParticleInstance {
    velocity: Vec2,
    age: f32,
    lifetime: f32,
    color_start: Vec4,
    color_end: Vec4,
    size_start: f32,
    size_end: f32,
    gravity_multiplier: f32,
}

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticlePresetLibrary>().add_systems(
            Update,
            (emit_particles, update_particles, cleanup_transient_emitters)
                .run_if(crate::game_runtime::gameplay_systems_enabled),
        );
    }
}

fn emit_particles(
    mut commands: Commands,
    time: Res<Time>,
    presets: Res<ParticlePresetLibrary>,
    headless: Res<HeadlessMode>,
    mut query: Query<(&GamePosition, &mut ParticleEmitter)>,
) {
    let dt = time.delta_secs();
    for (pos, mut emitter) in query.iter_mut() {
        if !emitter.enabled {
            continue;
        }
        let profile = resolve_profile(&emitter, &presets);
        if profile.one_shot {
            if emitter.fired_once {
                continue;
            }
            emitter.fired_once = true;
            let count = profile.burst_count.clamp(1, 256);
            for i in 0..count {
                spawn_one_particle(&mut commands, pos, &profile, i, count, headless.0);
            }
            continue;
        }

        let emit_rate = profile.emit_rate.max(0.0);
        if emit_rate <= 0.0 {
            continue;
        }
        emitter.timer += dt;
        let mut spawn_count = (emitter.timer * emit_rate).floor() as u32;
        if spawn_count == 0 {
            continue;
        }
        spawn_count = spawn_count.min(64);
        emitter.timer -= spawn_count as f32 / emit_rate;
        for i in 0..spawn_count {
            spawn_one_particle(
                &mut commands,
                pos,
                &profile,
                i,
                spawn_count.max(1),
                headless.0,
            );
        }
    }
}

fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(
        Entity,
        &mut GamePosition,
        &mut ParticleInstance,
        Option<&mut Sprite>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut pos, mut particle, sprite) in query.iter_mut() {
        particle.age += dt;
        if particle.age >= particle.lifetime.max(0.01) {
            commands.entity(entity).despawn();
            continue;
        }

        particle.velocity.y += -980.0 * particle.gravity_multiplier * dt;
        pos.x += particle.velocity.x * dt;
        pos.y += particle.velocity.y * dt;

        if let Some(mut sprite) = sprite {
            let t = (particle.age / particle.lifetime.max(0.01)).clamp(0.0, 1.0);
            let color = particle.color_start.lerp(particle.color_end, t);
            let size = particle.size_start + (particle.size_end - particle.size_start) * t;
            sprite.color = Color::srgba(color.x, color.y, color.z, color.w);
            sprite.custom_size = Some(Vec2::splat(size.max(0.1)));
        }
    }
}

fn cleanup_transient_emitters(
    mut commands: Commands,
    query: Query<(Entity, &ParticleEmitter), With<TransientParticleEmitter>>,
) {
    for (entity, emitter) in query.iter() {
        if emitter.one_shot && emitter.fired_once {
            commands.entity(entity).despawn();
        }
    }
}

fn resolve_profile(
    emitter: &ParticleEmitter,
    presets: &ParticlePresetLibrary,
) -> ParticlePresetDef {
    if let Some(name) = emitter.preset.as_deref() {
        if let Some(preset) = presets.presets.get(name) {
            return preset.clone();
        }
    }
    ParticlePresetDef {
        color_start: emitter.color_start,
        color_end: emitter.color_end,
        size_start: emitter.size_start,
        size_end: emitter.size_end,
        lifetime: emitter.lifetime,
        emit_rate: emitter.emit_rate,
        spread_angle: emitter.spread_angle,
        speed_min: emitter.speed_min,
        speed_max: emitter.speed_max,
        gravity_multiplier: emitter.gravity_multiplier,
        one_shot: emitter.one_shot,
        burst_count: emitter.burst_count,
    }
}

fn spawn_one_particle(
    commands: &mut Commands,
    pos: &GamePosition,
    profile: &ParticlePresetDef,
    index: u32,
    total: u32,
    headless: bool,
) {
    let spread = profile.spread_angle.to_radians();
    let t = if total <= 1 {
        0.5
    } else {
        index as f32 / (total - 1) as f32
    };
    let angle = -spread * 0.5 + spread * t;
    let dir = Vec2::new(angle.cos(), angle.sin());
    let speed = profile.speed_min + (profile.speed_max - profile.speed_min) * t;
    let velocity = dir * speed.max(0.0);

    let mut entity = commands.spawn((
        GamePosition { x: pos.x, y: pos.y },
        ParticleInstance {
            velocity,
            age: 0.0,
            lifetime: profile.lifetime.max(0.01),
            color_start: Vec4::from_array(profile.color_start),
            color_end: Vec4::from_array(profile.color_end),
            size_start: profile.size_start.max(0.1),
            size_end: profile.size_end.max(0.1),
            gravity_multiplier: profile.gravity_multiplier,
        },
    ));

    if !headless {
        entity.insert((
            Sprite::from_color(
                Color::srgba(
                    profile.color_start[0],
                    profile.color_start[1],
                    profile.color_start[2],
                    profile.color_start[3],
                ),
                Vec2::splat(profile.size_start.max(0.1)),
            ),
            Transform::from_xyz(pos.x, pos.y, 200.0),
        ));
    }
}
