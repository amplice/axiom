use bevy::prelude::*;
use std::collections::HashMap;

use crate::events::GameEventBus;

pub struct StateMachinePlugin;

impl Plugin for StateMachinePlugin {
    fn build(&self, _app: &mut App) {
        // No tick system needed — transitions are imperative via script/API.
    }
}

/// Configuration for a single state in the machine.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StateConfig {
    #[serde(default)]
    pub allowed_transitions: Vec<String>,
    #[serde(default)]
    pub on_enter_event: Option<String>,
    #[serde(default)]
    pub on_exit_event: Option<String>,
}

/// Entity state machine component.
#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityStateMachine {
    pub states: HashMap<String, StateConfig>,
    pub current: String,
    #[serde(default)]
    pub previous: Option<String>,
    #[serde(default)]
    pub entered_at_frame: u64,
}

impl EntityStateMachine {
    /// Attempt to transition to a new state. Returns Ok(()) on success.
    pub fn transition(
        &mut self,
        new_state: &str,
        entity_id: u64,
        frame: u64,
        events: &mut GameEventBus,
    ) -> Result<(), String> {
        if new_state == self.current {
            return Ok(());
        }

        // Validate the transition is allowed
        if let Some(config) = self.states.get(&self.current) {
            if !config.allowed_transitions.is_empty()
                && !config.allowed_transitions.iter().any(|s| s == new_state)
            {
                return Err(format!(
                    "Transition from '{}' to '{}' not allowed",
                    self.current, new_state
                ));
            }
        }

        // Check target state exists
        if !self.states.contains_key(new_state) {
            return Err(format!("State '{}' not defined", new_state));
        }

        let old_state = self.current.clone();

        // Fire exit event
        if let Some(config) = self.states.get(&old_state) {
            if let Some(event_name) = &config.on_exit_event {
                events.emit(
                    event_name.clone(),
                    serde_json::json!({
                        "entity": entity_id,
                        "state": old_state,
                        "next": new_state,
                    }),
                    Some(entity_id),
                );
            }
        }

        // Fire generic state_exit event
        events.emit(
            "state_exit",
            serde_json::json!({
                "entity": entity_id,
                "state": old_state,
                "next": new_state,
            }),
            Some(entity_id),
        );

        self.previous = Some(old_state);
        self.current = new_state.to_string();
        self.entered_at_frame = frame;

        // Fire enter event
        if let Some(config) = self.states.get(new_state) {
            if let Some(event_name) = &config.on_enter_event {
                events.emit(
                    event_name.clone(),
                    serde_json::json!({
                        "entity": entity_id,
                        "state": new_state,
                        "previous": self.previous,
                    }),
                    Some(entity_id),
                );
            }
        }

        // Fire generic state_enter event
        events.emit(
            "state_enter",
            serde_json::json!({
                "entity": entity_id,
                "state": new_state,
                "previous": self.previous,
            }),
            Some(entity_id),
        );

        Ok(())
    }
}
