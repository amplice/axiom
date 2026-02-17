use bevy::prelude::*;

use crate::api::types::GameplayTelemetry;
use crate::components::Alive;
use crate::events::GameEventBus;
use crate::input::VirtualInput;

pub struct TelemetryPlugin;

impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameplayTelemetry::default())
            .add_systems(
                FixedUpdate,
                update_telemetry.run_if(crate::game_runtime::gameplay_systems_enabled),
            );
    }
}

fn update_telemetry(
    mut telemetry: ResMut<GameplayTelemetry>,
    event_bus: Res<GameEventBus>,
    input: Res<VirtualInput>,
    alive_query: Query<&Alive>,
) {
    telemetry.total_frames += 1;
    let frame = telemetry.total_frames;

    // Track input presses
    for action in &input.just_pressed {
        *telemetry.input_counts.entry(action.clone()).or_insert(0) += 1;
    }

    // Sample entity count every 60 frames (max 300 samples)
    if frame % 60 == 0 {
        let alive_count = alive_query
            .iter()
            .filter(|a| a.0)
            .count();
        telemetry.entity_count_samples.push((frame, alive_count));
        if telemetry.entity_count_samples.len() > 300 {
            telemetry.entity_count_samples.remove(0);
        }
    }

    // Scan recent events for telemetry-relevant ones
    for event in event_bus.recent.iter().rev() {
        if event.frame != event_bus.frame {
            break;
        }
        match event.name.as_str() {
            "death" | "entity_died" => {
                let x = event.data.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let y = event.data.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                telemetry.death_locations.push([x, y, frame as f32]);
            }
            "goal_reached" => {
                if telemetry.goal_reached_at.is_none() {
                    telemetry.goal_reached_at = Some(frame);
                }
            }
            "pickup_collected" => {
                telemetry.pickups_collected += 1;
            }
            "damage" | "damage_dealt" => {
                let amount = event
                    .data
                    .get("amount")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                telemetry.damage_dealt += amount;
            }
            "damage_taken" => {
                let amount = event
                    .data
                    .get("amount")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                telemetry.damage_taken += amount;
            }
            _ => {}
        }
    }
}
