use super::*;

fn normalize_effect(effect: Option<String>) -> Option<String> {
    effect.and_then(|e| {
        let trimmed = e.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn set_runtime_state(
    runtime: &mut GameRuntimeStore,
    to: impl Into<String>,
    effect: Option<String>,
    duration: f32,
) {
    let to = to.into();
    let from = runtime.state.clone();
    let normalized_effect = normalize_effect(effect).or(Some("Instant".to_string()));
    runtime.state = to.clone();
    runtime.entered_at = std::time::Instant::now();
    runtime.transitions.push(RuntimeTransition {
        from: from.clone(),
        to: to.clone(),
        effect: normalized_effect.clone(),
        duration,
        at_unix_ms: unix_ms_now(),
    });
    runtime.active_transition = if duration > 0.0
        && normalized_effect
            .as_deref()
            .map(|e| !e.eq_ignore_ascii_case("instant"))
            .unwrap_or(false)
    {
        Some(RuntimeActiveTransition {
            from,
            to,
            effect: normalized_effect,
            duration,
            started_at: std::time::Instant::now(),
        })
    } else {
        None
    };
    if runtime.transitions.len() > 256 {
        let excess = runtime.transitions.len() - 256;
        runtime.transitions.drain(0..excess);
    }
}

fn prune_active_transition(runtime: &mut GameRuntimeStore) {
    let should_clear = runtime
        .active_transition
        .as_ref()
        .map(|t| t.duration <= 0.0 || t.started_at.elapsed().as_secs_f32() >= t.duration)
        .unwrap_or(false);
    if should_clear {
        runtime.active_transition = None;
    }
}

fn active_transition_info(runtime: &GameRuntimeStore) -> Option<GameActiveTransitionInfo> {
    runtime.active_transition.as_ref().and_then(|t| {
        let elapsed = t.started_at.elapsed().as_secs_f32();
        let duration = t.duration.max(0.0);
        if duration <= 0.0 || elapsed >= duration {
            return None;
        }
        let remaining = (duration - elapsed).max(0.0);
        let progress = (elapsed / duration).clamp(0.0, 1.0);
        Some(GameActiveTransitionInfo {
            from: t.from.clone(),
            to: t.to.clone(),
            effect: t.effect.clone(),
            duration,
            elapsed_seconds: elapsed,
            remaining_seconds: remaining,
            progress,
        })
    })
}

fn game_runtime_state_from_store(runtime: &GameRuntimeStore) -> GameRuntimeState {
    let last_transition = runtime
        .transitions
        .last()
        .cloned()
        .map(|t| GameTransitionInfo {
            from: t.from,
            to: t.to,
            effect: t.effect,
            duration: t.duration,
            at_unix_ms: t.at_unix_ms,
        });
    GameRuntimeState {
        state: runtime.state.clone(),
        time_in_state_seconds: runtime.entered_at.elapsed().as_secs_f32(),
        last_transition,
        transition_count: runtime.transitions.len(),
        active_transition: active_transition_info(runtime),
    }
}

pub(super) async fn transition_runtime_state(
    state: &AppState,
    to: impl Into<String>,
    effect: Option<String>,
    duration: f32,
) -> Result<(), String> {
    let to = to.into();
    let normalized_effect = normalize_effect(effect).or(Some("Instant".to_string()));
    {
        let mut runtime = state.game_runtime.write().unwrap();
        set_runtime_state(
            &mut runtime,
            to.clone(),
            normalized_effect.clone(),
            duration,
        );
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    if state
        .sender
        .send(ApiCommand::SetRuntimeState(
            to,
            normalized_effect,
            duration,
            tx,
        ))
        .is_err()
    {
        return Err("Failed to send runtime state command".into());
    }
    match rx.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Runtime state command channel closed".into()),
    }
}

pub(super) async fn list_examples() -> Json<ApiResponse<Vec<ExampleGameInfo>>> {
    let mut out = builtin_examples()
        .into_iter()
        .map(|e| ExampleGameInfo {
            name: e.name.to_string(),
            description: e.description.to_string(),
            genre: e.genre.to_string(),
            template: e.template.to_string(),
            default_difficulty: e.difficulty,
            default_seed: e.seed,
            constraints: e.constraints.iter().map(|c| c.to_string()).collect(),
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Json(ApiResponse::success(out))
}

pub(super) async fn load_example(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    body: Option<Json<ExampleLoadRequest>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let Some(example) = builtin_examples().into_iter().find(|e| e.name == name) else {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!("Unknown example: {name}")),
        });
    };

    let req = body.map(|v| v.0).unwrap_or(ExampleLoadRequest {
        difficulty: None,
        seed: None,
        config_overrides: serde_json::Value::Null,
    });
    let mut level_config = GameConfig::default();
    if let Err(e) = apply_config_overrides(&mut level_config, &example.config_overrides) {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!("Invalid example config overrides: {e}")),
        });
    }
    if let Err(e) = apply_config_overrides(&mut level_config, &req.config_overrides) {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        });
    }

    let mut generated = crate::generation::generate(
        &crate::generation::GenerateRequest {
            width: None,
            height: None,
            difficulty: req.difficulty.unwrap_or(example.difficulty).clamp(0.0, 1.0),
            seed: req.seed.unwrap_or(example.seed),
            constraints: example.constraints.iter().map(|s| s.to_string()).collect(),
            feel_target: None,
            template: Some(example.template.to_string()),
        },
        &level_config,
    );
    generated.entities.extend(example.extra_entities.clone());
    generated.scripts.extend(example.extra_scripts.clone());

    let loaded = match apply_generated_level(&state, &level_config, &generated).await {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(e),
            });
        }
    };

    {
        let mut runtime = state.game_runtime.write().unwrap();
        runtime.last_loaded = Some(LoadedLevelSnapshot {
            config: level_config.clone(),
            generated: generated.clone(),
        });
    }
    if let Err(e) =
        transition_runtime_state(&state, "Playing", Some("Instant".to_string()), 0.0).await
    {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        });
    }

    Json(ApiResponse::success(serde_json::json!({
        "name": example.name,
        "genre": example.genre,
        "description": example.description,
        "template": example.template,
        "difficulty": req.difficulty.unwrap_or(example.difficulty).clamp(0.0, 1.0),
        "seed": req.seed.unwrap_or(example.seed),
        "tilemap": {
            "width": generated.tilemap.width,
            "height": generated.tilemap.height,
        },
        "generated_entity_count": generated.entities.len(),
        "loaded": loaded,
    })))
}

pub(super) async fn get_game_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<GameRuntimeState>> {
    let mut runtime = state.game_runtime.write().unwrap();
    prune_active_transition(&mut runtime);
    Json(ApiResponse::success(game_runtime_state_from_store(
        &runtime,
    )))
}

pub(super) async fn get_game_transitions(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<GameTransitionInfo>>> {
    let runtime = state.game_runtime.read().unwrap();
    let items = runtime
        .transitions
        .iter()
        .cloned()
        .map(|t| GameTransitionInfo {
            from: t.from,
            to: t.to,
            effect: t.effect,
            duration: t.duration,
            at_unix_ms: t.at_unix_ms,
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::success(items))
}

pub(super) async fn set_game_state(
    State(state): State<AppState>,
    Json(req): Json<SetGameStateRequest>,
) -> Json<ApiResponse<GameRuntimeState>> {
    if req.state.trim().is_empty() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("State cannot be empty".into()),
        });
    }
    if let Err(e) = transition_runtime_state(
        &state,
        req.state.trim().to_string(),
        Some("Instant".to_string()),
        0.0,
    )
    .await
    {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        });
    }
    let runtime = state.game_runtime.read().unwrap();
    Json(ApiResponse::success(game_runtime_state_from_store(
        &runtime,
    )))
}

pub(super) async fn transition_game_state(
    State(state): State<AppState>,
    Json(req): Json<GameTransitionRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    if req.to.trim().is_empty() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Target state cannot be empty".into()),
        });
    }
    if let Err(e) = transition_runtime_state(
        &state,
        req.to.trim().to_string(),
        req.effect.clone(),
        req.duration.unwrap_or(0.0),
    )
    .await
    {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        });
    }
    Json(ApiResponse::success(serde_json::json!({
        "state": req.to,
        "effect": req.effect,
        "duration": req.duration.unwrap_or(0.0),
    })))
}

pub(super) async fn load_game_level(
    State(state): State<AppState>,
    Json(req): Json<GameLoadLevelRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    if req.template.trim().is_empty() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Template cannot be empty".into()),
        });
    }
    if !req.config_overrides.is_null() && !req.config_overrides.is_object() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("config_overrides must be an object".into()),
        });
    }

    let mut level_config = {
        let snap = state.snapshot.read().unwrap();
        snap.physics.clone()
    };
    if let Err(e) = apply_config_overrides(&mut level_config, &req.config_overrides) {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        });
    }

    let gen_req = crate::generation::GenerateRequest {
        width: req.width,
        height: req.height,
        difficulty: req.difficulty,
        seed: req.seed,
        constraints: req.constraints.clone(),
        feel_target: None,
        template: Some(req.template.clone()),
    };
    let generated = crate::generation::generate(&gen_req, &level_config);
    let loaded = match apply_generated_level(&state, &level_config, &generated).await {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(e),
            })
        }
    };

    {
        let mut runtime = state.game_runtime.write().unwrap();
        runtime.last_loaded = Some(LoadedLevelSnapshot {
            config: level_config.clone(),
            generated: generated.clone(),
        });
    }
    if let Err(e) =
        transition_runtime_state(&state, "Playing", Some("Instant".to_string()), 0.0).await
    {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        });
    }

    Json(ApiResponse::success(serde_json::json!({
        "template": req.template,
        "difficulty": req.difficulty,
        "seed": req.seed,
        "tilemap": {
            "width": generated.tilemap.width,
            "height": generated.tilemap.height,
        },
        "player_spawn": generated.player_spawn,
        "goal": generated.goal,
        "generated_entities": generated.entities.len(),
        "loaded": loaded,
    })))
}

pub(super) async fn restart_game_level(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let snapshot = {
        let runtime = state.game_runtime.read().unwrap();
        runtime.last_loaded.clone()
    };
    let Some(snapshot) = snapshot else {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("No level has been loaded yet".into()),
        });
    };

    let loaded = match apply_generated_level(&state, &snapshot.config, &snapshot.generated).await {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(e),
            })
        }
    };

    if let Err(e) =
        transition_runtime_state(&state, "Playing", Some("Instant".to_string()), 0.0).await
    {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        });
    }

    Json(ApiResponse::success(serde_json::json!({
        "tilemap": {
            "width": snapshot.generated.tilemap.width,
            "height": snapshot.generated.tilemap.height,
        },
        "player_spawn": snapshot.generated.player_spawn,
        "goal": snapshot.generated.goal,
        "loaded": loaded,
    })))
}

pub(super) async fn replay_record(
    State(state): State<AppState>,
    body: Option<Json<ReplayRecordRequest>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let requested_name = body
        .and_then(|b| b.0.name)
        .unwrap_or_else(|| format!("replay_{}", unix_ms_now()));
    let name = sanitize_replay_name(&requested_name);

    let (tilemap, config) = {
        let snap = state.snapshot.read().unwrap();
        (snap.tilemap.clone(), snap.physics.clone())
    };

    let session = ReplaySession {
        name: name.clone(),
        recorded_at_unix_ms: unix_ms_now(),
        initial_tilemap: tilemap,
        initial_config: config,
        steps: Vec::new(),
    };
    let mut replay = state.replay_store.write().unwrap();
    replay.active = Some(session);

    Json(ApiResponse::success(serde_json::json!({
        "recording": true,
        "name": name,
    })))
}

pub(super) async fn replay_stop(
    State(state): State<AppState>,
    body: Option<Json<ReplayStopRequest>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let maybe_name = body.and_then(|b| b.0.name);
    let mut replay = state.replay_store.write().unwrap();
    let Some(mut session) = replay.active.take() else {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("No active replay recording".into()),
        });
    };

    if let Some(name) = maybe_name {
        session.name = sanitize_replay_name(&name);
    }
    let dir = replay_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!("Failed to create replay dir: {e}")),
        });
    }
    let path = dir.join(format!("{}.json", sanitize_replay_name(&session.name)));
    let payload = match serde_json::to_string_pretty(&session) {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Failed to serialize replay: {e}")),
            })
        }
    };
    if let Err(e) = std::fs::write(&path, payload) {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!("Failed to write replay file: {e}")),
        });
    }

    Json(ApiResponse::success(serde_json::json!({
        "recording": false,
        "name": session.name,
        "path": path.to_string_lossy(),
        "steps": session.steps.len(),
    })))
}

pub(super) async fn replay_play(
    State(state): State<AppState>,
    Json(req): Json<ReplayPlayRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let name = sanitize_replay_name(&req.name);
    let path = replay_dir().join(format!("{name}.json"));
    let body = match std::fs::read_to_string(&path) {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Failed to read replay file: {e}")),
            })
        }
    };
    let session: ReplaySession = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Invalid replay format: {e}")),
            })
        }
    };

    let mut mismatches = Vec::new();
    let mut results = Vec::new();
    for (idx, step) in session.steps.iter().enumerate() {
        let replayed = run_simulation_from_recorded_state(
            &session.initial_tilemap,
            &session.initial_config,
            &step.request,
        );
        if replayed.outcome != step.result.outcome
            || replayed.frames_elapsed != step.result.frames_elapsed
        {
            mismatches.push(idx);
        }
        results.push(replayed);
    }

    // Optional: append replayed results to active recording if one is in progress.
    if let Ok(mut replay_store) = state.replay_store.write() {
        if let Some(active) = replay_store.active.as_mut() {
            for (i, step) in session.steps.iter().enumerate() {
                if let Some(result) = results.get(i) {
                    active.steps.push(ReplayStep {
                        request: step.request.clone(),
                        result: result.clone(),
                    });
                }
            }
        }
    }

    Json(ApiResponse::success(serde_json::json!({
        "name": session.name,
        "steps": session.steps.len(),
        "mismatch_count": mismatches.len(),
        "mismatch_indices": mismatches,
    })))
}

pub(super) async fn replay_list(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let mut items = Vec::new();
    if let Ok(rd) = std::fs::read_dir(replay_dir()) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                items.push(stem.to_string());
            }
        }
    }
    items.sort();
    let (recording, active_name) = {
        let replay = state.replay_store.read().unwrap();
        if let Some(active) = &replay.active {
            (true, Some(active.name.clone()))
        } else {
            (false, None)
        }
    };
    Json(ApiResponse::success(serde_json::json!({
        "recording": recording,
        "active": active_name,
        "items": items,
    })))
}

pub(super) async fn get_debug_overlay(
    State(state): State<AppState>,
) -> Json<ApiResponse<DebugOverlayState>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetDebugOverlay(tx));
    match rx.await {
        Ok(cfg) => Json(ApiResponse::success(cfg)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_debug_overlay(
    State(state): State<AppState>,
    Json(req): Json<DebugOverlayRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetDebugOverlay(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn apply_level_pack_level(
    state: &AppState,
    pack: &LevelPackRequest,
    index: usize,
) -> Result<serde_json::Value, String> {
    let Some(level) = pack.levels.get(index) else {
        return Err(format!("Level index {index} out of bounds"));
    };

    let base_config = {
        let snap = state.snapshot.read().unwrap();
        snap.physics.clone()
    };
    let mut level_config = base_config;
    apply_config_overrides(&mut level_config, &level.config_overrides)?;

    let req = crate::generation::GenerateRequest {
        width: level.width,
        height: level.height,
        difficulty: level.difficulty,
        seed: level.seed,
        constraints: level.constraints.clone(),
        feel_target: None,
        template: Some(level.template.clone()),
    };
    let generated = crate::generation::generate(&req, &level_config);
    let loaded = apply_generated_level(state, &level_config, &generated).await?;
    {
        let mut runtime = state.game_runtime.write().unwrap();
        runtime.last_loaded = Some(LoadedLevelSnapshot {
            config: level_config.clone(),
            generated: generated.clone(),
        });
    }

    Ok(serde_json::json!({
        "index": index,
        "template": level.template.clone(),
        "difficulty": level.difficulty,
        "seed": level.seed,
        "player_spawn": generated.player_spawn,
        "goal": generated.goal,
        "tilemap": {
            "width": generated.tilemap.width,
            "height": generated.tilemap.height,
        },
        "scripts_loaded": loaded.get("scripts_loaded").cloned().unwrap_or(serde_json::json!(0)),
        "spawned_entities": loaded.get("spawned_entities").cloned().unwrap_or(serde_json::json!([])),
        "generated_entity_count": generated.entities.len(),
    }))
}

pub(super) async fn apply_generated_level(
    state: &AppState,
    level_config: &GameConfig,
    generated: &crate::generation::GenerateResult,
) -> Result<serde_json::Value, String> {
    let (cfg_tx, cfg_rx) = tokio::sync::oneshot::channel();
    if state
        .sender
        .send(ApiCommand::SetConfig(level_config.clone(), cfg_tx))
        .is_err()
    {
        return Err("Failed to send config update command".into());
    }
    match cfg_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Config command channel closed".into()),
    }

    let level_req = SetLevelRequest {
        width: generated.tilemap.width,
        height: generated.tilemap.height,
        tiles: generated.tilemap.tiles.clone(),
        player_spawn: Some(generated.player_spawn),
        goal: Some(generated.goal),
    };
    let (lvl_tx, lvl_rx) = tokio::sync::oneshot::channel();
    if state
        .sender
        .send(ApiCommand::SetLevel(level_req, lvl_tx))
        .is_err()
    {
        return Err("Failed to send level update command".into());
    }
    match lvl_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Level command channel closed".into()),
    }

    let (reset_tx, reset_rx) = tokio::sync::oneshot::channel();
    if state
        .sender
        .send(ApiCommand::ResetNonPlayerEntities(reset_tx))
        .is_err()
    {
        return Err("Failed to send entity reset command".into());
    }
    match reset_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Entity reset command channel closed".into()),
    }

    let mut scripts_loaded = 0usize;
    for script in &generated.scripts {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let req = ScriptUpsertRequest {
            name: script.name.clone(),
            source: script.source.clone(),
            global: script.global,
            always_run: None,
        };
        if state.sender.send(ApiCommand::LoadScript(req, tx)).is_err() {
            return Err(format!("Failed to queue script load: {}", script.name));
        }
        match rx.await {
            Ok(Ok(())) => scripts_loaded += 1,
            Ok(Err(e)) => return Err(format!("Script load failed ({}): {e}", script.name)),
            Err(_) => return Err(format!("Script load channel closed ({})", script.name)),
        }
    }

    let mut spawned_entities = Vec::new();
    for placement in &generated.entities {
        let spawn_req = entity_spawn_request_from_placement(placement)?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        if state
            .sender
            .send(ApiCommand::SpawnEntity(spawn_req, tx))
            .is_err()
        {
            return Err(format!(
                "Failed to queue spawn for preset {}",
                placement.preset
            ));
        }
        match rx.await {
            Ok(Ok(id)) => spawned_entities.push(id),
            Ok(Err(e)) => return Err(format!("Spawn failed for preset {}: {e}", placement.preset)),
            Err(_) => {
                return Err(format!(
                    "Spawn channel closed for preset {}",
                    placement.preset
                ))
            }
        }
    }

    Ok(serde_json::json!({
        "scripts_loaded": scripts_loaded,
        "spawned_entities": spawned_entities,
    }))
}

fn entity_spawn_request_from_placement(
    placement: &crate::generation::EntityPlacement,
) -> Result<EntitySpawnRequest, String> {
    let mut req = crate::spawn::preset_to_request(&placement.preset, placement.x, placement.y);
    crate::spawn::apply_preset_config(&mut req, &placement.config)
        .map_err(|e| format!("Invalid config for '{}': {e}", placement.preset))?;
    Ok(req)
}

pub(super) async fn fetch_script_score(state: &AppState) -> Option<f64> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    if state.sender.send(ApiCommand::GetScriptVars(tx)).is_err() {
        return None;
    }
    let vars = rx.await.ok()?;
    let score = vars.get("score")?;
    score
        .as_f64()
        .or_else(|| score.as_i64().map(|v| v as f64))
        .or_else(|| score.as_u64().map(|v| v as f64))
}
