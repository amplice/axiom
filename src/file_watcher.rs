use bevy::prelude::*;
use crate::scripting::ScriptBackend;
use crossbeam_channel::Receiver;
use notify::{Event as NotifyEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;

pub struct FileWatcherPlugin;

pub enum FileWatchEvent {
    ScriptChanged { name: String, source: String },
    ConfigChanged(String),
    AssetChanged(String),
}

#[derive(Resource)]
pub struct FileWatcherReceiver(pub Receiver<FileWatchEvent>);

impl Plugin for FileWatcherPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = crossbeam_channel::unbounded::<FileWatchEvent>();
        app.insert_resource(FileWatcherReceiver(rx));

        std::thread::spawn(move || {
            run_watcher(tx);
        });

        app.add_systems(Update, process_file_watch_events);
    }
}

fn run_watcher(tx: crossbeam_channel::Sender<FileWatchEvent>) {
    let scripts_dir =
        std::env::var("AXIOM_SCRIPTS_DIR").unwrap_or_else(|_| "scripts".to_string());
    let config_path =
        std::env::var("AXIOM_WATCH_CONFIG").unwrap_or_else(|_| "game_config.json".to_string());
    let assets_dir = std::env::var("AXIOM_ASSETS_DIR").unwrap_or_else(|_| "assets".to_string());

    let scripts_path = PathBuf::from(&scripts_dir);
    let config_pb = PathBuf::from(&config_path);
    let assets_path = PathBuf::from(&assets_dir);

    let tx_clone = tx.clone();
    let scripts_path_clone = scripts_path.clone();
    let config_pb_clone = config_pb.clone();
    let assets_path_clone = assets_path.clone();

    let mut watcher: RecommendedWatcher =
        match notify::recommended_watcher(move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                handle_fs_event(
                    event,
                    &tx_clone,
                    &scripts_path_clone,
                    &config_pb_clone,
                    &assets_path_clone,
                );
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[Axiom FileWatcher] Failed to create watcher: {e}");
                return;
            }
        };

    // Watch scripts directory
    if scripts_path.exists() {
        if let Err(e) = watcher.watch(&scripts_path, RecursiveMode::Recursive) {
            eprintln!("[Axiom FileWatcher] Failed to watch scripts dir: {e}");
        } else {
            println!(
                "[Axiom FileWatcher] Watching scripts: {}",
                scripts_path.display()
            );
        }
    }

    // Watch config file's parent directory (notify needs a dir for single files)
    if let Some(parent) = config_pb.parent() {
        if parent.exists() {
            if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                eprintln!("[Axiom FileWatcher] Failed to watch config dir: {e}");
            } else {
                println!(
                    "[Axiom FileWatcher] Watching config: {}",
                    config_pb.display()
                );
            }
        }
    }

    // Watch assets directory
    if assets_path.exists() {
        if let Err(e) = watcher.watch(&assets_path, RecursiveMode::Recursive) {
            eprintln!("[Axiom FileWatcher] Failed to watch assets dir: {e}");
        } else {
            println!(
                "[Axiom FileWatcher] Watching assets: {}",
                assets_path.display()
            );
        }
    }

    // Keep thread alive — watcher is dropped when thread exits
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

fn handle_fs_event(
    event: NotifyEvent,
    tx: &crossbeam_channel::Sender<FileWatchEvent>,
    scripts_dir: &PathBuf,
    config_path: &PathBuf,
    assets_dir: &PathBuf,
) {
    if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
        return;
    }

    for path in &event.paths {
        // Config file changed
        if path_matches(path, config_path) {
            if let Ok(content) = std::fs::read_to_string(path) {
                let _ = tx.send(FileWatchEvent::ConfigChanged(content));
            }
            continue;
        }

        // Script changed
        if path_is_under(path, scripts_dir) {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "lua" || ext == "rhai" {
                    if let Ok(source) = std::fs::read_to_string(path) {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let _ = tx.send(FileWatchEvent::ScriptChanged { name, source });
                    }
                }
            }
            continue;
        }

        // Asset changed
        if path_is_under(path, assets_dir) {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "bmp") {
                    let rel = path
                        .strip_prefix(assets_dir)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string();
                    let _ = tx.send(FileWatchEvent::AssetChanged(rel));
                }
            }
        }
    }
}

fn path_matches(a: &std::path::Path, b: &PathBuf) -> bool {
    let ca = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

fn path_is_under(path: &std::path::Path, dir: &PathBuf) -> bool {
    let cp = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let cd = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    cp.starts_with(&cd)
}

fn process_file_watch_events(
    watcher: Option<Res<FileWatcherReceiver>>,
    mut script_engine: ResMut<crate::scripting::ScriptEngine>,
    mut config: ResMut<crate::components::GameConfig>,
    asset_server: Option<Res<AssetServer>>,
) {
    let Some(watcher) = watcher else { return };

    for event in watcher.0.try_iter().take(16) {
        match event {
            FileWatchEvent::ScriptChanged { name, source } => {
                println!("[Axiom FileWatcher] Reloading script: {name}");
                if let Err(e) = script_engine.load_script(name.clone(), source, false) {
                    eprintln!("[Axiom FileWatcher] Script reload failed for '{name}': {e}");
                }
            }
            FileWatchEvent::ConfigChanged(content) => {
                println!("[Axiom FileWatcher] Reloading config...");
                match serde_json::from_str::<crate::components::GameConfig>(&content) {
                    Ok(new_config) => {
                        *config = new_config;
                    }
                    Err(e) => {
                        eprintln!("[Axiom FileWatcher] Config parse error: {e}");
                    }
                }
            }
            FileWatchEvent::AssetChanged(path) => {
                println!("[Axiom FileWatcher] Asset changed: {path}");
                // Trigger Bevy asset reload if asset server is available
                if let Some(ref _server) = asset_server {
                    // Bevy's asset server handles hot-reload for watched files automatically.
                    // This event is informational — log it for the AI to know an asset changed.
                }
            }
        }
    }
}
