use super::*;

pub(super) async fn export_level(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state.sender.send(ApiCommand::GetSaveData(tx));
    let Ok(save) = rx.await else {
        return Json(ApiResponse {
            ok: false,
            data: None,
            error: Some("Channel closed".into()),
        });
    };
    match serde_json::to_value(save) {
        Ok(v) => Json(ApiResponse::success(v)),
        Err(e) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!("Failed to serialize level export: {e}")),
        }),
    }
}

pub(super) async fn import_level(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<ApiResponse<String>> {
    let save: SaveGameData = match serde_json::from_value(req) {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse::err(format!(
                "Invalid level import payload: {e}"
            )))
        }
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::LoadSaveData(Box::new(save), tx));
    match rx.await {
        Ok(Ok(())) => Json(ApiResponse::ok()),
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn export_game(
    State(state): State<AppState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let project = match collect_project_export_data(&state).await {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(e),
            });
        }
    };
    match serde_json::to_value(project) {
        Ok(v) => Json(ApiResponse::success(v)),
        Err(e) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!("Failed to serialize game export: {e}")),
        }),
    }
}

pub(super) async fn import_game(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<ApiResponse<String>> {
    // Backward compat: accept old SaveGameData payload or new ProjectExportData payload.
    let project = if let Ok(project) = serde_json::from_value::<ProjectExportData>(req.clone()) {
        project
    } else {
        let save = match serde_json::from_value::<SaveGameData>(req) {
            Ok(v) => v,
            Err(e) => {
                return Json(ApiResponse::err(format!(
                    "Invalid game import payload: {e}"
                )))
            }
        };
        ProjectExportData {
            version: 1,
            save,
            level_packs: Vec::new(),
        }
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = state
        .sender
        .send(ApiCommand::LoadSaveData(Box::new(project.save), tx));
    match rx.await {
        Ok(Ok(())) => {
            let mut store = state.level_packs.write().unwrap();
            store.packs.clear();
            store.progress.clear();
            for pack in project.level_packs {
                store.packs.insert(pack.name.clone(), pack);
            }
            Json(ApiResponse::ok())
        }
        Ok(Err(e)) => Json(ApiResponse::err(e)),
        Err(_) => Json(ApiResponse::err("Channel closed")),
    }
}

pub(super) async fn export_web(
    State(state): State<AppState>,
    body: Option<Json<ExportWebRequest>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let project = match collect_project_export_data(&state).await {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(e),
            });
        }
    };
    let req = body.map(|b| b.0).unwrap_or(ExportWebRequest {
        title: Some("AxiomGame".to_string()),
        width: Some(960),
        height: Some(540),
        levels: Some("all".to_string()),
        embed_assets: Some(true),
        strip_incompatible_scripts: Some(true),
        release: Some(false),
    });

    match tokio::task::spawn_blocking(move || build_web_export(req, project)).await {
        Ok(Ok(v)) => Json(ApiResponse::success(v)),
        Ok(Err(e)) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        }),
        Err(e) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!("Export task failed: {e}")),
        }),
    }
}

pub(super) async fn export_desktop(
    State(state): State<AppState>,
    body: Option<Json<ExportDesktopRequest>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let project = match collect_project_export_data(&state).await {
        Ok(v) => v,
        Err(e) => {
            return Json(ApiResponse {
                ok: false,
                data: None,
                error: Some(e),
            });
        }
    };
    let req = body.map(|b| b.0).unwrap_or(ExportDesktopRequest {
        title: Some("AxiomGame".to_string()),
        target: None,
        release: Some(true),
    });

    match tokio::task::spawn_blocking(move || build_desktop_export(req, project)).await {
        Ok(Ok(v)) => Json(ApiResponse::success(v)),
        Ok(Err(e)) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(e),
        }),
        Err(e) => Json(ApiResponse {
            ok: false,
            data: None,
            error: Some(format!("Export task failed: {e}")),
        }),
    }
}

async fn collect_project_export_data(state: &AppState) -> Result<ProjectExportData, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .sender
        .send(ApiCommand::GetSaveData(tx))
        .map_err(|_| "Failed to request save snapshot".to_string())?;
    let save = rx.await.map_err(|_| "Channel closed".to_string())?;

    let mut level_packs = {
        let store = state.level_packs.read().unwrap();
        store.packs.values().cloned().collect::<Vec<_>>()
    };
    level_packs.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ProjectExportData {
        version: 1,
        save,
        level_packs,
    })
}

fn build_web_export(
    req: ExportWebRequest,
    mut project: ProjectExportData,
) -> Result<serde_json::Value, String> {
    let title = req.title.unwrap_or_else(|| "AxiomGame".to_string());
    let safe_title = sanitize_name(&title, "AxiomGame");
    let width = req.width.unwrap_or(960).clamp(320, 4096);
    let height = req.height.unwrap_or(540).clamp(180, 4096);
    let levels = req.levels.unwrap_or_else(|| "all".to_string());
    let embed_assets = req.embed_assets.unwrap_or(true);
    let strip_incompatible_scripts = req.strip_incompatible_scripts.unwrap_or(true);
    let release = req.release.unwrap_or(false);
    let profile = if release { "release" } else { "debug" };
    let mut warnings = Vec::new();
    let mut stripped_scripts = Vec::<String>::new();
    let mut transpiled_scripts = Vec::<String>::new();

    if strip_incompatible_scripts {
        let stripped = strip_web_incompatible_scripts(&mut project);
        stripped_scripts = stripped.removed_script_names.clone();
        transpiled_scripts = stripped.transpiled_script_names.clone();
        if stripped.scripts_removed > 0 || stripped.entity_bindings_removed > 0 {
            let preview = stripped
                .removed_script_names
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if stripped.removed_script_names.len() > 6 {
                ", ..."
            } else {
                ""
            };
            warnings.push(format!(
                "Stripped {} script source(s) and {} entity script binding(s) for web export compatibility (Rhai backend): [{}{}].",
                stripped.scripts_removed, stripped.entity_bindings_removed, preview, suffix
            ));
        }
        if stripped.scripts_transpiled > 0 {
            let preview = stripped
                .transpiled_script_names
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if stripped.transpiled_script_names.len() > 6 {
                ", ..."
            } else {
                ""
            };
            warnings.push(format!(
                "Transpiled {} script source(s) from Lua-compat to Rhai for web export: [{}{}].",
                stripped.scripts_transpiled, preview, suffix
            ));
        }
    }

    let root = export_dir().join("web");
    std::fs::create_dir_all(&root).map_err(|e| format!("Failed to create export dir: {e}"))?;

    let project_json = serde_json::to_string_pretty(&project)
        .map_err(|e| format!("Failed to serialize project export: {e}"))?;
    std::fs::write(root.join("game_data.json"), project_json)
        .map_err(|e| format!("Failed to write game_data.json: {e}"))?;
    let embedded_data_path = root.join("game_data.json");

    let index_html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width,initial-scale=1" />
  <title>{title}</title>
  <style>
    body {{ margin: 0; background: #111; color: #eee; font-family: monospace; }}
    #app {{ width: 100vw; height: 100vh; display: grid; place-items: center; }}
    canvas {{ width: min(100vw, {width}px); height: min(100vh, {height}px); image-rendering: pixelated; }}
    #status {{ position: fixed; left: 8px; top: 8px; background: rgba(0,0,0,0.5); padding: 6px 8px; border-radius: 4px; }}
  </style>
</head>
<body>
  <div id="app">
    <canvas id="bevy" width="{width}" height="{height}"></canvas>
  </div>
  <div id="status">Loading {title}...</div>
  <script type="module" src="./bootstrap.js"></script>
</body>
</html>
"#
    );
    std::fs::write(root.join("index.html"), index_html)
        .map_err(|e| format!("Failed to write index.html: {e}"))?;

    let mut build_cmd = std::process::Command::new("cargo");
    build_cmd
        .arg("build")
        .arg("--target")
        .arg("wasm32-unknown-unknown")
        .arg("--features")
        .arg("web_export")
        .env("AXIOM_EMBED_GAME_DATA_PATH", &embedded_data_path);
    if release {
        build_cmd.arg("--release");
    }
    let build_out = build_cmd
        .output()
        .map_err(|e| format!("Failed to run cargo build for wasm export: {e}"))?;
    if !build_out.status.success() {
        return Err(format!(
            "cargo wasm build failed: {}",
            format_command_error(&build_out)
        ));
    }

    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let wasm_src = wasm_artifact_path(std::path::Path::new(&target_dir), profile);
    if !wasm_src.exists() {
        return Err(format!(
            "Built wasm artifact not found at {}",
            wasm_src.display()
        ));
    }
    std::fs::copy(&wasm_src, root.join("axiom.wasm"))
        .map_err(|e| format!("Failed to copy wasm artifact: {e}"))?;

    let bindgen_out = std::process::Command::new("wasm-bindgen")
        .arg(&wasm_src)
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg(&root)
        .output();
    let bindgen_ok = match bindgen_out {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            warnings.push(format!(
                "wasm-bindgen failed; output includes raw axiom.wasm only: {}",
                format_command_error(&out)
            ));
            false
        }
        Err(_) => {
            warnings.push(
                "wasm-bindgen is not installed; run `cargo install wasm-bindgen-cli` for JS glue"
                    .to_string(),
            );
            false
        }
    };

    let bootstrap = if bindgen_ok {
        r#"import init from "./axiom.js";
const status = document.getElementById("status");
init().then(() => {
  if (status) status.textContent = "Running";
}).catch((err) => {
  const text = String(err || "");
  if (text.includes("Using exceptions for control flow")) {
    if (status) status.textContent = "Running";
    console.warn("[Axiom export] Ignoring expected wasm control-flow exception.");
    return;
  }
  if (status) status.textContent = "Failed to start";
  console.error(err);
});
"#
        .to_string()
    } else {
        r#"const status = document.getElementById("status");
if (status) {
  status.textContent = "Build incomplete: install wasm-bindgen-cli to finish export.";
}
"#
        .to_string()
    };
    std::fs::write(root.join("bootstrap.js"), bootstrap)
        .map_err(|e| format!("Failed to write bootstrap.js: {e}"))?;

    let manifest = serde_json::json!({
        "title": title,
        "safe_title": safe_title,
        "mode": "web",
        "ready": bindgen_ok,
        "levels": levels,
        "embed_assets": embed_assets,
        "strip_incompatible_scripts": strip_incompatible_scripts,
        "stripped_scripts": stripped_scripts,
        "transpiled_scripts": transpiled_scripts,
        "release": release,
        "profile": profile,
        "artifacts": {
            "root": root.to_string_lossy(),
            "index_html": root.join("index.html").to_string_lossy(),
            "bootstrap_js": root.join("bootstrap.js").to_string_lossy(),
            "game_data_json": root.join("game_data.json").to_string_lossy(),
            "embedded_game_data": embedded_data_path.to_string_lossy(),
            "raw_wasm": root.join("axiom.wasm").to_string_lossy(),
            "bindgen_js": root.join("axiom.js").to_string_lossy(),
            "bindgen_wasm": root.join("axiom_bg.wasm").to_string_lossy(),
        },
        "warnings": warnings,
    });
    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize export manifest: {e}"))?;
    std::fs::write(root.join("manifest.json"), manifest_text)
        .map_err(|e| format!("Failed to write manifest.json: {e}"))?;

    Ok(manifest)
}

fn build_desktop_export(
    req: ExportDesktopRequest,
    project: ProjectExportData,
) -> Result<serde_json::Value, String> {
    let title = req.title.unwrap_or_else(|| "AxiomGame".to_string());
    let safe_title = sanitize_name(&title, "AxiomGame");
    let release = req.release.unwrap_or(true);
    let profile = if release { "release" } else { "debug" };
    let triple = resolve_desktop_target(req.target.as_deref())?;
    let out_dir = export_dir()
        .join("desktop")
        .join(sanitize_name(&triple, "desktop"));
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("Failed to create export dir: {e}"))?;

    let project_json = serde_json::to_string_pretty(&project)
        .map_err(|e| format!("Failed to serialize project export: {e}"))?;
    let embedded_data_path = out_dir.join("game_data.json");
    std::fs::write(&embedded_data_path, project_json)
        .map_err(|e| format!("Failed to write game_data.json: {e}"))?;

    let mut build_cmd = std::process::Command::new("cargo");
    build_cmd
        .arg("build")
        .arg("--target")
        .arg(&triple)
        .arg("--features")
        .arg("desktop_export")
        .env("AXIOM_EMBED_GAME_DATA_PATH", &embedded_data_path);
    if release {
        build_cmd.arg("--release");
    }
    let build_out = build_cmd
        .output()
        .map_err(|e| format!("Failed to run cargo build for desktop export: {e}"))?;
    if !build_out.status.success() {
        return Err(format!(
            "cargo desktop build failed: {}",
            format_command_error(&build_out)
        ));
    }

    let exe_name = if triple.contains("windows") {
        "axiom.exe"
    } else {
        "axiom"
    };
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let built_bin = desktop_artifact_path(std::path::Path::new(&target_dir), &triple, profile);
    debug_assert!(built_bin
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == exe_name));
    if !built_bin.exists() {
        return Err(format!(
            "Built desktop artifact not found at {}",
            built_bin.display()
        ));
    }

    let out_bin = if triple.contains("windows") {
        out_dir.join(format!("{safe_title}.exe"))
    } else {
        out_dir.join(safe_title.clone())
    };
    std::fs::copy(&built_bin, &out_bin)
        .map_err(|e| format!("Failed to copy desktop artifact: {e}"))?;

    let manifest = serde_json::json!({
        "title": title,
        "safe_title": safe_title,
        "mode": "desktop",
        "target": triple,
        "release": release,
        "profile": profile,
        "artifacts": {
            "root": out_dir.to_string_lossy(),
            "binary": out_bin.to_string_lossy(),
            "game_data_json": out_dir.join("game_data.json").to_string_lossy(),
            "embedded_game_data": embedded_data_path.to_string_lossy(),
        },
    });
    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize export manifest: {e}"))?;
    std::fs::write(out_dir.join("manifest.json"), manifest_text)
        .map_err(|e| format!("Failed to write manifest.json: {e}"))?;

    Ok(manifest)
}

fn resolve_desktop_target(input: Option<&str>) -> Result<String, String> {
    let host = if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let target = input.unwrap_or(host).trim();
    if target.is_empty() {
        return Ok(host.to_string());
    }
    let normalized = match target.to_ascii_lowercase().as_str() {
        "windows" | "win" => "x86_64-pc-windows-msvc".to_string(),
        "linux" => "x86_64-unknown-linux-gnu".to_string(),
        "macos" | "mac" | "darwin" => "x86_64-apple-darwin".to_string(),
        _ => target.to_string(),
    };
    Ok(normalized)
}

fn wasm_artifact_path(target_dir: &std::path::Path, profile: &str) -> std::path::PathBuf {
    target_dir
        .join("wasm32-unknown-unknown")
        .join(profile)
        .join("axiom.wasm")
}

fn desktop_artifact_path(
    target_dir: &std::path::Path,
    triple: &str,
    profile: &str,
) -> std::path::PathBuf {
    let exe = if triple.contains("windows") {
        "axiom.exe"
    } else {
        "axiom"
    };
    target_dir.join(triple).join(profile).join(exe)
}

struct ScriptStripSummary {
    scripts_removed: usize,
    scripts_transpiled: usize,
    entity_bindings_removed: usize,
    removed_script_names: Vec<String>,
    transpiled_script_names: Vec<String>,
}

fn strip_web_incompatible_scripts(project: &mut ProjectExportData) -> ScriptStripSummary {
    let mut scripts_removed = 0usize;
    let mut scripts_transpiled = 0usize;
    let mut removed_script_names = Vec::<String>::new();
    let mut transpiled_script_names = Vec::<String>::new();
    let global_set = project
        .save
        .global_scripts
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let rhai_engine = rhai::Engine::new();

    let mut normalized_scripts = std::collections::HashMap::<String, String>::new();
    for (name, source) in project.save.scripts.iter() {
        let is_global = global_set.contains(name);
        if script_compiles_for_wasm(&rhai_engine, source, is_global)
            && !has_unhandled_lua_syntax(source)
        {
            normalized_scripts.insert(name.clone(), source.clone());
            continue;
        }

        if let Some(transpiled) = crate::scripting::lua_compat::transpile_lua_compat_to_rhai(source)
        {
            if script_compiles_for_wasm(&rhai_engine, &transpiled, is_global)
                && !has_unhandled_lua_syntax(&transpiled)
            {
                normalized_scripts.insert(name.clone(), transpiled);
                scripts_transpiled = scripts_transpiled.saturating_add(1);
                transpiled_script_names.push(name.clone());
                continue;
            }
        }

        scripts_removed = scripts_removed.saturating_add(1);
        removed_script_names.push(name.clone());
    }

    project.save.scripts = normalized_scripts;
    project
        .save
        .global_scripts
        .retain(|name| project.save.scripts.contains_key(name));

    let mut entity_bindings_removed = 0usize;
    for entity in &mut project.save.entities {
        let should_remove_binding = entity
            .script
            .as_ref()
            .is_some_and(|name| !project.save.scripts.contains_key(name));
        if should_remove_binding {
            entity.script = None;
            entity.script_state = None;
            entity_bindings_removed = entity_bindings_removed.saturating_add(1);
        }
    }

    removed_script_names.sort();
    transpiled_script_names.sort();

    ScriptStripSummary {
        scripts_removed,
        scripts_transpiled,
        entity_bindings_removed,
        removed_script_names,
        transpiled_script_names,
    }
}

fn script_compiles_for_wasm(engine: &rhai::Engine, source: &str, global: bool) -> bool {
    let wrapped = wrap_rhai_source(source, !global);
    engine.compile(wrapped).is_ok()
}

fn wrap_rhai_source(source: &str, entity_script: bool) -> String {
    if entity_script {
        format!(
            "{source}\nfn __axiom_entity_entry(entity, world, dt) {{ update(entity, world, dt); }}"
        )
    } else {
        format!("{source}\nfn __axiom_global_entry(world, dt) {{ update(world, dt); }}")
    }
}

fn has_unhandled_lua_syntax(source: &str) -> bool {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return false;
    }

    // Tokens we do not currently transpile into Rhai-safe equivalents.
    let word_tokens = ["repeat", "until", "goto", "break", "continue"];
    if word_tokens
        .iter()
        .any(|token| contains_word_token(source, token))
    {
        return true;
    }

    // Lua numeric for-loop marker.
    source.contains(" do\n")
        || source.contains(" do\r\n")
        || source.contains(" do;")
        || source.contains("::")
}

fn contains_word_token(source: &str, token: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let token_chars: Vec<char> = token.chars().collect();
    if token_chars.is_empty() {
        return false;
    }

    let mut i = 0usize;
    while i + token_chars.len() <= chars.len() {
        if chars[i..(i + token_chars.len())] == token_chars[..] {
            let prev_ok = i == 0 || !is_word_char(chars[i - 1]);
            let next_ok =
                i + token_chars.len() == chars.len() || !is_word_char(chars[i + token_chars.len()]);
            if prev_ok && next_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn format_command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = stderr.trim();
    let stdout = stdout.trim();
    if !stderr.is_empty() {
        stderr.lines().take(12).collect::<Vec<_>>().join(" | ")
    } else if !stdout.is_empty() {
        stdout.lines().take(12).collect::<Vec<_>>().join(" | ")
    } else {
        format!("exit status {}", output.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_desktop_target_aliases() {
        assert_eq!(
            resolve_desktop_target(Some("windows")).unwrap(),
            "x86_64-pc-windows-msvc"
        );
        assert_eq!(
            resolve_desktop_target(Some("linux")).unwrap(),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            resolve_desktop_target(Some("macos")).unwrap(),
            "x86_64-apple-darwin"
        );
        assert_eq!(
            resolve_desktop_target(Some("x86_64-unknown-linux-gnu")).unwrap(),
            "x86_64-unknown-linux-gnu"
        );
    }

    #[test]
    fn export_artifact_paths_are_stable() {
        let root = std::path::Path::new("target");
        let wasm = wasm_artifact_path(root, "release");
        assert!(wasm.ends_with(std::path::Path::new(
            "target/wasm32-unknown-unknown/release/axiom.wasm"
        )));

        let win = desktop_artifact_path(root, "x86_64-pc-windows-msvc", "debug");
        assert!(win.ends_with(std::path::Path::new(
            "target/x86_64-pc-windows-msvc/debug/axiom.exe"
        )));

        let linux = desktop_artifact_path(root, "x86_64-unknown-linux-gnu", "release");
        assert!(linux.ends_with(std::path::Path::new(
            "target/x86_64-unknown-linux-gnu/release/axiom"
        )));
    }

    #[test]
    fn strip_web_incompatible_scripts_clears_only_incompatible_payloads() {
        let mut project = ProjectExportData {
            version: 1,
            save: SaveGameData {
                version: 1,
                config: GameConfig::default(),
                tilemap: Tilemap::test_level(),
                game_state: "Playing".to_string(),
                next_network_id: 2,
                entities: vec![SaveEntity {
                    network_id: Some(1),
                    x: 0.0,
                    y: 0.0,
                    vx: 0.0,
                    vy: 0.0,
                    is_player: false,
                    components: Vec::new(),
                    script: Some("enemy_patrol".to_string()),
                    script_state: Some(serde_json::json!({"phase":"idle"})),
                    tags: vec!["enemy".to_string()],
                    alive: true,
                    ai_state: None,
                    invincibility_frames: None,
                    path_follower_path: Vec::new(),
                    path_follower_frames_until_recalc: None,
                }],
                scripts: std::collections::HashMap::from([(
                    "enemy_patrol".to_string(),
                    "function update(entity, world, dt)\nend".to_string(),
                ), (
                    "rhai_ok".to_string(),
                    "fn update(entity, world, dt) { entity.vx += 1; }".to_string(),
                ), (
                    "lua_hard".to_string(),
                    "function update(entity, world, dt)\n  repeat\n    entity.vx = entity.vx + 1\n  until entity.vx > 5\nend".to_string(),
                )]),
                global_scripts: vec!["enemy_patrol".to_string()],
                game_vars: std::collections::HashMap::new(),
                animation_graphs: std::collections::HashMap::new(),
                sprite_sheets: std::collections::HashMap::new(),
                particle_presets: std::collections::HashMap::new(),
            },
            level_packs: Vec::new(),
        };

        let summary = strip_web_incompatible_scripts(&mut project);
        assert_eq!(summary.scripts_removed, 1);
        assert_eq!(summary.scripts_transpiled, 1);
        assert_eq!(summary.entity_bindings_removed, 0);
        assert_eq!(summary.removed_script_names, vec!["lua_hard".to_string()]);
        assert_eq!(
            summary.transpiled_script_names,
            vec!["enemy_patrol".to_string()]
        );
        assert!(project.save.scripts.contains_key("enemy_patrol"));
        assert!(project.save.scripts.contains_key("rhai_ok"));
        assert!(!project.save.scripts.contains_key("lua_hard"));
        assert!(project
            .save
            .global_scripts
            .contains(&"enemy_patrol".to_string()));
        assert_eq!(
            project.save.entities[0].script,
            Some("enemy_patrol".to_string())
        );
    }

    #[test]
    fn strip_web_incompatible_scripts_removes_broken_entity_bindings() {
        let mut project = ProjectExportData {
            version: 1,
            save: SaveGameData {
                version: 1,
                config: GameConfig::default(),
                tilemap: Tilemap::test_level(),
                game_state: "Playing".to_string(),
                next_network_id: 2,
                entities: vec![SaveEntity {
                    network_id: Some(1),
                    x: 0.0,
                    y: 0.0,
                    vx: 0.0,
                    vy: 0.0,
                    is_player: false,
                    components: Vec::new(),
                    script: Some("lua_hard".to_string()),
                    script_state: Some(serde_json::json!({"phase":"idle"})),
                    tags: vec!["enemy".to_string()],
                    alive: true,
                    ai_state: None,
                    invincibility_frames: None,
                    path_follower_path: Vec::new(),
                    path_follower_frames_until_recalc: None,
                }],
                scripts: std::collections::HashMap::from([(
                    "lua_hard".to_string(),
                    "function update(entity, world, dt)\n  repeat\n    entity.vx = entity.vx + 1\n  until entity.vx > 5\nend".to_string(),
                )]),
                global_scripts: vec!["lua_hard".to_string()],
                game_vars: std::collections::HashMap::new(),
                animation_graphs: std::collections::HashMap::new(),
                sprite_sheets: std::collections::HashMap::new(),
                particle_presets: std::collections::HashMap::new(),
            },
            level_packs: Vec::new(),
        };

        let summary = strip_web_incompatible_scripts(&mut project);
        assert_eq!(summary.scripts_removed, 1);
        assert_eq!(summary.entity_bindings_removed, 1);
        assert!(project.save.scripts.is_empty());
        assert!(project.save.global_scripts.is_empty());
        assert!(project.save.entities[0].script.is_none());
        assert!(project.save.entities[0].script_state.is_none());
    }
}
