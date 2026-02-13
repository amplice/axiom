pub mod types;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use crossbeam_channel::{Receiver, Sender};
use bevy::prelude::*;
use std::sync::{Arc, Mutex};

use crate::components::*;
use crate::tilemap::{Tilemap, TileEntity};
use crate::simulation::{self, SimulationRequest, SimulationResult};
use crate::sprites::SpriteAssets;
use types::*;

/// Commands sent from API → Bevy
pub enum ApiCommand {
    GetState(tokio::sync::oneshot::Sender<GameState>),
    GetPlayer(tokio::sync::oneshot::Sender<PlayerState>),
    SetLevel(SetLevelRequest, tokio::sync::oneshot::Sender<Result<(), String>>),
    TeleportPlayer(f32, f32, tokio::sync::oneshot::Sender<Result<(), String>>),
    GetPhysicsConfig(tokio::sync::oneshot::Sender<PhysicsConfig>),
    SetPhysicsConfig(PhysicsConfig, tokio::sync::oneshot::Sender<Result<(), String>>),
    GetSprites(tokio::sync::oneshot::Sender<crate::sprites::SpriteManifest>),
    SetSprites(crate::sprites::SpriteManifest, tokio::sync::oneshot::Sender<Result<(), String>>),
}

#[derive(Resource, Default)]
pub struct PendingLevelChange(pub Option<SetLevelRequest>);

#[derive(Resource, Default)]
pub struct PendingPhysicsChange(pub Option<PhysicsConfig>);

#[derive(Resource)]
pub struct ApiChannels {
    pub receiver: Receiver<ApiCommand>,
}

/// Shared snapshot of game data for simulation (updated each frame)
#[derive(Resource)]
pub struct SharedSnapshot {
    pub data: Arc<Mutex<SnapshotData>>,
}

pub struct SnapshotData {
    pub tilemap: Tilemap,
    pub physics: PhysicsConfig,
}

#[derive(Clone)]
struct AppState {
    sender: Sender<ApiCommand>,
    snapshot: Arc<Mutex<SnapshotData>>,
}

pub struct ApiPlugin;

impl Plugin for ApiPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = crossbeam_channel::unbounded::<ApiCommand>();

        let snapshot = Arc::new(Mutex::new(SnapshotData {
            tilemap: Tilemap::test_level(),
            physics: PhysicsConfig::default(),
        }));

        app.insert_resource(ApiChannels { receiver: rx })
            .insert_resource(PendingLevelChange::default())
            .insert_resource(PendingPhysicsChange::default())
            .insert_resource(SharedSnapshot { data: snapshot.clone() })
            .add_systems(Update, (
                update_snapshot,
                process_api_commands,
                apply_level_change,
                apply_physics_change,
            ).chain());

        let state = AppState { sender: tx, snapshot };
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let app = Router::new()
                    .route("/state", get(get_state))
                    .route("/player", get(get_player))
                    .route("/level", post(set_level))
                    .route("/player/position", post(teleport_player))
                    .route("/physics", get(get_physics))
                    .route("/physics", post(set_physics))
                    .route("/simulate", post(simulate))
                    .route("/validate", post(validate))
                    .route("/feel/jump", get(get_feel_jump))
                    .route("/feel/compare", get(compare_feel))
                    .route("/feel/tune", post(tune_feel))
                    .route("/generate", post(generate_level))
                    .route("/sprites", get(get_sprites))
                    .route("/sprites", post(set_sprites))
                    .with_state(state);

                let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
                    .await
                    .expect("Failed to bind to port 3000");

                println!("[Axiom API] Listening on http://127.0.0.1:3000");

                axum::serve(listener, app).await.unwrap();
            });
        });
    }
}

/// Keep the shared snapshot in sync with current game state
fn update_snapshot(
    tilemap: Res<Tilemap>,
    physics: Res<PhysicsConfig>,
    shared: Res<SharedSnapshot>,
) {
    if tilemap.is_changed() || physics.is_changed() {
        if let Ok(mut snap) = shared.data.try_lock() {
            snap.tilemap = tilemap.clone();
            snap.physics = physics.clone();
        }
    }
}

fn process_api_commands(
    channels: Res<ApiChannels>,
    mut player_query: Query<(&mut GamePosition, &mut Velocity, &Grounded, &Alive), With<Player>>,
    tilemap: Res<Tilemap>,
    physics: Res<PhysicsConfig>,
    mut pending_level: ResMut<PendingLevelChange>,
    mut pending_physics: ResMut<PendingPhysicsChange>,
    mut sprite_assets: ResMut<SpriteAssets>,
    asset_server: Res<AssetServer>,
) {
    while let Ok(cmd) = channels.receiver.try_recv() {
        match cmd {
            ApiCommand::GetState(tx) => {
                let player = if let Ok((pos, vel, grounded, alive)) = player_query.get_single() {
                    PlayerState {
                        x: pos.x, y: pos.y, vx: vel.x, vy: vel.y,
                        grounded: grounded.0, alive: alive.0,
                    }
                } else {
                    PlayerState { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, grounded: false, alive: false }
                };
                let state = GameState {
                    tilemap: TilemapState {
                        width: tilemap.width,
                        height: tilemap.height,
                        tiles: tilemap.tiles.clone(),
                        player_spawn: tilemap.player_spawn,
                        goal: tilemap.goal,
                    },
                    player,
                };
                let _ = tx.send(state);
            }
            ApiCommand::GetPlayer(tx) => {
                let player = if let Ok((pos, vel, grounded, alive)) = player_query.get_single() {
                    PlayerState {
                        x: pos.x, y: pos.y, vx: vel.x, vy: vel.y,
                        grounded: grounded.0, alive: alive.0,
                    }
                } else {
                    PlayerState { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, grounded: false, alive: false }
                };
                let _ = tx.send(player);
            }
            ApiCommand::SetLevel(req, tx) => {
                if req.tiles.len() != req.width * req.height {
                    let _ = tx.send(Err(format!(
                        "Tile count {} doesn't match {}x{}={}",
                        req.tiles.len(), req.width, req.height, req.width * req.height
                    )));
                    continue;
                }
                pending_level.0 = Some(req);
                let _ = tx.send(Ok(()));
            }
            ApiCommand::TeleportPlayer(x, y, tx) => {
                if let Ok((mut pos, mut vel, _, _)) = player_query.get_single_mut() {
                    pos.x = x;
                    pos.y = y;
                    vel.x = 0.0;
                    vel.y = 0.0;
                    let _ = tx.send(Ok(()));
                } else {
                    let _ = tx.send(Err("No player found".to_string()));
                }
            }
            ApiCommand::GetPhysicsConfig(tx) => {
                let _ = tx.send(physics.clone());
            }
            ApiCommand::SetPhysicsConfig(config, tx) => {
                pending_physics.0 = Some(config);
                let _ = tx.send(Ok(()));
            }
            ApiCommand::GetSprites(tx) => {
                let _ = tx.send(sprite_assets.manifest.clone());
            }
            ApiCommand::SetSprites(manifest, tx) => {
                crate::sprites::reload_from_manifest(&manifest, &asset_server, &mut sprite_assets);
                let _ = tx.send(Ok(()));
            }
        }
    }
}

fn apply_level_change(
    mut pending: ResMut<PendingLevelChange>,
    mut tilemap: ResMut<Tilemap>,
    mut commands: Commands,
    tile_entities: Query<Entity, With<TileEntity>>,
    physics: Res<PhysicsConfig>,
    sprite_assets: Res<SpriteAssets>,
    mut player_query: Query<(&mut GamePosition, &mut Velocity), With<Player>>,
) {
    let Some(req) = pending.0.take() else { return };

    tilemap.width = req.width;
    tilemap.height = req.height;
    tilemap.tiles = req.tiles;
    if let Some(spawn) = req.player_spawn {
        tilemap.player_spawn = spawn;
    }
    tilemap.goal = req.goal;

    for entity in tile_entities.iter() {
        commands.entity(entity).despawn();
    }

    let ts = physics.tile_size;
    for y in 0..tilemap.height {
        for x in 0..tilemap.width {
            let tile_type = tilemap.get(x as i32, y as i32);
            if tile_type == TileType::Empty { continue; }

            let sprite = if let Some(handle) = sprite_assets.get_tile(tile_type) {
                Sprite {
                    image: handle.clone(),
                    custom_size: Some(Vec2::new(ts, ts)),
                    ..default()
                }
            } else {
                let color = match tile_type {
                    TileType::Solid => Color::srgb(0.4, 0.4, 0.45),
                    TileType::Spike => Color::srgb(0.9, 0.15, 0.15),
                    TileType::Goal => Color::srgb(0.15, 0.9, 0.3),
                    TileType::Empty => unreachable!(),
                };
                Sprite::from_color(color, Vec2::new(ts, ts))
            };

            commands.spawn((
                TileEntity,
                Tile { tile_type },
                GridPosition { x: x as i32, y: y as i32 },
                sprite,
                Transform::from_xyz(x as f32 * ts + ts / 2.0, y as f32 * ts + ts / 2.0, 0.0),
            ));
        }
    }

    if let Ok((mut pos, mut vel)) = player_query.get_single_mut() {
        pos.x = tilemap.player_spawn.0;
        pos.y = tilemap.player_spawn.1;
        vel.x = 0.0;
        vel.y = 0.0;
    }
}

fn apply_physics_change(
    mut pending: ResMut<PendingPhysicsChange>,
    mut physics: ResMut<PhysicsConfig>,
) {
    if let Some(new_config) = pending.0.take() {
        *physics = new_config;
    }
}

// --- HTTP Handlers ---

async fn get_state(State(state): State<AppState>) -> Json<ApiResponse<GameState>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetState(tx));
    match rx.await {
        Ok(game_state) => Json(ApiResponse::success(game_state)),
        Err(_) => Json(ApiResponse { ok: false, data: None, error: Some("Channel closed".into()) }),
    }
}

async fn get_player(State(state): State<AppState>) -> Json<ApiResponse<PlayerState>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetPlayer(tx));
    match rx.await {
        Ok(player) => Json(ApiResponse::success(player)),
        Err(_) => Json(ApiResponse { ok: false, data: None, error: Some("Channel closed".into()) }),
    }
}

async fn set_level(State(state): State<AppState>, Json(req): Json<SetLevelRequest>) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetLevel(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

async fn teleport_player(State(state): State<AppState>, Json(req): Json<TeleportRequest>) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::TeleportPlayer(req.x, req.y, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

async fn get_physics(State(state): State<AppState>) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetPhysicsConfig(tx));
    match rx.await {
        Ok(config) => {
            let val = serde_json::json!({
                "gravity": config.gravity,
                "jump_velocity": config.jump_velocity,
                "move_speed": config.move_speed,
                "fall_multiplier": config.fall_multiplier,
                "coyote_frames": config.coyote_frames,
                "jump_buffer_frames": config.jump_buffer_frames,
                "tile_size": config.tile_size,
            });
            Json(ApiResponse::success(val))
        }
        Err(_) => Json(ApiResponse { ok: false, data: None, error: Some("Channel closed".into()) }),
    }
}

async fn set_physics(State(state): State<AppState>, Json(req): Json<serde_json::Value>) -> Json<ApiResponse<String>> {
    let config = PhysicsConfig {
        gravity: req.get("gravity").and_then(|v| v.as_f64()).unwrap_or(980.0) as f32,
        jump_velocity: req.get("jump_velocity").and_then(|v| v.as_f64()).unwrap_or(400.0) as f32,
        move_speed: req.get("move_speed").and_then(|v| v.as_f64()).unwrap_or(200.0) as f32,
        fall_multiplier: req.get("fall_multiplier").and_then(|v| v.as_f64()).unwrap_or(1.5) as f32,
        coyote_frames: req.get("coyote_frames").and_then(|v| v.as_u64()).unwrap_or(5) as u32,
        jump_buffer_frames: req.get("jump_buffer_frames").and_then(|v| v.as_u64()).unwrap_or(4) as u32,
        tile_size: req.get("tile_size").and_then(|v| v.as_f64()).unwrap_or(16.0) as f32,
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetPhysicsConfig(config, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

async fn simulate(
    State(state): State<AppState>,
    Json(req): Json<SimulationRequest>,
) -> Json<ApiResponse<SimulationResult>> {
    let snap = state.snapshot.lock().unwrap();
    let tilemap = if let Some(ref tm) = req.tilemap {
        Tilemap {
            width: tm.width,
            height: tm.height,
            tiles: tm.tiles.clone(),
            player_spawn: tm.player_spawn.unwrap_or(snap.tilemap.player_spawn),
            goal: tm.goal,
        }
    } else {
        snap.tilemap.clone()
    };

    let mut physics = snap.physics.clone();
    if let Some(ref p) = req.physics {
        if let Some(v) = p.gravity { physics.gravity = v; }
        if let Some(v) = p.jump_velocity { physics.jump_velocity = v; }
        if let Some(v) = p.move_speed { physics.move_speed = v; }
        if let Some(v) = p.fall_multiplier { physics.fall_multiplier = v; }
        if let Some(v) = p.coyote_frames { physics.coyote_frames = v; }
        if let Some(v) = p.jump_buffer_frames { physics.jump_buffer_frames = v; }
    }
    drop(snap);

    let result = simulation::run_simulation(&tilemap, &physics, &req);
    Json(ApiResponse::success(result))
}

async fn validate(
    State(state): State<AppState>,
    Json(req): Json<crate::constraints::ValidateRequest>,
) -> Json<ApiResponse<crate::constraints::ValidateResult>> {
    let snap = state.snapshot.lock().unwrap();
    let tilemap = if let Some(ref tm) = req.tilemap {
        Tilemap {
            width: tm.width,
            height: tm.height,
            tiles: tm.tiles.clone(),
            player_spawn: tm.player_spawn.unwrap_or(snap.tilemap.player_spawn),
            goal: tm.goal,
        }
    } else {
        snap.tilemap.clone()
    };
    let physics = snap.physics.clone();
    drop(snap);

    let result = crate::constraints::validate(&tilemap, &physics, &req.constraints);
    Json(ApiResponse::success(result))
}

async fn get_feel_jump(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::feel::JumpProfile>> {
    let snap = state.snapshot.lock().unwrap();
    let tilemap = snap.tilemap.clone();
    let physics = snap.physics.clone();
    drop(snap);

    let profile = crate::feel::measure_jump(&tilemap, &physics);
    Json(ApiResponse::success(profile))
}

async fn compare_feel(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<crate::feel::FeelComparison>> {
    let target_name = params.get("target").map(|s| s.as_str()).unwrap_or("celeste");

    let snap = state.snapshot.lock().unwrap();
    let tilemap = snap.tilemap.clone();
    let physics = snap.physics.clone();
    drop(snap);

    let current = crate::feel::measure_jump(&tilemap, &physics);
    let target = crate::feel::get_reference_profile(target_name);
    let comparison = crate::feel::compare(&current, &target);
    Json(ApiResponse::success(comparison))
}

async fn tune_feel(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<ApiResponse<serde_json::Value>> {
    let target_name = req.get("target").and_then(|v| v.as_str()).unwrap_or("celeste").to_string();

    let snapshot = state.snapshot.clone();
    let sender = state.sender.clone();

    let result = tokio::task::spawn_blocking(move || {
        let snap = snapshot.lock().unwrap();
        let tilemap = snap.tilemap.clone();
        let physics = snap.physics.clone();
        drop(snap);

        let target = crate::feel::get_reference_profile(&target_name);
        let tune_result = crate::feel::auto_tune(&tilemap, &physics, &target);

        // Apply the tuned physics
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let _ = sender.send(ApiCommand::SetPhysicsConfig(tune_result.physics.clone(), tx));

        serde_json::to_value(&tune_result).unwrap_or(serde_json::json!({"error": "serialize failed"}))
    }).await.unwrap_or(serde_json::json!({"error": "task failed"}));

    Json(ApiResponse::success(result))
}

async fn generate_level(
    State(state): State<AppState>,
    Json(req): Json<crate::generation::GenerateRequest>,
) -> Json<ApiResponse<crate::generation::GenerateResult>> {
    let snap = state.snapshot.lock().unwrap();
    let physics = snap.physics.clone();
    drop(snap);

    let result = crate::generation::generate(&req, &physics);
    Json(ApiResponse::success(result))
}

async fn get_sprites(State(state): State<AppState>) -> Json<ApiResponse<crate::sprites::SpriteManifest>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetSprites(tx));
    match rx.await {
        Ok(manifest) => Json(ApiResponse::success(manifest)),
        Err(_) => Json(ApiResponse { ok: false, data: None, error: Some("Channel closed".into()) }),
    }
}

async fn set_sprites(State(state): State<AppState>, Json(req): Json<crate::sprites::SpriteManifest>) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetSprites(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}
