use super::*;

pub(super) async fn get_events(State(state): State<AppState>) -> Json<ApiResponse<Vec<GameEvent>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetEvents(tx));
    match rx.await {
        Ok(items) => Json(ApiResponse::success(items)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn subscribe_events(State(state): State<AppState>) -> impl IntoResponse {
    let sender = state.sender.clone();
    let stream = async_stream::stream! {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));
        let mut last_idx = 0usize;
        loop {
            tick.tick().await;
            let (tx, rx) = tokio::sync::oneshot::channel();
            if sender.send(ApiCommand::GetEvents(tx)).is_err() {
                break;
            }
            let Ok(events) = rx.await else {
                break;
            };
            if events.len() < last_idx {
                last_idx = 0;
            }
            for ev in events.iter().skip(last_idx) {
                let payload = serde_json::to_string(ev).unwrap_or_else(|_| "{}".to_string());
                yield Ok::<SseEvent, Infallible>(SseEvent::default().event("game_event").data(payload));
            }
            last_idx = events.len();
        }
    };
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(5))
            .text("keep-alive"),
    )
}

pub(super) async fn get_perf(State(state): State<AppState>) -> Json<ApiResponse<PerfStats>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetPerf(tx));
    match rx.await {
        Ok(perf) => Json(ApiResponse::success(perf)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_perf_history(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::perf::PerfHistory>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetPerfHistory(tx));
    match rx.await {
        Ok(history) => Json(ApiResponse::success(history)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn upsert_script(
    State(state): State<AppState>,
    Json(req): Json<ScriptUpsertRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::LoadScript(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn list_scripts(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<crate::scripting::api::ScriptInfo>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ListScripts(tx));
    match rx.await {
        Ok(items) => Json(ApiResponse::success(items)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_script(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<ScriptSource>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetScript(name, tx));
    match rx.await {
        Ok(Some(script)) => Json(ApiResponse::success(script)),
        Ok(None) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Script not found".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn delete_script(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::DeleteScript(name, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn test_named_script(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<String>> {
    let (get_tx, get_rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetScript(name, get_tx));
    let source = match get_rx.await {
        Ok(Some(script)) => script.source,
        Ok(None) => return Json(ApiResponse::err("Script not found")),
        Err(_) => return Json(ApiResponse::err("Channel closed")),
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::TestScript(ScriptTestRequest { source }, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_script_errors(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<ScriptError>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetScriptErrors(tx));
    match rx.await {
        Ok(items) => Json(ApiResponse::success(items)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn clear_script_errors(
    State(state): State<AppState>,
) -> Json<ApiResponse<()>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ClearScriptErrors(tx));
    match rx.await {
        Ok(()) => Json(ApiResponse::success(())),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_script_vars(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetScriptVars(tx));
    match rx.await {
        Ok(vars) => Json(ApiResponse::success(vars)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_script_vars(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetScriptVars(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_script_events(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<ScriptEvent>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetScriptEvents(tx));
    match rx.await {
        Ok(items) => Json(ApiResponse::success(items)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_debug_input(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetDebugInput(tx));
    match rx.await {
        Ok(data) => Json(ApiResponse::success(data)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_script_logs(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<crate::scripting::ScriptLogEntry>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetScriptLogs(tx));
    match rx.await {
        Ok(entries) => Json(ApiResponse::success(entries)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn clear_script_logs(
    State(state): State<AppState>,
) -> Json<ApiResponse<()>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ClearScriptLogs(tx));
    match rx.await {
        Ok(()) => Json(ApiResponse::success(())),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_script_stats(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::scripting::api::ScriptStats>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetScriptStats(tx));
    match rx.await {
        Ok(stats) => Json(ApiResponse::success(stats)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn list_animation_graphs(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<crate::animation::AnimationGraphInfo>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ListAnimationGraphs(tx));
    match rx.await {
        Ok(items) => Json(ApiResponse::success(items)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_animation_graph(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<crate::animation::AnimationGraphDef>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetAnimationGraph(name, tx));
    match rx.await {
        Ok(Some(graph)) => Json(ApiResponse::success(graph)),
        Ok(None) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Animation graph not found".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn upsert_animation_graph(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<AnimationGraphRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::SetAnimationGraph(name, req.graph, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn delete_animation_graph(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::DeleteAnimationGraph(name, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_animation_states(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<crate::animation::AnimationEntityState>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetAnimationStates(tx));
    match rx.await {
        Ok(items) => Json(ApiResponse::success(items)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_docs() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(serde_json::json!({
        "name": "Axiom API",
        "version": "0.1",
        "endpoints": docs::docs_endpoints(),
        "components": docs::docs_components(),
        "presets": docs::docs_presets(),
        "templates": docs::docs_templates(),
        "constraints": docs::docs_constraints(),
        "scripts": docs::docs_scripts(),
        "examples": docs::docs_examples(),
        "security": docs::docs_security(),
    })))
}

pub(super) async fn get_docs_html() -> Html<String> {
    fn esc(input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    let mut html = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"/>\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"/>\
         <title>Axiom API Docs</title>\
         <style>body{font-family:ui-sans-serif,system-ui,sans-serif;margin:24px;line-height:1.4;}\
         h1{margin-bottom:4px;}table{border-collapse:collapse;width:100%;margin-top:12px;}\
         th,td{border:1px solid #ddd;padding:8px;text-align:left;vertical-align:top;}\
         th{background:#f5f5f5;}code{background:#f2f2f2;padding:1px 4px;border-radius:4px;}\
         .muted{color:#666;font-size:0.95em;}</style></head><body>\
         <h1>Axiom API</h1><div class=\"muted\">Generated runtime docs</div>\
         <h2>Endpoints</h2><table><thead><tr><th>Method</th><th>Path</th><th>Description</th></tr></thead><tbody>",
    );

    for ep in docs::docs_endpoints() {
        let method = ep
            .get("method")
            .and_then(|v| v.as_str())
            .map(esc)
            .unwrap_or_else(|| "-".to_string());
        let path = ep
            .get("path")
            .and_then(|v| v.as_str())
            .map(esc)
            .unwrap_or_else(|| "-".to_string());
        let description = ep
            .get("description")
            .and_then(|v| v.as_str())
            .map(esc)
            .unwrap_or_else(|| "".to_string());
        html.push_str(&format!(
            "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td></tr>",
            method, path, description
        ));
    }

    html.push_str("</tbody></table>");

    html.push_str(
        "<h2>Built-in Examples</h2><table><thead><tr>\
         <th>Name</th><th>Genre</th><th>Template</th><th>Difficulty</th><th>Seed</th><th>Constraints</th>\
         </tr></thead><tbody>",
    );
    for ex in docs::docs_examples() {
        let name = ex
            .get("name")
            .and_then(|v| v.as_str())
            .map(esc)
            .unwrap_or_else(|| "-".to_string());
        let genre = ex
            .get("genre")
            .and_then(|v| v.as_str())
            .map(esc)
            .unwrap_or_else(|| "-".to_string());
        let template = ex
            .get("template")
            .and_then(|v| v.as_str())
            .map(esc)
            .unwrap_or_else(|| "-".to_string());
        let difficulty = ex
            .get("default_difficulty")
            .and_then(|v| v.as_f64())
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".to_string());
        let seed = ex
            .get("default_seed")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string());
        let constraints = ex
            .get("constraints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(esc)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(String::new);
        html.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td><code>{}</code></td>\
             <td>{}</td><td>{}</td><td>{}</td></tr>",
            name, genre, template, difficulty, seed, constraints
        ));
    }
    html.push_str("</tbody></table>");

    let security = docs::docs_security();
    let token_env = security
        .get("authentication")
        .and_then(|v| v.get("env_var"))
        .and_then(|v| v.as_str())
        .unwrap_or("AXIOM_API_TOKEN");
    let rate_env = security
        .get("rate_limit")
        .and_then(|v| v.get("env_var"))
        .and_then(|v| v.as_str())
        .unwrap_or("AXIOM_API_RATE_LIMIT_PER_SEC");
    let rate_default = security
        .get("rate_limit")
        .and_then(|v| v.get("default"))
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_API_RATE_LIMIT_PER_SEC as u64);
    let script_envs = security
        .get("script_runtime")
        .and_then(|v| v.get("env_vars"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| format!("<code>{}</code>", esc(s)))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    html.push_str(&format!(
        "<h2>Security</h2>\
         <p><code>{}</code> enables token auth via <code>Authorization: Bearer &lt;token&gt;</code> or <code>X-API-Key</code>.</p>\
         <p><code>{}</code> controls per-client request rate (default <code>{}</code>/sec).</p>\
         <p>Script runtime guards: {}</p>",
        esc(token_env),
        esc(rate_env),
        rate_default,
        if script_envs.is_empty() {
            "<em>not configured</em>".to_string()
        } else {
            script_envs
        }
    ));

    html.push_str("</body></html>");
    Html(html)
}

pub(super) async fn get_docs_endpoints() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(serde_json::json!(
        docs::docs_endpoints()
    )))
}

pub(super) async fn get_docs_components() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(serde_json::json!(
        docs::docs_components()
    )))
}

pub(super) async fn get_docs_presets() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(
        serde_json::json!(docs::docs_presets()),
    ))
}

pub(super) async fn get_docs_templates() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(serde_json::json!(
        docs::docs_templates()
    )))
}

pub(super) async fn get_docs_constraints() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(serde_json::json!(
        docs::docs_constraints()
    )))
}

pub(super) async fn get_docs_scripts() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(
        serde_json::json!(docs::docs_scripts()),
    ))
}

pub(super) async fn get_docs_examples() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(serde_json::json!(
        docs::docs_examples()
    )))
}

pub(super) async fn get_docs_security() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::success(serde_json::json!(
        docs::docs_security()
    )))
}

pub(super) async fn set_audio_sfx(
    State(state): State<AppState>,
    Json(req): Json<AudioSfxRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetAudioSfx(req.effects, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_audio_music(
    State(state): State<AppState>,
    Json(req): Json<AudioMusicRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetAudioMusic(req.tracks, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn play_audio(
    State(state): State<AppState>,
    Json(req): Json<AudioPlayRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::PlayAudio(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn stop_audio(
    State(state): State<AppState>,
    Json(req): Json<AudioStopRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::StopAudio(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_audio_config(
    State(state): State<AppState>,
    Json(req): Json<AudioConfigRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetAudioConfig(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_audio_triggers(
    State(state): State<AppState>,
    Json(req): Json<AudioTriggerRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::SetAudioTriggers(req.mappings, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn set_particle_presets(
    State(state): State<AppState>,
    Json(req): Json<ParticlePresetRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::SetParticlePresets(req.presets, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_audio_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::audio::AudioStateSnapshot>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetAudioState(tx));
    match rx.await {
        Ok(snapshot) => Json(ApiResponse::success(snapshot)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_camera_config(
    State(state): State<AppState>,
    Json(req): Json<CameraConfigRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetCameraConfig(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn camera_shake(
    State(state): State<AppState>,
    Json(req): Json<CameraShakeRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::CameraShake(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn camera_look_at(
    State(state): State<AppState>,
    Json(req): Json<CameraLookAtRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::CameraLookAt(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_camera_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<CameraStateResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetCameraState(tx));
    match rx.await {
        Ok(snapshot) => Json(ApiResponse::success(snapshot)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn ui_define_screen(
    State(state): State<AppState>,
    Json(req): Json<UiScreenRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetUiScreen(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn ui_show_screen(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ShowUiScreen(name, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn ui_hide_screen(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::HideUiScreen(name, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn ui_update_node(
    State(state): State<AppState>,
    axum::extract::Path((screen, node_id)): axum::extract::Path<(String, String)>,
    Json(update): Json<UiNodeUpdateRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::UpdateUiNode(screen, node_id, update, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_ui_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::ui::UiStateSnapshot>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetUiState(tx));
    match rx.await {
        Ok(snapshot) => Json(ApiResponse::success(snapshot)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_dialogue_conversation(
    State(state): State<AppState>,
    Json(req): Json<DialogueConversationRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::SetDialogueConversation(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn start_dialogue(
    State(state): State<AppState>,
    Json(req): Json<DialogueStartRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::StartDialogue(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn choose_dialogue(
    State(state): State<AppState>,
    Json(req): Json<DialogueChooseRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ChooseDialogue(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_dialogue_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<crate::ui::DialogueStateSnapshot>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetDialogueState(tx));
    match rx.await {
        Ok(snapshot) => Json(ApiResponse::success(snapshot)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn scene_describe(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (state_tx, state_rx) = tokio::sync::oneshot::channel();
    let (entities_tx, entities_rx) = tokio::sync::oneshot::channel();
    let (ui_tx, ui_rx) = tokio::sync::oneshot::channel();
    let (audio_tx, audio_rx) = tokio::sync::oneshot::channel();
    let (camera_tx, camera_rx) = tokio::sync::oneshot::channel();
    let (dialogue_tx, dialogue_rx) = tokio::sync::oneshot::channel();
    let (vars_tx, vars_rx) = tokio::sync::oneshot::channel();
    let (perf_tx, perf_rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetState(state_tx));
    let _ = state.sender.send(ApiCommand::ListEntities(entities_tx));
    let _ = state.sender.send(ApiCommand::GetUiState(ui_tx));
    let _ = state.sender.send(ApiCommand::GetAudioState(audio_tx));
    let _ = state.sender.send(ApiCommand::GetCameraState(camera_tx));
    let _ = state.sender.send(ApiCommand::GetDialogueState(dialogue_tx));
    let _ = state.sender.send(ApiCommand::GetScriptVars(vars_tx));
    let _ = state.sender.send(ApiCommand::GetPerf(perf_tx));

    let game_state = state_rx.await.ok();
    let entities = entities_rx.await.ok().unwrap_or_default();
    let ui = ui_rx.await.ok();
    let audio = audio_rx.await.ok();
    let camera = camera_rx.await.ok();
    let dialogue = dialogue_rx.await.ok();
    let vars = vars_rx.await.ok();
    let perf = perf_rx.await.ok();
    let visible_entities: Vec<serde_json::Value> = entities
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "pos": [e.x, e.y],
                "tags": e.tags,
                "animation": e.animation_state,
                "animation_frame": e.animation_frame,
                "flip_x": e.animation_facing_right.map(|v| !v),
                "health": e.health,
                "alive": e.alive,
                "sprite_sheet": e.animation_graph,
            })
        })
        .collect();
    let tile_summary = game_state
        .as_ref()
        .map(|g| build_tile_summary(&g.tilemap.tiles))
        .unwrap_or_else(|| serde_json::json!({}));
    let game = {
        let runtime = state.game_runtime.read().unwrap();
        let last_transition = runtime.transitions.last().map(|t| {
            serde_json::json!({
                "from": t.from,
                "to": t.to,
                "effect": t.effect,
                "duration": t.duration,
                "at_unix_ms": t.at_unix_ms,
            })
        });
        let active_transition = runtime.active_transition.as_ref().and_then(|t| {
            let elapsed = t.started_at.elapsed().as_secs_f32();
            let duration = t.duration.max(0.0);
            if duration <= 0.0 || elapsed >= duration {
                return None;
            }
            let remaining = (duration - elapsed).max(0.0);
            let progress = (elapsed / duration).clamp(0.0, 1.0);
            Some(serde_json::json!({
                "from": t.from,
                "to": t.to,
                "effect": t.effect,
                "duration": duration,
                "elapsed_seconds": elapsed,
                "remaining_seconds": remaining,
                "progress": progress,
            }))
        });
        serde_json::json!({
            "state": runtime.state,
            "time_in_state_seconds": runtime.entered_at.elapsed().as_secs_f32(),
            "transition_count": runtime.transitions.len(),
            "last_transition": last_transition,
            "active_transition": active_transition,
        })
    };

    let mut result = serde_json::json!({
        "game": game,
        "visible_entities": visible_entities,
        "tile_summary": tile_summary,
        "ui": ui,
        "dialogue": dialogue,
        "audio": audio,
        "camera": camera,
        "vars": vars.unwrap_or_else(|| serde_json::json!({})),
        "perf": perf,
    });

    // Spatial grid: ?grid=N divides world into NxN cells
    if let Some(grid_str) = params.get("grid") {
        if let Ok(grid_n) = grid_str.parse::<usize>() {
            if grid_n > 0 {
                if let Some(ref gs) = game_state {
                    let spatial = build_spatial_grid(gs, &visible_entities, grid_n);
                    result.as_object_mut().unwrap().insert(
                        "spatial_grid".to_string(),
                        serde_json::to_value(spatial).unwrap_or_default(),
                    );
                }
            }
        }
    }

    Json(ApiResponse::success(result))
}

fn build_spatial_grid(
    game_state: &GameState,
    entities: &[serde_json::Value],
    grid_n: usize,
) -> SpatialGrid {
    let tile_size = 16.0f32; // default tile size
    let world_w = game_state.tilemap.width as f32 * tile_size;
    let world_h = game_state.tilemap.height as f32 * tile_size;
    let cell_w = world_w / grid_n as f32;
    let cell_h = world_h / grid_n as f32;

    let tile_type_name = |t: u8| -> &'static str {
        match t {
            0 => "empty",
            1 => "solid",
            2 => "spike",
            3 => "goal",
            4 => "platform",
            5 => "slope_up",
            6 => "slope_down",
            7 => "ladder",
            _ => "unknown",
        }
    };

    let mut cells = Vec::with_capacity(grid_n * grid_n);
    for row in 0..grid_n {
        for col in 0..grid_n {
            let min_x = col as f32 * cell_w;
            let min_y = row as f32 * cell_h;
            let max_x = min_x + cell_w;
            let max_y = min_y + cell_h;

            // Sample tile types in this cell
            let tile_min_col = (min_x / tile_size).floor() as usize;
            let tile_max_col = ((max_x / tile_size).ceil() as usize).min(game_state.tilemap.width);
            let tile_min_row = (min_y / tile_size).floor() as usize;
            let tile_max_row =
                ((max_y / tile_size).ceil() as usize).min(game_state.tilemap.height);

            let mut tile_types_set = std::collections::BTreeSet::new();
            for ty in tile_min_row..tile_max_row {
                for tx in tile_min_col..tile_max_col {
                    let idx = ty * game_state.tilemap.width + tx;
                    if let Some(&tile) = game_state.tilemap.tiles.get(idx) {
                        tile_types_set.insert(tile_type_name(tile).to_string());
                    }
                }
            }

            // Map entities to this cell
            let mut cell_entity_ids = Vec::new();
            let mut cell_tags_set = std::collections::BTreeSet::new();
            for e in entities {
                let pos = e.get("pos").and_then(|v| v.as_array());
                if let Some(pos) = pos {
                    let ex = pos.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let ey = pos.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    if ex >= min_x && ex < max_x && ey >= min_y && ey < max_y {
                        if let Some(id) = e.get("id").and_then(|v| v.as_u64()) {
                            cell_entity_ids.push(id);
                        }
                        if let Some(tags) = e.get("tags").and_then(|v| v.as_array()) {
                            for tag in tags {
                                if let Some(s) = tag.as_str() {
                                    cell_tags_set.insert(s.to_string());
                                }
                            }
                        }
                    }
                }
            }

            let entity_count = cell_entity_ids.len();
            cells.push(SpatialGridCell {
                row,
                col,
                world_min: [min_x, min_y],
                world_max: [max_x, max_y],
                tile_types: tile_types_set.into_iter().collect(),
                entity_ids: cell_entity_ids,
                entity_tags: cell_tags_set.into_iter().collect(),
                entity_count,
            });
        }
    }

    SpatialGrid {
        rows: grid_n,
        cols: grid_n,
        cell_width: cell_w,
        cell_height: cell_h,
        cells,
    }
}

fn build_tile_summary(tiles: &[u8]) -> serde_json::Value {
    let mut empty = 0usize;
    let mut solid = 0usize;
    let mut spike = 0usize;
    let mut goal = 0usize;
    let mut platform = 0usize;
    let mut slope_up = 0usize;
    let mut slope_down = 0usize;
    let mut ladder = 0usize;
    for t in tiles {
        match *t {
            0 => empty += 1,
            1 => solid += 1,
            2 => spike += 1,
            3 => goal += 1,
            4 => platform += 1,
            5 => slope_up += 1,
            6 => slope_down += 1,
            7 => ladder += 1,
            _ => {}
        }
    }
    serde_json::json!({
        "empty": empty,
        "solid": solid,
        "spike": spike,
        "goal": goal,
        "platform": platform,
        "slope_up": slope_up,
        "slope_down": slope_down,
        "ladder": ladder,
    })
}

// === Gamepad ===

pub(super) async fn get_gamepad_config(
    State(state): State<AppState>,
) -> Json<ApiResponse<GamepadConfigResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetGamepadConfig(tx));
    match rx.await {
        Ok(cfg) => Json(ApiResponse::success(cfg)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_gamepad_config(
    State(state): State<AppState>,
    Json(req): Json<GamepadConfigRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetGamepadConfig(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Tween ===

pub(super) async fn tween_entity(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<TweenRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityTween(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn tween_sequence_entity(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<TweenSequenceRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityTweenSequence(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Screen Effects ===

pub(super) async fn trigger_screen_effect(
    State(state): State<AppState>,
    Json(req): Json<ScreenEffectRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::TriggerScreenEffect(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_screen_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetScreenState(tx));
    match rx.await {
        Ok(val) => Json(ApiResponse::success(val)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

// === Lighting ===

pub(super) async fn set_lighting_config(
    State(state): State<AppState>,
    Json(req): Json<LightingConfigRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetLightingConfig(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_lighting_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<LightingStateResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetLightingState(tx));
    match rx.await {
        Ok(val) => Json(ApiResponse::success(val)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

// === Entity Tint ===

pub(super) async fn set_entity_tint(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<TintRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityTint(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Entity Trail ===

pub(super) async fn set_entity_trail(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<Option<TrailRequest>>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetEntityTrail(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Input Bindings ===

pub(super) async fn get_input_bindings(
    State(state): State<AppState>,
) -> Json<ApiResponse<InputBindingsResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetInputBindings(tx));
    match rx.await {
        Ok(val) => Json(ApiResponse::success(val)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_input_bindings(
    State(state): State<AppState>,
    Json(req): Json<InputBindingsRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetInputBindings(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Day/Night Cycle ===

pub(super) async fn get_day_night(
    State(state): State<AppState>,
) -> Json<ApiResponse<DayNightResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetDayNight(tx));
    match rx.await {
        Ok(val) => Json(ApiResponse::success(val)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_day_night(
    State(state): State<AppState>,
    Json(req): Json<DayNightRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetDayNight(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === World Text ===

pub(super) async fn spawn_world_text(
    State(state): State<AppState>,
    Json(req): Json<WorldTextRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SpawnWorldText(req, tx));
    match rx.await {
        Ok(Ok(id)) => Json(ApiResponse::success(serde_json::json!({ "id": id }))),
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

// === Entity State Machine ===

pub(super) async fn get_entity_state(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
) -> Json<ApiResponse<StateMachineResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetEntityState(id, tx));
    match rx.await {
        Ok(Some(val)) => Json(ApiResponse::success(val)),
        Ok(None) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Entity or state machine not found".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn transition_entity_state(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<StateTransitionRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::TransitionEntityState(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Auto-Tile ===

pub(super) async fn set_auto_tile(
    State(state): State<AppState>,
    Json(req): Json<AutoTileRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetAutoTile(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Parallax ===

pub(super) async fn get_parallax(
    State(state): State<AppState>,
) -> Json<ApiResponse<ParallaxResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetParallax(tx));
    match rx.await {
        Ok(val) => Json(ApiResponse::success(val)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_parallax(
    State(state): State<AppState>,
    Json(req): Json<ParallaxRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetParallax(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Weather ===

pub(super) async fn get_weather(
    State(state): State<AppState>,
) -> Json<ApiResponse<WeatherResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetWeather(tx));
    match rx.await {
        Ok(val) => Json(ApiResponse::success(val)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_weather(
    State(state): State<AppState>,
    Json(req): Json<WeatherRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetWeather(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn clear_weather(
    State(state): State<AppState>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ClearWeather(tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Items/Inventory ===

pub(super) async fn define_items(
    State(state): State<AppState>,
    Json(req): Json<ItemDefineRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::DefineItems(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_entity_inventory(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
) -> Json<ApiResponse<InventoryResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetEntityInventory(id, tx));
    match rx.await {
        Ok(Some(val)) => Json(ApiResponse::success(val)),
        Ok(None) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Entity or inventory not found".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn entity_inventory_action(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
    Json(req): Json<InventoryActionRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::EntityInventoryAction(id, req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Cutscene ===

pub(super) async fn define_cutscene(
    State(state): State<AppState>,
    Json(req): Json<CutsceneDefineRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::DefineCutscene(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn play_cutscene(
    State(state): State<AppState>,
    Json(req): Json<CutscenePlayRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::PlayCutscene(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn stop_cutscene(
    State(state): State<AppState>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::StopCutscene(tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_cutscene_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<CutsceneStateResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetCutsceneState(tx));
    match rx.await {
        Ok(val) => Json(ApiResponse::success(val)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

// === Spawn Presets ===

pub(super) async fn define_presets(
    State(state): State<AppState>,
    Json(presets): Json<std::collections::HashMap<String, EntitySpawnRequest>>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::DefinePresets(presets, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn list_presets(
    State(state): State<AppState>,
) -> Json<ApiResponse<std::collections::HashMap<String, EntitySpawnRequest>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ListPresets(tx));
    match rx.await {
        Ok(presets) => Json(ApiResponse::success(presets)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

// === Tile Layers ===

pub(super) async fn set_tile_layer(
    State(state): State<AppState>,
    Json(req): Json<TileLayerRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetTileLayer(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn get_tile_layers(
    State(state): State<AppState>,
) -> Json<ApiResponse<TileLayersResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetTileLayers(tx));
    match rx.await {
        Ok(resp) => Json(ApiResponse::success(resp)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn delete_tile_layer(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::DeleteTileLayer(name, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Entity Pool ===

pub(super) async fn init_pool(
    State(state): State<AppState>,
    Json(req): Json<PoolInitRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::InitPool(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn acquire_from_pool(
    State(state): State<AppState>,
    Json(req): Json<PoolAcquireRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::AcquireFromPool(req, tx));
    match rx.await {
        Ok(Ok(id)) => Json(ApiResponse::success(serde_json::json!({"id": id}))),
        Ok(Err(e)) => Json(ApiResponse { ok: false, data: None, error: Some(e) }),
        Err(_) => Json(ApiResponse { ok: false, data: None, error: Some("Channel closed".into()) }),
    }
}

pub(super) async fn release_to_pool(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<u64>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ReleaseToPool(id, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === Telemetry ===

pub(super) async fn get_telemetry(
    State(state): State<AppState>,
) -> Json<ApiResponse<GameplayTelemetry>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetTelemetry(tx));
    match rx.await {
        Ok(data) => Json(ApiResponse::success(data)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn reset_telemetry(
    State(state): State<AppState>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ResetTelemetry(tx));
    match rx.await {
        Ok(()) => Json(ApiResponse::ok()),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

// === World Simulation ===

pub(super) async fn simulate_world(
    State(state): State<AppState>,
    Json(req): Json<WorldSimRequest>,
) -> Json<ApiResponse<WorldSimResult>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SimulateWorld(req, tx));
    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(Ok(result))) => Json(ApiResponse::success(result)),
        Ok(Ok(Err(e))) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        }),
        Ok(Err(_)) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("World simulation timed out (30s)".into()),
        }),
    }
}

// === Scenario Testing ===

pub(super) async fn run_scenario(
    State(state): State<AppState>,
    Json(req): Json<ScenarioRequest>,
) -> Json<ApiResponse<ScenarioResult>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::RunScenario(req, tx));
    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(Ok(result))) => Json(ApiResponse::success(result)),
        Ok(Ok(Err(e))) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        }),
        Ok(Err(_)) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Scenario timed out (30s)".into()),
        }),
    }
}

// === Asset Pipeline ===

pub(super) async fn upload_asset(
    State(state): State<AppState>,
    Json(req): Json<AssetUploadRequest>,
) -> Json<ApiResponse<AssetInfo>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::UploadAsset(req, tx));
    match rx.await {
        Ok(Ok(info)) => Json(ApiResponse::success(info)),
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

pub(super) async fn generate_asset(
    State(state): State<AppState>,
    Json(req): Json<AssetGenerateRequest>,
) -> Json<ApiResponse<AssetInfo>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GenerateAsset(req, tx));
    match rx.await {
        Ok(Ok(info)) => Json(ApiResponse::success(info)),
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

pub(super) async fn list_assets(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<AssetInfo>>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::ListAssets(tx));
    match rx.await {
        Ok(items) => Json(ApiResponse::success(items)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn get_pool_status(
    State(state): State<AppState>,
) -> Json<ApiResponse<PoolStatusResponse>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetPoolStatus(tx));
    match rx.await {
        Ok(resp) => Json(ApiResponse::success(resp)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

// === Playtest ===

pub(super) async fn health_check(
    State(state): State<AppState>,
) -> Json<ApiResponse<HealthCheckResult>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::HealthCheck(tx));
    match rx.await {
        Ok(result) => Json(ApiResponse::success(result)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn set_window_config(
    State(state): State<AppState>,
    Json(req): Json<WindowConfigRequest>,
) -> Json<ApiResponse<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::SetWindowConfig(req, tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn evaluate_game(
    State(state): State<AppState>,
) -> Json<ApiResponse<EvaluationResult>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::EvaluateGame(tx));
    match rx.await {
        Ok(result) => Json(ApiResponse::success(result)),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
    }
}

pub(super) async fn evaluate_screenshot(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    // 1. Trigger screenshot
    let (ss_tx, ss_rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::TakeScreenshot(ss_tx));
    let _ = ss_rx.await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 2. Get screenshot path and analyze
    let path = screenshot_path();
    let screenshot_analysis = if path.exists() {
        let (cam_x, cam_y) = {
            let snap = state.snapshot.read().unwrap();
            (snap.tilemap.player_spawn.0, snap.tilemap.player_spawn.1)
        };
        let (entities_tx, entities_rx) = tokio::sync::oneshot::channel();
        let _ = state.sender.send(ApiCommand::ListEntities(entities_tx));
        let entities = entities_rx.await.unwrap_or_default();
        match analyze_screenshot_public(&path, &entities, cam_x, cam_y) {
            Ok(a) => Some(serde_json::to_value(a).unwrap_or_default()),
            Err(e) => Some(serde_json::json!({"error": e})),
        }
    } else {
        None
    };

    // 3. Get scene state (entities, game state, vars)
    let (state_tx, state_rx) = tokio::sync::oneshot::channel();
    let (entities_tx2, entities_rx2) = tokio::sync::oneshot::channel();
    let (vars_tx, vars_rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetState(state_tx));
    let _ = state.sender.send(ApiCommand::ListEntities(entities_tx2));
    let _ = state.sender.send(ApiCommand::GetScriptVars(vars_tx));

    let game_state = state_rx.await.ok();
    let entities = entities_rx2.await.ok().unwrap_or_default();
    let vars = vars_rx.await.ok();

    let runtime_state = {
        let rt = state.game_runtime.read().unwrap();
        rt.state.clone()
    };

    let entity_summary: Vec<serde_json::Value> = entities
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "pos": [e.x, e.y],
                "tags": e.tags,
                "alive": e.alive,
                "health": e.health,
            })
        })
        .collect();

    let result = serde_json::json!({
        "screenshot_path": path.to_string_lossy(),
        "screenshot_exists": path.exists(),
        "analysis": screenshot_analysis,
        "scene": {
            "game_state": runtime_state,
            "entity_count": entities.len(),
            "entities": entity_summary,
            "vars": vars,
            "tilemap": game_state.as_ref().map(|g| serde_json::json!({
                "width": g.tilemap.width,
                "height": g.tilemap.height,
            })),
        },
    });

    Json(ApiResponse::success(result))
}

pub(super) async fn run_playtest(
    State(state): State<AppState>,
    Json(req): Json<PlaytestRequest>,
) -> Json<ApiResponse<PlaytestResult>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::RunPlaytest(req, tx));
    match tokio::time::timeout(std::time::Duration::from_secs(120), rx).await {
        Ok(Ok(Ok(result))) => Json(ApiResponse::success(result)),
        Ok(Ok(Err(e))) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        }),
        Ok(Err(_)) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        }),
        Err(_) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Playtest timed out (120s)".into()),
        }),
    }
}
