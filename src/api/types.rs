use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct GameState {
    pub tilemap: TilemapState,
    pub player: PlayerState,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TilemapState {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub player_spawn: (f32, f32),
    pub goal: Option<(i32, i32)>,
}

#[derive(Serialize, Clone)]
pub struct PlayerState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    pub alive: bool,
}

#[derive(Deserialize)]
pub struct SetLevelRequest {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub player_spawn: Option<(f32, f32)>,
    pub goal: Option<(i32, i32)>,
}

#[derive(Deserialize)]
pub struct TeleportRequest {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self { ok: true, data: Some(data), error: None }
    }
}

impl ApiResponse<()> {
    pub fn ok() -> ApiResponse<String> {
        ApiResponse { ok: true, data: Some("ok".to_string()), error: None }
    }

    pub fn err(msg: impl Into<String>) -> ApiResponse<String> {
        ApiResponse { ok: false, data: None, error: Some(msg.into()) }
    }
}
