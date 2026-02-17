use bevy::prelude::*;
use bevy::utils::Instant;
use rhai::{
    Array, Dynamic, Engine, EvalAltResult, FnPtr, ImmutableString, Map, NativeCallContext, Scope,
    AST, FLOAT, INT,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::ai;
use crate::components::{
    Alive, AnimationController, Collider, GameConfig, GamePosition, Grounded, Health, Hitbox,
    NetworkId, NextNetworkId, PathFollower, Player, Tags, TileType, Velocity,
};
use crate::events::GameEventBus;
use crate::input::VirtualInput;
use crate::perf::PerfAccum;
use crate::raycast::{raycast_aabbs, RaycastAabb};
use crate::scripting::lua_compat::transpile_lua_compat_to_rhai;
use crate::scripting::{ScriptError, ScriptEvent};
use crate::tilemap::Tilemap;

const MAX_SCRIPT_ERRORS: usize = 100;
const MAX_SCRIPT_EVENTS: usize = 200;
const MAX_ENTITY_SCRIPT_ERROR_STREAK: u32 = 8;
const MAX_GLOBAL_SCRIPT_ERROR_STREAK: u32 = 8;

const ENTITY_ENTRY_FN: &str = "__axiom_entity_entry";
const GLOBAL_ENTRY_FN: &str = "__axiom_global_entry";

#[derive(Serialize, Deserialize, Clone, Default, Component)]
pub struct LuaScript {
    pub script_name: String,
    #[serde(default)]
    pub state: serde_json::Value,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub error_streak: u32,
    #[serde(default)]
    pub disabled_reason: Option<String>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Resource, Default)]
pub struct ScriptErrors {
    pub entries: Vec<ScriptError>,
}

impl ScriptErrors {
    pub fn push(&mut self, entry: ScriptError) {
        self.entries.push(entry);
        if self.entries.len() > MAX_SCRIPT_ERRORS {
            let excess = self.entries.len() - MAX_SCRIPT_ERRORS;
            self.entries.drain(0..excess);
        }
    }
}

#[derive(Resource, Default)]
pub struct ScriptFrame {
    pub frame: u64,
    pub seconds: f64,
}

#[derive(Resource, Default)]
pub struct ScriptEngine {
    pub scripts: HashMap<String, String>,
    pub global_scripts: HashSet<String>,
    pub vars: HashMap<String, serde_json::Value>,
    pub events: Vec<ScriptEvent>,
    pub pending_events: Vec<ScriptEvent>,
    pub dropped_events: u64,
    pub last_overflow_log_frame: u64,
    pub global_error_streaks: HashMap<String, u32>,
    pub disabled_global_scripts: HashSet<String>,
    pub entity_event_cursors: HashMap<(String, u64), u64>,
    pub global_event_cursors: HashMap<String, u64>,
}

#[derive(Default)]
struct RhaiScriptCache {
    compiled: HashMap<String, (u64, AST)>,
}

struct RhaiRuntime {
    engine: Engine,
    cache: RhaiScriptCache,
}

impl Default for RhaiRuntime {
    fn default() -> Self {
        Self {
            engine: make_rhai_engine(),
            cache: RhaiScriptCache::default(),
        }
    }
}

impl RhaiRuntime {
    fn compile_ast(
        &mut self,
        script_name: &str,
        source: &str,
        is_entity: bool,
    ) -> Result<AST, String> {
        get_or_compile_ast(
            &self.engine,
            &mut self.cache,
            script_name,
            source,
            is_entity,
        )
    }
}

#[derive(Clone)]
struct WorldEntitySnapshot {
    id: u64,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    grounded: bool,
    alive: bool,
    health: Option<f32>,
    max_health: Option<f32>,
    is_player: bool,
    tags: Vec<String>,
    aabb: Option<(Vec2, Vec2)>,
}

#[derive(Resource, Default)]
struct ScriptEntitySnapshotCache {
    entities: Arc<Vec<WorldEntitySnapshot>>,
    network_lookup: HashMap<u64, Entity>,
}

struct ScriptCallContext {
    tilemap: Arc<Tilemap>,
    physics: GameConfig,
    entities: Arc<Vec<WorldEntitySnapshot>>,
    tile_edits: Vec<(i32, i32, u8)>,
    tile_overrides: HashMap<(i32, i32), u8>,
}

impl Default for ScriptCallContext {
    fn default() -> Self {
        Self {
            tilemap: Arc::new(Tilemap::test_level()),
            physics: GameConfig::default(),
            entities: Arc::new(Vec::new()),
            tile_edits: Vec::new(),
            tile_overrides: HashMap::new(),
        }
    }
}

#[derive(Resource)]
struct ScriptTilemapCache {
    tilemap: Arc<Tilemap>,
}

impl Default for ScriptTilemapCache {
    fn default() -> Self {
        Self {
            tilemap: Arc::new(Tilemap::test_level()),
        }
    }
}

thread_local! {
    static SCRIPT_CALL_CONTEXT: RefCell<ScriptCallContext> = RefCell::new(ScriptCallContext::default());
}

#[derive(Clone)]
enum ScriptWorldCommand {
    Spawn {
        x: f32,
        y: f32,
        components: Vec<SpawnComponentSpec>,
        script: Option<String>,
        tags: Vec<String>,
        is_player: bool,
        health: Option<f32>,
        max_health: Option<f32>,
        script_name: String,
        source_entity: Option<u64>,
    },
    Despawn {
        target_id: u64,
        script_name: String,
        source_entity: Option<u64>,
    },
    SpawnProjectile {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        speed: f32,
        damage: f32,
        lifetime_frames: u32,
        owner_id: u64,
        damage_tag: String,
        script_name: String,
        source_entity: Option<u64>,
    },
    SpawnParticles {
        preset: String,
        x: f32,
        y: f32,
        script_name: String,
        source_entity: Option<u64>,
    },
}

#[derive(Clone)]
enum SpawnComponentSpec {
    Named(String),
    Collider {
        width: f32,
        height: f32,
    },
    GravityBody,
    HorizontalMover {
        speed: f32,
        left_action: String,
        right_action: String,
    },
    Jumper {
        velocity: f32,
        action: String,
        fall_multiplier: f32,
        variable_height: bool,
        coyote_frames: u32,
        buffer_frames: u32,
    },
    TopDownMover {
        speed: f32,
        up_action: String,
        down_action: String,
        left_action: String,
        right_action: String,
    },
    Health {
        current: f32,
        max: f32,
    },
}

impl ScriptEngine {
    pub fn load_script(
        &mut self,
        name: String,
        source: String,
        global: bool,
    ) -> Result<(), String> {
        let normalized = normalize_script_source(&source, global)?;
        if normalized.transpiled_from_lua {
            warn!("[Axiom scripts] Transpiled Lua-compatible script '{name}' to Rhai for wasm runtime");
        }
        self.scripts.insert(name.clone(), normalized.source);
        if global {
            self.global_scripts.insert(name.clone());
        } else {
            self.global_scripts.remove(&name);
        }
        self.global_error_streaks.remove(&name);
        self.disabled_global_scripts.remove(&name);
        self.global_event_cursors.remove(&name);
        self.entity_event_cursors
            .retain(|(script_name, _), _| script_name != &name);
        Ok(())
    }

    pub fn remove_script(&mut self, name: &str) {
        self.scripts.remove(name);
        self.global_scripts.remove(name);
        self.global_error_streaks.remove(name);
        self.disabled_global_scripts.remove(name);
        self.global_event_cursors.remove(name);
        self.entity_event_cursors
            .retain(|(script_name, _), _| script_name != name);
    }

    pub fn list_scripts(&self) -> Vec<crate::scripting::api::ScriptInfo> {
        let mut out: Vec<_> = self
            .scripts
            .keys()
            .map(|name| crate::scripting::api::ScriptInfo {
                name: name.clone(),
                global: self.global_scripts.contains(name),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn push_event(&mut self, event: ScriptEvent) {
        self.events.push(event.clone());
        self.pending_events.push(event);
        if self.events.len() > MAX_SCRIPT_EVENTS {
            let excess = self.events.len() - MAX_SCRIPT_EVENTS;
            self.events.drain(0..excess);
            self.dropped_events = self.dropped_events.saturating_add(excess as u64);
            let frame = self.events.last().map(|e| e.frame).unwrap_or(0);
            if frame.saturating_sub(self.last_overflow_log_frame) >= 60 {
                self.last_overflow_log_frame = frame;
                warn!(
                    "[Axiom scripts] Dropped {} script events (total dropped: {})",
                    excess, self.dropped_events
                );
            }
        }
    }
}

pub struct ScriptingPlugin;

impl Plugin for ScriptingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ScriptEngine::default())
            .insert_resource(ScriptErrors::default())
            .insert_resource(ScriptFrame::default())
            .init_resource::<ScriptTilemapCache>()
            .init_resource::<ScriptEntitySnapshotCache>()
            .init_non_send_resource::<RhaiRuntime>()
            .add_systems(
                FixedUpdate,
                (
                    tick_script_frame,
                    refresh_script_tilemap_cache,
                    refresh_script_entity_cache,
                    run_entity_scripts,
                    refresh_script_tilemap_cache,
                    refresh_script_entity_cache,
                    run_global_scripts,
                    flush_script_events_to_game_events,
                )
                    .chain()
                    .run_if(crate::game_runtime::gameplay_systems_enabled),
            );
    }
}

impl crate::scripting::ScriptBackend for ScriptEngine {
    fn load_script(&mut self, name: String, source: String, global: bool) -> Result<(), String> {
        ScriptEngine::load_script(self, name, source, global)
    }

    fn remove_script(&mut self, name: &str) {
        ScriptEngine::remove_script(self, name);
    }

    fn list_scripts(&self) -> Vec<crate::scripting::api::ScriptInfo> {
        ScriptEngine::list_scripts(self)
    }

    fn set_vars(&mut self, vars: HashMap<String, serde_json::Value>) {
        self.vars = vars;
    }

    fn vars(&self) -> &HashMap<String, serde_json::Value> {
        &self.vars
    }

    fn snapshot(&self) -> crate::scripting::ScriptRuntimeSnapshot {
        crate::scripting::ScriptRuntimeSnapshot {
            scripts: self.scripts.clone(),
            global_scripts: self.global_scripts.clone(),
            vars: self.vars.clone(),
        }
    }

    fn restore_snapshot(&mut self, snapshot: crate::scripting::ScriptRuntimeSnapshot) {
        let crate::scripting::ScriptRuntimeSnapshot {
            scripts,
            global_scripts,
            vars,
        } = snapshot;

        let global_names = global_scripts.clone();
        let mut valid_scripts = HashMap::new();
        let mut dropped = 0usize;
        for (name, source) in scripts {
            let is_global = global_names.contains(&name);
            match normalize_script_source(&source, is_global) {
                Ok(normalized) => {
                    if normalized.transpiled_from_lua {
                        warn!(
                            "[Axiom scripts] Transpiled restored Lua-compatible wasm script '{name}'"
                        );
                    }
                    valid_scripts.insert(name, normalized.source);
                }
                Err(err) => {
                    dropped = dropped.saturating_add(1);
                    warn!("[Axiom scripts] Dropping invalid restored wasm script '{name}': {err}");
                }
            }
        }
        if dropped > 0 {
            warn!("[Axiom scripts] Dropped {dropped} invalid wasm script(s) during restore");
        }

        self.scripts = valid_scripts;
        self.global_scripts = global_scripts
            .into_iter()
            .filter(|name| self.scripts.contains_key(name))
            .collect();
        self.vars = vars;
        self.global_error_streaks.clear();
        self.disabled_global_scripts.clear();
        self.entity_event_cursors.clear();
        self.global_event_cursors.clear();
        self.dropped_events = 0;
        self.last_overflow_log_frame = 0;
    }

    fn get_script_source(&self, name: &str) -> Option<crate::scripting::api::ScriptSource> {
        self.scripts
            .get(name)
            .map(|source| crate::scripting::api::ScriptSource {
                name: name.to_string(),
                source: source.clone(),
                global: self.global_scripts.contains(name),
            })
    }

    fn events(&self) -> &[ScriptEvent] {
        &self.events
    }
}

fn validate_script_source(source: &str, global: bool) -> Result<(), String> {
    compile_script_source(source, global)
}

struct NormalizedScriptSource {
    source: String,
    transpiled_from_lua: bool,
}

fn normalize_script_source(source: &str, global: bool) -> Result<NormalizedScriptSource, String> {
    match compile_script_source(source, global) {
        Ok(_) => {
            return Ok(NormalizedScriptSource {
                source: source.to_string(),
                transpiled_from_lua: false,
            });
        }
        Err(primary_err) => {
            if let Some(transpiled) = transpile_lua_compat_to_rhai(source) {
                match compile_script_source(&transpiled, global) {
                    Ok(_) => {
                        return Ok(NormalizedScriptSource {
                            source: transpiled,
                            transpiled_from_lua: true,
                        });
                    }
                    Err(transpile_err) => {
                        return Err(format!(
                            "{primary_err}; Lua-compat transpile failed: {transpile_err}"
                        ));
                    }
                }
            }
            Err(primary_err)
        }
    }
}

fn compile_script_source(source: &str, global: bool) -> Result<(), String> {
    let wrapped = wrap_source(source, !global);
    make_rhai_engine()
        .compile(wrapped)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn tick_script_frame(time: Res<Time<Fixed>>, mut frame: ResMut<ScriptFrame>) {
    frame.frame = frame.frame.saturating_add(1);
    frame.seconds += time.delta_secs_f64();
}

fn refresh_script_tilemap_cache(tilemap: Res<Tilemap>, mut cache: ResMut<ScriptTilemapCache>) {
    if tilemap.is_added() || tilemap.is_changed() {
        cache.tilemap = Arc::new(tilemap.clone());
    }
}

fn refresh_script_entity_cache(
    query: Query<(
        Entity,
        &NetworkId,
        &GamePosition,
        Option<&Collider>,
        Option<&Velocity>,
        Option<&Grounded>,
        Option<&Alive>,
        Option<&Health>,
        Option<&Tags>,
        Option<&Player>,
    )>,
    mut cache: ResMut<ScriptEntitySnapshotCache>,
) {
    let mut network_lookup = HashMap::<u64, Entity>::new();
    let entities = collect_world_entity_snapshots(&query, Some(&mut network_lookup));
    cache.entities = Arc::new(entities);
    cache.network_lookup = network_lookup;
}

fn run_entity_scripts(
    mut commands: Commands,
    mut engine: ResMut<ScriptEngine>,
    mut runtime: NonSendMut<RhaiRuntime>,
    mut errors: ResMut<ScriptErrors>,
    mut perf: ResMut<PerfAccum>,
    frame: Res<ScriptFrame>,
    time: Res<Time<Fixed>>,
    config: Res<GameConfig>,
    mut next_network_id: ResMut<NextNetworkId>,
    vinput: Res<VirtualInput>,
    event_bus: Res<GameEventBus>,
    runtime_state: Res<crate::game_runtime::RuntimeState>,
    mut tilemap: ResMut<Tilemap>,
    mut tilemap_cache: ResMut<ScriptTilemapCache>,
    entity_cache: Res<ScriptEntitySnapshotCache>,
    mut query: Query<(
        &NetworkId,
        &mut LuaScript,
        &mut GamePosition,
        Option<&mut Velocity>,
        Option<&Grounded>,
        Option<&mut Alive>,
        Option<&mut Health>,
        Option<&mut Tags>,
        Option<&mut Hitbox>,
        Option<&mut AnimationController>,
        Option<&mut PathFollower>,
    )>,
) {
    let start = Instant::now();
    let dt = time.delta_secs_f64() as FLOAT;
    let bus_events = &event_bus.recent;
    let mut used_entity_cursor_keys = HashSet::<(String, u64)>::new();
    let mut network_lookup = entity_cache.network_lookup.clone();
    let entity_snapshots = entity_cache.entities.clone();
    let mut pending_world_commands = Vec::<ScriptWorldCommand>::new();
    script_context_set_from_frame(&tilemap_cache.tilemap, &config, entity_snapshots.clone());

    for (
        network_id,
        mut script,
        mut pos,
        vel,
        grounded,
        alive,
        health,
        mut tags,
        mut hitbox,
        mut animation,
        mut path_follower,
    ) in
        query.iter_mut()
    {
        if !script.enabled {
            continue;
        }
        script_context_begin_call();
        let script_name = script.script_name.clone();
        let cursor_key = (script_name.clone(), network_id.0);
        used_entity_cursor_keys.insert(cursor_key.clone());
        let Some(source) = engine.scripts.get(&script.script_name).cloned() else {
            continue;
        };
        let last_event_frame = engine
            .entity_event_cursors
            .get(&cursor_key)
            .copied()
            .unwrap_or(0);
        let world_events = bus_events
            .iter()
            .filter(|ev| ev.frame > last_event_frame)
            .cloned()
            .collect::<Vec<_>>();
        let ast = match runtime.compile_ast(&script_name, &source, true) {
            Ok(ast) => ast,
            Err(err_msg) => {
                on_entity_script_error(
                    &mut script,
                    &mut errors,
                    &script_name,
                    network_id.0,
                    frame.frame,
                    err_msg,
                );
                continue;
            }
        };

        let mut entity = Map::new();
        entity.insert("id".into(), Dynamic::from_int(network_id.0 as INT));
        entity.insert("x".into(), Dynamic::from_float(pos.x as FLOAT));
        entity.insert("y".into(), Dynamic::from_float(pos.y as FLOAT));
        entity.insert(
            "vx".into(),
            Dynamic::from_float(vel.as_ref().map_or(0.0, |v| v.x) as FLOAT),
        );
        entity.insert(
            "vy".into(),
            Dynamic::from_float(vel.as_ref().map_or(0.0, |v| v.y) as FLOAT),
        );
        entity.insert(
            "grounded".into(),
            Dynamic::from_bool(grounded.is_some_and(|g| g.0)),
        );
        entity.insert(
            "alive".into(),
            Dynamic::from_bool(alive.as_ref().is_none_or(|a| a.0)),
        );
        if let Some(h) = health.as_ref() {
            entity.insert("health".into(), Dynamic::from_float(h.current as FLOAT));
            entity.insert("max_health".into(), Dynamic::from_float(h.max as FLOAT));
        }
        let mut tags_map = Map::new();
        if let Some(entity_tags) = tags.as_ref() {
            for tag in entity_tags.0.iter() {
                tags_map.insert(tag.as_str().into(), true.into());
            }
        }
        entity.insert("tags".into(), tags_map.into());
        if let Some(hb) = hitbox.as_ref() {
            let mut hitbox_map = Map::new();
            hitbox_map.insert("width".into(), (hb.width as FLOAT).into());
            hitbox_map.insert("height".into(), (hb.height as FLOAT).into());
            hitbox_map.insert("offset_x".into(), (hb.offset.x as FLOAT).into());
            hitbox_map.insert("offset_y".into(), (hb.offset.y as FLOAT).into());
            hitbox_map.insert("active".into(), hb.active.into());
            hitbox_map.insert("damage".into(), (hb.damage as FLOAT).into());
            hitbox_map.insert("damage_tag".into(), hb.damage_tag.clone().into());
            entity.insert("hitbox".into(), hitbox_map.into());
        }
        if let Some(anim) = animation.as_ref() {
            entity.insert("animation".into(), anim.state.clone().into());
            entity.insert("animation_frame".into(), (anim.frame as INT).into());
            entity.insert("flip_x".into(), (!anim.facing_right).into());
        }
        entity.insert("state".into(), json_to_dynamic(&script.state));

        let mut world = build_world_map(
            &engine.vars,
            frame.frame,
            frame.seconds,
            dt,
            &vinput,
            &world_events,
            &runtime_state.state,
        );
        if let Err(err_msg) = execute_script(
            &runtime.engine,
            &ast,
            ENTITY_ENTRY_FN,
            Some(&mut entity),
            &mut world,
            dt,
        ) {
            script_context_discard_tile_edits();
            on_entity_script_error(
                &mut script,
                &mut errors,
                &script_name,
                network_id.0,
                frame.frame,
                err_msg,
            );
            continue;
        }

        script.error_streak = 0;
        script.disabled_reason = None;

        pos.x = map_get_f32(&entity, "x", pos.x);
        pos.y = map_get_f32(&entity, "y", pos.y);
        if let Some(mut v) = vel {
            v.x = map_get_f32(&entity, "vx", v.x);
            v.y = map_get_f32(&entity, "vy", v.y);
        }
        if let Some(mut a) = alive {
            a.0 = map_get_bool(&entity, "alive", a.0);
        }
        if let Some(mut h) = health {
            h.current = map_get_f32(&entity, "health", h.current);
            h.max = map_get_f32(&entity, "max_health", h.max);
        }
        if let Some(tags_dyn) = entity.get("tags").and_then(|d| d.clone().try_cast::<Map>()) {
            if let Some(tags_comp) = tags.as_deref_mut() {
                let mut new_tags = HashSet::new();
                for (k, v) in tags_dyn {
                    let keep = v.clone().try_cast::<bool>().unwrap_or(true);
                    if keep {
                        new_tags.insert(k.to_string());
                    }
                }
                tags_comp.0 = new_tags;
            }
        }
        if let Some(state_dyn) = entity.get("state") {
            script.state = dynamic_to_json(state_dyn);
        }
        if let Some(hb_map) = entity.get("hitbox").and_then(|d| d.clone().try_cast::<Map>()) {
            if let Some(hb) = hitbox.as_deref_mut() {
                hb.width = map_get_f32(&hb_map, "width", hb.width).max(0.1);
                hb.height = map_get_f32(&hb_map, "height", hb.height).max(0.1);
                hb.offset.x = map_get_f32(&hb_map, "offset_x", hb.offset.x);
                hb.offset.y = map_get_f32(&hb_map, "offset_y", hb.offset.y);
                hb.active = map_get_bool(&hb_map, "active", hb.active);
                hb.damage = map_get_f32(&hb_map, "damage", hb.damage).max(0.0);
                if let Some(tag) = map_get_string(&hb_map, "damage_tag") {
                    hb.damage_tag = tag;
                }
            }
        }
        if let Some(anim) = animation.as_deref_mut() {
            if let Some(state_name) = map_get_string(&entity, "animation") {
                if !state_name.trim().is_empty() {
                    anim.state = state_name;
                }
            }
            if let Some(frame) = entity
                .get("animation_frame")
                .and_then(|d| d.clone().try_cast::<INT>())
            {
                anim.frame = frame.max(0) as usize;
            }
            if let Some(flip_x) = entity
                .get("flip_x")
                .and_then(|d| d.clone().try_cast::<bool>())
            {
                anim.facing_right = !flip_x;
            }
        }
        if let Some(path) = entity
            .get("__axiom_follow_path")
            .and_then(|d| d.clone().try_cast::<Array>())
        {
            if let Some(follower) = path_follower.as_deref_mut() {
                let mut points = Vec::new();
                for item in path {
                    let Some(point) = item.try_cast::<Map>() else {
                        continue;
                    };
                    let px = map_get_f32(&point, "x", 0.0);
                    let py = map_get_f32(&point, "y", 0.0);
                    points.push(Vec2::new(px, py));
                }
                if !points.is_empty() {
                    follower.path = points;
                }
                if let Some(speed) = entity
                    .get("__axiom_follow_speed")
                    .and_then(|d| d.clone().try_cast::<FLOAT>())
                {
                    follower.speed = (speed as f32).max(0.0);
                }
            }
        }

        if let Some(vars_dyn) = world.get("vars").and_then(|d| d.clone().try_cast::<Map>()) {
            engine.vars = map_to_json_object(&vars_dyn);
        }
        for event in drain_emit_queue(&world, frame.frame, Some(network_id.0)) {
            engine.push_event(event);
        }
        pending_world_commands.extend(drain_command_queue(
            &world,
            &script_name,
            Some(network_id.0),
        ));
        let tilemap_changed =
            apply_script_tile_edits(&mut tilemap, script_context_take_tile_edits());
        if tilemap_changed {
            tilemap_cache.tilemap = Arc::new(tilemap.clone());
            script_context_set_from_frame(
                &tilemap_cache.tilemap,
                &config,
                entity_snapshots.clone(),
            );
        }
        engine.entity_event_cursors.insert(cursor_key, frame.frame);
    }

    engine
        .entity_event_cursors
        .retain(|key, _| used_entity_cursor_keys.contains(key));

    apply_script_world_commands(
        &mut commands,
        &config,
        &mut next_network_id,
        &mut network_lookup,
        &mut errors,
        frame.frame,
        pending_world_commands,
    );

    perf.script_time_ms += start.elapsed().as_secs_f32() * 1000.0;
}

fn run_global_scripts(
    mut commands: Commands,
    mut engine: ResMut<ScriptEngine>,
    mut runtime: NonSendMut<RhaiRuntime>,
    mut errors: ResMut<ScriptErrors>,
    mut perf: ResMut<PerfAccum>,
    frame: Res<ScriptFrame>,
    time: Res<Time<Fixed>>,
    config: Res<GameConfig>,
    mut next_network_id: ResMut<NextNetworkId>,
    vinput: Res<VirtualInput>,
    event_bus: Res<GameEventBus>,
    runtime_state: Res<crate::game_runtime::RuntimeState>,
    mut tilemap: ResMut<Tilemap>,
    mut tilemap_cache: ResMut<ScriptTilemapCache>,
    entity_cache: Res<ScriptEntitySnapshotCache>,
) {
    let start = Instant::now();
    let dt = time.delta_secs_f64() as FLOAT;
    let names = engine.global_scripts.iter().cloned().collect::<Vec<_>>();
    let bus_events = &event_bus.recent;
    let mut used_global_cursor_keys = HashSet::<String>::new();
    let mut network_lookup = entity_cache.network_lookup.clone();
    let entity_snapshots = entity_cache.entities.clone();
    let mut pending_world_commands = Vec::<ScriptWorldCommand>::new();
    script_context_set_from_frame(&tilemap_cache.tilemap, &config, entity_snapshots.clone());

    for name in names {
        script_context_begin_call();
        used_global_cursor_keys.insert(name.clone());
        if engine.disabled_global_scripts.contains(&name) {
            continue;
        }

        let Some(source) = engine.scripts.get(&name).cloned() else {
            continue;
        };
        let last_event_frame = engine.global_event_cursors.get(&name).copied().unwrap_or(0);
        let world_events = bus_events
            .iter()
            .filter(|ev| ev.frame > last_event_frame)
            .cloned()
            .collect::<Vec<_>>();
        let ast = match runtime.compile_ast(&name, &source, false) {
            Ok(ast) => ast,
            Err(err_msg) => {
                on_global_script_error(&mut engine, &mut errors, &name, frame.frame, err_msg);
                continue;
            }
        };

        let mut world = build_world_map(
            &engine.vars,
            frame.frame,
            frame.seconds,
            dt,
            &vinput,
            &world_events,
            &runtime_state.state,
        );
        if let Err(err_msg) =
            execute_script(&runtime.engine, &ast, GLOBAL_ENTRY_FN, None, &mut world, dt)
        {
            script_context_discard_tile_edits();
            on_global_script_error(&mut engine, &mut errors, &name, frame.frame, err_msg);
            continue;
        }

        engine.global_error_streaks.remove(&name);
        if let Some(vars_dyn) = world.get("vars").and_then(|d| d.clone().try_cast::<Map>()) {
            engine.vars = map_to_json_object(&vars_dyn);
        }
        for event in drain_emit_queue(&world, frame.frame, None) {
            engine.push_event(event);
        }
        pending_world_commands.extend(drain_command_queue(&world, &name, None));
        let tilemap_changed =
            apply_script_tile_edits(&mut tilemap, script_context_take_tile_edits());
        if tilemap_changed {
            tilemap_cache.tilemap = Arc::new(tilemap.clone());
            script_context_set_from_frame(
                &tilemap_cache.tilemap,
                &config,
                entity_snapshots.clone(),
            );
        }
        engine
            .global_event_cursors
            .insert(name.clone(), frame.frame);
    }

    engine
        .global_event_cursors
        .retain(|name, _| used_global_cursor_keys.contains(name));

    apply_script_world_commands(
        &mut commands,
        &config,
        &mut next_network_id,
        &mut network_lookup,
        &mut errors,
        frame.frame,
        pending_world_commands,
    );

    perf.script_time_ms += start.elapsed().as_secs_f32() * 1000.0;
}

fn flush_script_events_to_game_events(
    mut engine: ResMut<ScriptEngine>,
    mut bus: ResMut<GameEventBus>,
) {
    let pending = std::mem::take(&mut engine.pending_events);
    for ev in pending {
        bus.emit(ev.name, ev.data, ev.source_entity);
    }
}

fn on_entity_script_error(
    script: &mut LuaScript,
    errors: &mut ScriptErrors,
    script_name: &str,
    entity_id: u64,
    frame: u64,
    error_message: String,
) {
    script.error_streak = script.error_streak.saturating_add(1);
    errors.push(ScriptError {
        script_name: script_name.to_string(),
        entity_id: Some(entity_id),
        error_message,
        frame,
    });
    if script.error_streak >= MAX_ENTITY_SCRIPT_ERROR_STREAK {
        script.enabled = false;
        script.disabled_reason = Some(format!(
            "Disabled after {} consecutive errors",
            script.error_streak
        ));
    }
}

fn on_global_script_error(
    engine: &mut ScriptEngine,
    errors: &mut ScriptErrors,
    script_name: &str,
    frame: u64,
    error_message: String,
) {
    let streak = engine
        .global_error_streaks
        .entry(script_name.to_string())
        .or_insert(0);
    *streak = streak.saturating_add(1);
    errors.push(ScriptError {
        script_name: script_name.to_string(),
        entity_id: None,
        error_message,
        frame,
    });
    if *streak >= MAX_GLOBAL_SCRIPT_ERROR_STREAK {
        engine
            .disabled_global_scripts
            .insert(script_name.to_string());
    }
}

fn normalize_component_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn apply_named_component(
    entity: &mut EntityCommands,
    name: &str,
    config: &GameConfig,
    x: f32,
    y: f32,
) {
    match normalize_component_name(name).as_str() {
        "collider" => {
            entity.insert(crate::components::Collider {
                width: 12.0,
                height: 14.0,
            });
        }
        "gravitybody" => {
            entity.insert(crate::components::GravityBody);
        }
        "horizontalmover" => {
            entity.insert(crate::components::HorizontalMover {
                speed: config.move_speed,
                left_action: "left".to_string(),
                right_action: "right".to_string(),
            });
        }
        "jumper" => {
            entity.insert((
                crate::components::Jumper {
                    velocity: config.jump_velocity,
                    action: "jump".to_string(),
                    fall_multiplier: config.fall_multiplier,
                    variable_height: true,
                    coyote_frames: config.coyote_frames,
                    buffer_frames: config.jump_buffer_frames,
                },
                crate::components::CoyoteTimer::default(),
                crate::components::JumpBuffer::default(),
            ));
        }
        "topdownmover" => {
            entity.insert(crate::components::TopDownMover {
                speed: config.move_speed,
                up_action: "up".to_string(),
                down_action: "down".to_string(),
                left_action: "left".to_string(),
                right_action: "right".to_string(),
            });
        }
        "health" => {
            entity.insert(crate::components::Health {
                current: 1.0,
                max: 1.0,
            });
        }
        "pathfollower" => {
            entity.insert(crate::components::PathFollower::new(
                Vec2::new(x, y),
                crate::components::PathType::TopDown,
                20,
                config.move_speed,
            ));
        }
        "aibehavior" | "ai" => {
            entity.insert(crate::components::AiBehavior {
                behavior: crate::components::BehaviorType::Wander {
                    speed: config.move_speed,
                    radius: 48.0,
                    pause_frames: 30,
                },
                state: crate::components::AiState::Idle,
            });
        }
        _ => {}
    }
}

fn apply_spawn_component_spec(
    entity: &mut EntityCommands,
    component: SpawnComponentSpec,
    config: &GameConfig,
    x: f32,
    y: f32,
) {
    match component {
        SpawnComponentSpec::Named(name) => apply_named_component(entity, &name, config, x, y),
        SpawnComponentSpec::Collider { width, height } => {
            entity.insert(crate::components::Collider { width, height });
        }
        SpawnComponentSpec::GravityBody => {
            entity.insert(crate::components::GravityBody);
        }
        SpawnComponentSpec::HorizontalMover {
            speed,
            left_action,
            right_action,
        } => {
            entity.insert(crate::components::HorizontalMover {
                speed,
                left_action,
                right_action,
            });
        }
        SpawnComponentSpec::Jumper {
            velocity,
            action,
            fall_multiplier,
            variable_height,
            coyote_frames,
            buffer_frames,
        } => {
            entity.insert((
                crate::components::Jumper {
                    velocity,
                    action,
                    fall_multiplier,
                    variable_height,
                    coyote_frames,
                    buffer_frames,
                },
                crate::components::CoyoteTimer::default(),
                crate::components::JumpBuffer::default(),
            ));
        }
        SpawnComponentSpec::TopDownMover {
            speed,
            up_action,
            down_action,
            left_action,
            right_action,
        } => {
            entity.insert(crate::components::TopDownMover {
                speed,
                up_action,
                down_action,
                left_action,
                right_action,
            });
        }
        SpawnComponentSpec::Health { current, max } => {
            entity.insert(crate::components::Health { current, max });
        }
    }
}

fn apply_script_world_commands(
    commands: &mut Commands,
    config: &GameConfig,
    next_network_id: &mut NextNetworkId,
    network_lookup: &mut HashMap<u64, Entity>,
    errors: &mut ScriptErrors,
    frame: u64,
    world_commands: impl IntoIterator<Item = ScriptWorldCommand>,
) {
    for cmd in world_commands {
        match cmd {
            ScriptWorldCommand::Spawn {
                x,
                y,
                components,
                script,
                tags,
                is_player,
                health,
                max_health,
                script_name: _script_name,
                source_entity: _source_entity,
            } => {
                let assigned_id = next_network_id.0.max(1);
                next_network_id.0 = assigned_id.saturating_add(1);
                let mut tag_set: HashSet<String> = tags
                    .into_iter()
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if is_player {
                    tag_set.insert("player".to_string());
                }
                let mut entity = commands.spawn((
                    NetworkId(assigned_id),
                    GamePosition { x, y },
                    Velocity::default(),
                    Grounded(false),
                    Alive(true),
                    Tags(tag_set),
                    Transform::from_xyz(x, y, 10.0),
                ));
                if is_player {
                    entity.insert((
                        Player,
                        Sprite::from_color(Color::srgb(0.2, 0.4, 0.9), Vec2::new(12.0, 14.0)),
                    ));
                }
                for component in components {
                    apply_spawn_component_spec(&mut entity, component, config, x, y);
                }
                if let Some(current) = health {
                    entity.insert(Health {
                        current,
                        max: max_health.unwrap_or(current).max(0.0),
                    });
                }
                if let Some(script_name) = script {
                    let trimmed = script_name.trim();
                    if !trimmed.is_empty() {
                        entity.insert(LuaScript {
                            script_name: trimmed.to_string(),
                            state: serde_json::json!({}),
                            enabled: true,
                            error_streak: 0,
                            disabled_reason: None,
                        });
                    }
                }
                network_lookup.insert(assigned_id, entity.id());
            }
            ScriptWorldCommand::Despawn {
                target_id,
                script_name,
                source_entity,
            } => {
                if let Some(entity) = network_lookup.remove(&target_id) {
                    commands.entity(entity).despawn();
                } else {
                    errors.push(ScriptError {
                        script_name,
                        entity_id: source_entity,
                        error_message: format!(
                            "world.despawn() failed: entity id {target_id} not found"
                        ),
                        frame,
                    });
                }
            }
            ScriptWorldCommand::SpawnProjectile {
                x,
                y,
                dx,
                dy,
                speed,
                damage,
                lifetime_frames,
                owner_id,
                damage_tag,
                script_name: _script_name,
                source_entity: _source_entity,
            } => {
                let assigned_id = next_network_id.0.max(1);
                next_network_id.0 = assigned_id.saturating_add(1);
                let mut tags = HashSet::new();
                tags.insert("projectile".to_string());
                let direction = Vec2::new(dx, dy).normalize_or_zero();
                let direction = if direction.length_squared() == 0.0 {
                    Vec2::X
                } else {
                    direction
                };
                let mut entity = commands.spawn((
                    NetworkId(assigned_id),
                    GamePosition { x, y },
                    Velocity {
                        x: direction.x * speed,
                        y: direction.y * speed,
                    },
                    Grounded(false),
                    Alive(true),
                    Tags(tags),
                    Transform::from_xyz(x, y, 10.0),
                    crate::components::Collider {
                        width: 6.0,
                        height: 6.0,
                    },
                    crate::components::Projectile {
                        speed,
                        direction,
                        lifetime_frames: lifetime_frames.max(1),
                        damage,
                        owner_id,
                        damage_tag,
                    },
                ));
                entity.insert(Sprite::from_color(
                    Color::srgb(0.95, 0.8, 0.2),
                    Vec2::new(6.0, 6.0),
                ));
                network_lookup.insert(assigned_id, entity.id());
            }
            ScriptWorldCommand::SpawnParticles {
                preset,
                x,
                y,
                script_name,
                source_entity,
            } => {
                let preset_name = preset.trim().to_string();
                if preset_name.is_empty() {
                    errors.push(ScriptError {
                        script_name,
                        entity_id: source_entity,
                        error_message: "world.spawn_particles() requires a preset name".to_string(),
                        frame,
                    });
                    continue;
                }
                commands.queue(move |world: &mut World| {
                    let mut emitter = if let Some(library) =
                        world.get_resource::<crate::particles::ParticlePresetLibrary>()
                    {
                        if let Some(def) = library.presets.get(&preset_name) {
                            crate::particles::ParticleEmitter::from_preset(
                                def,
                                Some(preset_name.clone()),
                            )
                        } else {
                            crate::particles::ParticleEmitter::preset_only(preset_name.clone())
                        }
                    } else {
                        crate::particles::ParticleEmitter::preset_only(preset_name.clone())
                    };
                    emitter.one_shot = true;
                    emitter.enabled = true;
                    emitter.timer = 0.0;
                    emitter.fired_once = false;
                    emitter.burst_count = emitter.burst_count.max(1);
                    world.spawn((
                        GamePosition { x, y },
                        emitter,
                        crate::particles::TransientParticleEmitter,
                    ));
                });
            }
        }
    }
}

fn collect_world_entity_snapshots(
    query: &Query<(
        Entity,
        &NetworkId,
        &GamePosition,
        Option<&Collider>,
        Option<&Velocity>,
        Option<&Grounded>,
        Option<&Alive>,
        Option<&Health>,
        Option<&Tags>,
        Option<&Player>,
    )>,
    mut network_lookup: Option<&mut HashMap<u64, Entity>>,
) -> Vec<WorldEntitySnapshot> {
    query
        .iter()
        .map(
            |(entity, id, pos, collider, vel, grounded, alive, health, tags, player)| {
                if let Some(lookup) = network_lookup.as_deref_mut() {
                    lookup.insert(id.0, entity);
                }
                let tags = tags
                    .map(|t| t.0.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                let is_player = player.is_some() || tags.iter().any(|t| t == "player");
                let aabb = collider.map(|c| {
                    let hw = c.width * 0.5;
                    let hh = c.height * 0.5;
                    (
                        Vec2::new(pos.x - hw, pos.y - hh),
                        Vec2::new(pos.x + hw, pos.y + hh),
                    )
                });
                WorldEntitySnapshot {
                    id: id.0,
                    x: pos.x,
                    y: pos.y,
                    vx: vel.map_or(0.0, |v| v.x),
                    vy: vel.map_or(0.0, |v| v.y),
                    grounded: grounded.is_some_and(|g| g.0),
                    alive: alive.is_none_or(|a| a.0),
                    health: health.map(|h| h.current),
                    max_health: health.map(|h| h.max),
                    is_player,
                    tags,
                    aabb,
                }
            },
        )
        .collect()
}

fn script_context_set_from_frame(
    tilemap: &Arc<Tilemap>,
    config: &GameConfig,
    entities: Arc<Vec<WorldEntitySnapshot>>,
) {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.tilemap = tilemap.clone();
        ctx.physics = config.clone();
        ctx.entities = entities;
        ctx.tile_edits.clear();
        ctx.tile_overrides.clear();
    });
}

fn script_context_begin_call() {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.tile_edits.clear();
        ctx.tile_overrides.clear();
    });
}

fn script_context_discard_tile_edits() {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.tile_edits.clear();
        ctx.tile_overrides.clear();
    });
}

fn script_context_take_tile_edits() -> Vec<(i32, i32, u8)> {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.tile_overrides.clear();
        std::mem::take(&mut ctx.tile_edits)
    })
}

fn apply_script_tile_edits(tilemap: &mut Tilemap, edits: Vec<(i32, i32, u8)>) -> bool {
    if edits.is_empty() {
        return false;
    }
    for (x, y, tile) in edits {
        tilemap.set_tile(x, y, tile);
    }
    true
}

fn with_script_tilemap_and_physics<R>(f: impl FnOnce(&Tilemap, &GameConfig) -> R) -> R {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        if ctx.tile_overrides.is_empty() {
            f(ctx.tilemap.as_ref(), &ctx.physics)
        } else {
            let mut tilemap = (*ctx.tilemap).clone();
            for ((x, y), tile) in &ctx.tile_overrides {
                tilemap.set_tile(*x, *y, *tile);
            }
            f(&tilemap, &ctx.physics)
        }
    })
}

fn build_world_map(
    vars: &HashMap<String, serde_json::Value>,
    frame: u64,
    seconds: f64,
    dt: FLOAT,
    vinput: &VirtualInput,
    world_events: &[crate::events::GameEvent],
    runtime_state: &str,
) -> Map {
    let mut world = Map::new();
    let mut input_active = Map::new();
    for action in &vinput.active {
        input_active.insert(action.as_str().into(), true.into());
    }
    let mut input_just_pressed = Map::new();
    for action in &vinput.just_pressed {
        input_just_pressed.insert(action.as_str().into(), true.into());
    }
    world.insert("frame".into(), Dynamic::from_int(frame as INT));
    world.insert("time".into(), Dynamic::from_float(seconds as FLOAT));
    world.insert("dt".into(), Dynamic::from_float(dt));
    // Backward-compat legacy flat field.
    world.insert("game_state".into(), runtime_state.into());
    // Lua-compatible nested game API shape.
    let mut game = Map::new();
    game.insert("state".into(), runtime_state.into());
    game.insert("emit_queue".into(), Array::new().into());
    game.insert("command_queue".into(), Array::new().into());
    world.insert("game".into(), game.into());

    let mut camera = Map::new();
    camera.insert("emit_queue".into(), Array::new().into());
    world.insert("camera".into(), camera.into());

    let mut ui = Map::new();
    ui.insert("emit_queue".into(), Array::new().into());
    world.insert("ui".into(), ui.into());

    let mut dialogue = Map::new();
    dialogue.insert("emit_queue".into(), Array::new().into());
    world.insert("dialogue".into(), dialogue.into());

    world.insert("vars".into(), json_object_to_map(vars).into());
    world.insert("input_active".into(), input_active.into());
    world.insert("input_just_pressed".into(), input_just_pressed.into());
    let mut events = Array::new();
    for ev in world_events {
        let mut event_map = Map::new();
        event_map.insert("name".into(), ev.name.clone().into());
        event_map.insert("data".into(), json_to_dynamic(&ev.data));
        event_map.insert("frame".into(), Dynamic::from_int(ev.frame as INT));
        if let Some(source_entity) = ev.source_entity {
            event_map.insert(
                "source_entity".into(),
                Dynamic::from_int(source_entity as INT),
            );
        }
        events.push(event_map.into());
    }
    world.insert("events".into(), events.into());
    world.insert("emit_queue".into(), Array::new().into());
    world.insert("command_queue".into(), Array::new().into());
    world
}

fn execute_script(
    engine: &Engine,
    ast: &AST,
    entry_fn: &str,
    mut entity: Option<&mut Map>,
    world: &mut Map,
    dt: FLOAT,
) -> Result<(), String> {
    let mut scope = Scope::new();
    if let Some(entity_map) = entity.as_ref() {
        scope.push("entity", (*entity_map).clone());
    }
    scope.push("world", world.clone());
    scope.push("dt", dt);
    engine
        .call_fn::<()>(&mut scope, ast, entry_fn, ())
        .map_err(|e| e.to_string())?;

    if let Some(entity_map) = entity.as_mut() {
        if let Some(updated_entity) = scope.get_value::<Map>("entity") {
            **entity_map = updated_entity;
        }
    }
    if let Some(mut updated_world) = scope.get_value::<Map>("world") {
        merge_nested_module_queues(&mut updated_world);
        *world = updated_world;
    }
    Ok(())
}

fn merge_nested_module_queues(world: &mut Map) {
    for module in ["game", "camera", "ui", "dialogue"] {
        let mut nested = world
            .remove(module)
            .and_then(|d| d.try_cast::<Map>())
            .unwrap_or_default();
        if let Some(queue) = nested
            .remove("emit_queue")
            .and_then(|d| d.try_cast::<Array>())
        {
            let mut root = world
                .remove("emit_queue")
                .and_then(|d| d.try_cast::<Array>())
                .unwrap_or_default();
            root.extend(queue);
            world.insert("emit_queue".into(), root.into());
        }
        if let Some(queue) = nested
            .remove("command_queue")
            .and_then(|d| d.try_cast::<Array>())
        {
            let mut root = world
                .remove("command_queue")
                .and_then(|d| d.try_cast::<Array>())
                .unwrap_or_default();
            root.extend(queue);
            world.insert("command_queue".into(), root.into());
        }
        world.insert(module.into(), nested.into());
    }
}

fn script_cache_key(name: &str, is_entity: bool) -> String {
    if is_entity {
        format!("{name}#entity")
    } else {
        format!("{name}#global")
    }
}

fn script_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn wrap_source(source: &str, is_entity: bool) -> String {
    if is_entity {
        format!("{source}\nfn {ENTITY_ENTRY_FN}() {{\n    update(entity, world, dt);\n}}\n")
    } else {
        format!("{source}\nfn {GLOBAL_ENTRY_FN}() {{\n    update(world, dt);\n}}\n")
    }
}

fn get_or_compile_ast(
    engine: &Engine,
    cache: &mut RhaiScriptCache,
    script_name: &str,
    source: &str,
    is_entity: bool,
) -> Result<AST, String> {
    let key = script_cache_key(script_name, is_entity);
    let hash = script_hash(source);
    if let Some((old_hash, ast)) = cache.compiled.get(&key) {
        if *old_hash == hash {
            return Ok(ast.clone());
        }
    }
    let wrapped = wrap_source(source, is_entity);
    let compiled = engine.compile(wrapped).map_err(|e| e.to_string())?;
    cache.compiled.insert(key, (hash, compiled.clone()));
    Ok(compiled)
}

fn make_rhai_engine() -> Engine {
    let mut engine = Engine::new();
    let max_ops = std::env::var("AXIOM_RHAI_MAX_OPERATIONS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(crate::scripting::DEFAULT_RHAI_MAX_OPERATIONS)
        .max(10_000);
    let max_call_levels = std::env::var("AXIOM_RHAI_MAX_CALL_LEVELS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(crate::scripting::DEFAULT_RHAI_MAX_CALL_LEVELS)
        .max(8);
    engine.set_max_operations(max_ops);
    engine.set_max_call_levels(max_call_levels);
    engine.register_fn("get_tile", world_get_tile);
    engine.register_fn("is_solid", world_is_solid);
    engine.register_fn("is_platform", world_is_platform);
    engine.register_fn("is_climbable", world_is_climbable);
    engine.register_fn("tile_friction", world_tile_friction);
    engine.register_fn("set_tile", world_set_tile);
    engine.register_fn("get_var", world_get_var);
    engine.register_fn("set_var", world_set_var);
    engine.register_fn("emit", world_emit);
    engine.register_fn("on", world_on_payloads);
    engine.register_fn("on", world_on_with_handler);
    engine.register_fn("pressed", world_pressed);
    engine.register_fn("just_pressed", world_just_pressed);
    engine.register_fn("player", world_player);
    engine.register_fn("get_entity", world_get_entity);
    engine.register_fn("find_all", world_find_all);
    engine.register_fn("find_all", world_find_all_unfiltered);
    engine.register_fn("find_entities", world_find_all);
    engine.register_fn("find_entities", world_find_all_unfiltered);
    engine.register_fn("find_in_radius", world_find_in_radius);
    engine.register_fn("find_in_radius", world_find_in_radius_unfiltered);
    engine.register_fn("find_nearest", world_find_nearest);
    engine.register_fn("find_nearest", world_find_nearest_unfiltered);
    engine.register_fn("find_path", world_find_path);
    engine.register_fn("find_path", world_find_path_with_type);
    engine.register_fn("line_of_sight", world_line_of_sight);
    engine.register_fn("raycast", world_raycast);
    engine.register_fn("raycast_entities", world_raycast_entities);
    engine.register_fn("raycast_entities", world_raycast_entities_unfiltered);
    engine.register_fn("pause", world_pause);
    engine.register_fn("resume", world_resume);
    engine.register_fn("transition", world_transition);
    engine.register_fn("transition", world_transition_with_effect);
    engine.register_fn("transition", world_transition_full);
    engine.register_fn("play_sfx", world_play_sfx);
    engine.register_fn("play_music", world_play_music);
    engine.register_fn("stop_music", world_stop_music);
    engine.register_fn("set_volume", world_set_volume);
    engine.register_fn("camera_shake", world_camera_shake);
    engine.register_fn("camera_zoom", world_camera_zoom);
    engine.register_fn("camera_look_at", world_camera_look_at);
    engine.register_fn("show_screen", world_ui_show_screen);
    engine.register_fn("hide_screen", world_ui_hide_screen);
    engine.register_fn("set_text", world_ui_set_text);
    engine.register_fn("set_progress", world_ui_set_progress);
    engine.register_fn("start", world_dialogue_start);
    engine.register_fn("choose", world_dialogue_choose);
    engine.register_fn("damage", entity_damage);
    engine.register_fn("heal", entity_heal);
    engine.register_fn("knockback", entity_knockback);
    engine.register_fn("follow_path", entity_follow_path);
    engine.register_fn("has_tag", entity_has_tag);
    engine.register_fn("add_tag", entity_add_tag);
    engine.register_fn("remove_tag", entity_remove_tag);
    engine.register_fn("spawn", world_spawn);
    engine.register_fn("despawn", world_despawn);
    engine.register_fn("spawn_projectile", world_spawn_projectile);
    engine.register_fn("spawn_particles", world_spawn_particles);
    engine
}

fn world_get_tile(_world: &mut Map, x: INT, y: INT) -> INT {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        if x < 0 || y < 0 || x as usize >= ctx.tilemap.width() || y as usize >= ctx.tilemap.height()
        {
            return 0;
        }
        let key = (x as i32, y as i32);
        if let Some(tile) = ctx.tile_overrides.get(&key) {
            return *tile as INT;
        }
        ctx.tilemap.get_tile(x as i32, y as i32) as INT
    })
}

fn world_is_solid(world: &mut Map, x: INT, y: INT) -> bool {
    TileType::from_u8(world_get_tile(world, x, y) as u8).is_solid()
}

fn world_is_platform(world: &mut Map, x: INT, y: INT) -> bool {
    TileType::from_u8(world_get_tile(world, x, y) as u8).is_platform()
}

fn world_is_climbable(_world: &mut Map, x: INT, y: INT) -> bool {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        let tile_id = if x < 0
            || y < 0
            || x as usize >= ctx.tilemap.width()
            || y as usize >= ctx.tilemap.height()
        {
            0
        } else {
            let key = (x as i32, y as i32);
            ctx.tile_overrides
                .get(&key)
                .copied()
                .unwrap_or_else(|| ctx.tilemap.get_tile(x as i32, y as i32))
        };
        ctx.physics.tile_types.is_climbable(tile_id)
    })
}

fn world_tile_friction(_world: &mut Map, x: INT, y: INT) -> FLOAT {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        let tile_id = if x < 0
            || y < 0
            || x as usize >= ctx.tilemap.width()
            || y as usize >= ctx.tilemap.height()
        {
            0
        } else {
            let key = (x as i32, y as i32);
            ctx.tile_overrides
                .get(&key)
                .copied()
                .unwrap_or_else(|| ctx.tilemap.get_tile(x as i32, y as i32))
        };
        ctx.physics.tile_types.friction(tile_id) as FLOAT
    })
}

fn world_set_tile(_world: &mut Map, x: INT, y: INT, tile: INT) {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        if x < 0 || y < 0 || x as usize >= ctx.tilemap.width() || y as usize >= ctx.tilemap.height()
        {
            return;
        }
        let tile = tile.clamp(0, 255) as u8;
        let key = (x as i32, y as i32);
        let current = ctx
            .tile_overrides
            .get(&key)
            .copied()
            .unwrap_or_else(|| ctx.tilemap.get_tile(key.0, key.1));
        if current != tile {
            ctx.tile_overrides.insert(key, tile);
            ctx.tile_edits.push((key.0, key.1, tile));
        }
    });
}

fn world_player(_world: &mut Map) -> Dynamic {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        ctx.entities
            .iter()
            .find(|e| e.is_player)
            .map(snapshot_to_dynamic)
            .unwrap_or(Dynamic::UNIT)
    })
}

fn world_get_entity(_world: &mut Map, id: INT) -> Dynamic {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        ctx.entities
            .iter()
            .find(|e| e.id == id as u64)
            .map(snapshot_to_dynamic)
            .unwrap_or(Dynamic::UNIT)
    })
}

fn world_find_all(_world: &mut Map, tag: ImmutableString) -> Array {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        ctx.entities
            .iter()
            .filter(|e| snapshot_matches_tag(e, Some(tag.as_str())))
            .map(snapshot_to_dynamic)
            .collect()
    })
}

fn world_find_all_unfiltered(_world: &mut Map) -> Array {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        ctx.entities.iter().map(snapshot_to_dynamic).collect()
    })
}

fn world_find_in_radius(
    _world: &mut Map,
    x: FLOAT,
    y: FLOAT,
    radius: FLOAT,
    tag: ImmutableString,
) -> Array {
    world_find_in_radius_impl(x, y, radius, Some(tag.as_str()))
}

fn world_find_in_radius_unfiltered(_world: &mut Map, x: FLOAT, y: FLOAT, radius: FLOAT) -> Array {
    world_find_in_radius_impl(x, y, radius, None)
}

fn world_find_nearest(_world: &mut Map, x: FLOAT, y: FLOAT, tag: ImmutableString) -> Dynamic {
    world_find_nearest_impl(x, y, Some(tag.as_str()))
}

fn world_find_nearest_unfiltered(_world: &mut Map, x: FLOAT, y: FLOAT) -> Dynamic {
    world_find_nearest_impl(x, y, None)
}

fn world_find_path(_world: &mut Map, sx: FLOAT, sy: FLOAT, tx: FLOAT, ty: FLOAT) -> Array {
    world_find_path_impl(sx, sy, tx, ty, None)
}

fn world_find_path_with_type(
    _world: &mut Map,
    sx: FLOAT,
    sy: FLOAT,
    tx: FLOAT,
    ty: FLOAT,
    path_type: ImmutableString,
) -> Array {
    world_find_path_impl(sx, sy, tx, ty, Some(path_type.as_str()))
}

fn world_line_of_sight(_world: &mut Map, x1: FLOAT, y1: FLOAT, x2: FLOAT, y2: FLOAT) -> bool {
    with_script_tilemap_and_physics(|tilemap, physics| {
        ai::has_line_of_sight_points(
            tilemap,
            physics.tile_size,
            Vec2::new(x1 as f32, y1 as f32),
            Vec2::new(x2 as f32, y2 as f32),
        )
    })
}

fn world_raycast(
    _world: &mut Map,
    ox: FLOAT,
    oy: FLOAT,
    dx: FLOAT,
    dy: FLOAT,
    max_dist: FLOAT,
) -> Dynamic {
    with_script_tilemap_and_physics(|tilemap, physics| {
        let origin = Vec2::new(ox as f32, oy as f32);
        let dir = Vec2::new(dx as f32, dy as f32);
        let len = dir.length();
        if len <= 0.0001 {
            return Dynamic::UNIT;
        }
        let dir_n = dir / len;
        let mut d = 0.0f32;
        let max_d = max_dist.max(0.0) as f32;
        let mut prev_tx = (origin.x / physics.tile_size).floor() as i32;
        let mut prev_ty = (origin.y / physics.tile_size).floor() as i32;
        while d <= max_d {
            let x = origin.x + dir_n.x * d;
            let y = origin.y + dir_n.y * d;
            let tx = (x / physics.tile_size).floor() as i32;
            let ty = (y / physics.tile_size).floor() as i32;
            if tilemap.is_solid(tx, ty) {
                let mut hit = Map::new();
                hit.insert("x".into(), (x as FLOAT).into());
                hit.insert("y".into(), (y as FLOAT).into());
                hit.insert("tile_x".into(), (tx as INT).into());
                hit.insert("tile_y".into(), (ty as INT).into());
                hit.insert("distance".into(), (d as FLOAT).into());
                hit.insert("normal_x".into(), ((prev_tx - tx) as FLOAT).into());
                hit.insert("normal_y".into(), ((prev_ty - ty) as FLOAT).into());
                return hit.into();
            }
            prev_tx = tx;
            prev_ty = ty;
            d += 0.5;
        }
        Dynamic::UNIT
    })
}

fn world_raycast_entities(
    _world: &mut Map,
    ox: FLOAT,
    oy: FLOAT,
    dx: FLOAT,
    dy: FLOAT,
    max_dist: FLOAT,
    tag: ImmutableString,
) -> Array {
    let origin = Vec2::new(ox as f32, oy as f32);
    let direction = Vec2::new(dx as f32, dy as f32);
    let max_distance = max_dist.max(0.0) as f32;
    let tag = tag.trim();
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        let targets = ctx.entities.iter().filter_map(|entity| {
            if !tag.is_empty() && !entity.tags.iter().any(|t| t == tag) {
                return None;
            }
            let (min, max) = entity.aabb?;
            Some(RaycastAabb {
                id: entity.id,
                min,
                max,
            })
        });
        let hits = raycast_aabbs(origin, direction, max_distance, targets);
        hits.into_iter()
            .map(|hit| {
                let mut map = Map::new();
                map.insert("id".into(), (hit.id as INT).into());
                map.insert("x".into(), (hit.x as FLOAT).into());
                map.insert("y".into(), (hit.y as FLOAT).into());
                map.insert("distance".into(), (hit.distance as FLOAT).into());
                map.into()
            })
            .collect()
    })
}

fn world_raycast_entities_unfiltered(
    world: &mut Map,
    ox: FLOAT,
    oy: FLOAT,
    dx: FLOAT,
    dy: FLOAT,
    max_dist: FLOAT,
) -> Array {
    world_raycast_entities(world, ox, oy, dx, dy, max_dist, "".into())
}

fn world_find_in_radius_impl(x: FLOAT, y: FLOAT, radius: FLOAT, tag: Option<&str>) -> Array {
    let radius_sq = (radius.max(0.0) as f32).powi(2);
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        ctx.entities
            .iter()
            .filter(|e| snapshot_matches_tag(e, tag))
            .filter(|e| {
                let dx = e.x - x as f32;
                let dy = e.y - y as f32;
                dx * dx + dy * dy <= radius_sq
            })
            .map(snapshot_to_dynamic)
            .collect()
    })
}

fn world_find_nearest_impl(x: FLOAT, y: FLOAT, tag: Option<&str>) -> Dynamic {
    SCRIPT_CALL_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        ctx.entities
            .iter()
            .filter(|e| snapshot_matches_tag(e, tag))
            .min_by(|a, b| {
                let da = (a.x - x as f32).powi(2) + (a.y - y as f32).powi(2);
                let db = (b.x - x as f32).powi(2) + (b.y - y as f32).powi(2);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(snapshot_to_dynamic)
            .unwrap_or(Dynamic::UNIT)
    })
}

fn world_find_path_impl(
    sx: FLOAT,
    sy: FLOAT,
    tx: FLOAT,
    ty: FLOAT,
    path_type: Option<&str>,
) -> Array {
    with_script_tilemap_and_physics(|tilemap, physics| {
        let start = Vec2::new(sx as f32, sy as f32);
        let goal = Vec2::new(tx as f32, ty as f32);
        let path = match path_type
            .map(str::trim)
            .map(|v| v.to_ascii_lowercase())
            .as_deref()
        {
            Some("platformer") => {
                crate::pathfinding::find_platformer_path_points(tilemap, physics, start, goal)
                    .unwrap_or_default()
            }
            _ => ai::find_top_down_path_points(tilemap, physics.tile_size, start, goal)
                .unwrap_or_default(),
        };
        let mut out = Array::new();
        for p in path {
            let mut point = Map::new();
            point.insert("x".into(), (p.x as FLOAT).into());
            point.insert("y".into(), (p.y as FLOAT).into());
            out.push(point.into());
        }
        out
    })
}

fn snapshot_matches_tag(snapshot: &WorldEntitySnapshot, tag: Option<&str>) -> bool {
    let Some(tag) = tag.map(str::trim).filter(|t| !t.is_empty()) else {
        return true;
    };
    snapshot.tags.iter().any(|t| t == tag)
}

fn snapshot_to_dynamic(snapshot: &WorldEntitySnapshot) -> Dynamic {
    let mut map = Map::new();
    map.insert("id".into(), Dynamic::from_int(snapshot.id as INT));
    map.insert("x".into(), Dynamic::from_float(snapshot.x as FLOAT));
    map.insert("y".into(), Dynamic::from_float(snapshot.y as FLOAT));
    map.insert("vx".into(), Dynamic::from_float(snapshot.vx as FLOAT));
    map.insert("vy".into(), Dynamic::from_float(snapshot.vy as FLOAT));
    map.insert("grounded".into(), snapshot.grounded.into());
    map.insert("alive".into(), snapshot.alive.into());
    if let Some(health) = snapshot.health {
        map.insert("health".into(), Dynamic::from_float(health as FLOAT));
    }
    if let Some(max_health) = snapshot.max_health {
        map.insert(
            "max_health".into(),
            Dynamic::from_float(max_health as FLOAT),
        );
    }
    map.insert("is_player".into(), snapshot.is_player.into());
    let mut tags_map = Map::new();
    let mut tag_list = Array::new();
    for tag in snapshot.tags.iter() {
        tags_map.insert(tag.as_str().into(), true.into());
        tag_list.push(tag.as_str().into());
    }
    map.insert("tags".into(), tags_map.into());
    map.insert("tag_list".into(), tag_list.into());
    map.into()
}

fn world_get_var(world: &mut Map, name: ImmutableString) -> Dynamic {
    world
        .get("vars")
        .and_then(|d| d.clone().try_cast::<Map>())
        .and_then(|vars| vars.get(name.as_str()).cloned())
        .unwrap_or(Dynamic::UNIT)
}

fn world_set_var(world: &mut Map, name: ImmutableString, value: Dynamic) {
    let mut vars = world
        .remove("vars")
        .and_then(|d| d.try_cast::<Map>())
        .unwrap_or_default();
    vars.insert(name.into(), value);
    world.insert("vars".into(), vars.into());
}

fn world_emit(world: &mut Map, name: ImmutableString, data: Dynamic) {
    push_emit_event(world, name.as_str(), data);
}

fn world_on_payloads(world: &mut Map, name: ImmutableString) -> Array {
    let mut payloads = Array::new();
    let Some(events) = world
        .get("events")
        .and_then(|d| d.clone().try_cast::<Array>())
    else {
        return payloads;
    };
    let target = name.as_str();
    for ev in events {
        let Some(map) = ev.try_cast::<Map>() else {
            continue;
        };
        let Some(ev_name) = map
            .get("name")
            .and_then(|d| d.clone().try_cast::<ImmutableString>())
        else {
            continue;
        };
        if ev_name.as_str() == target {
            payloads.push(map.get("data").cloned().unwrap_or(Dynamic::UNIT));
        }
    }
    payloads
}

fn world_on_with_handler(
    context: NativeCallContext,
    world: &mut Map,
    name: ImmutableString,
    handler: FnPtr,
) -> Result<INT, Box<EvalAltResult>> {
    let mut called = 0i64;
    let Some(events) = world
        .get("events")
        .and_then(|d| d.clone().try_cast::<Array>())
    else {
        return Ok(0);
    };
    let target = name.as_str();
    for ev in events {
        let Some(map) = ev.try_cast::<Map>() else {
            continue;
        };
        let Some(ev_name) = map
            .get("name")
            .and_then(|d| d.clone().try_cast::<ImmutableString>())
        else {
            continue;
        };
        if ev_name.as_str() != target {
            continue;
        }
        let payload = map.get("data").cloned().unwrap_or(Dynamic::UNIT);
        handler.call_within_context(&context, (payload,))?;
        called += 1;
    }
    Ok(called as INT)
}

fn world_pressed(world: &mut Map, action: ImmutableString) -> bool {
    world
        .get("input_active")
        .and_then(|d| d.clone().try_cast::<Map>())
        .and_then(|active| active.get(action.as_str()).cloned())
        .and_then(|d| d.try_cast::<bool>())
        .unwrap_or(false)
}

fn world_just_pressed(world: &mut Map, action: ImmutableString) -> bool {
    world
        .get("input_just_pressed")
        .and_then(|d| d.clone().try_cast::<Map>())
        .and_then(|active| active.get(action.as_str()).cloned())
        .and_then(|d| d.try_cast::<bool>())
        .unwrap_or(false)
}

fn entity_damage(entity: &mut Map, amount: FLOAT) -> FLOAT {
    let current = map_get_f32(entity, "health", 0.0) as FLOAT;
    let next = (current - amount.max(0.0)).max(0.0);
    entity.insert("health".into(), next.into());
    if next <= 0.0 {
        entity.insert("alive".into(), false.into());
    }
    next
}

fn entity_heal(entity: &mut Map, amount: FLOAT) -> FLOAT {
    let current = map_get_f32(entity, "health", 0.0) as FLOAT;
    let max_health = map_get_f32(entity, "max_health", (current + amount.max(0.0)) as f32) as FLOAT;
    let next = (current + amount.max(0.0)).min(max_health.max(0.0));
    entity.insert("health".into(), next.into());
    next
}

fn entity_knockback(entity: &mut Map, dx: FLOAT, dy: FLOAT) {
    let vx = map_get_f32(entity, "vx", 0.0) as FLOAT + dx;
    let vy = map_get_f32(entity, "vy", 0.0) as FLOAT + dy;
    entity.insert("vx".into(), vx.into());
    entity.insert("vy".into(), vy.into());
}

fn entity_follow_path(entity: &mut Map, path: Array, speed: FLOAT) {
    entity.insert("__axiom_follow_path".into(), path.into());
    entity.insert(
        "__axiom_follow_speed".into(),
        Dynamic::from_float(speed.max(0.0)),
    );
}

fn entity_has_tag(entity: &mut Map, tag: ImmutableString) -> bool {
    entity
        .get("tags")
        .and_then(|d| d.clone().try_cast::<Map>())
        .and_then(|tags| tags.get(tag.as_str()).cloned())
        .and_then(|d| d.try_cast::<bool>())
        .unwrap_or(false)
}

fn entity_add_tag(entity: &mut Map, tag: ImmutableString) {
    let mut tags = entity
        .remove("tags")
        .and_then(|d| d.try_cast::<Map>())
        .unwrap_or_default();
    tags.insert(tag.into(), true.into());
    entity.insert("tags".into(), tags.into());
}

fn entity_remove_tag(entity: &mut Map, tag: ImmutableString) {
    let mut tags = entity
        .remove("tags")
        .and_then(|d| d.try_cast::<Map>())
        .unwrap_or_default();
    let _ = tags.remove(tag.as_str());
    entity.insert("tags".into(), tags.into());
}

fn world_pause(world: &mut Map) {
    push_emit_event(world, "game_pause", Map::new().into());
}

fn world_resume(world: &mut Map) {
    push_emit_event(world, "game_resume", Map::new().into());
}

fn world_transition(world: &mut Map, to: ImmutableString) {
    let mut data = Map::new();
    data.insert("to".into(), to.into());
    push_emit_event(world, "game_transition", data.into());
}

fn world_transition_with_effect(world: &mut Map, to: ImmutableString, effect: ImmutableString) {
    let mut data = Map::new();
    data.insert("to".into(), to.into());
    data.insert("effect".into(), effect.into());
    push_emit_event(world, "game_transition", data.into());
}

fn world_transition_full(
    world: &mut Map,
    to: ImmutableString,
    effect: ImmutableString,
    duration: FLOAT,
) {
    let mut data = Map::new();
    data.insert("to".into(), to.into());
    data.insert("effect".into(), effect.into());
    data.insert("duration".into(), duration.into());
    push_emit_event(world, "game_transition", data.into());
}

fn world_play_sfx(world: &mut Map, name: ImmutableString) {
    let mut data = Map::new();
    data.insert("name".into(), name.into());
    push_emit_event(world, "audio_play_sfx", data.into());
}

fn world_play_music(world: &mut Map, name: ImmutableString) {
    let mut data = Map::new();
    data.insert("name".into(), name.into());
    push_emit_event(world, "audio_play_music", data.into());
}

fn world_stop_music(world: &mut Map) {
    push_emit_event(world, "audio_stop_music", Map::new().into());
}

fn world_set_volume(world: &mut Map, channel: ImmutableString, value: FLOAT) {
    let mut data = Map::new();
    data.insert("channel".into(), channel.into());
    data.insert("value".into(), value.into());
    push_emit_event(world, "audio_set_volume", data.into());
}

fn world_camera_shake(world: &mut Map, intensity: FLOAT, duration: FLOAT) {
    let mut data = Map::new();
    data.insert("intensity".into(), intensity.into());
    data.insert("duration".into(), duration.into());
    push_emit_event(world, "camera_shake", data.into());
}

fn world_camera_zoom(world: &mut Map, zoom: FLOAT) {
    let mut data = Map::new();
    data.insert("zoom".into(), zoom.into());
    push_emit_event(world, "camera_zoom", data.into());
}

fn world_camera_look_at(world: &mut Map, x: FLOAT, y: FLOAT) {
    let mut data = Map::new();
    data.insert("x".into(), x.into());
    data.insert("y".into(), y.into());
    push_emit_event(world, "camera_look_at", data.into());
}

fn world_ui_show_screen(world: &mut Map, name: ImmutableString) {
    let mut data = Map::new();
    data.insert("name".into(), name.into());
    push_emit_event(world, "ui_show_screen", data.into());
}

fn world_ui_hide_screen(world: &mut Map, name: ImmutableString) {
    let mut data = Map::new();
    data.insert("name".into(), name.into());
    push_emit_event(world, "ui_hide_screen", data.into());
}

fn world_ui_set_text(world: &mut Map, id: ImmutableString, text: ImmutableString) {
    let mut data = Map::new();
    data.insert("id".into(), id.into());
    data.insert("text".into(), text.into());
    push_emit_event(world, "ui_set_text", data.into());
}

fn world_ui_set_progress(world: &mut Map, id: ImmutableString, value: FLOAT, max: FLOAT) {
    let mut data = Map::new();
    data.insert("id".into(), id.into());
    data.insert("value".into(), value.into());
    data.insert("max".into(), max.into());
    push_emit_event(world, "ui_set_progress", data.into());
}

fn world_dialogue_start(world: &mut Map, conversation: ImmutableString) {
    let mut data = Map::new();
    data.insert("conversation".into(), conversation.into());
    push_emit_event(world, "dialogue_start", data.into());
}

fn world_dialogue_choose(world: &mut Map, choice: INT) {
    let mut data = Map::new();
    data.insert("choice".into(), choice.into());
    push_emit_event(world, "dialogue_choose", data.into());
}

fn world_spawn(world: &mut Map, spec: Map) {
    let mut command = Map::new();
    command.insert("type".into(), "spawn".into());
    command.insert("spec".into(), spec.into());
    push_world_command(world, command);
}

fn world_despawn(world: &mut Map, id: INT) {
    let mut command = Map::new();
    command.insert("type".into(), "despawn".into());
    command.insert("id".into(), id.into());
    push_world_command(world, command);
}

fn world_spawn_projectile(world: &mut Map, spec: Map) {
    let mut command = Map::new();
    command.insert("type".into(), "spawn_projectile".into());
    command.insert("spec".into(), spec.into());
    push_world_command(world, command);
}

fn world_spawn_particles(world: &mut Map, preset: ImmutableString, x: FLOAT, y: FLOAT) {
    let mut command = Map::new();
    command.insert("type".into(), "spawn_particles".into());
    command.insert("preset".into(), preset.into());
    command.insert("x".into(), x.into());
    command.insert("y".into(), y.into());
    push_world_command(world, command);
}

fn push_emit_event(world: &mut Map, name: &str, data: Dynamic) {
    let mut queue = world
        .remove("emit_queue")
        .and_then(|d| d.try_cast::<Array>())
        .unwrap_or_default();
    let mut event = Map::new();
    event.insert("name".into(), name.into());
    event.insert("data".into(), data);
    queue.push(event.into());
    world.insert("emit_queue".into(), queue.into());
}

fn push_world_command(world: &mut Map, command: Map) {
    let mut queue = world
        .remove("command_queue")
        .and_then(|d| d.try_cast::<Array>())
        .unwrap_or_default();
    queue.push(command.into());
    world.insert("command_queue".into(), queue.into());
}

fn parse_spawn_component_spec(component: &Map) -> Option<SpawnComponentSpec> {
    let type_name = map_get_string(component, "type")?;
    match normalize_component_name(&type_name).as_str() {
        "collider" => Some(SpawnComponentSpec::Collider {
            width: map_get_f32(component, "width", 12.0).max(0.0),
            height: map_get_f32(component, "height", 14.0).max(0.0),
        }),
        "gravitybody" => Some(SpawnComponentSpec::GravityBody),
        "horizontalmover" => Some(SpawnComponentSpec::HorizontalMover {
            speed: map_get_f32(component, "speed", 200.0).max(0.0),
            left_action: map_get_string(component, "left_action")
                .unwrap_or_else(|| "left".to_string()),
            right_action: map_get_string(component, "right_action")
                .unwrap_or_else(|| "right".to_string()),
        }),
        "jumper" => Some(SpawnComponentSpec::Jumper {
            velocity: map_get_f32(component, "velocity", 400.0),
            action: map_get_string(component, "action").unwrap_or_else(|| "jump".to_string()),
            fall_multiplier: map_get_f32(component, "fall_multiplier", 1.5).max(0.0),
            variable_height: map_get_bool(component, "variable_height", true),
            coyote_frames: map_get_u64(component, "coyote_frames", 5) as u32,
            buffer_frames: map_get_u64(component, "buffer_frames", 4) as u32,
        }),
        "topdownmover" => Some(SpawnComponentSpec::TopDownMover {
            speed: map_get_f32(component, "speed", 200.0).max(0.0),
            up_action: map_get_string(component, "up_action").unwrap_or_else(|| "up".to_string()),
            down_action: map_get_string(component, "down_action")
                .unwrap_or_else(|| "down".to_string()),
            left_action: map_get_string(component, "left_action")
                .unwrap_or_else(|| "left".to_string()),
            right_action: map_get_string(component, "right_action")
                .unwrap_or_else(|| "right".to_string()),
        }),
        "health" => {
            let current = map_get_f32(component, "current", 1.0).max(0.0);
            let max = map_get_f32(component, "max", current).max(0.0);
            Some(SpawnComponentSpec::Health { current, max })
        }
        _ => Some(SpawnComponentSpec::Named(type_name)),
    }
}

fn drain_command_queue(
    world: &Map,
    script_name: &str,
    source_entity: Option<u64>,
) -> Vec<ScriptWorldCommand> {
    let mut out = Vec::new();
    let Some(queue) = world
        .get("command_queue")
        .and_then(|d| d.clone().try_cast::<Array>())
    else {
        return out;
    };

    for item in queue {
        let Some(command) = item.try_cast::<Map>() else {
            continue;
        };
        let Some(kind) = map_get_string(&command, "type") else {
            continue;
        };
        match kind.as_str() {
            "spawn" => {
                let Some(spec) = command
                    .get("spec")
                    .and_then(|d| d.clone().try_cast::<Map>())
                else {
                    continue;
                };
                let mut components = Vec::<SpawnComponentSpec>::new();
                if let Some(items) = map_get_array(&spec, "components") {
                    for item in items {
                        if let Some(name) = item.clone().try_cast::<ImmutableString>() {
                            let name = name.trim();
                            if !name.is_empty() {
                                components.push(SpawnComponentSpec::Named(name.to_string()));
                            }
                            continue;
                        }
                        if let Some(map) = item.try_cast::<Map>() {
                            if let Some(spec) = parse_spawn_component_spec(&map) {
                                components.push(spec);
                            }
                        }
                    }
                }
                let mut tags = Vec::<String>::new();
                if let Some(items) = map_get_array(&spec, "tags") {
                    for item in items {
                        if let Some(tag) = item.try_cast::<ImmutableString>() {
                            let tag = tag.trim();
                            if !tag.is_empty() {
                                tags.push(tag.to_string());
                            }
                        }
                    }
                }
                out.push(ScriptWorldCommand::Spawn {
                    x: map_get_f32(&spec, "x", 0.0),
                    y: map_get_f32(&spec, "y", 0.0),
                    components,
                    script: map_get_string(&spec, "script"),
                    tags,
                    is_player: map_get_bool(&spec, "is_player", false),
                    health: map_get_optional_f32(&spec, "health"),
                    max_health: map_get_optional_f32(&spec, "max_health"),
                    script_name: script_name.to_string(),
                    source_entity,
                });
            }
            "despawn" => {
                let target_id = map_get_u64(&command, "id", 0);
                if target_id != 0 {
                    out.push(ScriptWorldCommand::Despawn {
                        target_id,
                        script_name: script_name.to_string(),
                        source_entity,
                    });
                }
            }
            "spawn_projectile" => {
                let Some(spec) = command
                    .get("spec")
                    .and_then(|d| d.clone().try_cast::<Map>())
                else {
                    continue;
                };
                let mut dx = map_get_f32(&spec, "dx", 0.0);
                let mut dy = map_get_f32(&spec, "dy", 0.0);
                if let Some(direction) = spec
                    .get("direction")
                    .and_then(|d| d.clone().try_cast::<Map>())
                {
                    dx = map_get_f32(&direction, "x", dx);
                    dy = map_get_f32(&direction, "y", dy);
                }
                if dx == 0.0 && dy == 0.0 {
                    dx = 1.0;
                }
                out.push(ScriptWorldCommand::SpawnProjectile {
                    x: map_get_f32(&spec, "x", 0.0),
                    y: map_get_f32(&spec, "y", 0.0),
                    dx,
                    dy,
                    speed: map_get_f32(&spec, "speed", 240.0).max(0.0),
                    damage: map_get_f32(&spec, "damage", 1.0).max(0.0),
                    lifetime_frames: map_get_u64(&spec, "lifetime", 60).max(1) as u32,
                    owner_id: map_get_u64(&spec, "owner", source_entity.unwrap_or(0)),
                    damage_tag: map_get_string(&spec, "damage_tag")
                        .unwrap_or_else(|| "player".to_string()),
                    script_name: script_name.to_string(),
                    source_entity,
                });
            }
            "spawn_particles" => {
                let preset = map_get_string(&command, "preset").unwrap_or_default();
                if preset.trim().is_empty() {
                    continue;
                }
                out.push(ScriptWorldCommand::SpawnParticles {
                    preset: preset.trim().to_string(),
                    x: map_get_f32(&command, "x", 0.0),
                    y: map_get_f32(&command, "y", 0.0),
                    script_name: script_name.to_string(),
                    source_entity,
                });
            }
            _ => {}
        }
    }

    out
}

fn map_get_f32(map: &Map, key: &str, default: f32) -> f32 {
    map.get(key).map_or(default, |d| {
        if let Some(f) = d.clone().try_cast::<FLOAT>() {
            f as f32
        } else if let Some(i) = d.clone().try_cast::<INT>() {
            i as f32
        } else {
            default
        }
    })
}

fn map_get_bool(map: &Map, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|d| d.clone().try_cast::<bool>())
        .unwrap_or(default)
}

fn map_get_u64(map: &Map, key: &str, default: u64) -> u64 {
    map.get(key).map_or(default, |d| {
        if let Some(i) = d.clone().try_cast::<INT>() {
            if i < 0 {
                0
            } else {
                i as u64
            }
        } else if let Some(f) = d.clone().try_cast::<FLOAT>() {
            if f <= 0.0 {
                0
            } else {
                f as u64
            }
        } else {
            default
        }
    })
}

fn map_get_optional_f32(map: &Map, key: &str) -> Option<f32> {
    map.get(key).and_then(|d| {
        if let Some(f) = d.clone().try_cast::<FLOAT>() {
            Some(f as f32)
        } else if let Some(i) = d.clone().try_cast::<INT>() {
            Some(i as f32)
        } else {
            None
        }
    })
}

fn map_get_string(map: &Map, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|d| d.clone().try_cast::<ImmutableString>())
        .map(|s| s.to_string())
}

fn map_get_array(map: &Map, key: &str) -> Option<Array> {
    map.get(key).and_then(|d| d.clone().try_cast::<Array>())
}

fn drain_emit_queue(world: &Map, frame: u64, source_entity: Option<u64>) -> Vec<ScriptEvent> {
    let mut out = Vec::new();
    let Some(queue) = world
        .get("emit_queue")
        .and_then(|d| d.clone().try_cast::<Array>())
    else {
        return out;
    };

    for item in queue {
        let Some(event_obj) = item.try_cast::<Map>() else {
            continue;
        };
        let name = event_obj
            .get("name")
            .and_then(|d| d.clone().try_cast::<ImmutableString>())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "script_event".to_string());
        let data = event_obj
            .get("data")
            .map(dynamic_to_json)
            .unwrap_or_else(|| serde_json::json!({}));
        out.push(ScriptEvent {
            name,
            data,
            frame,
            source_entity,
        });
    }
    out
}

fn json_object_to_map(vars: &HashMap<String, serde_json::Value>) -> Map {
    let mut map = Map::new();
    for (k, v) in vars {
        map.insert(k.as_str().into(), json_to_dynamic(v));
    }
    map
}

fn map_to_json_object(map: &Map) -> HashMap<String, serde_json::Value> {
    let mut out = HashMap::new();
    for (k, v) in map {
        out.insert(k.to_string(), dynamic_to_json(v));
    }
    out
}

fn json_to_dynamic(value: &serde_json::Value) -> Dynamic {
    match value {
        serde_json::Value::Null => ().into(),
        serde_json::Value::Bool(v) => (*v).into(),
        serde_json::Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                (i as INT).into()
            } else if let Some(f) = v.as_f64() {
                (f as FLOAT).into()
            } else {
                ().into()
            }
        }
        serde_json::Value::String(v) => v.as_str().into(),
        serde_json::Value::Array(arr) => {
            let mut out = Array::new();
            for item in arr {
                out.push(json_to_dynamic(item));
            }
            out.into()
        }
        serde_json::Value::Object(obj) => {
            let mut out = Map::new();
            for (k, v) in obj {
                out.insert(k.as_str().into(), json_to_dynamic(v));
            }
            out.into()
        }
    }
}

fn dynamic_to_json(value: &Dynamic) -> serde_json::Value {
    if value.is::<()>() {
        return serde_json::Value::Null;
    }
    if let Some(v) = value.clone().try_cast::<bool>() {
        return serde_json::Value::Bool(v);
    }
    if let Some(v) = value.clone().try_cast::<INT>() {
        return serde_json::json!(v);
    }
    if let Some(v) = value.clone().try_cast::<FLOAT>() {
        return serde_json::json!(v);
    }
    if let Some(v) = value.clone().try_cast::<ImmutableString>() {
        return serde_json::Value::String(v.to_string());
    }
    if let Some(v) = value.clone().try_cast::<Array>() {
        return serde_json::Value::Array(v.iter().map(dynamic_to_json).collect());
    }
    if let Some(v) = value.clone().try_cast::<Map>() {
        let mut out = serde_json::Map::new();
        for (k, item) in v {
            out.insert(k.to_string(), dynamic_to_json(&item));
        }
        return serde_json::Value::Object(out);
    }
    serde_json::Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_engine_load_and_list() {
        let mut engine = ScriptEngine::default();
        let source = r#"fn update(entity, world, dt) { entity.x += 1.0; }"#;
        engine
            .load_script("test_script".into(), source.into(), false)
            .expect("should load valid script");
        assert!(engine.scripts.contains_key("test_script"));
        assert!(!engine.global_scripts.contains("test_script"));
        let list = engine.list_scripts();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test_script");
    }

    #[test]
    fn script_engine_load_global() {
        let mut engine = ScriptEngine::default();
        let source = r#"fn update(world, dt) { world.set_var("score", 1); }"#;
        engine
            .load_script("global_rules".into(), source.into(), true)
            .expect("should load global script");
        assert!(engine.scripts.contains_key("global_rules"));
        assert!(engine.global_scripts.contains("global_rules"));
    }

    #[test]
    fn script_engine_remove() {
        let mut engine = ScriptEngine::default();
        let source = r#"fn update(entity, world, dt) { }"#;
        engine
            .load_script("removable".into(), source.into(), false)
            .unwrap();
        assert!(engine.scripts.contains_key("removable"));
        engine.remove_script("removable");
        assert!(!engine.scripts.contains_key("removable"));
    }

    #[test]
    fn script_engine_rejects_invalid_syntax() {
        let mut engine = ScriptEngine::default();
        let result = engine.load_script("bad".into(), "fn update( { }}}".into(), false);
        assert!(result.is_err());
    }

    #[test]
    fn script_engine_transpiles_lua_compat() {
        let mut engine = ScriptEngine::default();
        let lua_source = r#"
function update(entity, world, dt)
    local x = entity.x or 0
    if x ~= nil then
        entity.vx = 1
    end
end
"#;
        engine
            .load_script("lua_test".into(), lua_source.into(), false)
            .expect("Lua-compat script should transpile and load");
        assert!(engine.scripts.contains_key("lua_test"));
        // The stored source should be the transpiled Rhai version
        let stored = &engine.scripts["lua_test"];
        assert!(stored.contains("fn update"));
        assert!(!stored.contains("local "));
    }

    #[test]
    fn rhai_engine_has_tile_query_functions() {
        let engine = make_rhai_engine();
        // Verify the engine can compile scripts that reference the tile query functions.
        // We use entity-style wrapping to match the actual runtime.
        let source = r#"
fn update(entity, world, dt) {
    let s = world.is_solid(0, 0);
    let p = world.is_platform(1, 1);
    let c = world.is_climbable(2, 2);
    let f = world.tile_friction(3, 3);
}
fn __axiom_entity_entry() {
    update(entity, world, dt);
}
"#;
        engine
            .compile(source)
            .expect("tile query functions should be registered");
    }

    #[test]
    fn rhai_engine_has_world_functions() {
        let engine = make_rhai_engine();
        let source = r#"
fn update(entity, world, dt) {
    world.emit("test_event", #{});
    world.set_var("score", 42);
    let v = world.get_var("score");
    world.camera_shake(5.0, 0.25);
    world.camera_zoom(2.0);
    world.pause();
    world.resume();
}
fn __axiom_entity_entry() {
    update(entity, world, dt);
}
"#;
        engine
            .compile(source)
            .expect("world functions should be registered");
    }

    #[test]
    fn rhai_engine_has_entity_functions() {
        let engine = make_rhai_engine();
        let source = r#"
fn update(entity, world, dt) {
    entity.damage(1.0);
    entity.heal(1.0);
    entity.knockback(10.0, 5.0);
    entity.has_tag("enemy");
    entity.add_tag("test");
    entity.remove_tag("test");
}
fn __axiom_entity_entry() {
    update(entity, world, dt);
}
"#;
        engine
            .compile(source)
            .expect("entity functions should be registered");
    }

    #[test]
    fn compile_and_cache_ast() {
        let mut runtime = RhaiRuntime::default();
        let source = r#"fn update(entity, world, dt) { entity.x += 1.0; }"#;
        let ast1 = runtime
            .compile_ast("test", source, true)
            .expect("should compile");
        let ast2 = runtime
            .compile_ast("test", source, true)
            .expect("should return cached");
        // Second call should return the cached version (same AST)
        assert_eq!(format!("{:?}", ast1), format!("{:?}", ast2));
    }

    #[test]
    fn compile_ast_recompiles_on_source_change() {
        let mut runtime = RhaiRuntime::default();
        let source1 = r#"fn update(entity, world, dt) { entity.x += 1.0; }"#;
        let source2 = r#"fn update(entity, world, dt) { entity.x += 2.0; }"#;
        runtime
            .compile_ast("test", source1, true)
            .expect("should compile v1");
        runtime
            .compile_ast("test", source2, true)
            .expect("should compile v2 (new source)");
    }

    #[test]
    fn json_dynamic_roundtrip() {
        let original = serde_json::json!({
            "name": "test",
            "score": 42,
            "alive": true,
            "items": [1, 2, 3]
        });
        let dynamic = json_to_dynamic(&original);
        let back = dynamic_to_json(&dynamic);
        assert_eq!(original, back);
    }

    #[test]
    fn script_context_tile_queries() {
        use crate::tilemap::Tilemap;
        let mut tiles = vec![0u8; 4 * 4];
        tiles[0] = TileType::Platform as u8;
        tiles[1] = TileType::Solid as u8;
        let tilemap = Tilemap {
            width: 4,
            height: 4,
            tiles,
            player_spawn: (8.0, 8.0),
            goal: None,
            ..Default::default()
        };
        let config = GameConfig::default();
        let entities = Arc::new(Vec::new());
        SCRIPT_CALL_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = ScriptCallContext {
                tilemap: Arc::new(tilemap),
                physics: config,
                entities,
                tile_edits: Vec::new(),
                tile_overrides: HashMap::new(),
            };
        });
        let mut dummy = Map::new();
        assert!(world_is_solid(&mut dummy, 1, 0));
        assert!(!world_is_solid(&mut dummy, 0, 0));
        assert!(world_is_platform(&mut dummy, 0, 0));
        assert!(!world_is_platform(&mut dummy, 1, 0));
    }
}
