#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

mod ai;
mod animation;
#[cfg(not(target_arch = "wasm32"))]
mod api;
mod audio;
mod camera;
mod components;
mod constraints;
mod cutscene;
mod debug;
mod events;
mod feel;
mod game_runtime;
mod generation;
mod input;
mod interaction;
mod inventory;
pub mod lighting;
mod parallax;
mod particles;
mod pathfinding;
mod perf;
mod physics;
mod physics_core;
mod player;
mod raycast;
mod render;
pub mod screen_effects;
mod scripting;
mod simulation;
mod spatial_hash;
mod state_machine;
#[cfg(not(target_arch = "wasm32"))]
pub mod spawn;
mod sprites;
mod telemetry;
mod tilemap;
mod trail;
pub mod tween;
mod ui;
mod weather;
#[cfg(any(feature = "web_export", feature = "desktop_export"))]
mod web_bootstrap;
mod world_text;

use bevy::prelude::*;
use components::{GameConfig, HeadlessMode, NextNetworkId};

#[derive(serde::Deserialize, Default)]
struct StartupConfig {
    window_title: Option<String>,
    window_width: Option<f32>,
    window_height: Option<f32>,
    background_color: Option<[f32; 3]>,
    texture_filter: Option<String>,
    assets_dir: Option<String>,
}

fn load_startup_config() -> StartupConfig {
    let path = std::env::var("AXIOM_GAME_CONFIG")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "game.json".to_string());
    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<StartupConfig>(&contents) {
            Ok(cfg) => {
                println!("[Axiom] Loaded startup config from {}", path);
                cfg
            }
            Err(e) => {
                eprintln!("[Axiom] Failed to parse {}: {}", path, e);
                StartupConfig::default()
            }
        },
        Err(_) => StartupConfig::default(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    #[cfg(any(feature = "web_export", feature = "desktop_export"))]
    if args.iter().any(|a| a == "--verify-embedded-data") {
        match web_bootstrap::embedded_data_sanity() {
            Ok(s) => {
                println!(
                    "[Axiom export] Embedded snapshot OK: {}x{} ({} tiles)",
                    s.width, s.height, s.tile_count
                );
                return;
            }
            Err(e) => {
                eprintln!("[Axiom export] Embedded snapshot invalid: {e}");
                std::process::exit(2);
            }
        }
    }

    let startup_config = load_startup_config();
    let mut app = App::new();

    app.insert_resource(HeadlessMode(headless));

    if headless {
        // Headless mode: no window, no rendering, just ECS + API
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::state::app::StatesPlugin);
        println!("[Axiom] Starting in HEADLESS mode");
    } else {
        // Windowed mode: full rendering
        // Env vars override game.json values
        let assets_dir = std::env::var("AXIOM_ASSETS_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .or(startup_config.assets_dir)
            .unwrap_or_else(|| "assets".to_string());
        if assets_dir != "assets" {
            println!("[Axiom] Using game assets dir: {}", assets_dir);
        }
        let nearest_filter = std::env::var("AXIOM_TEXTURE_FILTER")
            .ok()
            .filter(|s| !s.is_empty())
            .or(startup_config.texture_filter)
            .map_or(false, |v| v.eq_ignore_ascii_case("nearest"));

        let window_title = startup_config.window_title.unwrap_or_else(|| "Axiom".to_string());
        let window_width = startup_config.window_width.unwrap_or(960.0);
        let window_height = startup_config.window_height.unwrap_or(540.0);

        let mut plugins = DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: window_title,
                    resolution: (window_width, window_height).into(),
                    present_mode: bevy::window::PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            })
            .set(bevy::asset::AssetPlugin {
                file_path: assets_dir,
                ..default()
            });

        if nearest_filter {
            plugins = plugins.set(bevy::render::texture::ImagePlugin::default_nearest());
            println!("[Axiom] Texture filter: nearest (pixel-art mode)");
        }

        app.add_plugins(plugins);
        let bg = startup_config.background_color.unwrap_or([0.12, 0.18, 0.1]);
        app.insert_resource(ClearColor(Color::srgb(bg[0], bg[1], bg[2])));
        app.add_plugins(sprites::SpritePlugin);
        app.add_plugins(render::RenderPlugin);
        println!("[Axiom] Starting in WINDOWED mode");
    }

    app.insert_resource(GameConfig::default())
        .insert_resource(NextNetworkId::default())
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .add_plugins(input::InputPlugin)
        .add_plugins(animation::AnimationPlugin)
        .add_plugins(game_runtime::RuntimeStatePlugin)
        .add_plugins(audio::AudioPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(ui::UiPlugin)
        .add_plugins(ai::AiPlugin)
        .add_plugins(particles::ParticlesPlugin)
        .add_plugins(tilemap::TilemapPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(physics::PhysicsPlugin)
        .add_plugins(events::GameEventsPlugin)
        .add_plugins(spatial_hash::SpatialHashPlugin)
        .add_plugins(perf::PerfPlugin)
        .add_plugins(debug::DebugPlugin)
        .add_plugins(interaction::InteractionPlugin)
        .add_plugins(scripting::ScriptingPlugin)
        .add_plugins(tween::TweenPlugin)
        .add_plugins(screen_effects::ScreenEffectsPlugin)
        .add_plugins(lighting::LightingPlugin)
        .add_plugins(parallax::ParallaxPlugin)
        .add_plugins(weather::WeatherPlugin)
        .add_plugins(cutscene::CutscenePlugin)
        .add_plugins(inventory::InventoryPlugin)
        .add_plugins(state_machine::StateMachinePlugin)
        .add_plugins(trail::TrailPlugin)
        .add_plugins(world_text::WorldTextPlugin)
        .add_plugins(telemetry::TelemetryPlugin);

    #[cfg(any(feature = "web_export", feature = "desktop_export"))]
    app.add_plugins(web_bootstrap::WebBootstrapPlugin);

    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(api::ApiPlugin);

    app.run();
}
