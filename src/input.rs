use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashSet;

/// Abstraction layer between raw input and game systems.
/// Both keyboard (windowed) and simulation (headless) write to this.
#[derive(Resource, Default, Clone)]
pub struct VirtualInput {
    pub active: HashSet<String>,
    pub just_pressed: HashSet<String>,
    pub just_released: HashSet<String>,
}

impl VirtualInput {
    pub fn pressed(&self, action: &str) -> bool {
        self.active.contains(action)
    }

    pub fn just_pressed(&self, action: &str) -> bool {
        self.just_pressed.contains(action)
    }

    #[allow(dead_code)]
    pub fn clear_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

#[derive(Resource, Default, Clone)]
pub struct MouseInput {
    pub screen_x: f32,
    pub screen_y: f32,
    pub world_x: f32,
    pub world_y: f32,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub left_just_pressed: bool,
    pub right_just_pressed: bool,
    pub middle_just_pressed: bool,
}

#[derive(Resource, Default, Clone, Copy)]
pub struct LocalPlayerId(pub Option<u64>);

/// Gamepad configuration resource
#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct GamepadConfig {
    pub enabled: bool,
    pub deadzone: f32,
}

impl Default for GamepadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            deadzone: 0.2,
        }
    }
}

/// Configurable input bindings.
#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputBindings {
    pub keyboard: std::collections::HashMap<String, Vec<String>>,
    pub gamepad: std::collections::HashMap<String, Vec<String>>,
}

impl Default for InputBindings {
    fn default() -> Self {
        let mut keyboard = std::collections::HashMap::new();
        keyboard.insert("KeyA".into(), vec!["left".into()]);
        keyboard.insert("ArrowLeft".into(), vec!["left".into()]);
        keyboard.insert("KeyD".into(), vec!["right".into()]);
        keyboard.insert("ArrowRight".into(), vec!["right".into()]);
        keyboard.insert("KeyW".into(), vec!["jump".into(), "up".into()]);
        keyboard.insert("ArrowUp".into(), vec!["jump".into(), "up".into()]);
        keyboard.insert("KeyS".into(), vec!["down".into()]);
        keyboard.insert("ArrowDown".into(), vec!["down".into()]);
        keyboard.insert("Space".into(), vec!["attack".into()]);
        keyboard.insert("KeyZ".into(), vec!["attack".into()]);
        keyboard.insert("KeyX".into(), vec!["attack".into()]);
        keyboard.insert("Enter".into(), vec!["attack".into()]);
        keyboard.insert("ShiftLeft".into(), vec!["sprint".into()]);
        keyboard.insert("ShiftRight".into(), vec!["sprint".into()]);

        let mut gamepad = std::collections::HashMap::new();
        gamepad.insert("South".into(), vec!["jump".into()]);
        gamepad.insert("West".into(), vec!["attack".into()]);
        gamepad.insert("East".into(), vec!["attack".into()]);

        Self { keyboard, gamepad }
    }
}

fn keycode_from_name(name: &str) -> Option<KeyCode> {
    match name {
        "KeyA" => Some(KeyCode::KeyA),
        "KeyB" => Some(KeyCode::KeyB),
        "KeyC" => Some(KeyCode::KeyC),
        "KeyD" => Some(KeyCode::KeyD),
        "KeyE" => Some(KeyCode::KeyE),
        "KeyF" => Some(KeyCode::KeyF),
        "KeyG" => Some(KeyCode::KeyG),
        "KeyH" => Some(KeyCode::KeyH),
        "KeyI" => Some(KeyCode::KeyI),
        "KeyJ" => Some(KeyCode::KeyJ),
        "KeyK" => Some(KeyCode::KeyK),
        "KeyL" => Some(KeyCode::KeyL),
        "KeyM" => Some(KeyCode::KeyM),
        "KeyN" => Some(KeyCode::KeyN),
        "KeyO" => Some(KeyCode::KeyO),
        "KeyP" => Some(KeyCode::KeyP),
        "KeyQ" => Some(KeyCode::KeyQ),
        "KeyR" => Some(KeyCode::KeyR),
        "KeyS" => Some(KeyCode::KeyS),
        "KeyT" => Some(KeyCode::KeyT),
        "KeyU" => Some(KeyCode::KeyU),
        "KeyV" => Some(KeyCode::KeyV),
        "KeyW" => Some(KeyCode::KeyW),
        "KeyX" => Some(KeyCode::KeyX),
        "KeyY" => Some(KeyCode::KeyY),
        "KeyZ" => Some(KeyCode::KeyZ),
        "Digit1" => Some(KeyCode::Digit1),
        "Digit2" => Some(KeyCode::Digit2),
        "Digit3" => Some(KeyCode::Digit3),
        "Digit4" => Some(KeyCode::Digit4),
        "Digit5" => Some(KeyCode::Digit5),
        "Digit6" => Some(KeyCode::Digit6),
        "Digit7" => Some(KeyCode::Digit7),
        "Digit8" => Some(KeyCode::Digit8),
        "Digit9" => Some(KeyCode::Digit9),
        "Digit0" => Some(KeyCode::Digit0),
        "ArrowLeft" => Some(KeyCode::ArrowLeft),
        "ArrowRight" => Some(KeyCode::ArrowRight),
        "ArrowUp" => Some(KeyCode::ArrowUp),
        "ArrowDown" => Some(KeyCode::ArrowDown),
        "Space" => Some(KeyCode::Space),
        "Enter" => Some(KeyCode::Enter),
        "ShiftLeft" => Some(KeyCode::ShiftLeft),
        "ShiftRight" => Some(KeyCode::ShiftRight),
        "Escape" => Some(KeyCode::Escape),
        "Tab" => Some(KeyCode::Tab),
        "Backspace" => Some(KeyCode::Backspace),
        _ => None,
    }
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VirtualInput::default())
            .insert_resource(LocalPlayerId::default())
            .insert_resource(MouseInput::default())
            .insert_resource(GamepadConfig::default())
            .insert_resource(InputBindings::default())
            .add_systems(
                PreUpdate,
                (
                    sync_local_player_id,
                    keyboard_to_virtual.run_if(resource_exists::<ButtonInput<KeyCode>>),
                    gamepad_to_virtual,
                    track_mouse.run_if(resource_exists::<ButtonInput<MouseButton>>),
                )
                    .chain(),
            )
            .add_systems(FixedPostUpdate, (clear_just_pressed, clear_mouse_just_pressed));
    }
}

fn sync_local_player_id(
    mut local_player: ResMut<LocalPlayerId>,
    players: Query<&crate::components::NetworkId, With<crate::components::Player>>,
) {
    local_player.0 = players.iter().next().map(|id| id.0);
}

/// Translate keyboard input to VirtualInput action names.
/// NOTE: `just_pressed` is NOT cleared here — it accumulates across frames so that
/// FixedUpdate (which may not tick every frame) never misses a key-press event.
/// It is cleared in `FixedPostUpdate` after scripts have consumed it.
fn keyboard_to_virtual(
    keyboard: Res<ButtonInput<KeyCode>>,
    bindings: Res<InputBindings>,
    mut vinput: ResMut<VirtualInput>,
) {
    vinput.active.clear();

    for (key_name, actions) in &bindings.keyboard {
        let Some(keycode) = keycode_from_name(key_name) else {
            continue;
        };
        if keyboard.pressed(keycode) {
            for action in actions {
                vinput.active.insert(action.clone());
            }
        }
        if keyboard.just_pressed(keycode) {
            for action in actions {
                vinput.just_pressed.insert(action.clone());
            }
        }
    }
}

/// Track mouse position and button state each frame.
fn track_mouse(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>,
    mut mouse: ResMut<MouseInput>,
) {
    mouse.left = mouse_buttons.pressed(MouseButton::Left);
    mouse.right = mouse_buttons.pressed(MouseButton::Right);
    mouse.middle = mouse_buttons.pressed(MouseButton::Middle);
    if mouse_buttons.just_pressed(MouseButton::Left) {
        mouse.left_just_pressed = true;
    }
    if mouse_buttons.just_pressed(MouseButton::Right) {
        mouse.right_just_pressed = true;
    }
    if mouse_buttons.just_pressed(MouseButton::Middle) {
        mouse.middle_just_pressed = true;
    }

    if let Ok(window) = windows.get_single() {
        if let Some(cursor_pos) = window.cursor_position() {
            mouse.screen_x = cursor_pos.x;
            mouse.screen_y = cursor_pos.y;
            // Convert screen to world coordinates
            if let Ok((camera, camera_transform)) = camera_q.get_single() {
                if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, cursor_pos) {
                    mouse.world_x = world_pos.x;
                    mouse.world_y = world_pos.y;
                }
            }
        }
    }
}

/// Clear just_pressed/just_released after FixedUpdate scripts have consumed them.
/// Runs in FixedPostUpdate so that accumulated events from multiple frames are
/// available to scripts even when FixedUpdate doesn't tick every frame.
fn clear_just_pressed(mut vinput: ResMut<VirtualInput>) {
    vinput.just_pressed.clear();
    vinput.just_released.clear();
}

fn clear_mouse_just_pressed(mut mouse: ResMut<MouseInput>) {
    mouse.left_just_pressed = false;
    mouse.right_just_pressed = false;
    mouse.middle_just_pressed = false;
}

/// Map gamepad buttons/axes to VirtualInput actions. Additive with keyboard.
fn gamepad_to_virtual(
    gamepads: Query<&Gamepad>,
    config: Res<GamepadConfig>,
    mut vinput: ResMut<VirtualInput>,
) {
    if !config.enabled {
        return;
    }
    let dz = config.deadzone;
    for gamepad in gamepads.iter() {
        // Left stick + D-pad
        let stick = gamepad.left_stick();
        let dpad = gamepad.dpad();
        let x = stick.x + dpad.x;
        let y = stick.y + dpad.y;

        if x < -dz {
            vinput.active.insert("left".into());
        }
        if x > dz {
            vinput.active.insert("right".into());
        }
        if y > dz {
            vinput.active.insert("up".into());
            vinput.active.insert("jump".into());
        }
        if y < -dz {
            vinput.active.insert("down".into());
        }

        // South (A) → jump
        if gamepad.pressed(GamepadButton::South) {
            vinput.active.insert("jump".into());
        }
        if gamepad.digital().just_pressed(GamepadButton::South) {
            vinput.just_pressed.insert("jump".into());
        }

        // West (X) → attack
        if gamepad.pressed(GamepadButton::West) {
            vinput.active.insert("attack".into());
        }
        if gamepad.digital().just_pressed(GamepadButton::West) {
            vinput.just_pressed.insert("attack".into());
        }

        // East (B) → attack (alternative)
        if gamepad.pressed(GamepadButton::East) {
            vinput.active.insert("attack".into());
        }
        if gamepad.digital().just_pressed(GamepadButton::East) {
            vinput.just_pressed.insert("attack".into());
        }

        // Triggers → sprint
        let left_trigger = gamepad.get(GamepadButton::LeftTrigger2).unwrap_or(0.0);
        let right_trigger = gamepad.get(GamepadButton::RightTrigger2).unwrap_or(0.0);
        if left_trigger > dz
            || right_trigger > dz
            || gamepad.pressed(GamepadButton::LeftTrigger)
            || gamepad.pressed(GamepadButton::RightTrigger)
        {
            vinput.active.insert("sprint".into());
        }
    }
}
