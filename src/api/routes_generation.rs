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
            ..Default::default()
        }
    } else {
        snap.tilemap.clone()
    };
    let physics = snap.physics.clone();
    drop(snap);

    let result = crate::constraints::validate_full(
        &tilemap, &physics, &req.constraints, &req.entities,
        &req.script_errors, req.perf_fps, &req.available_assets,
    );
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
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::TakeScreenshot(tx));
    let path_str = match rx.await {
        Ok(Ok(p)) => p,
        _ => return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Failed to initiate screenshot".into()),
        }),
    };
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let path = std::path::PathBuf::from(&path_str);
    if !path.exists() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Screenshot not saved yet - try again".into()),
        });
    }

    let screenshot_b64 = read_screenshot_base64(&path);

    if params.get("analyze").map(|v| v == "true").unwrap_or(false) {
        // Fetch entities before calling sync analysis (to avoid holding image types across await)
        let (cam_x, cam_y) = {
            let snap = state.snapshot.read().unwrap();
            (snap.tilemap.player_spawn.0, snap.tilemap.player_spawn.1)
        };
        let (entities_tx, entities_rx) = tokio::sync::oneshot::channel();
        let _ = state.sender.send(ApiCommand::ListEntities(entities_tx));
        let entities = entities_rx.await.unwrap_or_default();

        match analyze_screenshot(&path, &entities, cam_x, cam_y) {
            Ok(analysis) => {
                let mut val = serde_json::to_value(analysis).unwrap_or_default();
                if let Some((b64, w, h)) = screenshot_b64 {
                    if let Some(obj) = val.as_object_mut() {
                        obj.insert("base64".into(), serde_json::json!(b64));
                        obj.insert("width".into(), serde_json::json!(w));
                        obj.insert("height".into(), serde_json::json!(h));
                    }
                }
                Json(ApiResponse::success(val))
            }
            Err(e) => {
                let mut resp = serde_json::json!({
                    "path": path_str,
                    "analysis_error": e,
                });
                if let Some((b64, w, h)) = screenshot_b64 {
                    resp["base64"] = serde_json::json!(b64);
                    resp["width"] = serde_json::json!(w);
                    resp["height"] = serde_json::json!(h);
                }
                Json(ApiResponse::success(resp))
            }
        }
    } else {
        let mut resp = serde_json::json!({ "path": path_str });
        if let Some((b64, w, h)) = screenshot_b64 {
            resp["base64"] = serde_json::json!(b64);
            resp["width"] = serde_json::json!(w);
            resp["height"] = serde_json::json!(h);
        }
        Json(ApiResponse::success(resp))
    }
}

pub(super) fn analyze_screenshot_public(
    path: &std::path::Path,
    entities: &[EntityInfo],
    cam_x: f32,
    cam_y: f32,
) -> Result<ScreenshotAnalysis, String> {
    analyze_screenshot(path, entities, cam_x, cam_y)
}

fn analyze_screenshot(
    path: &std::path::Path,
    entities: &[EntityInfo],
    cam_x: f32,
    cam_y: f32,
) -> Result<ScreenshotAnalysis, String> {
    let img = image::open(path).map_err(|e| format!("Failed to open screenshot: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    // Compute quadrant colors
    let quadrants = [
        ("top_left", 0, 0, w / 2, h / 2),
        ("top_right", w / 2, 0, w, h / 2),
        ("bottom_left", 0, h / 2, w / 2, h),
        ("bottom_right", w / 2, h / 2, w, h),
    ];

    let mut quadrant_colors = Vec::new();
    for (name, x0, y0, x1, y1) in &quadrants {
        let mut r_sum = 0u64;
        let mut g_sum = 0u64;
        let mut b_sum = 0u64;
        let mut count = 0u64;
        for py in *y0..*y1 {
            for px in *x0..*x1 {
                let p = rgba.get_pixel(px, py);
                r_sum += p[0] as u64;
                g_sum += p[1] as u64;
                b_sum += p[2] as u64;
                count += 1;
            }
        }
        let count = count.max(1);
        let avg_r = (r_sum / count) as u8;
        let avg_g = (g_sum / count) as u8;
        let avg_b = (b_sum / count) as u8;
        let brightness =
            (avg_r as f32 * 0.299 + avg_g as f32 * 0.587 + avg_b as f32 * 0.114) / 255.0;
        quadrant_colors.push(QuadrantInfo {
            name: name.to_string(),
            avg_color: [avg_r, avg_g, avg_b],
            avg_brightness: brightness,
        });
    }

    // Project entity positions to screen space (simplified)
    let mut entity_bboxes = Vec::new();
    let mut overlap_pairs = Vec::new();
    let cam_zoom = 1.0f32;
    let half_w = w as f32 / (2.0 * cam_zoom);
    let half_h = h as f32 / (2.0 * cam_zoom);

    for e in entities {
        let screen_x = (e.x - cam_x + half_w) * cam_zoom;
        let screen_y = (h as f32) - (e.y - cam_y + half_h) * cam_zoom;
        let ew = 16.0 * cam_zoom;
        let eh = 16.0 * cam_zoom;
        entity_bboxes.push(EntityBBox {
            id: e.id,
            screen_x,
            screen_y,
            width: ew,
            height: eh,
        });
    }

    // Check AABB overlaps
    for i in 0..entity_bboxes.len() {
        for j in (i + 1)..entity_bboxes.len() {
            let a = &entity_bboxes[i];
            let b = &entity_bboxes[j];
            if a.screen_x < b.screen_x + b.width
                && a.screen_x + a.width > b.screen_x
                && a.screen_y < b.screen_y + b.height
                && a.screen_y + a.height > b.screen_y
            {
                overlap_pairs.push((a.id, b.id));
            }
        }
    }

    Ok(ScreenshotAnalysis {
        path: path.to_string_lossy().to_string(),
        width: w,
        height: h,
        quadrant_colors,
        entity_bboxes,
        overlap_pairs,
    })
}

pub(super) async fn screenshot_baseline(
    State(state): State<AppState>,
) -> Json<ApiResponse<String>> {
    let dir = {
        let snap = state.snapshot.read().unwrap();
        screenshot_dir(snap.physics.screenshot_path.as_deref())
    };
    let src = match latest_screenshot_in_dir(&dir) {
        Some(p) => p,
        None => {
            return Json(ApiResponse::err(
                "No screenshot exists yet. Call GET /screenshot first.",
            ));
        }
    };
    let baseline = dir.join("screenshot_baseline.png");
    match std::fs::copy(&src, &baseline) {
        Ok(_) => Json(ApiResponse::success(
            baseline.to_string_lossy().to_string(),
        )),
        Err(e) => Json(ApiResponse::err(format!("Failed to copy baseline: {e}"))),
    }
}

pub(super) async fn screenshot_diff(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotDiffRequest>,
) -> Json<ApiResponse<ScreenshotDiffResult>> {
    // Take a new screenshot first
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::TakeScreenshot(tx));
    let path_str = match rx.await {
        Ok(Ok(p)) => p,
        _ => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some("Failed to initiate screenshot".into()),
            })
        }
    };
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let current_path = std::path::PathBuf::from(&path_str);
    if !current_path.exists() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Screenshot not saved".into()),
        });
    }

    let dir = {
        let snap = state.snapshot.read().unwrap();
        screenshot_dir(snap.physics.screenshot_path.as_deref())
    };
    let baseline_path = req
        .baseline_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dir.join("screenshot_baseline.png"));

    if !baseline_path.exists() {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!(
                "Baseline not found at {}. Call POST /screenshot/baseline first.",
                baseline_path.display()
            )),
        });
    }

    let baseline_img = match image::open(&baseline_path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Failed to open baseline: {e}")),
            })
        }
    };
    let current_img = match image::open(&current_path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(format!("Failed to open current screenshot: {e}")),
            })
        }
    };

    let (bw, bh) = baseline_img.dimensions();
    let (cw, ch) = current_img.dimensions();
    let w = bw.min(cw);
    let h = bh.min(ch);

    let threshold = 10u8;
    let total_pixels = (w * h) as f64;

    let quadrants = [
        ("top_left", 0u32, 0u32, w / 2, h / 2),
        ("top_right", w / 2, 0, w, h / 2),
        ("bottom_left", 0, h / 2, w / 2, h),
        ("bottom_right", w / 2, h / 2, w, h),
    ];

    let mut total_diff = 0u64;
    let mut quadrant_diffs = Vec::new();

    for (name, x0, y0, x1, y1) in &quadrants {
        let mut diff_count = 0u64;
        let quad_pixels = ((*x1 - *x0) as u64) * ((*y1 - *y0) as u64);
        for py in *y0..*y1 {
            for px in *x0..*x1 {
                if px < w && py < h {
                    let bp = baseline_img.get_pixel(px, py);
                    let cp = current_img.get_pixel(px, py);
                    let dr = (bp[0] as i16 - cp[0] as i16).unsigned_abs() as u8;
                    let dg = (bp[1] as i16 - cp[1] as i16).unsigned_abs() as u8;
                    let db = (bp[2] as i16 - cp[2] as i16).unsigned_abs() as u8;
                    if dr > threshold || dg > threshold || db > threshold {
                        diff_count += 1;
                    }
                }
            }
        }
        total_diff += diff_count;
        quadrant_diffs.push(QuadrantDiff {
            name: name.to_string(),
            diff_percentage: if quad_pixels > 0 {
                (diff_count as f32 / quad_pixels as f32) * 100.0
            } else {
                0.0
            },
        });
    }

    let diff_percentage = if total_pixels > 0.0 {
        (total_diff as f64 / total_pixels * 100.0) as f32
    } else {
        0.0
    };

    Json(ApiResponse::success(ScreenshotDiffResult {
        diff_percentage,
        quadrant_diffs,
        baseline_path: baseline_path.to_string_lossy().to_string(),
        current_path: current_path.to_string_lossy().to_string(),
    }))
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
