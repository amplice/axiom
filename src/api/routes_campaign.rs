use super::*;

pub(super) async fn define_level_pack(
    State(state): State<AppState>,
    Json(mut req): Json<LevelPackRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Level pack name cannot be empty".into()),
        });
    }
    if req.levels.is_empty() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Level pack must contain at least one level".into()),
        });
    }
    for level in &mut req.levels {
        if level.template.trim().is_empty() {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some("Each level must provide a non-empty template".into()),
            });
        }
        if level.config_overrides.is_null() {
            level.config_overrides = serde_json::json!({});
        } else if !level.config_overrides.is_object() {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some("config_overrides must be an object".into()),
            });
        }
    }
    req.name = name.clone();

    let mut store = state.level_packs.write().unwrap();
    store.packs.insert(name.clone(), req);
    store.progress.remove(&name);

    Json(ApiResponse::success(serde_json::json!({
        "name": name,
        "levels": store.packs.get(&name).map(|p| p.levels.len()).unwrap_or(0),
    })))
}

pub(super) async fn start_level_pack(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let pack = {
        let store = state.level_packs.read().unwrap();
        match store.packs.get(&name) {
            Some(p) => p.clone(),
            None => {
                return Json(ApiResponse {
                    ok: false,
                    data: None,
                    error: Some(format!("Level pack not found: {name}")),
                })
            }
        }
    };

    let loaded = match apply_level_pack_level(&state, &pack, 0).await {
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

    let mut store = state.level_packs.write().unwrap();
    store.progress.insert(
        name.clone(),
        LevelPackProgressState {
            current_level: 0,
            completed: false,
            history: Vec::new(),
            level_started_at: std::time::Instant::now(),
        },
    );

    Json(ApiResponse::success(serde_json::json!({
        "name": name,
        "started": true,
        "current_level": 0,
        "total_levels": pack.levels.len(),
        "completed": false,
        "loaded": loaded,
    })))
}

pub(super) async fn next_level_pack(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (pack, mut progress) = {
        let store = state.level_packs.read().unwrap();
        let Some(pack) = store.packs.get(&name).cloned() else {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Level pack not found: {name}")),
            });
        };
        let Some(progress) = store.progress.get(&name).cloned() else {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(
                    "Pack has not been started. Call /levels/pack/{name}/start first.".into(),
                ),
            });
        };
        (pack, progress)
    };

    if progress.completed || pack.levels.is_empty() {
        return Json(ApiResponse::success(serde_json::json!({
            "name": name,
            "completed": true,
            "current_level": progress.current_level,
            "total_levels": pack.levels.len(),
        })));
    }

    let finished_level = progress
        .current_level
        .min(pack.levels.len().saturating_sub(1));
    let finished_def = &pack.levels[finished_level];
    let elapsed = progress.level_started_at.elapsed().as_secs_f32();
    let score = fetch_script_score(&state).await;
    progress.history.push(LevelPackProgressEntry {
        level_index: finished_level,
        template: finished_def.template.clone(),
        difficulty: finished_def.difficulty,
        time_seconds: elapsed,
        score,
    });

    let next_index = finished_level + 1;
    if next_index >= pack.levels.len() {
        progress.completed = true;
        {
            let mut store = state.level_packs.write().unwrap();
            store.progress.insert(name.clone(), progress.clone());
        }
        return Json(ApiResponse::success(serde_json::json!({
            "name": name,
            "completed": true,
            "current_level": finished_level,
            "total_levels": pack.levels.len(),
            "history_len": progress.history.len(),
        })));
    }

    let loaded = match apply_level_pack_level(&state, &pack, next_index).await {
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

    progress.current_level = next_index;
    progress.level_started_at = std::time::Instant::now();
    progress.completed = false;
    {
        let mut store = state.level_packs.write().unwrap();
        store.progress.insert(name.clone(), progress.clone());
    }

    Json(ApiResponse::success(serde_json::json!({
        "name": name,
        "completed": false,
        "current_level": next_index,
        "total_levels": pack.levels.len(),
        "loaded": loaded,
    })))
}

pub(super) async fn level_pack_progress(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<LevelPackProgressResponse>> {
    let (pack, progress) = {
        let store = state.level_packs.read().unwrap();
        let Some(pack) = store.packs.get(&name).cloned() else {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Level pack not found: {name}")),
            });
        };
        (pack, store.progress.get(&name).cloned())
    };

    let response = if let Some(p) = progress {
        LevelPackProgressResponse {
            name,
            total_levels: pack.levels.len(),
            started: true,
            current_level: p.current_level,
            completed: p.completed,
            current_level_elapsed_seconds: if p.completed {
                0.0
            } else {
                p.level_started_at.elapsed().as_secs_f32()
            },
            history: p.history,
        }
    } else {
        LevelPackProgressResponse {
            name,
            total_levels: pack.levels.len(),
            started: false,
            current_level: 0,
            completed: false,
            current_level_elapsed_seconds: 0.0,
            history: Vec::new(),
        }
    };

    Json(ApiResponse::success(response))
}
