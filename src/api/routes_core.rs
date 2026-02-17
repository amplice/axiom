use super::*;

pub(super) async fn get_state(State(state): State<AppState>) -> Json<ApiResponse<GameState>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetState(tx));
    match rx.await {
        Ok(game_state) => Json(ApiResponse::success(game_state)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_player(State(state): State<AppState>) -> Json<ApiResponse<PlayerState>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetPlayer(tx));
    match rx.await {
        Ok(player) => Json(ApiResponse::success(player)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_level(
    State(state): State<AppState>,
    Json(req): Json<SetLevelRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetLevel(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn teleport_player(
    State(state): State<AppState>,
    Json(req): Json<TeleportRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::TeleportPlayer(req.x, req.y, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_physics(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetPhysicsConfig(tx));
    match rx.await {
        Ok(cfg) => {
            let val = serde_json::json!({
                "gravity": cfg.gravity_magnitude(),
                "tile_size": cfg.tile_size,
                "move_speed": cfg.move_speed,
                "jump_velocity": cfg.jump_velocity,
                "fall_multiplier": cfg.fall_multiplier,
                "coyote_frames": cfg.coyote_frames,
                "jump_buffer_frames": cfg.jump_buffer_frames,
            });
            Json(ApiResponse::success(val))
        }
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_physics(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<ApiResponse<String>> {
    let grav_scalar = req.get("gravity").and_then(|v| v.as_f64()).unwrap_or(980.0) as f32;
    let cfg = GameConfig {
        gravity: Vec2::new(0.0, -grav_scalar),
        tile_size: req
            .get("tile_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(16.0) as f32,
        tile_types: req
            .get("tile_types")
            .map(|v| serde_json::from_value::<TileTypeRegistry>(v.clone()))
            .transpose()
            .unwrap_or(None)
            .unwrap_or_default(),
        move_speed: req
            .get("move_speed")
            .and_then(|v| v.as_f64())
            .unwrap_or(200.0) as f32,
        jump_velocity: req
            .get("jump_velocity")
            .and_then(|v| v.as_f64())
            .unwrap_or(400.0) as f32,
        fall_multiplier: req
            .get("fall_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.5) as f32,
        coyote_frames: req
            .get("coyote_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32,
        jump_buffer_frames: req
            .get("jump_buffer_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(4) as u32,
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetPhysicsConfig(cfg, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn physics_raycast(
    State(state): State<AppState>,
    Json(req): Json<RaycastRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let snap = state.snapshot.read().unwrap();
    let tilemap = snap.tilemap.clone();
    let tile_size = snap.physics.tile_size;
    drop(snap);

    let [ox, oy] = req.origin;
    let [dx, dy] = req.direction;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0001 {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Direction vector cannot be zero".into()),
        });
    }
    let nx = dx / len;
    let ny = dy / len;

    let mut d = 0.0f32;
    let mut prev_tx = (ox / tile_size).floor() as i32;
    let mut prev_ty = (oy / tile_size).floor() as i32;
    while d <= req.max_distance.max(0.0) {
        let x = ox + nx * d;
        let y = oy + ny * d;
        let tx = (x / tile_size).floor() as i32;
        let ty = (y / tile_size).floor() as i32;
        if tilemap.is_solid(tx, ty) {
            let normal_x = (prev_tx - tx) as f32;
            let normal_y = (prev_ty - ty) as f32;
            return Json(ApiResponse::success(serde_json::json!({
                "hit": true,
                "point": [x, y],
                "tile": [tx, ty],
                "distance": d,
                "normal": [normal_x, normal_y],
            })));
        }
        prev_tx = tx;
        prev_ty = ty;
        d += 0.5;
    }
    Json(ApiResponse::success(serde_json::json!({
        "hit": false,
    })))
}

pub(super) async fn physics_raycast_entities(
    State(state): State<AppState>,
    Json(req): Json<EntityRaycastRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let [dx, dy] = req.direction;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0001 {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Direction vector cannot be zero".into()),
        });
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    if state
        .sender
        .send(ApiCommand::RaycastEntities(req, tx))
        .is_err()
    {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Failed to queue raycast command".into()),
        });
    }
    match rx.await {
        Ok(hits) => Json(ApiResponse::success(serde_json::json!({
            "hit": !hits.is_empty(),
            "hits": hits,
            "first": hits.first(),
        }))),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn ai_pathfind(
    State(state): State<AppState>,
    Json(req): Json<PathfindRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let snap = state.snapshot.read().unwrap();
    let tilemap = snap.tilemap.clone();
    let physics = snap.physics.clone();
    drop(snap);

    let from = Vec2::new(req.from[0], req.from[1]);
    let to = Vec2::new(req.to[0], req.to[1]);
    let path_type = req.path_type.as_deref().unwrap_or("top_down");
    let path = if path_type == "top_down" {
        crate::ai::find_top_down_path_points(&tilemap, physics.tile_size, from, to)
    } else if path_type == "platformer" {
        crate::pathfinding::find_platformer_path_points(&tilemap, &physics, from, to)
    } else {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Unsupported path_type. Use top_down or platformer".into()),
        });
    };
    let points = path
        .unwrap_or_default()
        .into_iter()
        .map(|p| [p.x, p.y])
        .collect::<Vec<_>>();
    Json(ApiResponse::success(serde_json::json!({
        "path": points,
        "found": !points.is_empty(),
    })))
}

pub(super) async fn ai_line_of_sight(
    State(state): State<AppState>,
    Json(req): Json<LineOfSightRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let snap = state.snapshot.read().unwrap();
    let tilemap = snap.tilemap.clone();
    let tile_size = snap.physics.tile_size;
    drop(snap);

    let from = Vec2::new(req.from[0], req.from[1]);
    let to = Vec2::new(req.to[0], req.to[1]);
    let has_los = crate::ai::has_line_of_sight_points(&tilemap, tile_size, from, to);
    Json(ApiResponse::success(serde_json::json!({
        "line_of_sight": has_los,
    })))
}

pub(super) async fn simulate(
    State(state): State<AppState>,
    Json(req): Json<SimulationRequest>,
) -> Json<ApiResponse<SimulationResult>> {
    let (tilemap, physics) = {
        let snap = state.snapshot.read().unwrap();
        resolve_simulation_context(&snap.tilemap, &snap.physics, &req)
    };

    let result = simulation::run_simulation(&tilemap, &physics, &req);
    if let Ok(mut replay) = state.replay_store.write() {
        if let Some(active) = replay.active.as_mut() {
            active.steps.push(ReplayStep {
                request: req.clone(),
                result: result.clone(),
            });
        }
    }
    Json(ApiResponse::success(result))
}

pub(super) async fn save_game(
    State(state): State<AppState>,
    Json(req): Json<SaveSlotRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetSaveData(tx));
    let Ok(save) = rx.await else {
        return Json(ApiResponse::err("Channel closed"));
    };
    let slot = sanitize_slot(&req.slot);
    let dir = save_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Json(ApiResponse::err(format!("Failed to create saves dir: {e}")));
    }
    let path = dir.join(format!("{slot}.json"));
    let body = match serde_json::to_string_pretty(&save) {
        Ok(v) => v,
        Err(e) => return Json(ApiResponse::err(format!("Failed to serialize save: {e}"))),
    };
    if let Err(e) = std::fs::write(&path, body) {
        return Json(ApiResponse::err(format!("Failed to write save file: {e}")));
    }
    Json(ApiResponse::success(path.to_string_lossy().to_string()))
}

pub(super) async fn load_game(
    State(state): State<AppState>,
    Json(req): Json<SaveSlotRequest>,
) -> Json<ApiResponse<ImportResult>> {
    let slot = sanitize_slot(&req.slot);
    let path = save_dir().join(format!("{slot}.json"));
    let body = match std::fs::read_to_string(&path) {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Failed to read save file: {e}")),
            })
        }
    };
    let save: SaveGameData = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Invalid save format: {e}")),
            })
        }
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::LoadSaveData(Box::new(save), tx));
    match rx.await {
        Ok(Ok(result)) => Json(ApiResponse::success(result)),
        Ok(Err(e)) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn list_saves() -> Json<ApiResponse<Vec<String>>> {
    let dir = save_dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    Json(ApiResponse::success(out))
}
