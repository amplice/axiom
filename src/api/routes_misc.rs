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

    Json(ApiResponse::success(serde_json::json!({
        "game": game,
        "visible_entities": visible_entities,
        "tile_summary": tile_summary,
        "ui": ui,
        "dialogue": dialogue,
        "audio": audio,
        "camera": camera,
        "vars": vars.unwrap_or_else(|| serde_json::json!({})),
        "perf": perf,
    })))
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
