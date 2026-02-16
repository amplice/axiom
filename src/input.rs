use bevy::prelude::*;
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

#[derive(Resource, Default, Clone, Copy)]
pub struct LocalPlayerId(pub Option<u64>);

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VirtualInput::default())
            .insert_resource(LocalPlayerId::default())
            .add_systems(
                PreUpdate,
                (
                    sync_local_player_id,
                    keyboard_to_virtual.run_if(resource_exists::<ButtonInput<KeyCode>>),
                )
                    .chain(),
            )
            .add_systems(Last, clear_virtual_input);
    }
}

fn sync_local_player_id(
    mut local_player: ResMut<LocalPlayerId>,
    players: Query<&crate::components::NetworkId, With<crate::components::Player>>,
) {
    local_player.0 = players.iter().next().map(|id| id.0);
}

/// Translate keyboard input to VirtualInput action names
fn keyboard_to_virtual(keyboard: Res<ButtonInput<KeyCode>>, mut vinput: ResMut<VirtualInput>) {
    vinput.active.clear();
    vinput.just_pressed.clear();
    vinput.just_released.clear();

    // Left
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        vinput.active.insert("left".into());
    }
    if keyboard.just_pressed(KeyCode::KeyA) || keyboard.just_pressed(KeyCode::ArrowLeft) {
        vinput.just_pressed.insert("left".into());
    }

    // Right
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        vinput.active.insert("right".into());
    }
    if keyboard.just_pressed(KeyCode::KeyD) || keyboard.just_pressed(KeyCode::ArrowRight) {
        vinput.just_pressed.insert("right".into());
    }

    // Jump / Up
    if keyboard.pressed(KeyCode::Space)
        || keyboard.pressed(KeyCode::KeyW)
        || keyboard.pressed(KeyCode::ArrowUp)
    {
        vinput.active.insert("jump".into());
        vinput.active.insert("up".into());
    }
    if keyboard.just_pressed(KeyCode::Space)
        || keyboard.just_pressed(KeyCode::KeyW)
        || keyboard.just_pressed(KeyCode::ArrowUp)
    {
        vinput.just_pressed.insert("jump".into());
        vinput.just_pressed.insert("up".into());
    }

    // Down
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        vinput.active.insert("down".into());
    }
    if keyboard.just_pressed(KeyCode::KeyS) || keyboard.just_pressed(KeyCode::ArrowDown) {
        vinput.just_pressed.insert("down".into());
    }
}

fn clear_virtual_input(mut vinput: ResMut<VirtualInput>) {
    vinput.just_pressed.clear();
    vinput.just_released.clear();
}
