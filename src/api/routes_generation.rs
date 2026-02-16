use super::*;

pub(super) async fn validate(
    State(state): State<AppState>,
    Json(req): Json<crate::constraints::ValidateRequest>,
) -> Json<ApiResponse<crate::constraints::ValidateResult>> {
    let snap = state.snapshot.read().unwrap();
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

    let result = crate::constraints::validate(&tilemap, &physics, &req.constraints, &req.entities);
    Json(ApiResponse::success(result))
}

pub(super) async fn get_feel_jump(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::feel::JumpProfile>> {
    let snap = state.snapshot.read().unwrap();
    let tilemap = snap.tilemap.clone();
    let physics = snap.physics.clone();
    drop(snap);

    let profile = crate::feel::measure_jump(&tilemap, &physics);
    Json(ApiResponse::success(profile))
}

pub(super) async fn compare_feel(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<crate::feel::FeelComparison>> {
    let target_name = params
        .get("target")
        .map(|s| s.as_str())
        .unwrap_or("celeste");

    let snap = state.snapshot.read().unwrap();
    let tilemap = snap.tilemap.clone();
    let physics = snap.physics.clone();
    drop(snap);

    let current = crate::feel::measure_jump(&tilemap, &physics);
    let target = crate::feel::get_reference_profile(target_name);
    let comparison = crate::feel::compare(&current, &target);
    Json(ApiResponse::success(comparison))
}

pub(super) async fn tune_feel(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<ApiResponse<serde_json::Value>> {
    let target_name = req
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("celeste")
        .to_string();

    let snapshot = state.snapshot.clone();
    let sender = state.sender.clone();

    let result = tokio::task::spawn_blocking(move || {
        let snap = snapshot.read().unwrap();
        let tilemap = snap.tilemap.clone();
        let physics = snap.physics.clone();
        drop(snap);

        let target = crate::feel::get_reference_profile(&target_name);
        let tune_result = crate::feel::auto_tune(&tilemap, &physics, &target);

        // Apply the tuned physics
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let _ = sender.send(ApiCommand::SetPhysicsConfig(
            tune_result.physics.clone(),
            tx,
        ));

        serde_json::to_value(&tune_result)
            .unwrap_or(serde_json::json!({"error": "serialize failed"}))
    })
    .await
    .unwrap_or(serde_json::json!({"error": "task failed"}));

    Json(ApiResponse::success(result))
}

pub(super) async fn generate_level(
    State(state): State<AppState>,
    Json(req): Json<crate::generation::GenerateRequest>,
) -> Json<ApiResponse<crate::generation::GenerateResult>> {
    let snap = state.snapshot.read().unwrap();
    let physics = snap.physics.clone();
    drop(snap);

    let result = crate::generation::generate(&req, &physics);
    Json(ApiResponse::success(result))
}

pub(super) async fn get_sprites(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::sprites::SpriteManifest>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetSprites(tx));
    match rx.await {
        Ok(manifest) => Json(ApiResponse::success(manifest)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_sprite_sheets(
    State(state): State<AppState>,
) -> Json<ApiResponse<std::collections::HashMap<String, crate::sprites::SpriteSheetDef>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetSpriteSheets(tx));
    match rx.await {
        Ok(items) => Json(ApiResponse::success(items)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn take_screenshot_api(
    State(state): State<AppState>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::TakeScreenshot(tx));
    let _ = rx.await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let path = screenshot_path();
    if path.exists() {
        Json(ApiResponse::success(path.to_string_lossy().to_string()))
    } else {
        Json(ApiResponse::err("Screenshot not saved yet - try again"))
    }
}

pub(super) async fn set_sprites(
    State(state): State<AppState>,
    Json(req): Json<crate::sprites::SpriteManifest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetSprites(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn upsert_sprite_sheet(
    State(state): State<AppState>,
    Json(req): Json<SpriteSheetUpsertRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::UpsertSpriteSheet(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn solve_level(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::pathfinding::SolveResult>> {
    let snap = state.snapshot.read().unwrap();
    let tilemap = snap.tilemap.clone();
    let physics = snap.physics.clone();
    drop(snap);

    let result = crate::pathfinding::solve(&tilemap, &physics);
    Json(ApiResponse::success(result))
}
