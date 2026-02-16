use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::events::GameEventBus;

fn default_visible() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UiNode {
    pub id: String,
    pub node_type: UiNodeType,
    #[serde(default)]
    pub position: serde_json::Value,
    #[serde(default)]
    pub size: serde_json::Value,
    #[serde(default = "default_visible")]
    pub visible: bool,
    #[serde(default)]
    pub children: Vec<UiNode>,
    #[serde(default)]
    pub style: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiNodeType {
    Panel {
        #[serde(default)]
        color: Option<String>,
    },
    Text {
        text: String,
        #[serde(default)]
        font_size: Option<f32>,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        align: Option<String>,
    },
    Image {
        path: String,
    },
    Button {
        text: String,
        action: String,
    },
    ProgressBar {
        value: f32,
        max: f32,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        bg_color: Option<String>,
    },
    Container {
        #[serde(default)]
        direction: Option<String>,
        #[serde(default)]
        gap: Option<f32>,
    },
    DialogueBox {
        #[serde(default)]
        speaker: Option<String>,
        text: String,
        #[serde(default)]
        choices: Vec<String>,
    },
    Slot {
        index: usize,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UiScreen {
    pub name: String,
    pub layer: i32,
    pub nodes: Vec<UiNode>,
    #[serde(default = "default_visible")]
    pub visible: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UiNodeUpdate {
    #[serde(default)]
    pub node_type: Option<UiNodeType>,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub value: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
}

#[derive(Serialize, Clone, Default)]
pub struct UiStateSnapshot {
    pub active_screens: Vec<String>,
    pub dialogue_active: bool,
    pub hud_state: serde_json::Value,
    pub screens: Vec<UiScreen>,
}

#[derive(Resource, Default)]
pub struct UiManager {
    pub screens: HashMap<String, UiScreen>,
    pub dialogue_active: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DialogueChoice {
    pub text: String,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DialogueNode {
    pub id: String,
    #[serde(default)]
    pub speaker: Option<String>,
    pub text: String,
    #[serde(default)]
    pub choices: Vec<DialogueChoice>,
    #[serde(default)]
    pub event: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DialogueConversation {
    pub name: String,
    pub nodes: Vec<DialogueNode>,
}

#[derive(Serialize, Clone, Default)]
pub struct DialogueStateSnapshot {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    pub choices: Vec<String>,
}

#[derive(Clone)]
struct ActiveDialogue {
    conversation: String,
    node_id: String,
}

#[derive(Resource, Default)]
pub struct DialogueManager {
    conversations: HashMap<String, DialogueConversation>,
    active: Option<ActiveDialogue>,
}

impl UiManager {
    pub fn upsert_screen(&mut self, mut screen: UiScreen) {
        if screen.name.trim().is_empty() {
            return;
        }
        if let Some(existing) = self.screens.get(&screen.name) {
            screen.visible = existing.visible;
        }
        self.screens.insert(screen.name.clone(), screen);
    }

    pub fn show_screen(&mut self, name: &str) -> Result<(), String> {
        let Some(screen) = self.screens.get_mut(name) else {
            return Err(format!("Unknown screen: {name}"));
        };
        screen.visible = true;
        Ok(())
    }

    pub fn hide_screen(&mut self, name: &str) -> Result<(), String> {
        let Some(screen) = self.screens.get_mut(name) else {
            return Err(format!("Unknown screen: {name}"));
        };
        screen.visible = false;
        Ok(())
    }

    pub fn update_node(
        &mut self,
        screen_name: &str,
        node_id: &str,
        update: UiNodeUpdate,
    ) -> Result<(), String> {
        let Some(screen) = self.screens.get_mut(screen_name) else {
            return Err(format!("Unknown screen: {screen_name}"));
        };
        let Some(node) = find_node_mut(&mut screen.nodes, node_id) else {
            return Err(format!(
                "Node '{node_id}' not found in screen '{screen_name}'"
            ));
        };
        apply_node_update(node, update);
        Ok(())
    }

    pub fn update_node_any(&mut self, node_id: &str, update: UiNodeUpdate) -> bool {
        for screen in self.screens.values_mut() {
            if let Some(node) = find_node_mut(&mut screen.nodes, node_id) {
                apply_node_update(node, update);
                return true;
            }
        }
        false
    }

    pub fn snapshot(&self) -> UiStateSnapshot {
        let mut screens: Vec<UiScreen> = self.screens.values().cloned().collect();
        screens.sort_by(|a, b| a.layer.cmp(&b.layer).then(a.name.cmp(&b.name)));

        let mut active_screens: Vec<String> = screens
            .iter()
            .filter(|s| s.visible)
            .map(|s| s.name.clone())
            .collect();
        active_screens.sort();

        let mut hud = serde_json::Map::new();
        for screen in screens.iter().filter(|s| s.visible) {
            for node in &screen.nodes {
                collect_hud_state(node, &mut hud);
            }
        }

        UiStateSnapshot {
            active_screens,
            dialogue_active: self.dialogue_active,
            hud_state: serde_json::Value::Object(hud),
            screens,
        }
    }
}

impl DialogueManager {
    pub fn upsert_conversation(&mut self, convo: DialogueConversation) {
        if convo.name.trim().is_empty() || convo.nodes.is_empty() {
            return;
        }
        self.conversations.insert(convo.name.clone(), convo);
    }

    pub fn start(&mut self, name: &str) -> Result<(), String> {
        let Some(convo) = self.conversations.get(name) else {
            return Err(format!("Unknown conversation: {name}"));
        };
        let Some(first) = convo.nodes.first() else {
            return Err(format!("Conversation '{name}' has no nodes"));
        };
        self.active = Some(ActiveDialogue {
            conversation: name.to_string(),
            node_id: first.id.clone(),
        });
        Ok(())
    }

    pub fn choose(
        &mut self,
        choice_idx: usize,
    ) -> Result<(Option<String>, Option<String>), String> {
        let Some(active) = self.active.clone() else {
            return Err("No active dialogue".to_string());
        };
        let Some(convo) = self.conversations.get(&active.conversation) else {
            return Err("Active dialogue conversation missing".to_string());
        };
        let Some(node) = convo.nodes.iter().find(|n| n.id == active.node_id) else {
            return Err("Active dialogue node missing".to_string());
        };
        let Some(choice) = node.choices.get(choice_idx).cloned() else {
            return Err(format!("Choice index {choice_idx} out of range"));
        };

        let node_event = node.event.clone();
        let choice_event = choice.event.clone();

        if let Some(next_id) = choice.next {
            if convo.nodes.iter().any(|n| n.id == next_id) {
                self.active = Some(ActiveDialogue {
                    conversation: active.conversation,
                    node_id: next_id,
                });
            } else {
                self.active = None;
            }
        } else {
            self.active = None;
        }
        Ok((node_event, choice_event))
    }

    pub fn snapshot(&self) -> DialogueStateSnapshot {
        let Some(active) = self.active.as_ref() else {
            return DialogueStateSnapshot::default();
        };
        let Some(convo) = self.conversations.get(&active.conversation) else {
            return DialogueStateSnapshot::default();
        };
        let Some(node) = convo.nodes.iter().find(|n| n.id == active.node_id) else {
            return DialogueStateSnapshot::default();
        };
        DialogueStateSnapshot {
            active: true,
            conversation: Some(active.conversation.clone()),
            node_id: Some(node.id.clone()),
            speaker: node.speaker.clone(),
            text: Some(node.text.clone()),
            choices: node.choices.iter().map(|c| c.text.clone()).collect(),
        }
    }
}

fn apply_node_update(node: &mut UiNode, update: UiNodeUpdate) {
    if let Some(node_type) = update.node_type {
        node.node_type = node_type;
    }
    if let Some(visible) = update.visible {
        node.visible = visible;
    }
    if let Some(text) = update.text {
        match &mut node.node_type {
            UiNodeType::Text { text: current, .. } => *current = text,
            UiNodeType::DialogueBox {
                text: current_text, ..
            } => *current_text = text,
            UiNodeType::Button { text: current, .. } => *current = text,
            _ => {}
        }
    }
    if update.value.is_some() || update.max.is_some() {
        if let UiNodeType::ProgressBar { value, max, .. } = &mut node.node_type {
            if let Some(v) = update.value {
                *value = v;
            }
            if let Some(m) = update.max {
                *max = m.max(0.0001);
            }
        }
    }
}

fn find_node_mut<'a>(nodes: &'a mut [UiNode], node_id: &str) -> Option<&'a mut UiNode> {
    for node in nodes {
        if node.id == node_id {
            return Some(node);
        }
        if let Some(hit) = find_node_mut(&mut node.children, node_id) {
            return Some(hit);
        }
    }
    None
}

fn collect_hud_state(node: &UiNode, hud: &mut serde_json::Map<String, serde_json::Value>) {
    if !node.visible {
        return;
    }
    match &node.node_type {
        UiNodeType::Text { text, .. } => {
            hud.insert(node.id.clone(), serde_json::Value::String(text.clone()));
        }
        UiNodeType::ProgressBar { value, max, .. } => {
            hud.insert(
                node.id.clone(),
                serde_json::json!({
                    "value": value,
                    "max": max,
                }),
            );
        }
        _ => {}
    }
    for child in &node.children {
        collect_hud_state(child, hud);
    }
}

#[derive(Resource, Default)]
struct UiEventCursor {
    last_frame: u64,
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UiManager::default())
            .insert_resource(DialogueManager::default())
            .insert_resource(UiEventCursor::default())
            .add_systems(Update, apply_ui_events);
    }
}

fn apply_ui_events(
    mut ui: ResMut<UiManager>,
    mut dialogue: ResMut<DialogueManager>,
    events: Res<GameEventBus>,
    mut cursor: ResMut<UiEventCursor>,
) {
    let mut newest = cursor.last_frame;
    for event in events.recent.iter().filter(|e| e.frame > cursor.last_frame) {
        newest = newest.max(event.frame);
        match event.name.as_str() {
            "ui_show_screen" => {
                if let Some(name) = event.data.get("name").and_then(|v| v.as_str()) {
                    let _ = ui.show_screen(name);
                }
            }
            "ui_hide_screen" => {
                if let Some(name) = event.data.get("name").and_then(|v| v.as_str()) {
                    let _ = ui.hide_screen(name);
                }
            }
            "ui_set_text" => {
                if let Some(id) = event.data.get("id").and_then(|v| v.as_str()) {
                    let text = event
                        .data
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let _ = ui.update_node_any(
                        id,
                        UiNodeUpdate {
                            node_type: None,
                            visible: None,
                            text: Some(text),
                            value: None,
                            max: None,
                        },
                    );
                }
            }
            "ui_set_progress" => {
                if let Some(id) = event.data.get("id").and_then(|v| v.as_str()) {
                    let value = event
                        .data
                        .get("value")
                        .and_then(|v| v.as_f64())
                        .map(|v| v as f32);
                    let max = event
                        .data
                        .get("max")
                        .and_then(|v| v.as_f64())
                        .map(|v| v as f32);
                    let _ = ui.update_node_any(
                        id,
                        UiNodeUpdate {
                            node_type: None,
                            visible: None,
                            text: None,
                            value,
                            max,
                        },
                    );
                }
            }
            "dialogue_start" => {
                if let Some(name) = event.data.get("conversation").and_then(|v| v.as_str()) {
                    let _ = dialogue.start(name);
                    ui.dialogue_active = dialogue.snapshot().active;
                }
            }
            "dialogue_choose" => {
                if let Some(choice) = event.data.get("choice").and_then(|v| v.as_u64()) {
                    let _ = dialogue.choose(choice as usize);
                    ui.dialogue_active = dialogue.snapshot().active;
                }
            }
            _ => {}
        }
    }
    cursor.last_frame = newest;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_update_node_text_and_progress() {
        let mut ui = UiManager::default();
        ui.upsert_screen(UiScreen {
            name: "hud".to_string(),
            layer: 0,
            visible: true,
            nodes: vec![
                UiNode {
                    id: "score".to_string(),
                    node_type: UiNodeType::Text {
                        text: "Score: 0".to_string(),
                        font_size: None,
                        color: None,
                        align: None,
                    },
                    position: serde_json::Value::Null,
                    size: serde_json::Value::Null,
                    visible: true,
                    children: vec![],
                    style: serde_json::Value::Null,
                },
                UiNode {
                    id: "health".to_string(),
                    node_type: UiNodeType::ProgressBar {
                        value: 3.0,
                        max: 5.0,
                        color: None,
                        bg_color: None,
                    },
                    position: serde_json::Value::Null,
                    size: serde_json::Value::Null,
                    visible: true,
                    children: vec![],
                    style: serde_json::Value::Null,
                },
            ],
        });

        ui.update_node(
            "hud",
            "score",
            UiNodeUpdate {
                node_type: None,
                visible: None,
                text: Some("Score: 42".to_string()),
                value: None,
                max: None,
            },
        )
        .expect("update score text");
        ui.update_node(
            "hud",
            "health",
            UiNodeUpdate {
                node_type: None,
                visible: None,
                text: None,
                value: Some(4.0),
                max: Some(6.0),
            },
        )
        .expect("update health progress");

        let snap = ui.snapshot();
        assert!(snap.active_screens.iter().any(|s| s == "hud"));
        assert_eq!(
            snap.hud_state
                .get("score")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "Score: 42"
        );
        assert_eq!(
            snap.hud_state
                .get("health")
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_f64())
                .unwrap_or_default(),
            4.0
        );
    }

    #[test]
    fn dialogue_manager_start_choose() {
        let mut dialogue = DialogueManager::default();
        dialogue.upsert_conversation(DialogueConversation {
            name: "intro".to_string(),
            nodes: vec![
                DialogueNode {
                    id: "start".to_string(),
                    speaker: Some("Guide".to_string()),
                    text: "Welcome".to_string(),
                    choices: vec![DialogueChoice {
                        text: "Continue".to_string(),
                        next: Some("end".to_string()),
                        event: Some("dialogue_continue".to_string()),
                    }],
                    event: Some("dialogue_started".to_string()),
                },
                DialogueNode {
                    id: "end".to_string(),
                    speaker: Some("Guide".to_string()),
                    text: "Good luck".to_string(),
                    choices: vec![],
                    event: None,
                },
            ],
        });

        dialogue.start("intro").expect("start dialogue");
        let snap = dialogue.snapshot();
        assert!(snap.active);
        assert_eq!(snap.node_id.as_deref(), Some("start"));
        let (node_event, choice_event) = dialogue.choose(0).expect("choose first option");
        assert_eq!(node_event.as_deref(), Some("dialogue_started"));
        assert_eq!(choice_event.as_deref(), Some("dialogue_continue"));
        let snap2 = dialogue.snapshot();
        assert!(snap2.active);
        assert_eq!(snap2.node_id.as_deref(), Some("end"));
    }
}
