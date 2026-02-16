use bevy::prelude::*;
use bevy::utils::Instant;
use serde::Serialize;

use crate::events::GameEventBus;

#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash, Serialize)]
pub enum EngineFlowState {
    Loading,
    Menu,
    #[default]
    Playing,
    Paused,
    GameOver,
    LevelTransition,
    Cutscene,
    Custom,
}

#[derive(Resource, Clone, Default, Serialize)]
pub struct CustomFlowStateLabel(pub Option<String>);

impl EngineFlowState {
    fn from_label(label: &str) -> Self {
        let normalized = label.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "loading" => Self::Loading,
            "menu" => Self::Menu,
            "playing" => Self::Playing,
            "paused" => Self::Paused,
            "gameover" | "game_over" => Self::GameOver,
            "leveltransition" | "level_transition" => Self::LevelTransition,
            "cutscene" => Self::Cutscene,
            _ => Self::Custom,
        }
    }
}

#[derive(Clone)]
struct ActiveTransition {
    from: String,
    to: String,
    effect: Option<String>,
    duration: f32,
    started_at: Instant,
}

#[derive(Resource, Clone)]
pub struct RuntimeState {
    pub state: String,
    entered_at: Instant,
    active_transition: Option<ActiveTransition>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            state: "Playing".to_string(),
            entered_at: Instant::now(),
            active_transition: None,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct ActiveTransitionSnapshot {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    pub duration: f32,
    pub elapsed_seconds: f32,
    pub remaining_seconds: f32,
    pub progress: f32,
}

#[derive(Serialize, Clone)]
pub struct RuntimeStateSnapshot {
    pub state: String,
    pub time_in_state_seconds: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_transition: Option<ActiveTransitionSnapshot>,
}

impl Default for RuntimeStateSnapshot {
    fn default() -> Self {
        RuntimeState::default().snapshot()
    }
}

impl RuntimeState {
    pub fn set_state(&mut self, to: String, effect: Option<String>, duration: f32) {
        let from = self.state.clone();
        let normalized_effect = effect
            .and_then(|e| {
                let t = e.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            })
            .or(Some("Instant".to_string()));
        self.state = to.clone();
        self.entered_at = Instant::now();
        self.active_transition = if duration > 0.0
            && normalized_effect
                .as_deref()
                .map(|e| !e.eq_ignore_ascii_case("instant"))
                .unwrap_or(false)
        {
            Some(ActiveTransition {
                from,
                to,
                effect: normalized_effect,
                duration,
                started_at: Instant::now(),
            })
        } else {
            None
        };
    }

    pub fn is_gameplay_active(&self) -> bool {
        EngineFlowState::from_label(&self.state) == EngineFlowState::Playing
    }

    pub fn snapshot(&self) -> RuntimeStateSnapshot {
        RuntimeStateSnapshot {
            state: self.state.clone(),
            time_in_state_seconds: self.entered_at.elapsed().as_secs_f32(),
            active_transition: self.active_transition.as_ref().map(|t| {
                let elapsed = t.started_at.elapsed().as_secs_f32();
                let duration = t.duration.max(0.0);
                let remaining = (duration - elapsed).max(0.0);
                let progress = if duration <= 0.0 {
                    1.0
                } else {
                    (elapsed / duration).clamp(0.0, 1.0)
                };
                ActiveTransitionSnapshot {
                    from: t.from.clone(),
                    to: t.to.clone(),
                    effect: t.effect.clone(),
                    duration,
                    elapsed_seconds: elapsed,
                    remaining_seconds: remaining,
                    progress,
                }
            }),
        }
    }
}

pub fn gameplay_systems_enabled(
    state: Option<Res<State<EngineFlowState>>>,
    runtime: Option<Res<RuntimeState>>,
) -> bool {
    if let Some(state) = state {
        return *state.get() == EngineFlowState::Playing;
    }
    runtime.map(|r| r.is_gameplay_active()).unwrap_or(false)
}

#[derive(Resource, Default)]
struct RuntimeEventCursor {
    last_frame: u64,
    processed_in_frame: usize,
}

fn tick_runtime_state(mut runtime: ResMut<RuntimeState>) {
    if let Some(active) = runtime.active_transition.as_ref() {
        if active.started_at.elapsed().as_secs_f32() >= active.duration {
            runtime.active_transition = None;
        }
    }
}

fn sync_bevy_state_from_runtime(
    runtime: Res<RuntimeState>,
    state: Res<State<EngineFlowState>>,
    mut next_state: ResMut<NextState<EngineFlowState>>,
    mut custom_label: ResMut<CustomFlowStateLabel>,
) {
    let desired = EngineFlowState::from_label(&runtime.state);
    if desired == EngineFlowState::Custom {
        custom_label.0 = Some(runtime.state.clone());
    } else if custom_label.0.is_some() {
        custom_label.0 = None;
    }
    if state.get() != &desired {
        next_state.set(desired);
    }
}

fn apply_runtime_events(
    bus: Res<GameEventBus>,
    mut runtime: ResMut<RuntimeState>,
    mut cursor: ResMut<RuntimeEventCursor>,
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
            "game_pause" => {
                runtime.set_state("Paused".to_string(), Some("Instant".to_string()), 0.0);
            }
            "game_resume" => {
                runtime.set_state("Playing".to_string(), Some("Instant".to_string()), 0.0);
            }
            "game_transition" => {
                let to = ev
                    .data
                    .get("to")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty());
                if let Some(to) = to {
                    let effect = ev
                        .data
                        .get("effect")
                        .and_then(|v| v.as_str())
                        .map(|v| v.to_string());
                    let duration = ev
                        .data
                        .get("duration")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as f32;
                    runtime.set_state(to.to_string(), effect, duration.max(0.0));
                }
            }
            _ => {}
        }

        cursor.last_frame = ev.frame;
        cursor.processed_in_frame = count_in_frame;
    }
}

pub struct RuntimeStatePlugin;

impl Plugin for RuntimeStatePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RuntimeState::default())
            .insert_resource(CustomFlowStateLabel::default())
            .insert_resource(RuntimeEventCursor::default())
            .init_state::<EngineFlowState>()
            .add_systems(
                Update,
                (
                    tick_runtime_state,
                    apply_runtime_events,
                    sync_bevy_state_from_runtime,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_state_tracks_transition_progress() {
        let mut state = RuntimeState::default();
        state.set_state("Paused".to_string(), Some("FadeBlack".to_string()), 0.5);
        let snap = state.snapshot();
        assert_eq!(snap.state, "Paused");
        assert!(snap.active_transition.is_some());
    }

    #[test]
    fn runtime_plugin_applies_game_events() {
        let mut app = App::new();
        app.insert_resource(GameEventBus::default())
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(RuntimeStatePlugin);

        {
            let mut bus = app.world_mut().resource_mut::<GameEventBus>();
            bus.frame = 1;
            bus.emit("game_pause", serde_json::json!({}), None);
        }
        app.update();
        let runtime = app.world().resource::<RuntimeState>();
        assert_eq!(runtime.state, "Paused");
        app.update();
        let state = app.world().resource::<State<EngineFlowState>>();
        assert_eq!(state.get(), &EngineFlowState::Paused);

        {
            let mut bus = app.world_mut().resource_mut::<GameEventBus>();
            bus.frame = 2;
            bus.emit(
                "game_transition",
                serde_json::json!({"to":"Cutscene","effect":"FadeBlack","duration":0.25}),
                None,
            );
        }
        app.update();
        let runtime = app.world().resource::<RuntimeState>();
        assert_eq!(runtime.state, "Cutscene");
        assert!(runtime.snapshot().active_transition.is_some());
        app.update();
        let state = app.world().resource::<State<EngineFlowState>>();
        assert_eq!(state.get(), &EngineFlowState::Cutscene);

        {
            let mut bus = app.world_mut().resource_mut::<GameEventBus>();
            bus.frame = 3;
            bus.emit("game_resume", serde_json::json!({}), None);
        }
        app.update();
        let runtime = app.world().resource::<RuntimeState>();
        assert_eq!(runtime.state, "Playing");
        app.update();
        let state = app.world().resource::<State<EngineFlowState>>();
        assert_eq!(state.get(), &EngineFlowState::Playing);
    }

    #[test]
    fn custom_state_label_is_preserved_in_sidecar_resource() {
        let mut app = App::new();
        app.insert_resource(GameEventBus::default())
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins(RuntimeStatePlugin);

        {
            let mut runtime = app.world_mut().resource_mut::<RuntimeState>();
            runtime.set_state("PhotoMode".to_string(), Some("Instant".to_string()), 0.0);
        }
        app.update();
        app.update();

        let state = app.world().resource::<State<EngineFlowState>>();
        assert_eq!(state.get(), &EngineFlowState::Custom);
        let custom = app.world().resource::<CustomFlowStateLabel>();
        assert_eq!(custom.0.as_deref(), Some("PhotoMode"));
    }
}
