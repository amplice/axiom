use std::collections::VecDeque;

use bevy::prelude::*;
use serde::Serialize;

const MAX_EVENTS: usize = 500;

#[derive(Serialize, Clone)]
pub struct GameEvent {
    pub name: String,
    pub data: serde_json::Value,
    pub frame: u64,
    pub source_entity: Option<u64>,
}

#[derive(Resource, Default)]
pub struct GameEventBus {
    pub recent: VecDeque<GameEvent>,
    pub frame: u64,
    pub dropped_events: u64,
    last_overflow_log_frame: u64,
}

impl GameEventBus {
    pub fn emit(
        &mut self,
        name: impl Into<String>,
        data: serde_json::Value,
        source_entity: Option<u64>,
    ) {
        self.recent.push_back(GameEvent {
            name: name.into(),
            data,
            frame: self.frame,
            source_entity,
        });
        if self.recent.len() > MAX_EVENTS {
            let excess = self.recent.len() - MAX_EVENTS;
            // O(1) amortized front removal with VecDeque
            for _ in 0..excess {
                self.recent.pop_front();
            }
            self.dropped_events = self.dropped_events.saturating_add(excess as u64);
            if self.frame.saturating_sub(self.last_overflow_log_frame) >= 60 {
                self.last_overflow_log_frame = self.frame;
                warn!(
                    "[Axiom events] Dropped {} buffered events (total dropped: {})",
                    excess, self.dropped_events
                );
            }
        }
    }
}

pub struct GameEventsPlugin;

impl Plugin for GameEventsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameEventBus::default()).add_systems(
            FixedUpdate,
            tick_event_frame.run_if(crate::game_runtime::gameplay_systems_enabled),
        );
    }
}

fn tick_event_frame(mut bus: ResMut<GameEventBus>) {
    bus.frame = bus.frame.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_bus_tracks_dropped_events() {
        let mut bus = GameEventBus::default();
        for i in 0..(MAX_EVENTS + 25) {
            bus.emit("test", serde_json::json!({ "i": i }), None);
        }
        assert_eq!(bus.recent.len(), MAX_EVENTS);
        assert!(bus.dropped_events >= 25);
    }
}
