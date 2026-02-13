mod components;
mod player;
mod camera;
mod render;
mod tilemap;
mod physics;
mod api;
mod simulation;
mod constraints;
mod feel;
mod generation;

use bevy::prelude::*;
use components::PhysicsConfig;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Axiom".to_string(),
                resolution: (960.0, 540.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(PhysicsConfig::default())
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .add_plugins(tilemap::TilemapPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(physics::PhysicsPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(render::RenderPlugin)
        .add_plugins(api::ApiPlugin)
        .run();
}
