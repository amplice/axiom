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

    let mut app = App::new();

    app.insert_resource(HeadlessMode(headless));

    if headless {
        // Headless mode: no window, no rendering, just ECS + API
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::state::app::StatesPlugin);
        println!("[Axiom] Starting in HEADLESS mode");
    } else {
        // Windowed mode: full rendering
        // AXIOM_ASSETS_DIR env var lets games point at their own assets folder.
        let assets_dir = std::env::var("AXIOM_ASSETS_DIR").unwrap_or_else(|_| "assets".to_string());
        if assets_dir != "assets" {
            println!("[Axiom] Using game assets dir: {}", assets_dir);
        }
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Axiom".to_string(),
                        resolution: (960.0, 540.0).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::asset::AssetPlugin {
                    file_path: assets_dir,
                    ..default()
                }),
        );
        app.insert_resource(ClearColor(Color::srgb(0.12, 0.18, 0.1)));
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
        .add_plugins(lighting::LightingPlugin);

    #[cfg(any(feature = "web_export", feature = "desktop_export"))]
    app.add_plugins(web_bootstrap::WebBootstrapPlugin);

    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(api::ApiPlugin);

    app.run();
}
