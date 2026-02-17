use super::*;

pub(super) async fn get_config(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetConfig(tx));
    match rx.await {
        Ok(cfg) => {
            let json = serde_json::json!({
                "gravity": { "x": cfg.gravity.x, "y": cfg.gravity.y },
                "tile_size": cfg.tile_size,
                "tile_types": cfg.tile_types,
                "move_speed": cfg.move_speed,
                "jump_velocity": cfg.jump_velocity,
                "fall_multiplier": cfg.fall_multiplier,
                "coyote_frames": cfg.coyote_frames,
                "jump_buffer_frames": cfg.jump_buffer_frames,
                "screenshot_path": cfg.screenshot_path,
                "asset_path": cfg.asset_path,
            });
            Json(ApiResponse::success(json))
        }
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_config(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<ApiResponse<String>> {
    // Get current config
    let (tx_get, rx_get) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetConfig(tx_get));
    let mut config = match rx_get.await {
        Ok(v) => v,
        Err(_) => return Json(ApiResponse::err("Channel closed")),
    };

    // Partial merge — only supplied fields are updated
    if let Err(e) = apply_config_overrides(&mut config, &req) {
        return Json(ApiResponse::err(e));
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetConfig(config, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_tile_types(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<ApiResponse<String>> {
    let (tx_get, rx_get) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetConfig(tx_get));
    let mut cfg = match rx_get.await {
        Ok(v) => v,
        Err(_) => return Json(ApiResponse::err("Channel closed")),
    };
    cfg.tile_types = match serde_json::from_value::<TileTypeRegistry>(req) {
        Ok(v) => v,
        Err(_) => return Json(ApiResponse::err("Invalid tile type registry format")),
    };

    let (tx_set, rx_set) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetConfig(cfg, tx_set));
    match rx_set.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn create_entity(
    State(state): State<AppState>,
    Json(req): Json<EntitySpawnRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SpawnEntity(req, tx));
    match rx.await {
        Ok(Ok(id)) => Json(ApiResponse::success(serde_json::json!({"id": id}))),
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

pub(super) async fn create_entity_preset(
    State(state): State<AppState>,
    Json(req): Json<PresetRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let preset_name = req.preset.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SpawnPreset(req, tx));
    match rx.await {
        Ok(Ok(id)) => Json(ApiResponse::success(
            serde_json::json!({"id": id, "preset": preset_name}),
        )),
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

pub(super) async fn reset_non_player_entities(
    State(state): State<AppState>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ResetNonPlayerEntities(tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn list_entities(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<Vec<EntityInfo>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ListEntities(tx));
    match rx.await {
        Ok(entities) => {
            let filtered = filter_entities(entities, &params);
            Json(ApiResponse::success(filtered))
        }
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

fn filter_entities(
    entities: Vec<EntityInfo>,
    params: &std::collections::HashMap<String, String>,
) -> Vec<EntityInfo> {
    let tag = params.get("tag");
    let alive = params.get("alive").and_then(|v| v.parse::<bool>().ok());
    let has_script = params.get("has_script").and_then(|v| v.parse::<bool>().ok());
    let near_x = params.get("near_x").and_then(|v| v.parse::<f32>().ok());
    let near_y = params.get("near_y").and_then(|v| v.parse::<f32>().ok());
    let near_radius = params.get("near_radius").and_then(|v| v.parse::<f32>().ok());
    let limit = params.get("limit").and_then(|v| v.parse::<usize>().ok());
    let entity_state = params.get("entity_state");
    let component = params.get("component");

    // If no filters specified, return all
    if tag.is_none()
        && alive.is_none()
        && has_script.is_none()
        && near_x.is_none()
        && entity_state.is_none()
        && component.is_none()
    {
        return if let Some(lim) = limit {
            entities.into_iter().take(lim).collect()
        } else {
            entities
        };
    }

    let mut result: Vec<EntityInfo> = entities
        .into_iter()
        .filter(|e| {
            if let Some(tag_filter) = tag {
                if !e.tags.iter().any(|t| t == tag_filter) {
                    return false;
                }
            }
            if let Some(alive_filter) = alive {
                if e.alive != Some(alive_filter) {
                    return false;
                }
            }
            if let Some(script_filter) = has_script {
                if script_filter != e.script.is_some() {
                    return false;
                }
            }
            if let (Some(nx), Some(ny), Some(nr)) = (near_x, near_y, near_radius) {
                let dx = e.x - nx;
                let dy = e.y - ny;
                if dx * dx + dy * dy > nr * nr {
                    return false;
                }
            }
            if let Some(state_filter) = entity_state {
                match &e.machine_state {
                    Some(s) if s == state_filter => {}
                    _ => return false,
                }
            }
            if let Some(comp_filter) = component {
                if !e.components.iter().any(|c| c == comp_filter) {
                    return false;
                }
            }
            true
        })
        .collect();

    // Sort by distance if spatial query
    if let (Some(nx), Some(ny)) = (near_x, near_y) {
        result.sort_by(|a, b| {
            let da = (a.x - nx).powi(2) + (a.y - ny).powi(2);
            let db = (b.x - nx).powi(2) + (b.y - ny).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    if let Some(lim) = limit {
        result.truncate(lim);
    }

    result
}

pub(super) async fn get_entity(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetEntity(id, tx));
    match rx.await {
        Ok(Some(entity)) => Json(ApiResponse::success(serde_json::to_value(entity).unwrap())),
        Ok(None) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Entity not found".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn delete_entity(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::DeleteEntity(id, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_entity_position(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<EntityPositionRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityPosition(id, req.x, req.y, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_entity_velocity(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<EntityVelocityRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityVelocity(id, req.vx, req.vy, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn modify_entity_tags(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<EntityTagsRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ModifyEntityTags(id, req.add, req.remove, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_entity_health(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<EntityHealthRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityHealth(id, req.current, req.max, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn bulk_entity_mutate(
    State(state): State<AppState>,
    Json(req): Json<BulkEntityRequest>,
) -> Json<ApiResponse<BulkEntityResult>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::BulkEntityMutate(req, tx));
    match rx.await {
        Ok(result) => Json(ApiResponse::success(result)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_entity_contact_damage(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<EntityContactDamageRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityContactDamage(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_entity_hitbox(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<EntityHitboxRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityHitbox(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_entity_animation(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetEntityAnimation(id, tx));
    match rx.await {
        Ok(Some(anim)) => Json(ApiResponse::success(serde_json::to_value(anim).unwrap())),
        Ok(None) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Entity animation not found".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_entity_animation(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<EntityAnimationRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::SetEntityAnimation(id, req.animation, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_entity_particles(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<EntityParticlesRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::SetEntityParticles(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn damage_entity(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<DamageRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::DamageEntity(id, req.amount, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}
