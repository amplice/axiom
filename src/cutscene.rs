use bevy::prelude::*;
use std::collections::HashMap;

use crate::api::types::EntitySpawnRequest;
use crate::events::GameEventBus;
use crate::game_runtime::RuntimeState;

pub struct CutscenePlugin;

impl Plugin for CutscenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CutsceneManager::default())
            .add_systems(Update, tick_cutscene);
    }
}

/// Global cutscene manager.
#[derive(Resource, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CutsceneManager {
    pub definitions: HashMap<String, CutsceneDef>,
    #[serde(default)]
    pub active: Option<ActiveCutscene>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CutsceneDef {
    pub name: String,
    pub steps: Vec<CutsceneStep>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CutsceneStep {
    Wait { seconds: f32 },
    MoveCameraTo { x: f32, y: f32, speed: Option<f32> },
    ShowDialogue { conversation: String, node: Option<String> },
    FadeOut { duration: f32, color: Option<[f32; 3]> },
    FadeIn { duration: f32 },
    SpawnEntity { request: EntitySpawnRequest },
    DespawnEntity { id: u64 },
    SetVar { name: String, value: serde_json::Value },
    EmitEvent { name: String, data: Option<serde_json::Value> },
    PlaySfx { name: String },
    WaitForEvent { name: String, timeout: Option<f32> },
    SetGameState { state: String },
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveCutscene {
    pub name: String,
    pub step_index: usize,
    pub step_elapsed: f32,
    pub previous_game_state: String,
    pub waiting_for_event: Option<String>,
}

fn tick_cutscene(
    mut manager: ResMut<CutsceneManager>,
    mut events: ResMut<GameEventBus>,
    mut runtime_state: ResMut<RuntimeState>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();

    // Clone active name to avoid borrow conflict with manager.definitions
    let active_name = match manager.active.as_ref() {
        Some(a) => a.name.clone(),
        None => return,
    };

    let def = match manager.definitions.get(&active_name) {
        Some(d) => d.clone(),
        None => {
            manager.active = None;
            return;
        }
    };

    let Some(active) = manager.active.as_mut() else {
        return;
    };

    if active.step_index >= def.steps.len() {
        // Cutscene complete
        let name = active.name.clone();
        let prev_state = active.previous_game_state.clone();
        events.emit(
            "cutscene_completed",
            serde_json::json!({ "name": name }),
            None,
        );
        runtime_state.set_state(prev_state, None, 0.0);
        manager.active = None;
        return;
    }

    active.step_elapsed += dt;

    let step = &def.steps[active.step_index];
    let advance = match step {
        CutsceneStep::Wait { seconds } => active.step_elapsed >= *seconds,
        CutsceneStep::MoveCameraTo { speed, .. } => {
            // If speed is given, allow some travel time; otherwise advance instantly.
            // The actual camera movement is triggered via cutscene_camera_move event.
            let travel_time = speed.map_or(0.0, |s| if s > 0.0 { 1.0 / s * 500.0 } else { 0.0 });
            active.step_elapsed >= travel_time
        }
        CutsceneStep::ShowDialogue { .. } => {
            // Wait for dialogue_completed event or timeout after 30s
            if active.waiting_for_event.is_none() {
                active.waiting_for_event = Some("dialogue_completed".to_string());
            }
            let event_fired = events
                .recent
                .iter()
                .any(|e| e.name == "dialogue_completed");
            let timed_out = active.step_elapsed >= 30.0;
            event_fired || timed_out
        }
        CutsceneStep::FadeOut { duration, .. } | CutsceneStep::FadeIn { duration } => {
            active.step_elapsed >= *duration
        }
        CutsceneStep::SpawnEntity { .. }
        | CutsceneStep::DespawnEntity { .. }
        | CutsceneStep::SetVar { .. }
        | CutsceneStep::EmitEvent { .. }
        | CutsceneStep::PlaySfx { .. }
        | CutsceneStep::SetGameState { .. } => true, // Instant steps
        CutsceneStep::WaitForEvent { name, timeout } => {
            if active.waiting_for_event.is_none() {
                active.waiting_for_event = Some(name.clone());
            }
            // Check if the event has fired
            let event_fired = events
                .recent
                .iter()
                .any(|e| e.name == *name);
            let timed_out = timeout.map_or(false, |t| active.step_elapsed >= t);
            event_fired || timed_out
        }
    };

    // Execute one-shot side effects on first frame of step
    if active.step_elapsed <= dt * 1.5 {
        match step {
            CutsceneStep::EmitEvent { name, data } => {
                events.emit(
                    name.clone(),
                    data.clone().unwrap_or(serde_json::Value::Null),
                    None,
                );
            }
            CutsceneStep::SetVar { name, value } => {
                events.emit(
                    "cutscene_set_var",
                    serde_json::json!({ "name": name, "value": value }),
                    None,
                );
            }
            CutsceneStep::SetGameState { state } => {
                runtime_state.set_state(state.clone(), None, 0.0);
            }
            CutsceneStep::FadeOut { duration, color } => {
                events.emit(
                    "cutscene_screen_effect",
                    serde_json::json!({ "effect": "fade_out", "duration": duration, "color": color }),
                    None,
                );
            }
            CutsceneStep::FadeIn { duration } => {
                events.emit(
                    "cutscene_screen_effect",
                    serde_json::json!({ "effect": "fade_in", "duration": duration }),
                    None,
                );
            }
            CutsceneStep::MoveCameraTo { x, y, speed } => {
                events.emit(
                    "cutscene_camera_move",
                    serde_json::json!({ "x": x, "y": y, "speed": speed }),
                    None,
                );
            }
            CutsceneStep::ShowDialogue { conversation, node } => {
                events.emit(
                    "dialogue_start",
                    serde_json::json!({ "conversation": conversation, "node": node }),
                    None,
                );
            }
            CutsceneStep::PlaySfx { name } => {
                events.emit(
                    "audio_play_sfx",
                    serde_json::json!({ "name": name }),
                    None,
                );
            }
            _ => {}
        }

        events.emit(
            "cutscene_step",
            serde_json::json!({
                "name": active.name,
                "step": active.step_index,
                "total_steps": def.steps.len(),
            }),
            None,
        );
    }

    if advance {
        active.step_index += 1;
        active.step_elapsed = 0.0;
        active.waiting_for_event = None;
    }
}

pub fn play_cutscene(
    manager: &mut CutsceneManager,
    events: &mut GameEventBus,
    runtime_state: &mut RuntimeState,
    name: &str,
) -> Result<(), String> {
    if !manager.definitions.contains_key(name) {
        return Err(format!("Cutscene '{}' not defined", name));
    }
    if manager.active.is_some() {
        return Err("A cutscene is already playing".to_string());
    }
    let previous_state = runtime_state.state.clone();
    runtime_state.set_state("Cutscene".to_string(), None, 0.0);
    manager.active = Some(ActiveCutscene {
        name: name.to_string(),
        step_index: 0,
        step_elapsed: 0.0,
        previous_game_state: previous_state,
        waiting_for_event: None,
    });
    events.emit(
        "cutscene_started",
        serde_json::json!({ "name": name }),
        None,
    );
    Ok(())
}

pub fn stop_cutscene(
    manager: &mut CutsceneManager,
    events: &mut GameEventBus,
    runtime_state: &mut RuntimeState,
) {
    if let Some(active) = manager.active.take() {
        runtime_state.set_state(active.previous_game_state, None, 0.0);
        events.emit(
            "cutscene_completed",
            serde_json::json!({ "name": active.name, "stopped": true }),
            None,
        );
    }
}
