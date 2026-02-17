use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use mlua::{Function, HookTriggers, Lua, LuaSerdeExt, RegistryKey, Value, VmState};
use serde::{Deserialize, Serialize};

use crate::ai;
use crate::api::types::{
    AiBehaviorDef, ComponentDef, EntitySpawnRequest, PathTypeDef, PickupEffectDef, Vec2Def,
};
use crate::components::{
    AiBehavior, AiState, Alive, AnimationController, Collider, GameConfig, GamePosition, Grounded,
    Health, Hitbox, NetworkId, NextNetworkId, PathFollower, PendingDeath, Player, RenderLayer, Tags,
    TopDownMover, Velocity,
};
use crate::events::{GameEvent, GameEventBus};
use crate::input::{MouseInput, VirtualInput};
use crate::scripting::{ScriptLogBuffer, ScriptLogEntry};
use crate::perf::PerfAccum;
use crate::raycast::{raycast_aabbs, RaycastAabb};
use crate::scripting::{ScriptError, ScriptEvent};
use crate::tilemap::Tilemap;

const MAX_SCRIPT_ERRORS: usize = 100;
const MAX_SCRIPT_EVENTS: usize = 200;
const MAX_ENTITY_SCRIPT_ERROR_STREAK: u32 = 8;
const MAX_GLOBAL_SCRIPT_ERROR_STREAK: u32 = 8;

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

fn validate_script_source(source: &str) -> Result<(), String> {
    let lua = Lua::new();
    lua.load(source)
        .set_name("script_validation")
        .exec()
        .map_err(|e| e.to_string())?;
    let globals = lua.globals();
    globals
        .get::<Function>("update")
        .map(|_| ())
        .map_err(|_| "Script must define a global `update` function".to_string())
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
pub struct ScriptEngine {
    pub scripts: HashMap<String, String>,
    pub global_scripts: HashSet<String>,
    pub always_run_scripts: HashSet<String>,
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

impl ScriptEngine {
    pub fn load_script(
        &mut self,
        name: String,
        source: String,
        global: bool,
    ) -> Result<(), String> {
        validate_script_source(&source)?;
        self.scripts.insert(name.clone(), source);
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
        let mut items: Vec<_> = self
            .scripts
            .keys()
            .map(|name| {
                let is_global = self.global_scripts.contains(name);
                let disabled = self.disabled_global_scripts.contains(name);
                let disabled_reason = if disabled {
                    let streak = self.global_error_streaks.get(name).copied().unwrap_or(0);
                    Some(format!("Disabled after {} consecutive errors", streak))
                } else {
                    None
                };
                crate::scripting::api::ScriptInfo {
                    name: name.clone(),
                    global: is_global,
                    enabled: !disabled,
                    disabled_reason,
                }
            })
            .collect();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        items
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

        let mut valid_scripts = HashMap::new();
        let mut dropped = 0usize;
        for (name, source) in scripts {
            match validate_script_source(&source) {
                Ok(()) => {
                    valid_scripts.insert(name, source);
                }
                Err(err) => {
                    dropped = dropped.saturating_add(1);
                    warn!("[Axiom scripts] Dropping invalid restored script '{name}': {err}");
                }
            }
        }

        if dropped > 0 {
            warn!("[Axiom scripts] Dropped {dropped} invalid script(s) during restore");
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

#[derive(Resource, Default)]
pub struct ScriptFrame {
    pub frame: u64,
    pub seconds: f64,
}

#[derive(Clone, Copy)]
struct ScriptExecutionLimits {
    entity_budget_ms: u64,
    global_budget_ms: u64,
    instruction_interval: u32,
}

impl Default for ScriptExecutionLimits {
    fn default() -> Self {
        Self {
            entity_budget_ms: env_u64(
                "AXIOM_SCRIPT_ENTITY_BUDGET_MS",
                crate::scripting::DEFAULT_ENTITY_SCRIPT_BUDGET_MS,
            )
            .max(1),
            global_budget_ms: env_u64(
                "AXIOM_SCRIPT_GLOBAL_BUDGET_MS",
                crate::scripting::DEFAULT_GLOBAL_SCRIPT_BUDGET_MS,
            )
            .max(1),
            instruction_interval: env_u64(
                "AXIOM_SCRIPT_HOOK_INSTRUCTION_INTERVAL",
                crate::scripting::DEFAULT_SCRIPT_HOOK_INSTRUCTION_INTERVAL as u64,
            )
            .clamp(100, 1_000_000) as u32,
        }
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn script_execution_limits() -> &'static ScriptExecutionLimits {
    static LIMITS: OnceLock<ScriptExecutionLimits> = OnceLock::new();
    LIMITS.get_or_init(ScriptExecutionLimits::default)
}

pub struct ScriptingPlugin;

impl Plugin for ScriptingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ScriptEngine::default())
            .insert_resource(ScriptErrors::default())
            .insert_resource(ScriptFrame::default())
            .insert_resource(ScriptLogBuffer::default())
            .init_resource::<ScriptTilemapCache>()
            .init_resource::<ScriptEntitySnapshotCache>()
            .init_non_send_resource::<EntityLuaRuntime>()
            .init_non_send_resource::<GlobalLuaRuntime>()
            .add_systems(
                FixedUpdate,
                (
                    tick_script_frame,
                    refresh_script_tilemap_cache,
                    refresh_script_entity_cache,
                    run_entity_scripts,
                    refresh_script_entity_cache,
                    run_global_scripts,
                    flush_script_events_to_game_events,
                    cleanup_pending_death,
                )
                    .chain()
                    .run_if(crate::game_runtime::gameplay_systems_enabled),
            )
            // Always-run scripts execute even during pause/game_over
            .add_systems(
                FixedUpdate,
                (
                    tick_script_frame_always,
                    run_always_global_scripts,
                )
                    .chain(),
            );
    }
}

struct LuaExecutionCache {
    lua: Lua,
    compiled: HashMap<String, (u64, RegistryKey)>,
}

impl Default for LuaExecutionCache {
    fn default() -> Self {
        let lua = Lua::new();
        if let Err(e) = install_entity_snapshot_metatable(&lua) {
            eprintln!("[Axiom] Failed to install entity snapshot metatable: {e}");
        }
        Self {
            lua,
            compiled: HashMap::new(),
        }
    }
}

#[derive(Default)]
struct EntityLuaRuntime(LuaExecutionCache);

#[derive(Default)]
struct GlobalLuaRuntime(LuaExecutionCache);

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

#[derive(Resource, Default)]
struct ScriptEntitySnapshotCache {
    entities: Arc<Vec<LuaEntitySnapshot>>,
    network_lookup: HashMap<u64, Entity>,
}

#[derive(Clone)]
struct LuaEntitySnapshot {
    id: u64,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    grounded: bool,
    alive: bool,
    is_player: bool,
    aabb: Option<(Vec2, Vec2)>,
    tags: HashSet<String>,
    health: Option<f32>,
    max_health: Option<f32>,
}

#[derive(Clone, Default)]
struct LuaInputSnapshot {
    active: HashSet<String>,
    just_pressed: HashSet<String>,
    mouse_x: f32,
    mouse_y: f32,
    mouse_left: bool,
    mouse_right: bool,
    mouse_middle: bool,
    mouse_left_just_pressed: bool,
    mouse_right_just_pressed: bool,
    mouse_middle_just_pressed: bool,
}

type ScriptEntityCacheQueryItem<'a> = (
    Entity,
    &'a GamePosition,
    Option<&'a Velocity>,
    Option<&'a Collider>,
    Option<&'a NetworkId>,
    Option<&'a Tags>,
    Option<&'a Player>,
    Option<&'a Grounded>,
    Option<&'a Alive>,
    Option<&'a Health>,
);

type ScriptEntityRuntimeQueryItem<'a> = (
    &'a NetworkId,
    &'a mut LuaScript,
    &'a mut GamePosition,
    Option<&'a mut Velocity>,
    Option<&'a Grounded>,
    Option<&'a mut Alive>,
    Option<&'a mut Tags>,
    Option<&'a mut Health>,
    Option<&'a mut Hitbox>,
    Option<&'a mut PathFollower>,
    Option<&'a mut AiBehavior>,
    Option<&'a mut AnimationController>,
    Option<&'a mut TopDownMover>,
    Option<&'a PendingDeath>,
    (
        Option<&'a mut RenderLayer>,
        Option<&'a mut crate::components::CollisionLayer>,
        Option<&'a crate::state_machine::EntityStateMachine>,
        Option<&'a crate::inventory::Inventory>,
    ),
);

type WorldTableBuildResult = mlua::Result<(
    mlua::Table,
    Rc<RefCell<Vec<ScriptEvent>>>,
    Rc<RefCell<Vec<ScriptWorldCommand>>>,
)>;

struct LuaEntitySnapshotParts<'a> {
    pos: &'a GamePosition,
    vel: Option<&'a Velocity>,
    collider: Option<&'a Collider>,
    network_id: Option<&'a NetworkId>,
    tags: Option<&'a Tags>,
    player: Option<&'a Player>,
    grounded: Option<&'a Grounded>,
    alive: Option<&'a Alive>,
    health: Option<&'a Health>,
}

struct WorldBuildArgs<'a> {
    vars: &'a HashMap<String, serde_json::Value>,
    script_name: &'a str,
    frame: u64,
    seconds: f64,
    dt: f32,
    source_entity: Option<u64>,
    tilemap: &'a Arc<Tilemap>,
    config: &'a GameConfig,
    entities: &'a Arc<Vec<LuaEntitySnapshot>>,
    input: &'a Arc<LuaInputSnapshot>,
    world_events: &'a Arc<Vec<GameEvent>>,
    game_state: &'a str,
}

#[derive(SystemParam)]
struct EntityScriptSystemCtx<'w, 's> {
    commands: Commands<'w, 's>,
    engine: ResMut<'w, ScriptEngine>,
    errors: ResMut<'w, ScriptErrors>,
    perf: ResMut<'w, PerfAccum>,
    frame: Res<'w, ScriptFrame>,
    time: Res<'w, Time<Fixed>>,
    vinput: Res<'w, VirtualInput>,
    mouse_input: Res<'w, MouseInput>,
    event_bus: Res<'w, GameEventBus>,
    tilemap: ResMut<'w, Tilemap>,
    config: Res<'w, GameConfig>,
    next_network_id: ResMut<'w, NextNetworkId>,
    runtime_state: Res<'w, crate::game_runtime::RuntimeState>,
    tilemap_cache: ResMut<'w, ScriptTilemapCache>,
    entity_cache: Res<'w, ScriptEntitySnapshotCache>,
    log_buffer: ResMut<'w, ScriptLogBuffer>,
    runtime: NonSendMut<'w, EntityLuaRuntime>,
    query: Query<'w, 's, ScriptEntityRuntimeQueryItem<'static>>,
}

#[derive(SystemParam)]
struct GlobalScriptSystemCtx<'w, 's> {
    commands: Commands<'w, 's>,
    engine: ResMut<'w, ScriptEngine>,
    errors: ResMut<'w, ScriptErrors>,
    perf: ResMut<'w, PerfAccum>,
    frame: Res<'w, ScriptFrame>,
    time: Res<'w, Time<Fixed>>,
    vinput: Res<'w, VirtualInput>,
    mouse_input: Res<'w, MouseInput>,
    event_bus: Res<'w, GameEventBus>,
    tilemap: ResMut<'w, Tilemap>,
    config: Res<'w, GameConfig>,
    next_network_id: ResMut<'w, NextNetworkId>,
    runtime_state: Res<'w, crate::game_runtime::RuntimeState>,
    tilemap_cache: ResMut<'w, ScriptTilemapCache>,
    entity_cache: Res<'w, ScriptEntitySnapshotCache>,
    log_buffer: ResMut<'w, ScriptLogBuffer>,
    runtime: NonSendMut<'w, GlobalLuaRuntime>,
}

#[derive(Clone)]
#[allow(dead_code)]
enum ScriptWorldCommand {
    Spawn {
        request: EntitySpawnRequest,
        script_name: String,
        source_entity: Option<u64>,
    },
    Despawn {
        target_id: u64,
        script_name: String,
        source_entity: Option<u64>,
    },
    SetTile {
        x: i32,
        y: i32,
        tile_id: u8,
        script_name: String,
        source_entity: Option<u64>,
    },
    SpawnParticles {
        preset: String,
        x: f32,
        y: f32,
        angle: Option<f32>,
        script_name: String,
        source_entity: Option<u64>,
    },
    DamageEntity {
        target_id: u64,
        amount: f32,
    },
    HealEntity {
        target_id: u64,
        amount: f32,
    },
    KnockbackEntity {
        target_id: u64,
        dx: f32,
        dy: f32,
    },
    SetPosition {
        target_id: u64,
        x: f32,
        y: f32,
    },
    SetVelocity {
        target_id: u64,
        vx: f32,
        vy: f32,
    },
    SetAlive {
        target_id: u64,
        alive: bool,
    },
    TweenEntity {
        target_id: u64,
        property: String,
        to: f32,
        from: Option<f32>,
        duration: f32,
        easing: Option<String>,
        tween_id: Option<String>,
    },
    TweenSequence {
        target_id: u64,
        steps: Vec<crate::tween::TweenStep>,
        sequence_id: Option<String>,
    },
    ScreenEffect {
        effect: String,
        duration: f32,
        color: Option<[f32; 3]>,
    },
    SetAmbient {
        intensity: f32,
        color: Option<[f32; 3]>,
    },
    SpawnText {
        x: f32,
        y: f32,
        text: String,
        font_size: f32,
        color: [f32; 4],
        duration: Option<f32>,
        fade: bool,
        rise_speed: f32,
        owner_id: Option<u64>,
    },
    SetWeather {
        weather_type: String,
        intensity: f32,
        wind: f32,
    },
    ClearWeather,
    SetParallax {
        layers: Vec<crate::parallax::ParallaxLayerDef>,
    },
    PlayCutscene {
        name: String,
    },
    StopCutscene,
    SetTimeOfDay {
        hour: f32,
    },
    TransitionEntityState {
        target_id: u64,
        state: String,
    },
    InventoryAdd {
        target_id: u64,
        item_id: String,
        count: u32,
    },
    InventoryRemove {
        target_id: u64,
        item_id: String,
        count: u32,
    },
    SetTint {
        target_id: u64,
        color: [f32; 4],
    },
    FlashTint {
        target_id: u64,
        color: [f32; 4],
        frames: u32,
    },
    SetTrail {
        target_id: u64,
        interval: u32,
        duration: f32,
        alpha_start: f32,
        alpha_end: f32,
    },
    ClearTrail {
        target_id: u64,
    },
    SetCollisionLayer {
        target_id: u64,
        layer: u16,
        mask: u16,
    },
    RebindInput {
        key: String,
        action: String,
    },
}



fn source_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn call_lua_with_budget<T, F>(
    lua: &Lua,
    max_duration: Duration,
    instruction_interval: u32,
    f: F,
) -> mlua::Result<T>
where
    F: FnOnce() -> mlua::Result<T>,
{
    let started = Instant::now();
    let budget_ms = max_duration.as_secs_f64() * 1000.0;
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(instruction_interval.max(1)),
        move |_lua, _debug| {
            if started.elapsed() >= max_duration {
                return Err(mlua::Error::RuntimeError(format!(
                    "Script execution budget exceeded ({budget_ms:.1}ms)"
                )));
            }
            Ok(VmState::Continue)
        },
    );
    let out = f();
    lua.remove_hook();
    out
}

fn get_or_compile_update(
    lua: &Lua,
    compiled: &mut HashMap<String, (u64, RegistryKey)>,
    script_name: &str,
    source: &str,
) -> Result<Function, String> {
    let hash = source_hash(source);
    if let Some((cached_hash, key)) = compiled.get(script_name) {
        if *cached_hash == hash {
            if let Ok(func) = lua.registry_value::<Function>(key) {
                return Ok(func);
            }
        }
    }

    lua.load(source)
        .set_name(script_name)
        .exec()
        .map_err(|e| e.to_string())?;
    let update = lua
        .globals()
        .get::<Function>("update")
        .map_err(|e| format!("Missing update() in script '{script_name}': {e}"))?;
    let key = lua
        .create_registry_value(update.clone())
        .map_err(|e| e.to_string())?;
    compiled.insert(script_name.to_string(), (hash, key));
    Ok(update)
}

fn tick_script_frame(mut frame: ResMut<ScriptFrame>, time: Res<Time<Fixed>>) {
    frame.frame = frame.frame.saturating_add(1);
    frame.seconds += time.delta_secs_f64();
}

fn refresh_script_tilemap_cache(tilemap: Res<Tilemap>, mut cache: ResMut<ScriptTilemapCache>) {
    if tilemap.is_added() || tilemap.is_changed() {
        cache.tilemap = Arc::new(tilemap.clone());
    }
}

fn refresh_script_entity_cache(
    query: Query<ScriptEntityCacheQueryItem<'_>>,
    mut cache: ResMut<ScriptEntitySnapshotCache>,
) {
    let mut network_lookup = HashMap::<u64, Entity>::new();
    let entities = query
        .iter()
        .filter_map(
            |(entity, pos, vel, collider, network_id, tags, player, grounded, alive, health)| {
                let snapshot = make_lua_entity_snapshot(LuaEntitySnapshotParts {
                    pos,
                    vel,
                    collider,
                    network_id,
                    tags,
                    player,
                    grounded,
                    alive,
                    health,
                })?;
                network_lookup.insert(snapshot.id, entity);
                Some(snapshot)
            },
        )
        .collect::<Vec<_>>();

    cache.entities = Arc::new(entities);
    cache.network_lookup = network_lookup;
}

fn run_entity_scripts(ctx: EntityScriptSystemCtx<'_, '_>) {
    let EntityScriptSystemCtx {
        mut commands,
        mut engine,
        mut errors,
        mut perf,
        frame,
        time,
        vinput,
        mouse_input,
        event_bus,
        mut tilemap,
        config,
        mut next_network_id,
        runtime_state,
        mut tilemap_cache,
        entity_cache,
        mut log_buffer,
        mut runtime,
        mut query,
    } = ctx;
    let start = std::time::Instant::now();
    let dt = time.delta_secs();
    let cache = &mut runtime.0;
    let shared_tilemap = tilemap_cache.tilemap.clone();
    let lua = &cache.lua;
    let mut used_scripts = HashSet::new();
    let input_snapshot = Arc::new(LuaInputSnapshot {
        active: vinput.active.clone(),
        just_pressed: vinput.just_pressed.clone(),
        mouse_x: mouse_input.world_x,
        mouse_y: mouse_input.world_y,
        mouse_left: mouse_input.left,
        mouse_right: mouse_input.right,
        mouse_middle: mouse_input.middle,
        mouse_left_just_pressed: mouse_input.left_just_pressed,
        mouse_right_just_pressed: mouse_input.right_just_pressed,
        mouse_middle_just_pressed: mouse_input.middle_just_pressed,
    });
    let bus_events = &event_bus.recent;
    let mut used_entity_cursor_keys = HashSet::<(String, u64)>::new();
    let mut network_lookup = entity_cache.network_lookup.clone();
    let raycast_entities = entity_cache.entities.clone();
    let mut pending_world_commands = Vec::<ScriptWorldCommand>::new();
    let mut pending_log_entries = Vec::<ScriptLogEntry>::new();

    for (
        network_id,
        mut script,
        mut pos,
        mut vel,
        grounded,
        mut alive,
        mut tags,
        mut health,
        mut hitbox,
        mut path_follower,
        mut ai_behavior,
        mut animation_controller,
        mut top_down_mover,
        pending_death,
        (mut render_layer, mut collision_layer, state_machine, inventory),
    ) in query.iter_mut()
    {
        lua.expire_registry_values();
        let cursor_key = (script.script_name.clone(), network_id.0);
        used_entity_cursor_keys.insert(cursor_key.clone());
        if !script.enabled {
            continue;
        }
        let (
            update,
            uses_world_api,
            uses_tag_helpers,
            _uses_damage_helpers,
            _uses_follow_path_helpers,
            uses_ai_helpers,
            uses_hitbox_helpers,
            uses_animation_helpers,
        ) = {
            let Some(source) = engine.scripts.get(&script.script_name) else {
                continue;
            };
            let uses_world_api = source.contains("world.");
            let uses_tag_helpers = source.contains("has_tag")
                || source.contains("add_tag")
                || source.contains("remove_tag")
                || source.contains(".tags");
            let uses_damage_helpers = source.contains("damage(")
                || source.contains("heal(")
                || source.contains("knockback(");
            let uses_follow_path_helpers = source.contains("follow_path");
            let uses_ai_helpers = source.contains(".ai") || source.contains("ai.");
            let uses_hitbox_helpers = source.contains("hitbox");
            let uses_animation_helpers = source.contains("animation") || source.contains("flip_x");
            used_scripts.insert(script.script_name.clone());
            match get_or_compile_update(lua, &mut cache.compiled, &script.script_name, source) {
                Ok(f) => (
                    f,
                    uses_world_api,
                    uses_tag_helpers,
                    uses_damage_helpers,
                    uses_follow_path_helpers,
                    uses_ai_helpers,
                    uses_hitbox_helpers,
                    uses_animation_helpers,
                ),
                Err(err_msg) => {
                    script.error_streak = script.error_streak.saturating_add(1);
                    errors.push(ScriptError {
                        script_name: script.script_name.clone(),
                        entity_id: Some(network_id.0),
                        error_message: err_msg,
                        frame: frame.frame,
                    });
                    if script.error_streak >= MAX_ENTITY_SCRIPT_ERROR_STREAK {
                        script.enabled = false;
                        script.disabled_reason = Some(format!(
                            "Disabled after {} consecutive errors",
                            script.error_streak
                        ));
                    }
                    continue;
                }
            }
        };
        let last_event_frame = engine
            .entity_event_cursors
            .get(&cursor_key)
            .copied()
            .unwrap_or(0);
        let world_events = Arc::new(
            bus_events
                .iter()
                .filter(|ev| ev.frame > last_event_frame)
                .cloned()
                .collect::<Vec<_>>(),
        );

        let globals = lua.globals();
        let _ = globals.set("__axiom_current_script", script.script_name.as_str());

        let entity_tbl = match lua.create_table() {
            Ok(t) => t,
            Err(err) => {
                script.error_streak = script.error_streak.saturating_add(1);
                errors.push(ScriptError {
                    script_name: script.script_name.clone(),
                    entity_id: Some(network_id.0),
                    error_message: err.to_string(),
                    frame: frame.frame,
                });
                if script.error_streak >= MAX_ENTITY_SCRIPT_ERROR_STREAK {
                    script.enabled = false;
                    script.disabled_reason = Some(format!(
                        "Disabled after {} consecutive errors",
                        script.error_streak
                    ));
                }
                continue;
            }
        };

        let _ = entity_tbl.set("id", network_id.0);
        let _ = entity_tbl.set("x", pos.x);
        let _ = entity_tbl.set("y", pos.y);
        if let Some(v) = vel.as_ref() {
            let _ = entity_tbl.set("vx", v.x);
            let _ = entity_tbl.set("vy", v.y);
        } else {
            let _ = entity_tbl.set("vx", 0.0f32);
            let _ = entity_tbl.set("vy", 0.0f32);
        }
        let _ = entity_tbl.set("grounded", grounded.is_some_and(|g| g.0));
        let _ = entity_tbl.set("alive", alive.as_ref().is_none_or(|a| a.0));
        if let Some(h) = health.as_ref() {
            let _ = entity_tbl.set("health", h.current);
            let _ = entity_tbl.set("max_health", h.max);
        }
        if let Some(tdm) = top_down_mover.as_ref() {
            let _ = entity_tbl.set("speed", tdm.speed);
        }
        let _ = entity_tbl.set("render_layer", render_layer.as_ref().map_or(0, |r| r.0));
        if let Some(cl) = collision_layer.as_ref() {
            let _ = entity_tbl.set("collision_layer", cl.layer);
            let _ = entity_tbl.set("collision_mask", cl.mask);
        }
        if let Some(sm) = state_machine.as_ref() {
            let _ = entity_tbl.set("machine_state", sm.current.clone());
            if let Some(prev) = &sm.previous {
                let _ = entity_tbl.set("previous_machine_state", prev.clone());
            }
        }
        if let Some(inv) = inventory.as_ref() {
            if let Ok(inv_tbl) = lua.create_table() {
                for (i, slot) in inv.slots.iter().enumerate() {
                    if let Ok(slot_tbl) = lua.create_table() {
                        let _ = slot_tbl.set("item_id", slot.item_id.clone());
                        let _ = slot_tbl.set("count", slot.count);
                        let _ = inv_tbl.set(i + 1, slot_tbl);
                    }
                }
                let _ = entity_tbl.set("__axiom_inventory", inv_tbl);
            }
        }
        let _ = entity_tbl.set("state", lua.to_value(&script.state).unwrap_or(Value::Nil));
        if uses_tag_helpers {
            let tags_tbl = match lua.create_table() {
                Ok(t) => t,
                Err(err) => {
                    script.error_streak = script.error_streak.saturating_add(1);
                    errors.push(ScriptError {
                        script_name: script.script_name.clone(),
                        entity_id: Some(network_id.0),
                        error_message: err.to_string(),
                        frame: frame.frame,
                    });
                    if script.error_streak >= MAX_ENTITY_SCRIPT_ERROR_STREAK {
                        script.enabled = false;
                        script.disabled_reason = Some(format!(
                            "Disabled after {} consecutive errors",
                            script.error_streak
                        ));
                    }
                    continue;
                }
            };
            let current_tags: Vec<String> = tags
                .as_ref()
                .map(|t| t.0.iter().cloned().collect())
                .unwrap_or_default();
            for tag in &current_tags {
                let _ = tags_tbl.set(tag.as_str(), true);
            }
            let _ = entity_tbl.set("tags", tags_tbl);
            // has_tag, add_tag, remove_tag are now in __axiom_entity_mt metatable
        }
        if uses_hitbox_helpers {
            if let Some(hb) = hitbox.as_ref() {
                if let Ok(hitbox_tbl) = lua.create_table() {
                    let _ = hitbox_tbl.set("width", hb.width);
                    let _ = hitbox_tbl.set("height", hb.height);
                    let _ = hitbox_tbl.set("offset_x", hb.offset.x);
                    let _ = hitbox_tbl.set("offset_y", hb.offset.y);
                    let _ = hitbox_tbl.set("active", hb.active);
                    let _ = hitbox_tbl.set("damage", hb.damage);
                    let _ = hitbox_tbl.set("damage_tag", hb.damage_tag.clone());
                    let _ = entity_tbl.set("hitbox", hitbox_tbl);
                }
            }
        }
        // damage, heal, knockback, follow_path are now in __axiom_entity_mt metatable
        if uses_ai_helpers {
            if let Some(ai) = ai_behavior.as_ref() {
                if let Ok(ai_tbl) = lua.create_table() {
                    let _ = ai_tbl.set("state", ai_state_label(&ai.state));
                    if let Some(target_id) = ai_state_target_id(&ai.state) {
                        let _ = ai_tbl.set("target_id", target_id);
                    }
                    // Store back-reference to entity for chase/idle methods.
                    // This is a Lua-only reference (no Rust Table handle clone)
                    // so Lua GC can collect the cycle.
                    let _ = lua.load(r#"
                        local ai, entity = ...
                        function ai.chase(target_id)
                            entity.__axiom_ai_override = { mode = "chase", target_id = target_id }
                        end
                        function ai.idle()
                            entity.__axiom_ai_override = { mode = "idle" }
                        end
                    "#).call::<()>((&ai_tbl, &entity_tbl));
                    let _ = entity_tbl.set("ai", ai_tbl);
                }
            }
        }
        if uses_animation_helpers {
            if let Some(anim) = animation_controller.as_ref() {
                let _ = entity_tbl.set("animation", anim.state.clone());
                let _ = entity_tbl.set("animation_frame", anim.frame);
                let _ = entity_tbl.set("flip_x", !anim.facing_right);
                let _ = entity_tbl.set("animation_graph", anim.graph.clone());
                let _ = entity_tbl.set("facing_direction", anim.facing_direction);
            }
        }

        // Install entity methods via a pre-compiled Lua function.
        // This creates pure Lua closures (no Rust Table handle captures),
        // so Lua GC can collect the entity_tbl → closure → entity_tbl cycle.
        if let Ok(setup_fn) = lua.globals().raw_get::<Function>("__axiom_setup_entity_methods") {
            let _ = setup_fn.call::<()>(entity_tbl.clone());
        }

        let world_tbl = if uses_world_api {
            build_world_table(
                lua,
                &WorldBuildArgs {
                    vars: &engine.vars,
                    script_name: &script.script_name,
                    frame: frame.frame,
                    seconds: frame.seconds,
                    dt,
                    source_entity: Some(network_id.0),
                    tilemap: &shared_tilemap,
                    config: &config,
                    entities: &raycast_entities,
                    input: &input_snapshot,
                    world_events: &world_events,
                    game_state: &runtime_state.state,
                },
            )
        } else {
            build_min_world_table(
                lua,
                &engine.vars,
                frame.frame,
                frame.seconds,
                dt,
                &runtime_state.state,
            )
        };
        let (world_tbl, pending_events, pending_commands) = match world_tbl {
            Ok(v) => v,
            Err(err) => {
                script.error_streak = script.error_streak.saturating_add(1);
                errors.push(ScriptError {
                    script_name: script.script_name.clone(),
                    entity_id: Some(network_id.0),
                    error_message: err.to_string(),
                    frame: frame.frame,
                });
                if script.error_streak >= MAX_ENTITY_SCRIPT_ERROR_STREAK {
                    script.enabled = false;
                    script.disabled_reason = Some(format!(
                        "Disabled after {} consecutive errors",
                        script.error_streak
                    ));
                }
                continue;
            }
        };

        let limits = script_execution_limits();
        let run_result = call_lua_with_budget(
            lua,
            Duration::from_millis(limits.entity_budget_ms),
            limits.instruction_interval,
            || update.call::<()>((entity_tbl.clone(), world_tbl.clone(), dt)),
        );
        if let Err(err) = run_result {
            script.error_streak = script.error_streak.saturating_add(1);
            errors.push(ScriptError {
                script_name: script.script_name.clone(),
                entity_id: Some(network_id.0),
                error_message: err.to_string(),
                frame: frame.frame,
            });
            if script.error_streak >= MAX_ENTITY_SCRIPT_ERROR_STREAK {
                script.enabled = false;
                script.disabled_reason = Some(format!(
                    "Disabled after {} consecutive errors",
                    script.error_streak
                ));
            }
            continue;
        }
        script.error_streak = 0;
        script.disabled_reason = None;

        if let Ok(v) = entity_tbl.get::<f32>("x") {
            if v.is_finite() { pos.x = v; }
        }
        if let Ok(v) = entity_tbl.get::<f32>("y") {
            if v.is_finite() { pos.y = v; }
        }
        if let Some(v) = vel.as_deref_mut() {
            if let Ok(x) = entity_tbl.get::<f32>("vx") {
                if x.is_finite() { v.x = x; }
            }
            if let Ok(y) = entity_tbl.get::<f32>("vy") {
                if y.is_finite() { v.y = y; }
            }
        }
        let mut alive_override = entity_tbl.get::<bool>("alive").ok();
        if let Some(h) = health.as_deref_mut() {
            if let Ok(max_health) = entity_tbl.get::<f32>("max_health") {
                if max_health.is_finite() { h.max = max_health.max(0.0); }
            }
            if let Ok(current_health) = entity_tbl.get::<f32>("health") {
                if current_health.is_finite() { h.current = current_health.clamp(0.0, h.max.max(0.0)); }
            }
            if h.current <= 0.0 {
                alive_override = Some(false);
            }
        }
        if let Some(a) = alive.as_deref_mut() {
            if let Some(is_alive) = alive_override {
                a.0 = is_alive;
            }
        }
        if let Some(hb) = hitbox.as_deref_mut() {
            if let Ok(hitbox_tbl) = entity_tbl.get::<mlua::Table>("hitbox") {
                if let Ok(width) = hitbox_tbl.get::<f32>("width") {
                    if width.is_finite() { hb.width = width.max(0.0); }
                }
                if let Ok(height) = hitbox_tbl.get::<f32>("height") {
                    if height.is_finite() { hb.height = height.max(0.0); }
                }
                if let Ok(offset_x) = hitbox_tbl.get::<f32>("offset_x") {
                    if offset_x.is_finite() { hb.offset.x = offset_x; }
                }
                if let Ok(offset_y) = hitbox_tbl.get::<f32>("offset_y") {
                    if offset_y.is_finite() { hb.offset.y = offset_y; }
                }
                if let Ok(active) = hitbox_tbl.get::<bool>("active") {
                    hb.active = active;
                }
                if let Ok(damage) = hitbox_tbl.get::<f32>("damage") {
                    if damage.is_finite() { hb.damage = damage.max(0.0); }
                }
                if let Ok(tag) = hitbox_tbl.get::<String>("damage_tag") {
                    hb.damage_tag = tag;
                }
            }
        }
        if let Some(pf) = path_follower.as_deref_mut() {
            if let Ok(speed) = entity_tbl.get::<f32>("__axiom_follow_speed") {
                if speed.is_finite() { pf.speed = speed.max(0.0); }
            }
            if let Ok(path_tbl) = entity_tbl.get::<mlua::Table>("__axiom_follow_path") {
                let mut points = Vec::new();
                for item in path_tbl.sequence_values::<Value>() {
                    let Ok(value) = item else {
                        continue;
                    };
                    let Value::Table(point) = value else {
                        continue;
                    };
                    let Ok(x) = point.get::<f32>("x") else {
                        continue;
                    };
                    let Ok(y) = point.get::<f32>("y") else {
                        continue;
                    };
                    if !x.is_finite() || !y.is_finite() { continue; }
                    points.push(Vec2::new(x, y));
                }
                if let Some(last) = points.last().copied() {
                    pf.target = last;
                    pf.path = points;
                    pf.frames_until_recalc = pf.recalculate_interval.max(1);
                }
            }
        }
        if let Some(ai) = ai_behavior.as_deref_mut() {
            if let Ok(cmd) = entity_tbl.get::<mlua::Table>("__axiom_ai_override") {
                if let Ok(mode) = cmd.get::<String>("mode") {
                    match mode.as_str() {
                        "chase" => {
                            if let Ok(target_id) = cmd.get::<u64>("target_id") {
                                ai.state = AiState::Chasing { target_id };
                            }
                        }
                        "idle" => {
                            ai.state = AiState::Idle;
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(anim) = animation_controller.as_deref_mut() {
            let orig_frame = anim.frame;
            let mut state_changed = false;
            if let Ok(next_state) = entity_tbl.get::<String>("animation") {
                let state = next_state.trim();
                if !state.is_empty() && state != anim.state {
                    anim.state = state.to_string();
                    anim.frame = 0;
                    anim.timer = 0.0;
                    anim.playing = true;
                    state_changed = true;
                }
            }
            if let Ok(frame_override) = entity_tbl.get::<usize>("animation_frame") {
                // Only apply frame override if the script explicitly changed it.
                // When animation state just changed, the old frame value would
                // incorrectly overwrite the reset to 0.
                if !state_changed || frame_override != orig_frame {
                    anim.frame = frame_override;
                }
            }
            if let Ok(flip_x) = entity_tbl.get::<bool>("flip_x") {
                anim.facing_right = !flip_x;
            }
            // Write back animation_graph (switch sprite sheet, e.g. bat → shotgun)
            if let Ok(next_graph) = entity_tbl.get::<String>("animation_graph") {
                let graph = next_graph.trim();
                if !graph.is_empty() && graph != anim.graph {
                    anim.graph = graph.to_string();
                    anim.frame = 0;
                    anim.timer = 0.0;
                    anim.playing = true;
                }
            }
            // Write back facing_direction (0-7)
            if let Ok(dir) = entity_tbl.get::<u8>("facing_direction") {
                anim.facing_direction = dir.min(7);
            }
        }
        // Write back speed to TopDownMover
        if let Some(tdm) = top_down_mover.as_deref_mut() {
            if let Ok(speed) = entity_tbl.get::<f32>("speed") {
                if speed.is_finite() { tdm.speed = speed.max(0.0); }
            }
        }
        // Write back render_layer
        if let Ok(layer) = entity_tbl.get::<i32>("render_layer") {
            if let Some(rl) = render_layer.as_deref_mut() {
                rl.0 = layer;
            }
        }
        // Write back collision_layer and collision_mask
        if let Some(cl) = collision_layer.as_deref_mut() {
            if let Ok(layer) = entity_tbl.get::<u16>("collision_layer") {
                cl.layer = layer;
            }
            if let Ok(mask) = entity_tbl.get::<u16>("collision_mask") {
                cl.mask = mask;
            }
        }
        // Process deferred entity commands: flash, tint, trail, transition_state, inventory
        let ent_id = network_id.0;
        if let Ok(flash_tbl) = entity_tbl.get::<mlua::Table>("__axiom_flash") {
            let r = flash_tbl.get::<f32>("r").unwrap_or(1.0);
            let g = flash_tbl.get::<f32>("g").unwrap_or(1.0);
            let b = flash_tbl.get::<f32>("b").unwrap_or(1.0);
            let a = flash_tbl.get::<f32>("a").unwrap_or(1.0);
            let frames = flash_tbl.get::<u32>("frames").unwrap_or(6);
            pending_world_commands.push(ScriptWorldCommand::FlashTint {
                target_id: ent_id,
                color: [r, g, b, a],
                frames,
            });
        }
        if let Ok(tint_tbl) = entity_tbl.get::<mlua::Table>("tint") {
            let r: f32 = tint_tbl.get(1).unwrap_or(1.0);
            let g: f32 = tint_tbl.get(2).unwrap_or(1.0);
            let b: f32 = tint_tbl.get(3).unwrap_or(1.0);
            let a: f32 = tint_tbl.get(4).unwrap_or(1.0);
            pending_world_commands.push(ScriptWorldCommand::SetTint {
                target_id: ent_id,
                color: [r, g, b, a],
            });
        }
        if let Ok(trail_tbl) = entity_tbl.get::<mlua::Table>("trail") {
            let interval = trail_tbl.get::<u32>("interval").unwrap_or(3);
            let duration = trail_tbl.get::<f32>("duration").unwrap_or(0.3);
            let alpha_start = trail_tbl.get::<f32>("alpha_start").unwrap_or(0.6);
            let alpha_end = trail_tbl.get::<f32>("alpha_end").unwrap_or(0.0);
            pending_world_commands.push(ScriptWorldCommand::SetTrail {
                target_id: ent_id,
                interval,
                duration,
                alpha_start,
                alpha_end,
            });
        }
        if let Ok(state_name) = entity_tbl.get::<String>("__axiom_transition_state") {
            pending_world_commands.push(ScriptWorldCommand::TransitionEntityState {
                target_id: ent_id,
                state: state_name,
            });
        }
        if let Ok(inv_cmds) = entity_tbl.get::<mlua::Table>("__axiom_inv_cmds") {
            for item in inv_cmds.sequence_values::<mlua::Table>() {
                if let Ok(cmd_tbl) = item {
                    let action: String = cmd_tbl.get("action").unwrap_or_default();
                    let item_id: String = cmd_tbl.get("item_id").unwrap_or_default();
                    let count: u32 = cmd_tbl.get("count").unwrap_or(1);
                    if !item_id.is_empty() {
                        match action.as_str() {
                            "add" => pending_world_commands.push(ScriptWorldCommand::InventoryAdd {
                                target_id: ent_id,
                                item_id,
                                count,
                            }),
                            "remove" => pending_world_commands.push(ScriptWorldCommand::InventoryRemove {
                                target_id: ent_id,
                                item_id,
                                count,
                            }),
                            _ => {}
                        }
                    }
                }
            }
        }
        // on_death lifecycle hook: if entity has PendingDeath, call on_death() if defined
        if pending_death.is_some() {
            if let Some(source) = engine.scripts.get(&script.script_name) {
                if source.contains("on_death") {
                    let on_death_result: Result<Function, _> = lua.globals().get("on_death");
                    if let Ok(on_death_fn) = on_death_result {
                        let _ = call_lua_with_budget(
                            lua,
                            Duration::from_millis(limits.entity_budget_ms),
                            limits.instruction_interval,
                            || on_death_fn.call::<()>((entity_tbl.clone(), world_tbl.clone())),
                        );
                    }
                }
            }
        }
        if let Ok(state_val) = entity_tbl.get::<Value>("state") {
            if let Ok(json_state) = lua.from_value::<serde_json::Value>(state_val) {
                script.state = json_state;
            }
        }
        if let Some(tags_comp) = tags.as_deref_mut() {
            if let Ok(tags_map) = entity_tbl.get::<mlua::Table>("tags") {
                let mut new_tags = HashSet::new();
                for (k, v) in tags_map.pairs::<String, Value>().flatten() {
                    let keep = !matches!(v, Value::Nil | Value::Boolean(false));
                    if keep {
                        new_tags.insert(k);
                    }
                }
                tags_comp.0 = new_tags;
            }
        }
        if let Ok(vars_val) = world_tbl.get::<Value>("vars") {
            if let Ok(vars) = lua.from_value::<HashMap<String, serde_json::Value>>(vars_val) {
                engine.vars = vars;
            }
        }

        for event in pending_events.borrow().iter() {
            engine.push_event(event.clone());
        }
        for cmd in pending_commands.borrow().iter() {
            pending_world_commands.push(cmd.clone());
        }
        if let Ok(logs_tbl) = world_tbl.get::<Value>("__axiom_pending_logs") {
            if let Ok(logs) = lua.from_value::<Vec<ScriptLogEntry>>(logs_tbl) {
                for entry in logs {
                    pending_log_entries.push(entry);
                }
            }
        }
        engine.entity_event_cursors.insert(cursor_key, frame.frame);

        // Drop tables to release closure registry refs. Multiple GC passes
        // are needed because closures captured Table handles that only get
        // queued for expiry when the closure itself is collected.
        drop(world_tbl);
        drop(entity_tbl);
        for _ in 0..3 {
            lua.expire_registry_values();
            let _ = lua.gc_collect();
        }
    }

    for entry in pending_log_entries {
        log_buffer.push(entry);
    }

    engine
        .entity_event_cursors
        .retain(|key, _| used_entity_cursor_keys.contains(key));

    let tilemap_changed = apply_script_world_commands(
        &mut commands,
        &mut tilemap,
        &mut next_network_id,
        &mut network_lookup,
        &mut errors,
        frame.frame,
        pending_world_commands,
    );
    if tilemap_changed {
        tilemap_cache.tilemap = Arc::new(tilemap.clone());
    }

    runtime
        .0
        .compiled
        .retain(|script_name, _| used_scripts.contains(script_name));

    // Force a GC cycle to reclaim the many temporary tables and closures created
    // per entity per frame (entity_tbl, world_tbl, ~35 closures each).
    // Without this, the Lua auxiliary stack fills up and panics at 8000 slots.
    let _ = runtime.0.lua.gc_collect();

    perf.script_time_ms += start.elapsed().as_secs_f32() * 1000.0;
}

fn flush_script_events_to_game_events(
    mut engine: ResMut<ScriptEngine>,
    mut bus: ResMut<GameEventBus>,
) {
    for ev in engine.pending_events.drain(..) {
        bus.emit(ev.name, ev.data, ev.source_entity);
    }
}

fn run_global_scripts(ctx: GlobalScriptSystemCtx<'_, '_>) {
    let GlobalScriptSystemCtx {
        mut commands,
        mut engine,
        mut errors,
        mut perf,
        frame,
        time,
        vinput,
        mouse_input,
        event_bus,
        mut tilemap,
        config,
        mut next_network_id,
        runtime_state,
        mut tilemap_cache,
        entity_cache,
        mut log_buffer,
        mut runtime,
    } = ctx;
    let start = std::time::Instant::now();
    let dt = time.delta_secs();
    let cache = &mut runtime.0;
    let shared_tilemap = tilemap_cache.tilemap.clone();
    let lua = &cache.lua;
    let mut used_scripts = HashSet::new();
    let input_snapshot = Arc::new(LuaInputSnapshot {
        active: vinput.active.clone(),
        just_pressed: vinput.just_pressed.clone(),
        mouse_x: mouse_input.world_x,
        mouse_y: mouse_input.world_y,
        mouse_left: mouse_input.left,
        mouse_right: mouse_input.right,
        mouse_middle: mouse_input.middle,
        mouse_left_just_pressed: mouse_input.left_just_pressed,
        mouse_right_just_pressed: mouse_input.right_just_pressed,
        mouse_middle_just_pressed: mouse_input.middle_just_pressed,
    });
    let bus_events = &event_bus.recent;
    let mut used_global_cursor_keys = HashSet::<String>::new();
    let mut network_lookup = entity_cache.network_lookup.clone();
    let raycast_entities = entity_cache.entities.clone();
    let mut pending_world_commands = Vec::<ScriptWorldCommand>::new();

    let global_names: Vec<String> = engine.global_scripts.iter().cloned().collect();
    for script_name in global_names {
        lua.expire_registry_values();
        used_global_cursor_keys.insert(script_name.clone());
        if engine.disabled_global_scripts.contains(&script_name) {
            continue;
        }
        let update = {
            let Some(source) = engine.scripts.get(&script_name) else {
                continue;
            };
            used_scripts.insert(script_name.clone());
            match get_or_compile_update(lua, &mut cache.compiled, &script_name, source) {
                Ok(f) => f,
                Err(err_msg) => {
                    let streak = engine
                        .global_error_streaks
                        .entry(script_name.clone())
                        .or_insert(0);
                    *streak = streak.saturating_add(1);
                    errors.push(ScriptError {
                        script_name: script_name.clone(),
                        entity_id: None,
                        error_message: err_msg,
                        frame: frame.frame,
                    });
                    if *streak >= MAX_GLOBAL_SCRIPT_ERROR_STREAK {
                        engine.disabled_global_scripts.insert(script_name.clone());
                    }
                    continue;
                }
            }
        };
        let last_event_frame = engine
            .global_event_cursors
            .get(&script_name)
            .copied()
            .unwrap_or(0);
        let world_events = Arc::new(
            bus_events
                .iter()
                .filter(|ev| ev.frame > last_event_frame)
                .cloned()
                .collect::<Vec<_>>(),
        );

        let globals = lua.globals();
        let _ = globals.set("__axiom_current_script", script_name.as_str());

        let world_tbl = build_world_table(
            lua,
            &WorldBuildArgs {
                vars: &engine.vars,
                script_name: &script_name,
                frame: frame.frame,
                seconds: frame.seconds,
                dt,
                source_entity: None,
                tilemap: &shared_tilemap,
                config: &config,
                entities: &raycast_entities,
                input: &input_snapshot,
                world_events: &world_events,
                game_state: &runtime_state.state,
            },
        );
        let (world_tbl, pending_events, pending_commands) = match world_tbl {
            Ok(v) => v,
            Err(err) => {
                let streak = engine
                    .global_error_streaks
                    .entry(script_name.clone())
                    .or_insert(0);
                *streak = streak.saturating_add(1);
                errors.push(ScriptError {
                    script_name: script_name.clone(),
                    entity_id: None,
                    error_message: err.to_string(),
                    frame: frame.frame,
                });
                if *streak >= MAX_GLOBAL_SCRIPT_ERROR_STREAK {
                    engine.disabled_global_scripts.insert(script_name.clone());
                }
                continue;
            }
        };
        let limits = script_execution_limits();
        let run_result = call_lua_with_budget(
            lua,
            Duration::from_millis(limits.global_budget_ms),
            limits.instruction_interval,
            || update.call::<()>((world_tbl.clone(), dt)),
        );
        if let Err(err) = run_result {
            let streak = engine
                .global_error_streaks
                .entry(script_name.clone())
                .or_insert(0);
            *streak = streak.saturating_add(1);
            errors.push(ScriptError {
                script_name: script_name.clone(),
                entity_id: None,
                error_message: err.to_string(),
                frame: frame.frame,
            });
            if *streak >= MAX_GLOBAL_SCRIPT_ERROR_STREAK {
                engine.disabled_global_scripts.insert(script_name.clone());
            }
            continue;
        }
        engine.global_error_streaks.insert(script_name.clone(), 0);

        if let Ok(vars_val) = world_tbl.get::<Value>("vars") {
            if let Ok(vars) = lua.from_value::<HashMap<String, serde_json::Value>>(vars_val) {
                engine.vars = vars;
            }
        }
        for event in pending_events.borrow().iter() {
            engine.push_event(event.clone());
        }
        for cmd in pending_commands.borrow().iter() {
            pending_world_commands.push(cmd.clone());
        }
        if let Ok(logs_tbl) = world_tbl.get::<Value>("__axiom_pending_logs") {
            if let Ok(logs) = lua.from_value::<Vec<ScriptLogEntry>>(logs_tbl) {
                for entry in logs {
                    log_buffer.push(entry);
                }
            }
        }
        engine
            .global_event_cursors
            .insert(script_name.clone(), frame.frame);

        // Drop world_tbl to release closure registry refs, then GC
        drop(world_tbl);
        lua.expire_registry_values();
        let _ = lua.gc_collect();
    }

    engine
        .global_event_cursors
        .retain(|script_name, _| used_global_cursor_keys.contains(script_name));

    let tilemap_changed = apply_script_world_commands(
        &mut commands,
        &mut tilemap,
        &mut next_network_id,
        &mut network_lookup,
        &mut errors,
        frame.frame,
        pending_world_commands,
    );
    if tilemap_changed {
        tilemap_cache.tilemap = Arc::new(tilemap.clone());
    }

    runtime
        .0
        .compiled
        .retain(|script_name, _| used_scripts.contains(script_name));

    // Force a GC cycle to reclaim temporary tables and closures from global scripts.
    let _ = runtime.0.lua.gc_collect();

    perf.script_time_ms += start.elapsed().as_secs_f32() * 1000.0;
}

fn make_lua_entity_snapshot(parts: LuaEntitySnapshotParts<'_>) -> Option<LuaEntitySnapshot> {
    let LuaEntitySnapshotParts {
        pos,
        vel,
        collider,
        network_id,
        tags,
        player,
        grounded,
        alive,
        health,
    } = parts;
    let network_id = network_id?;
    let tags_set = tags.map(|t| t.0.clone()).unwrap_or_default();
    let aabb = collider.map(|c| {
        (
            Vec2::new(pos.x - c.width * 0.5, pos.y - c.height * 0.5),
            Vec2::new(pos.x + c.width * 0.5, pos.y + c.height * 0.5),
        )
    });
    Some(LuaEntitySnapshot {
        id: network_id.0,
        x: pos.x,
        y: pos.y,
        vx: vel.map_or(0.0, |v| v.x),
        vy: vel.map_or(0.0, |v| v.y),
        grounded: grounded.is_some_and(|g| g.0),
        alive: alive.is_none_or(|a| a.0),
        is_player: player.is_some() || tags_set.contains("player"),
        aabb,
        tags: tags_set,
        health: health.map(|h| h.current),
        max_health: health.map(|h| h.max),
    })
}

fn normalize_component_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn component_from_name(name: &str, config: &GameConfig) -> Option<ComponentDef> {
    match normalize_component_name(name).as_str() {
        "collider" => Some(ComponentDef::Collider {
            width: 12.0,
            height: 14.0,
        }),
        "gravitybody" => Some(ComponentDef::GravityBody),
        "horizontalmover" => Some(ComponentDef::HorizontalMover {
            speed: config.move_speed,
            left_action: "left".to_string(),
            right_action: "right".to_string(),
        }),
        "jumper" => Some(ComponentDef::Jumper {
            velocity: config.jump_velocity,
            action: "jump".to_string(),
            fall_multiplier: config.fall_multiplier,
            variable_height: true,
            coyote_frames: config.coyote_frames,
            buffer_frames: config.jump_buffer_frames,
        }),
        "topdownmover" => Some(ComponentDef::TopDownMover {
            speed: config.move_speed,
            up_action: "up".to_string(),
            down_action: "down".to_string(),
            left_action: "left".to_string(),
            right_action: "right".to_string(),
        }),
        "health" => Some(ComponentDef::Health {
            current: 1.0,
            max: 1.0,
        }),
        "contactdamage" => Some(ComponentDef::ContactDamage {
            amount: 1.0,
            cooldown_frames: 12,
            knockback: 0.0,
            damage_tag: "player".to_string(),
        }),
        "triggerzone" => Some(ComponentDef::TriggerZone {
            radius: 16.0,
            trigger_tag: "player".to_string(),
            event_name: "trigger_enter".to_string(),
            one_shot: false,
        }),
        "pickup" => Some(ComponentDef::Pickup {
            pickup_tag: "player".to_string(),
            effect: PickupEffectDef::Heal { amount: 1.0 },
        }),
        "projectile" => Some(ComponentDef::Projectile {
            speed: 240.0,
            direction: Vec2Def { x: 1.0, y: 0.0 },
            lifetime_frames: 60,
            damage: 1.0,
            owner_id: 0,
            damage_tag: "player".to_string(),
        }),
        "hitbox" => Some(ComponentDef::Hitbox {
            width: 12.0,
            height: 12.0,
            offset: Vec2Def { x: 0.0, y: 0.0 },
            active: true,
            damage: 1.0,
            damage_tag: "player".to_string(),
        }),
        "movingplatform" => Some(ComponentDef::MovingPlatform {
            waypoints: vec![Vec2Def { x: 0.0, y: 0.0 }, Vec2Def { x: 32.0, y: 0.0 }],
            speed: 80.0,
            loop_mode: crate::api::types::PlatformLoopModeDef::PingPong,
            pause_frames: 0,
            carry_riders: true,
            current_waypoint: 0,
            direction: 1,
        }),
        "animationcontroller" | "animation" => Some(ComponentDef::AnimationController {
            graph: "basic_actor".to_string(),
            state: "idle".to_string(),
            frame: 0,
            timer: 0.0,
            speed: 1.0,
            playing: true,
            facing_right: true,
            auto_from_velocity: true,
            facing_direction: 5,
        }),
        "pathfollower" => Some(ComponentDef::PathFollower {
            target: Vec2Def { x: 0.0, y: 0.0 },
            recalculate_interval: 20,
            path_type: PathTypeDef::TopDown,
            speed: config.move_speed,
        }),
        "aibehavior" | "ai" => Some(ComponentDef::AiBehavior {
            behavior: AiBehaviorDef::Wander {
                speed: config.move_speed,
                radius: 48.0,
                pause_frames: 30,
            },
        }),
        _ => None,
    }
}

fn upsert_health_component(components: &mut Vec<ComponentDef>, current: f32, max: f32) {
    if let Some(ComponentDef::Health {
        current: existing_current,
        max: existing_max,
    }) = components
        .iter_mut()
        .find(|c| matches!(c, ComponentDef::Health { .. }))
    {
        *existing_current = current;
        *existing_max = max;
    } else {
        components.push(ComponentDef::Health { current, max });
    }
}

fn ai_state_label(state: &AiState) -> &'static str {
    match state {
        AiState::Idle => "idle",
        AiState::Patrolling { .. } => "patrolling",
        AiState::Chasing { .. } => "chasing",
        AiState::Fleeing { .. } => "fleeing",
        AiState::Attacking { .. } => "attacking",
        AiState::Returning => "returning",
        AiState::Wandering { .. } => "wandering",
    }
}

fn ai_state_target_id(state: &AiState) -> Option<u64> {
    match state {
        AiState::Chasing { target_id } | AiState::Attacking { target_id } => Some(*target_id),
        AiState::Fleeing { threat_id } => Some(*threat_id),
        _ => None,
    }
}

fn lua_spawn_request(
    lua: &Lua,
    spec: mlua::Table,
    config: &GameConfig,
) -> mlua::Result<EntitySpawnRequest> {
    let x = spec.get::<Option<f32>>("x")?.unwrap_or(0.0);
    let y = spec.get::<Option<f32>>("y")?.unwrap_or(0.0);
    let script = spec.get::<Option<String>>("script")?;
    let is_player = spec.get::<Option<bool>>("is_player")?.unwrap_or(false);

    let mut tags = Vec::<String>::new();
    if let Ok(tag_tbl) = spec.get::<mlua::Table>("tags") {
        for tag in tag_tbl.sequence_values::<String>() {
            let tag = tag?;
            let trimmed = tag.trim();
            if !trimmed.is_empty() {
                tags.push(trimmed.to_string());
            }
        }
    }

    let mut components = Vec::<ComponentDef>::new();
    if let Ok(comp_tbl) = spec.get::<mlua::Table>("components") {
        for item in comp_tbl.sequence_values::<Value>() {
            match item? {
                Value::String(s) => {
                    let name = s.to_str()?.to_string();
                    let Some(component) = component_from_name(&name, config) else {
                        return Err(mlua::Error::RuntimeError(format!(
                            "Unsupported component name in world.spawn(): {name}"
                        )));
                    };
                    components.push(component);
                }
                Value::Table(t) => {
                    let json: serde_json::Value = lua.from_value(Value::Table(t))?;
                    let parsed: ComponentDef = serde_json::from_value(json).map_err(|e| {
                        mlua::Error::RuntimeError(format!(
                            "Invalid component table in world.spawn(): {e}"
                        ))
                    })?;
                    components.push(parsed);
                }
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "components entries must be strings or tables".to_string(),
                    ));
                }
            }
        }
    }

    if let Ok(current) = spec.get::<f32>("health") {
        let max = spec.get::<Option<f32>>("max_health")?.unwrap_or(current);
        upsert_health_component(&mut components, current, max);
    }

    Ok(EntitySpawnRequest {
        x,
        y,
        components,
        script,
        tags,
        is_player,
    })
}

fn apply_script_world_commands(
    commands: &mut Commands,
    tilemap: &mut Tilemap,
    next_network_id: &mut NextNetworkId,
    network_lookup: &mut HashMap<u64, Entity>,
    errors: &mut ScriptErrors,
    frame: u64,
    world_commands: impl IntoIterator<Item = ScriptWorldCommand>,
) -> bool {
    let mut tilemap_changed = false;
    for cmd in world_commands {
        match cmd {
            ScriptWorldCommand::Spawn {
                request,
                script_name: _script_name,
                source_entity: _source_entity,
            } => {
                let assigned_id = next_network_id.0;
                let entity = crate::spawn::spawn_entity(commands, &request, next_network_id);
                network_lookup.insert(assigned_id, entity);
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
                            "world.despawn() failed: entity id {} not found",
                            target_id
                        ),
                        frame,
                    });
                }
            }
            ScriptWorldCommand::SetTile {
                x,
                y,
                tile_id,
                script_name,
                source_entity,
            } => {
                if x < 0 || y < 0 || x >= tilemap.width as i32 || y >= tilemap.height as i32 {
                    errors.push(ScriptError {
                        script_name,
                        entity_id: source_entity,
                        error_message: format!("world.set_tile() out of bounds: ({x}, {y})"),
                        frame,
                    });
                    continue;
                }
                tilemap.set(x, y, crate::components::TileType::from_u8(tile_id));
                tilemap_changed = true;
            }
            ScriptWorldCommand::SpawnParticles {
                preset,
                x,
                y,
                angle,
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

                    // Explicit one-shot effect emit; spawned particles do not remain as persistent emitters.
                    emitter.one_shot = true;
                    emitter.enabled = true;
                    emitter.timer = 0.0;
                    emitter.fired_once = false;
                    emitter.burst_count = emitter.burst_count.max(1);
                    if let Some(deg) = angle {
                        emitter.base_angle = deg;
                    }

                    world.spawn((
                        GamePosition { x, y },
                        emitter,
                        crate::particles::TransientParticleEmitter,
                    ));
                });
            }
            ScriptWorldCommand::DamageEntity { target_id, amount } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut health) = world.get_mut::<Health>(entity) {
                            health.current = (health.current - amount.max(0.0)).max(0.0);
                            let dead = health.current <= 0.0;
                            drop(health);
                            if dead {
                                if let Some(mut alive) = world.get_mut::<Alive>(entity) {
                                    alive.0 = false;
                                }
                            }
                        }
                    });
                }
            }
            ScriptWorldCommand::HealEntity { target_id, amount } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut health) = world.get_mut::<Health>(entity) {
                            health.current =
                                (health.current + amount.max(0.0)).min(health.max.max(0.0));
                        }
                    });
                }
            }
            ScriptWorldCommand::KnockbackEntity { target_id, dx, dy } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut vel) = world.get_mut::<Velocity>(entity) {
                            vel.x += dx;
                            vel.y += dy;
                        }
                    });
                }
            }
            ScriptWorldCommand::SetPosition { target_id, x, y } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut pos) = world.get_mut::<GamePosition>(entity) {
                            pos.x = x;
                            pos.y = y;
                        }
                    });
                }
            }
            ScriptWorldCommand::SetVelocity { target_id, vx, vy } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut vel) = world.get_mut::<Velocity>(entity) {
                            vel.x = vx;
                            vel.y = vy;
                        }
                    });
                }
            }
            ScriptWorldCommand::SetAlive { target_id, alive } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut a) = world.get_mut::<Alive>(entity) {
                            a.0 = alive;
                        }
                    });
                }
            }
            ScriptWorldCommand::TweenEntity {
                target_id,
                property,
                to,
                from,
                duration,
                easing,
                tween_id,
            } => {
                let req = crate::api::types::TweenRequest {
                    property,
                    to,
                    from,
                    duration,
                    easing,
                    tween_id,
                };
                commands.queue(move |world: &mut World| {
                    let _ = crate::tween::apply_tween_command(world, target_id, req);
                });
            }
            ScriptWorldCommand::TweenSequence {
                target_id,
                steps,
                sequence_id,
            } => {
                commands.queue(move |world: &mut World| {
                    let _ = crate::tween::apply_tween_sequence_command(
                        world, target_id, steps, sequence_id,
                    );
                });
            }
            ScriptWorldCommand::ScreenEffect {
                effect,
                duration,
                color,
            } => {
                let req = crate::api::types::ScreenEffectRequest {
                    effect,
                    duration,
                    color,
                    alpha: None,
                };
                commands.queue(move |world: &mut World| {
                    let _ = crate::screen_effects::trigger_effect_command(world, req);
                });
            }
            ScriptWorldCommand::SetAmbient { intensity, color } => {
                let req = crate::api::types::LightingConfigRequest {
                    enabled: Some(true),
                    ambient_intensity: Some(intensity),
                    ambient_color: color,
                };
                commands.queue(move |world: &mut World| {
                    let _ = crate::lighting::apply_lighting_config(world, req);
                });
            }
            ScriptWorldCommand::SpawnText {
                x, y, text, font_size, color, duration, fade, rise_speed, owner_id,
            } => {
                commands.queue(move |world: &mut World| {
                    let mut counter = world.resource_mut::<crate::world_text::WorldTextIdCounter>();
                    let text_id = counter.0;
                    counter.0 += 1;
                    world.spawn((
                        crate::world_text::WorldText {
                            text_id,
                            text,
                            font_size,
                            color,
                            offset: Vec2::ZERO,
                            owner_entity: owner_id,
                            duration,
                            elapsed: 0.0,
                            fade,
                            rise_speed,
                        },
                        Transform::from_xyz(x, y, 100.0),
                    ));
                });
            }
            ScriptWorldCommand::SetWeather { weather_type, intensity, wind } => {
                commands.queue(move |world: &mut World| {
                    let wtype = match weather_type.to_lowercase().as_str() {
                        "rain" => crate::weather::WeatherType::Rain,
                        "snow" => crate::weather::WeatherType::Snow,
                        "dust" => crate::weather::WeatherType::Dust,
                        _ => crate::weather::WeatherType::Rain,
                    };
                    let mut ws = world.resource_mut::<crate::weather::WeatherSystem>();
                    ws.active = Some(crate::weather::WeatherConfig {
                        weather_type: wtype,
                        intensity,
                        wind,
                    });
                });
            }
            ScriptWorldCommand::ClearWeather => {
                commands.queue(move |world: &mut World| {
                    let mut ws = world.resource_mut::<crate::weather::WeatherSystem>();
                    ws.active = None;
                });
            }
            ScriptWorldCommand::SetParallax { layers } => {
                commands.queue(move |world: &mut World| {
                    let mut config = world.resource_mut::<crate::parallax::ParallaxConfig>();
                    config.layers = layers;
                });
            }
            ScriptWorldCommand::PlayCutscene { name } => {
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut manager: Mut<crate::cutscene::CutsceneManager>| {
                        world.resource_scope(|world, mut events: Mut<GameEventBus>| {
                            world.resource_scope(|_world, mut runtime_state: Mut<crate::game_runtime::RuntimeState>| {
                                let _ = crate::cutscene::play_cutscene(
                                    &mut manager, &mut events, &mut runtime_state, &name,
                                );
                            });
                        });
                    });
                });
            }
            ScriptWorldCommand::StopCutscene => {
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut manager: Mut<crate::cutscene::CutsceneManager>| {
                        world.resource_scope(|world, mut events: Mut<GameEventBus>| {
                            world.resource_scope(|_world, mut runtime_state: Mut<crate::game_runtime::RuntimeState>| {
                                crate::cutscene::stop_cutscene(
                                    &mut manager, &mut events, &mut runtime_state,
                                );
                            });
                        });
                    });
                });
            }
            ScriptWorldCommand::SetTimeOfDay { hour } => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut cycle) = world.get_resource_mut::<crate::lighting::DayNightCycle>() {
                        cycle.time_of_day = hour.rem_euclid(24.0);
                    }
                });
            }
            ScriptWorldCommand::TransitionEntityState { target_id, state } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut sm) = world.get_mut::<crate::state_machine::EntityStateMachine>(entity) {
                            let prev = sm.current.clone();
                            // Validate transition if allowed_transitions is non-empty
                            if let Some(config) = sm.states.get(&sm.current) {
                                if !config.allowed_transitions.is_empty()
                                    && !config.allowed_transitions.contains(&state)
                                {
                                    return; // Invalid transition
                                }
                            }
                            sm.previous = Some(prev);
                            sm.current = state;
                        }
                    });
                }
            }
            ScriptWorldCommand::InventoryAdd { target_id, item_id, count } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        let registry = world.resource::<crate::inventory::ItemRegistry>().clone();
                        if let Some(mut inv) = world.get_mut::<crate::inventory::Inventory>(entity) {
                            let added = inv.add_item(&item_id, count, &registry);
                            if added > 0 {
                                let new_total = inv.count_item(&item_id);
                                drop(inv);
                                if let Some(mut bus) = world.get_resource_mut::<GameEventBus>() {
                                    bus.emit(
                                        "item_added",
                                        serde_json::json!({
                                            "entity": target_id,
                                            "item_id": item_id,
                                            "count": added,
                                            "new_total": new_total,
                                        }),
                                        None,
                                    );
                                }
                            }
                        }
                    });
                }
            }
            ScriptWorldCommand::InventoryRemove { target_id, item_id, count } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        let registry = world.resource::<crate::inventory::ItemRegistry>().clone();
                        drop(registry);
                        if let Some(mut inv) = world.get_mut::<crate::inventory::Inventory>(entity) {
                            let removed = inv.remove_item(&item_id, count);
                            if removed > 0 {
                                let new_total = inv.count_item(&item_id);
                                drop(inv);
                                if let Some(mut bus) = world.get_resource_mut::<GameEventBus>() {
                                    bus.emit(
                                        "item_removed",
                                        serde_json::json!({
                                            "entity": target_id,
                                            "item_id": item_id,
                                            "count": removed,
                                            "new_total": new_total,
                                        }),
                                        None,
                                    );
                                }
                            }
                        }
                    });
                }
            }
            ScriptWorldCommand::SetTint { target_id, color } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut tint) = world.get_mut::<crate::components::SpriteColorTint>(entity) {
                            tint.color = color;
                        } else {
                            world.entity_mut(entity).insert(crate::components::SpriteColorTint {
                                color,
                                flash_color: None,
                                flash_frames: 0,
                            });
                        }
                    });
                }
            }
            ScriptWorldCommand::FlashTint { target_id, color, frames } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut tint) = world.get_mut::<crate::components::SpriteColorTint>(entity) {
                            tint.flash_color = Some(color);
                            tint.flash_frames = frames;
                        } else {
                            world.entity_mut(entity).insert(crate::components::SpriteColorTint {
                                color: [1.0, 1.0, 1.0, 1.0],
                                flash_color: Some(color),
                                flash_frames: frames,
                            });
                        }
                    });
                }
            }
            ScriptWorldCommand::SetTrail { target_id, interval, duration, alpha_start, alpha_end } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut trail) = world.get_mut::<crate::trail::TrailEffect>(entity) {
                            trail.interval = interval;
                            trail.duration = duration;
                            trail.alpha_start = alpha_start;
                            trail.alpha_end = alpha_end;
                        } else {
                            world.entity_mut(entity).insert(crate::trail::TrailEffect {
                                interval,
                                duration,
                                alpha_start,
                                alpha_end,
                                frame_counter: 0,
                            });
                        }
                    });
                }
            }
            ScriptWorldCommand::ClearTrail { target_id } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        world.entity_mut(entity).remove::<crate::trail::TrailEffect>();
                    });
                }
            }
            ScriptWorldCommand::SetCollisionLayer { target_id, layer, mask } => {
                if let Some(&entity) = network_lookup.get(&target_id) {
                    commands.queue(move |world: &mut World| {
                        if let Some(mut cl) = world.get_mut::<crate::components::CollisionLayer>(entity) {
                            cl.layer = layer;
                            cl.mask = mask;
                        } else {
                            world.entity_mut(entity).insert(crate::components::CollisionLayer {
                                layer,
                                mask,
                            });
                        }
                    });
                }
            }
            ScriptWorldCommand::RebindInput { key, action } => {
                commands.queue(move |world: &mut World| {
                    if let Some(mut bindings) = world.get_resource_mut::<crate::input::InputBindings>() {
                        bindings.keyboard
                            .entry(key)
                            .or_insert_with(Vec::new)
                            .push(action);
                    }
                });
            }
        }
    }
    tilemap_changed
}

fn entity_matches_tag(entity: &LuaEntitySnapshot, tag: Option<&str>) -> bool {
    match tag {
        Some(t) if !t.is_empty() => entity.tags.contains(t),
        _ => true,
    }
}

/// Install a shared metatable and dispatch function for entity snapshots.
/// Called once per Lua runtime. The metatable routes method calls through
/// a single `__dispatch` closure stored on each table, reducing registry refs
/// from ~9 per entity to ~1.
fn install_entity_snapshot_metatable(lua: &Lua) -> mlua::Result<()> {
    // Shared metatables installed once per Lua runtime.
    // Entity snapshot methods route through a single __dispatch closure.
    // Entity script methods are pure Lua (no Rust closures needed).
    lua.load(r##"
        -- Entity snapshot metatable (read-only, methods via dispatch)
        -- __index is a FUNCTION so dot syntax works: snapshot.set_alive(false)
        -- When __index is a table, dot calls pass the first arg as self which breaks dispatch.
        __axiom_snap_mt = {
            __newindex = function(_, key, _)
                error("Cannot set '" .. tostring(key) .. "' on entity snapshot. Use methods like heal()/damage()/set_position()/set_velocity()/set_alive().")
            end,
            __index = function(tbl, key)
                local v = rawget(tbl, key)
                if v ~= nil then return v end

                if key == "has_tag" then
                    return function(tag_or_self, tag)
                        local t = tag
                        if type(tag_or_self) == "string" then t = tag_or_self end
                        local tags = rawget(tbl, "tags")
                        if tags and type(t) == "string" then return tags[t] == true end
                        return false
                    end
                end

                local dispatch = rawget(tbl, "__dispatch")
                if not dispatch then return nil end
                if key == "damage" or key == "heal" or key == "knockback"
                   or key == "set_position" or key == "set_velocity" or key == "set_alive" then
                    return function(first, ...)
                        if first == tbl then
                            return dispatch(key, tbl, ...)
                        else
                            return dispatch(key, tbl, first, ...)
                        end
                    end
                end
                return nil
            end,
        }

        -- Entity script setup: installs methods directly on entity table as Lua closures.
        -- These capture `entity` via Lua upvalues (NOT Rust Table handles), so Lua GC
        -- can collect the cycle. Supports both dot and colon syntax.
        local function _get_num(...)
            for i = 1, select("#", ...) do
                local v = select(i, ...)
                if type(v) == "number" then return v end
            end
            return 0
        end
        local function _get_num2(...)
            local a, b = nil, nil
            for i = 1, select("#", ...) do
                local v = select(i, ...)
                if type(v) == "number" then
                    if a == nil then a = v else b = v; break end
                end
            end
            return a or 0, b or 0
        end
        function __axiom_setup_entity_methods(entity)
            rawset(entity, "has_tag", function(tag_or_self, tag)
                local t = tag
                if type(tag_or_self) == "string" then t = tag_or_self end
                local tags = rawget(entity, "tags")
                if tags and type(t) == "string" then return tags[t] == true end
                return false
            end)
            rawset(entity, "add_tag", function(tag_or_self, tag)
                local t = tag
                if type(tag_or_self) == "string" then t = tag_or_self end
                local tags = rawget(entity, "tags")
                if tags and type(t) == "string" then tags[t] = true end
            end)
            rawset(entity, "remove_tag", function(tag_or_self, tag)
                local t = tag
                if type(tag_or_self) == "string" then t = tag_or_self end
                local tags = rawget(entity, "tags")
                if tags and type(t) == "string" then tags[t] = nil end
            end)
            rawset(entity, "damage", function(...)
                local amount = math.max(_get_num(...), 0)
                local current = rawget(entity, "health") or 0
                local nxt = math.max(current - amount, 0)
                rawset(entity, "health", nxt)
                if nxt <= 0 then rawset(entity, "alive", false) end
                return nxt
            end)
            rawset(entity, "heal", function(...)
                local amount = math.max(_get_num(...), 0)
                local current = rawget(entity, "health") or 0
                local max = rawget(entity, "max_health") or (current + amount)
                local nxt = math.min(current + amount, math.max(max, 0))
                rawset(entity, "health", nxt)
                return nxt
            end)
            rawset(entity, "knockback", function(...)
                local dx, dy = _get_num2(...)
                rawset(entity, "vx", (rawget(entity, "vx") or 0) + dx)
                rawset(entity, "vy", (rawget(entity, "vy") or 0) + dy)
            end)
            rawset(entity, "follow_path", function(path_or_self, path_or_speed, speed)
                local p, s
                if type(path_or_self) == "table" and path_or_self ~= entity then
                    p = path_or_self; s = path_or_speed
                else
                    p = path_or_speed; s = speed
                end
                rawset(entity, "__axiom_follow_path", p)
                if s then rawset(entity, "__axiom_follow_speed", math.max(s, 0)) end
            end)
            -- Flash tint: entity.flash({r,g,b,a}, frames)
            rawset(entity, "flash", function(first, ...)
                local color, frames
                if type(first) == "table" and first ~= entity then
                    color = first; frames = select(1, ...)
                else
                    color = select(1, ...); frames = select(2, ...)
                end
                if type(color) ~= "table" then return end
                rawset(entity, "__axiom_flash", {
                    r = color[1] or 1, g = color[2] or 1, b = color[3] or 1, a = color[4] or 1,
                    frames = frames or 6,
                })
            end)
            -- Transition entity state machine: entity.transition_state("attacking")
            rawset(entity, "transition_state", function(first, second)
                local st = first
                if type(first) ~= "string" and type(second) == "string" then st = second end
                if type(st) == "string" then
                    rawset(entity, "__axiom_transition_state", st)
                end
            end)
            -- Inventory: entity.add_item("id", count), entity.remove_item("id", count)
            rawset(entity, "add_item", function(first, second, third)
                local id, count
                if type(first) == "string" then id = first; count = second
                else id = second; count = third end
                local cmds = rawget(entity, "__axiom_inv_cmds") or {}
                cmds[#cmds + 1] = { action = "add", item_id = id, count = count or 1 }
                rawset(entity, "__axiom_inv_cmds", cmds)
            end)
            rawset(entity, "remove_item", function(first, second, third)
                local id, count
                if type(first) == "string" then id = first; count = second
                else id = second; count = third end
                local cmds = rawget(entity, "__axiom_inv_cmds") or {}
                cmds[#cmds + 1] = { action = "remove", item_id = id, count = count or 1 }
                rawset(entity, "__axiom_inv_cmds", cmds)
            end)
            -- Inventory read: entity.has_item("id"), entity.count_item("id")
            rawset(entity, "has_item", function(first, second)
                local id = first
                if type(first) ~= "string" then id = second end
                local inv = rawget(entity, "__axiom_inventory")
                if not inv then return false end
                for _, slot in ipairs(inv) do
                    if slot.item_id == id and slot.count > 0 then return true end
                end
                return false
            end)
            rawset(entity, "count_item", function(first, second)
                local id = first
                if type(first) ~= "string" then id = second end
                local inv = rawget(entity, "__axiom_inventory")
                if not inv then return 0 end
                local total = 0
                for _, slot in ipairs(inv) do
                    if slot.item_id == id then total = total + slot.count end
                end
                return total
            end)
        end
    "##).exec()?;
    Ok(())
}

fn lua_entity_table(
    lua: &Lua,
    entity: &LuaEntitySnapshot,
    pending_commands: &Rc<RefCell<Vec<ScriptWorldCommand>>>,
) -> mlua::Result<mlua::Table> {
    let tbl = lua.create_table()?;
    tbl.raw_set("id", entity.id)?;
    tbl.raw_set("x", entity.x)?;
    tbl.raw_set("y", entity.y)?;
    tbl.raw_set("vx", entity.vx)?;
    tbl.raw_set("vy", entity.vy)?;
    tbl.raw_set("grounded", entity.grounded)?;
    tbl.raw_set("alive", entity.alive)?;
    if let Some(h) = entity.health {
        tbl.raw_set("health", h)?;
    }
    if let Some(mh) = entity.max_health {
        tbl.raw_set("max_health", mh)?;
    }

    let tags_tbl = lua.create_table()?;
    for tag in &entity.tags {
        tags_tbl.raw_set(tag.as_str(), true)?;
    }
    tbl.raw_set("tags", tags_tbl)?;

    // Single dispatch closure — handles ALL method calls for this entity
    let entity_id = entity.id;
    let dispatch_tbl = tbl.clone();
    let cmds = pending_commands.clone();
    let dispatch = lua.create_function_mut(move |_lua, args: mlua::MultiValue| -> mlua::Result<Value> {
        // First arg is method name (String), rest are method args
        let args_vec: Vec<Value> = args.into_iter().collect();
        let method = match args_vec.first() {
            Some(Value::String(s)) => s.to_string_lossy().to_string(),
            _ => return Ok(Value::Nil),
        };
        // Skip method name and self table to get actual args
        let num_args: Vec<f32> = args_vec.iter().skip(2).filter_map(|v| match v {
            Value::Integer(n) => Some(*n as f32),
            Value::Number(n) => Some(*n as f32),
            _ => None,
        }).collect();

        match method.as_str() {
            "damage" => {
                let amount = num_args.first().copied().unwrap_or(0.0).max(0.0);
                let current = dispatch_tbl.raw_get::<f32>("health").unwrap_or(0.0);
                let next = (current - amount).max(0.0);
                dispatch_tbl.raw_set("health", next)?;
                if next <= 0.0 {
                    dispatch_tbl.raw_set("alive", false)?;
                }
                cmds.borrow_mut().push(ScriptWorldCommand::DamageEntity {
                    target_id: entity_id,
                    amount,
                });
                Ok(Value::Number(next as f64))
            }
            "heal" => {
                let amount = num_args.first().copied().unwrap_or(0.0).max(0.0);
                let current = dispatch_tbl.raw_get::<f32>("health").unwrap_or(0.0);
                let max = dispatch_tbl.raw_get::<f32>("max_health").unwrap_or(current + amount);
                let next = (current + amount).min(max.max(0.0));
                dispatch_tbl.raw_set("health", next)?;
                cmds.borrow_mut().push(ScriptWorldCommand::HealEntity {
                    target_id: entity_id,
                    amount,
                });
                Ok(Value::Number(next as f64))
            }
            "knockback" => {
                let dx = num_args.first().copied().unwrap_or(0.0);
                let dy = num_args.get(1).copied().unwrap_or(0.0);
                let vx = dispatch_tbl.raw_get::<f32>("vx").unwrap_or(0.0) + dx;
                let vy = dispatch_tbl.raw_get::<f32>("vy").unwrap_or(0.0) + dy;
                dispatch_tbl.raw_set("vx", vx)?;
                dispatch_tbl.raw_set("vy", vy)?;
                cmds.borrow_mut().push(ScriptWorldCommand::KnockbackEntity {
                    target_id: entity_id,
                    dx,
                    dy,
                });
                Ok(Value::Nil)
            }
            "set_position" => {
                let x = num_args.first().copied().unwrap_or(0.0);
                let y = num_args.get(1).copied().unwrap_or(0.0);
                dispatch_tbl.raw_set("x", x)?;
                dispatch_tbl.raw_set("y", y)?;
                cmds.borrow_mut().push(ScriptWorldCommand::SetPosition {
                    target_id: entity_id,
                    x,
                    y,
                });
                Ok(Value::Nil)
            }
            "set_velocity" => {
                let vx = num_args.first().copied().unwrap_or(0.0);
                let vy = num_args.get(1).copied().unwrap_or(0.0);
                dispatch_tbl.raw_set("vx", vx)?;
                dispatch_tbl.raw_set("vy", vy)?;
                cmds.borrow_mut().push(ScriptWorldCommand::SetVelocity {
                    target_id: entity_id,
                    vx,
                    vy,
                });
                Ok(Value::Nil)
            }
            "set_alive" => {
                let alive = match args_vec.get(2) {
                    Some(Value::Boolean(b)) => *b,
                    _ => true,
                };
                dispatch_tbl.raw_set("alive", alive)?;
                cmds.borrow_mut().push(ScriptWorldCommand::SetAlive {
                    target_id: entity_id,
                    alive,
                });
                Ok(Value::Nil)
            }
            _ => Ok(Value::Nil),
        }
    })?;
    tbl.raw_set("__dispatch", dispatch)?;

    // Apply shared metatable (installed once at runtime init)
    let mt: mlua::Table = lua.globals().raw_get("__axiom_snap_mt")?;
    tbl.set_metatable(Some(mt));

    Ok(tbl)
}

pub fn dry_run_script(source: &str) -> Result<(), String> {
    let lua = Lua::new();
    lua.load(source)
        .set_name("test_script")
        .exec()
        .map_err(|e| e.to_string())?;
    let globals = lua.globals();
    let update: mlua::Function = globals.get("update").map_err(|e| e.to_string())?;

    let entity_tbl = lua.create_table().map_err(|e| e.to_string())?;
    entity_tbl.set("id", 999u64).map_err(|e| e.to_string())?;
    entity_tbl.set("x", 0.0f32).map_err(|e| e.to_string())?;
    entity_tbl.set("y", 0.0f32).map_err(|e| e.to_string())?;
    entity_tbl.set("vx", 0.0f32).map_err(|e| e.to_string())?;
    entity_tbl.set("vy", 0.0f32).map_err(|e| e.to_string())?;
    entity_tbl
        .set("grounded", true)
        .map_err(|e| e.to_string())?;
    entity_tbl.set("alive", true).map_err(|e| e.to_string())?;
    entity_tbl
        .set("health", 3.0f32)
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("max_health", 3.0f32)
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("animation", "idle")
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("animation_frame", 0usize)
        .map_err(|e| e.to_string())?;
    entity_tbl.set("flip_x", false).map_err(|e| e.to_string())?;
    entity_tbl
        .set(
            "state",
            lua.to_value(&serde_json::json!({}))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let tags_tbl = lua.create_table().map_err(|e| e.to_string())?;
    entity_tbl
        .set("tags", tags_tbl.clone())
        .map_err(|e| e.to_string())?;
    let has_tag_tbl = tags_tbl.clone();
    let has_tag_fn = lua
        .create_function(move |_lua, tag: String| Ok(has_tag_tbl.get::<bool>(tag).unwrap_or(false)))
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("has_tag", has_tag_fn)
        .map_err(|e| e.to_string())?;
    let add_tag_tbl = tags_tbl.clone();
    let add_tag_fn = lua
        .create_function_mut(move |_lua, tag: String| {
            add_tag_tbl.set(tag, true)?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("add_tag", add_tag_fn)
        .map_err(|e| e.to_string())?;
    let remove_tag_tbl = tags_tbl.clone();
    let remove_tag_fn = lua
        .create_function_mut(move |_lua, tag: String| {
            remove_tag_tbl.set(tag, Value::Nil)?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("remove_tag", remove_tag_fn)
        .map_err(|e| e.to_string())?;
    let damage_tbl = entity_tbl.clone();
    let damage_fn = lua
        .create_function_mut(move |_lua, amount: f32| {
            let amount = amount.max(0.0);
            let current = damage_tbl.get::<f32>("health").unwrap_or(0.0);
            let next = (current - amount).max(0.0);
            damage_tbl.set("health", next)?;
            if next <= 0.0 {
                damage_tbl.set("alive", false)?;
            }
            Ok(next)
        })
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("damage", damage_fn)
        .map_err(|e| e.to_string())?;
    let heal_tbl = entity_tbl.clone();
    let heal_fn = lua
        .create_function_mut(move |_lua, amount: f32| {
            let amount = amount.max(0.0);
            let current = heal_tbl.get::<f32>("health").unwrap_or(0.0);
            let max = heal_tbl
                .get::<f32>("max_health")
                .unwrap_or(current + amount);
            let next = (current + amount).min(max.max(0.0));
            heal_tbl.set("health", next)?;
            Ok(next)
        })
        .map_err(|e| e.to_string())?;
    entity_tbl.set("heal", heal_fn).map_err(|e| e.to_string())?;
    let knock_tbl = entity_tbl.clone();
    let knockback_fn = lua
        .create_function_mut(move |_lua, (dx, dy): (f32, f32)| {
            let vx = knock_tbl.get::<f32>("vx").unwrap_or(0.0) + dx;
            let vy = knock_tbl.get::<f32>("vy").unwrap_or(0.0) + dy;
            knock_tbl.set("vx", vx)?;
            knock_tbl.set("vy", vy)?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("knockback", knockback_fn)
        .map_err(|e| e.to_string())?;
    let hitbox_tbl = lua.create_table().map_err(|e| e.to_string())?;
    hitbox_tbl
        .set("width", 12.0f32)
        .map_err(|e| e.to_string())?;
    hitbox_tbl
        .set("height", 12.0f32)
        .map_err(|e| e.to_string())?;
    hitbox_tbl
        .set("offset_x", 0.0f32)
        .map_err(|e| e.to_string())?;
    hitbox_tbl
        .set("offset_y", 0.0f32)
        .map_err(|e| e.to_string())?;
    hitbox_tbl.set("active", false).map_err(|e| e.to_string())?;
    hitbox_tbl
        .set("damage", 1.0f32)
        .map_err(|e| e.to_string())?;
    hitbox_tbl
        .set("damage_tag", "player")
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("hitbox", hitbox_tbl)
        .map_err(|e| e.to_string())?;
    let follow_path_tbl = entity_tbl.clone();
    let follow_path_fn = lua
        .create_function_mut(move |_lua, (path, speed): (mlua::Table, Option<f32>)| {
            follow_path_tbl.set("__axiom_follow_path", path)?;
            if let Some(speed) = speed {
                follow_path_tbl.set("__axiom_follow_speed", speed)?;
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    entity_tbl
        .set("follow_path", follow_path_fn)
        .map_err(|e| e.to_string())?;
    let ai_tbl = lua.create_table().map_err(|e| e.to_string())?;
    ai_tbl.set("state", "idle").map_err(|e| e.to_string())?;
    let ai_cmd_tbl = entity_tbl.clone();
    let ai_chase_fn = lua
        .create_function_mut(move |lua_ctx, target_id: u64| {
            let cmd = lua_ctx.create_table()?;
            cmd.set("mode", "chase")?;
            cmd.set("target_id", target_id)?;
            ai_cmd_tbl.set("__axiom_ai_override", cmd)?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    ai_tbl
        .set("chase", ai_chase_fn)
        .map_err(|e| e.to_string())?;
    let ai_cmd_tbl = entity_tbl.clone();
    let ai_idle_fn = lua
        .create_function_mut(move |lua_ctx, _: Option<mlua::Table>| {
            let cmd = lua_ctx.create_table()?;
            cmd.set("mode", "idle")?;
            ai_cmd_tbl.set("__axiom_ai_override", cmd)?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    ai_tbl.set("idle", ai_idle_fn).map_err(|e| e.to_string())?;
    entity_tbl.set("ai", ai_tbl).map_err(|e| e.to_string())?;

    let vars = HashMap::new();
    let tilemap = Arc::new(Tilemap::test_level());
    let config = GameConfig::default();
    let entities = Arc::new(Vec::<LuaEntitySnapshot>::new());
    let input = Arc::new(LuaInputSnapshot::default());
    let world_events = Arc::new(Vec::<GameEvent>::new());
    let (world_tbl, _events, _commands) = build_world_table(
        &lua,
        &WorldBuildArgs {
            vars: &vars,
            script_name: "test_script",
            frame: 1,
            seconds: 1.0 / 60.0,
            dt: 1.0 / 60.0,
            source_entity: Some(999),
            tilemap: &tilemap,
            config: &config,
            entities: &entities,
            input: &input,
            world_events: &world_events,
            game_state: "Playing",
        },
    )
    .map_err(|e| e.to_string())?;

    call_lua_with_budget(
        &lua,
        Duration::from_millis(100),
        crate::scripting::DEFAULT_SCRIPT_HOOK_INSTRUCTION_INTERVAL,
        || update.call::<()>((entity_tbl, world_tbl, 1.0f32 / 60.0f32)),
    )
    .map_err(|e| e.to_string())
}

fn build_world_table(lua: &Lua, args: &WorldBuildArgs<'_>) -> WorldTableBuildResult {
    let WorldBuildArgs {
        vars,
        script_name,
        frame,
        seconds,
        dt,
        source_entity,
        tilemap,
        config,
        entities,
        input,
        world_events,
        game_state,
    } = args;
    let frame = *frame;
    let seconds = *seconds;
    let dt = *dt;
    let source_entity = *source_entity;
    let game_state = *game_state;
    let vars = *vars;
    let script_name = *script_name;
    let tilemap = *tilemap;
    let config = *config;
    let entities = *entities;
    let input = *input;
    let world_events = *world_events;
    let world_tbl = lua.create_table()?;
    let pending_events = Rc::new(RefCell::new(Vec::<ScriptEvent>::new()));
    let pending_commands = Rc::new(RefCell::new(Vec::<ScriptWorldCommand>::new()));
    world_tbl.set("frame", frame)?;
    world_tbl.set("time", seconds)?;
    world_tbl.set("dt", dt)?;
    let vars_tbl = lua.create_table()?;
    for (k, v) in vars {
        vars_tbl.set(k.as_str(), lua.to_value(v)?)?;
    }
    world_tbl.set("vars", vars_tbl.clone())?;
    let vars_get_tbl = vars_tbl.clone();
    let get_var_fn = lua.create_function(move |_lua, key: String| {
        Ok(vars_get_tbl.get::<Value>(key).unwrap_or(Value::Nil))
    })?;
    world_tbl.set("get_var", get_var_fn)?;
    let vars_set_tbl = vars_tbl.clone();
    let set_var_fn = lua.create_function_mut(move |_lua, (key, value): (String, Value)| {
        vars_set_tbl.set(key, value)?;
        Ok(())
    })?;
    world_tbl.set("set_var", set_var_fn)?;
    let input_tbl = lua.create_table()?;
    let active_actions = input.active.clone();
    let input_pressed = lua
        .create_function(move |_lua, action: String| Ok(active_actions.contains(action.trim())))?;
    input_tbl.set("pressed", input_pressed)?;
    let just_pressed_actions = input.just_pressed.clone();
    let input_just_pressed = lua.create_function(move |_lua, action: String| {
        Ok(just_pressed_actions.contains(action.trim()))
    })?;
    input_tbl.set("just_pressed", input_just_pressed)?;
    // Mouse input
    input_tbl.set("mouse_x", input.mouse_x)?;
    input_tbl.set("mouse_y", input.mouse_y)?;
    let mouse_left = input.mouse_left;
    let mouse_right = input.mouse_right;
    let mouse_middle = input.mouse_middle;
    let mouse_left_jp = input.mouse_left_just_pressed;
    let mouse_right_jp = input.mouse_right_just_pressed;
    let mouse_middle_jp = input.mouse_middle_just_pressed;
    let mouse_pressed_fn = lua.create_function(move |_lua, button: String| {
        Ok(match button.trim().to_lowercase().as_str() {
            "left" => mouse_left,
            "right" => mouse_right,
            "middle" => mouse_middle,
            _ => false,
        })
    })?;
    input_tbl.set("mouse_pressed", mouse_pressed_fn)?;
    let mouse_just_pressed_fn = lua.create_function(move |_lua, button: String| {
        Ok(match button.trim().to_lowercase().as_str() {
            "left" => mouse_left_jp,
            "right" => mouse_right_jp,
            "middle" => mouse_middle_jp,
            _ => false,
        })
    })?;
    input_tbl.set("mouse_just_pressed", mouse_just_pressed_fn)?;
    world_tbl.set("input", input_tbl)?;
    let entities_for_all = entities.clone();
    let cmds_for_all = pending_commands.clone();
    let find_all_fn = lua.create_function(move |lua_ctx, tag: Option<String>| {
        let tag = tag.as_deref().map(str::trim).filter(|t| !t.is_empty());
        let out = lua_ctx.create_table()?;
        let mut index = 1usize;
        for entity in entities_for_all
            .iter()
            .filter(|e| entity_matches_tag(e, tag))
        {
            out.set(index, lua_entity_table(lua_ctx, entity, &cmds_for_all)?)?;
            index += 1;
        }
        Ok(out)
    })?;
    world_tbl.set("find_all", find_all_fn)?;
    let entities_for_radius = entities.clone();
    let cmds_for_radius = pending_commands.clone();
    let find_in_radius_fn = lua.create_function(
        move |lua_ctx, (x, y, radius, tag): (f32, f32, f32, Option<String>)| {
            let tag = tag.as_deref().map(str::trim).filter(|t| !t.is_empty());
            let max_d2 = radius.max(0.0).powi(2);
            let out = lua_ctx.create_table()?;
            let mut index = 1usize;
            for entity in entities_for_radius
                .iter()
                .filter(|e| entity_matches_tag(e, tag))
            {
                let dx = entity.x - x;
                let dy = entity.y - y;
                if dx * dx + dy * dy <= max_d2 {
                    out.set(index, lua_entity_table(lua_ctx, entity, &cmds_for_radius)?)?;
                    index += 1;
                }
            }
            Ok(out)
        },
    )?;
    world_tbl.set("find_in_radius", find_in_radius_fn)?;
    let entities_for_nearest = entities.clone();
    let cmds_for_nearest = pending_commands.clone();
    let find_nearest_fn =
        lua.create_function(move |lua_ctx, (x, y, tag): (f32, f32, Option<String>)| {
            let tag = tag.as_deref().map(str::trim).filter(|t| !t.is_empty());
            let nearest = entities_for_nearest
                .iter()
                .filter(|e| entity_matches_tag(e, tag))
                .min_by(|a, b| {
                    let adx = a.x - x;
                    let ady = a.y - y;
                    let bdx = b.x - x;
                    let bdy = b.y - y;
                    let da = adx * adx + ady * ady;
                    let db = bdx * bdx + bdy * bdy;
                    da.total_cmp(&db)
                });
            match nearest {
                Some(entity) => Ok(Value::Table(lua_entity_table(lua_ctx, entity, &cmds_for_nearest)?)),
                None => Ok(Value::Nil),
            }
        })?;
    world_tbl.set("find_nearest", find_nearest_fn)?;
    let entities_for_get = entities.clone();
    let cmds_for_get = pending_commands.clone();
    let get_entity_fn = lua.create_function(move |lua_ctx, id: u64| {
        match entities_for_get.iter().find(|e| e.id == id) {
            Some(entity) => Ok(Value::Table(lua_entity_table(lua_ctx, entity, &cmds_for_get)?)),
            None => Ok(Value::Nil),
        }
    })?;
    world_tbl.set("get_entity", get_entity_fn)?;
    let entities_for_player = entities.clone();
    let cmds_for_player = pending_commands.clone();
    let player_fn = lua.create_function(move |lua_ctx, _: Option<mlua::Table>| {
        match entities_for_player.iter().find(|e| e.is_player) {
            Some(entity) => Ok(Value::Table(lua_entity_table(lua_ctx, entity, &cmds_for_player)?)),
            None => Ok(Value::Nil),
        }
    })?;
    world_tbl.set("player", player_fn)?;
    let tm = tilemap.clone();
    let is_solid_fn = lua.create_function(move |_lua, (x, y): (i32, i32)| Ok(tm.is_solid(x, y)))?;
    world_tbl.set("is_solid", is_solid_fn)?;
    let tm2 = tilemap.clone();
    let get_tile_fn =
        lua.create_function(move |_lua, (x, y): (i32, i32)| Ok(tm2.get(x, y) as u8))?;
    world_tbl.set("get_tile", get_tile_fn)?;
    let set_tile_commands_ref = pending_commands.clone();
    let set_tile_script_name = script_name.to_string();
    let set_tile_fn = lua.create_function_mut(move |_lua, (x, y, tile_id): (i32, i32, u8)| {
        set_tile_commands_ref
            .borrow_mut()
            .push(ScriptWorldCommand::SetTile {
                x,
                y,
                tile_id,
                script_name: set_tile_script_name.clone(),
                source_entity,
            });
        Ok(())
    })?;
    world_tbl.set("set_tile", set_tile_fn)?;
    let tm_platform = tilemap.clone();
    let is_platform_fn = lua
        .create_function(move |_lua, (x, y): (i32, i32)| Ok(tm_platform.get(x, y).is_platform()))?;
    world_tbl.set("is_platform", is_platform_fn)?;
    let tm_climb = tilemap.clone();
    let tile_types_climb = config.tile_types.clone();
    let is_climbable_fn = lua.create_function(move |_lua, (x, y): (i32, i32)| {
        Ok(tile_types_climb.is_climbable(tm_climb.get_tile(x, y)))
    })?;
    world_tbl.set("is_climbable", is_climbable_fn)?;
    let tm_friction = tilemap.clone();
    let tile_types_friction = config.tile_types.clone();
    let tile_friction_fn = lua.create_function(move |_lua, (x, y): (i32, i32)| {
        Ok(tile_types_friction.friction(tm_friction.get_tile(x, y)))
    })?;
    world_tbl.set("tile_friction", tile_friction_fn)?;
    let tile_size = config.tile_size;
    let tm3 = tilemap.clone();
    let raycast_fn = lua.create_function(
        move |lua_ctx, (ox, oy, dx, dy, max_dist): (f32, f32, f32, f32, f32)| {
            let len = (dx * dx + dy * dy).sqrt();
            if len <= 0.0001 {
                return Ok(Value::Nil);
            }
            let ndx = dx / len;
            let ndy = dy / len;
            let mut d = 0.0f32;
            let mut prev_tx = (ox / tile_size).floor() as i32;
            let mut prev_ty = (oy / tile_size).floor() as i32;
            while d <= max_dist.max(0.0) {
                let x = ox + ndx * d;
                let y = oy + ndy * d;
                let tx = (x / tile_size).floor() as i32;
                let ty = (y / tile_size).floor() as i32;
                if tm3.is_solid(tx, ty) {
                    let hit = lua_ctx.create_table()?;
                    hit.set("x", x)?;
                    hit.set("y", y)?;
                    hit.set("tile_x", tx)?;
                    hit.set("tile_y", ty)?;
                    hit.set("distance", d)?;
                    hit.set("normal_x", (prev_tx - tx) as f32)?;
                    hit.set("normal_y", (prev_ty - ty) as f32)?;
                    return Ok(Value::Table(hit));
                }
                prev_tx = tx;
                prev_ty = ty;
                d += 0.5;
            }
            Ok(Value::Nil)
        },
    )?;
    world_tbl.set("raycast", raycast_fn)?;
    let tm4 = tilemap.clone();
    let config_for_path = config.clone();
    let find_path_fn = lua.create_function(
        move |lua_ctx, (sx, sy, tx, ty, path_type): (f32, f32, f32, f32, Option<String>)| {
            let path = if matches!(path_type.as_deref(), Some("platformer")) {
                crate::pathfinding::find_platformer_path_points(
                    &tm4,
                    &config_for_path,
                    Vec2::new(sx, sy),
                    Vec2::new(tx, ty),
                )
                .unwrap_or_default()
            } else {
                ai::find_top_down_path_points(&tm4, tile_size, Vec2::new(sx, sy), Vec2::new(tx, ty))
                    .unwrap_or_default()
            };
            let out = lua_ctx.create_table()?;
            for (i, p) in path.iter().enumerate() {
                let point = lua_ctx.create_table()?;
                point.set("x", p.x)?;
                point.set("y", p.y)?;
                out.set(i + 1, point)?;
            }
            Ok(out)
        },
    )?;
    world_tbl.set("find_path", find_path_fn)?;
    let tm5 = tilemap.clone();
    let line_of_sight_fn =
        lua.create_function(move |_lua, (x1, y1, x2, y2): (f32, f32, f32, f32)| {
            Ok(ai::has_line_of_sight_points(
                &tm5,
                tile_size,
                Vec2::new(x1, y1),
                Vec2::new(x2, y2),
            ))
        })?;
    world_tbl.set("line_of_sight", line_of_sight_fn)?;
    let raycast_entities_list = entities.clone();
    let raycast_entities_fn = lua.create_function(
        move |lua_ctx,
              (ox, oy, dx, dy, max_dist, tag): (f32, f32, f32, f32, f32, Option<String>)| {
            let tag_filter = tag
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(|s| s.to_string());
            let targets = raycast_entities_list.iter().filter_map(|e| {
                if let Some(tag_name) = tag_filter.as_deref() {
                    if !e.tags.contains(tag_name) {
                        return None;
                    }
                }
                let (min, max) = e.aabb?;
                Some(RaycastAabb { id: e.id, min, max })
            });
            let hits = raycast_aabbs(Vec2::new(ox, oy), Vec2::new(dx, dy), max_dist, targets);
            let out = lua_ctx.create_table()?;
            for (i, hit) in hits.into_iter().enumerate() {
                let item = lua_ctx.create_table()?;
                item.set("id", hit.id)?;
                item.set("x", hit.x)?;
                item.set("y", hit.y)?;
                item.set("distance", hit.distance)?;
                out.set(i + 1, item)?;
            }
            Ok(out)
        },
    )?;
    world_tbl.set("raycast_entities", raycast_entities_fn)?;

    let spawn_commands_ref = pending_commands.clone();
    let spawn_config = config.clone();
    let spawn_script_name = script_name.to_string();
    let spawn_fn = lua.create_function_mut(move |lua_ctx, spec: mlua::Table| {
        let request = lua_spawn_request(lua_ctx, spec, &spawn_config)?;
        spawn_commands_ref
            .borrow_mut()
            .push(ScriptWorldCommand::Spawn {
                request,
                script_name: spawn_script_name.clone(),
                source_entity,
            });
        Ok(())
    })?;
    world_tbl.set("spawn", spawn_fn)?;

    let spawn_projectile_commands_ref = pending_commands.clone();
    let spawn_projectile_script_name = script_name.to_string();
    let spawn_projectile_fn = lua.create_function_mut(move |_lua, spec: mlua::Table| {
        let x = spec.get::<Option<f32>>("x")?.unwrap_or(0.0);
        let y = spec.get::<Option<f32>>("y")?.unwrap_or(0.0);
        let speed = spec.get::<Option<f32>>("speed")?.unwrap_or(260.0).max(0.0);
        let damage = spec.get::<Option<f32>>("damage")?.unwrap_or(1.0).max(0.0);
        let lifetime = spec
            .get::<Option<u32>>("lifetime")
            .or_else(|_| spec.get::<Option<u32>>("lifetime_frames"))?
            .unwrap_or(60);
        let owner = spec
            .get::<Option<u64>>("owner")
            .or_else(|_| spec.get::<Option<u64>>("owner_id"))?
            .or(source_entity)
            .unwrap_or(0);
        let damage_tag = spec
            .get::<Option<String>>("damage_tag")?
            .unwrap_or_else(|| "player".to_string());
        let width = spec.get::<Option<f32>>("width")?.unwrap_or(4.0).max(0.1);
        let height = spec.get::<Option<f32>>("height")?.unwrap_or(4.0).max(0.1);
        let (mut dir_x, mut dir_y) = (1.0f32, 0.0f32);
        if let Ok(direction) = spec.get::<mlua::Table>("direction") {
            if let Ok(x) = direction.get::<f32>("x") {
                dir_x = x;
            } else if let Ok(x) = direction.get::<f32>(1) {
                dir_x = x;
            }
            if let Ok(y) = direction.get::<f32>("y") {
                dir_y = y;
            } else if let Ok(y) = direction.get::<f32>(2) {
                dir_y = y;
            }
        }
        let len = (dir_x * dir_x + dir_y * dir_y).sqrt();
        if len > 0.0001 {
            dir_x /= len;
            dir_y /= len;
        } else {
            dir_x = 1.0;
            dir_y = 0.0;
        }

        let mut tags = vec!["projectile".to_string()];
        if let Ok(tag_tbl) = spec.get::<mlua::Table>("tags") {
            for tag in tag_tbl.sequence_values::<String>() {
                let tag = tag?;
                let trimmed = tag.trim();
                if !trimmed.is_empty() && !tags.iter().any(|t| t == trimmed) {
                    tags.push(trimmed.to_string());
                }
            }
        }

        let request = EntitySpawnRequest {
            x,
            y,
            components: vec![
                ComponentDef::Collider { width, height },
                ComponentDef::Projectile {
                    speed,
                    direction: Vec2Def { x: dir_x, y: dir_y },
                    lifetime_frames: lifetime,
                    damage,
                    owner_id: owner,
                    damage_tag,
                },
            ],
            script: spec.get::<Option<String>>("script")?,
            tags,
            is_player: false,
        };
        spawn_projectile_commands_ref
            .borrow_mut()
            .push(ScriptWorldCommand::Spawn {
                request,
                script_name: spawn_projectile_script_name.clone(),
                source_entity,
            });
        Ok(())
    })?;
    world_tbl.set("spawn_projectile", spawn_projectile_fn)?;

    let spawn_particles_ref = pending_events.clone();
    let spawn_particles_commands_ref = pending_commands.clone();
    let spawn_particles_script_name = script_name.to_string();
    let spawn_particles_fn =
        lua.create_function_mut(move |lua_ctx, (preset, x, y, angle): (String, f32, f32, Option<f32>)| {
            let preset = preset.trim().to_string();
            if preset.is_empty() {
                return Ok(());
            }
            spawn_particles_commands_ref
                .borrow_mut()
                .push(ScriptWorldCommand::SpawnParticles {
                    preset: preset.clone(),
                    x,
                    y,
                    angle,
                    script_name: spawn_particles_script_name.clone(),
                    source_entity,
                });
            let data = lua_ctx.to_value(&serde_json::json!({
                "preset": preset,
                "x": x,
                "y": y,
            }))?;
            spawn_particles_ref.borrow_mut().push(ScriptEvent {
                name: "spawn_particles".to_string(),
                data: lua_ctx.from_value(data)?,
                frame,
                source_entity,
            });
            Ok(())
        })?;
    world_tbl.set("spawn_particles", spawn_particles_fn)?;

    let despawn_commands_ref = pending_commands.clone();
    let despawn_script_name = script_name.to_string();
    let despawn_fn = lua.create_function_mut(move |_lua, entity_id: u64| {
        despawn_commands_ref
            .borrow_mut()
            .push(ScriptWorldCommand::Despawn {
                target_id: entity_id,
                script_name: despawn_script_name.clone(),
                source_entity,
            });
        Ok(())
    })?;
    world_tbl.set("despawn", despawn_fn)?;

    // world.tween(entity_id, {property="x", to=100, duration=0.5, easing="ease_out"})
    let tween_commands_ref = pending_commands.clone();
    let tween_fn = lua.create_function_mut(move |_lua, (entity_id, opts): (u64, mlua::Table)| {
        let property: String = opts.get("property")?;
        let to: f32 = opts.get("to")?;
        let from: Option<f32> = opts.get("from").ok();
        let duration: f32 = opts.get("duration").unwrap_or(0.5);
        let easing: Option<String> = opts.get("easing").ok();
        let tween_id: Option<String> = opts.get("tween_id").ok();
        tween_commands_ref
            .borrow_mut()
            .push(ScriptWorldCommand::TweenEntity {
                target_id: entity_id,
                property,
                to,
                from,
                duration,
                easing,
                tween_id,
            });
        Ok(())
    })?;
    world_tbl.set("tween", tween_fn)?;

    // world.tween_sequence(entity_id, {{property="x", to=100, duration=0.5}, {property="y", to=200, duration=0.3}}, "seq_id")
    let tween_seq_commands_ref = pending_commands.clone();
    let tween_seq_fn = lua.create_function_mut(move |_lua, (entity_id, steps_tbl, seq_id): (u64, mlua::Table, Option<String>)| {
        let mut steps = Vec::new();
        for pair in steps_tbl.sequence_values::<mlua::Table>() {
            let step_tbl = pair?;
            let property: String = step_tbl.get("property")?;
            let to: f32 = step_tbl.get("to")?;
            let from: Option<f32> = step_tbl.get("from").ok();
            let duration: f32 = step_tbl.get("duration").unwrap_or(0.5);
            let easing: Option<String> = step_tbl.get("easing").ok();
            steps.push(crate::tween::TweenStep { property, to, from, duration, easing });
        }
        tween_seq_commands_ref
            .borrow_mut()
            .push(ScriptWorldCommand::TweenSequence {
                target_id: entity_id,
                steps,
                sequence_id: seq_id,
            });
        Ok(())
    })?;
    world_tbl.set("tween_sequence", tween_seq_fn)?;

    // world.screen_flash(duration, color?)
    let flash_commands_ref = pending_commands.clone();
    let screen_flash_fn =
        lua.create_function_mut(move |_lua, (duration, color): (f32, Option<mlua::Table>)| {
            let color_arr = if let Some(tbl) = color {
                let r: f32 = tbl.get(1).unwrap_or(1.0);
                let g: f32 = tbl.get(2).unwrap_or(1.0);
                let b: f32 = tbl.get(3).unwrap_or(1.0);
                Some([r, g, b])
            } else {
                None
            };
            flash_commands_ref
                .borrow_mut()
                .push(ScriptWorldCommand::ScreenEffect {
                    effect: "flash".to_string(),
                    duration,
                    color: color_arr,
                });
            Ok(())
        })?;
    world_tbl.set("screen_flash", screen_flash_fn)?;

    // world.screen_fade_out(duration, color?)
    let fade_out_commands_ref = pending_commands.clone();
    let screen_fade_out_fn =
        lua.create_function_mut(move |_lua, (duration, color): (f32, Option<mlua::Table>)| {
            let color_arr = if let Some(tbl) = color {
                let r: f32 = tbl.get(1).unwrap_or(0.0);
                let g: f32 = tbl.get(2).unwrap_or(0.0);
                let b: f32 = tbl.get(3).unwrap_or(0.0);
                Some([r, g, b])
            } else {
                None
            };
            fade_out_commands_ref
                .borrow_mut()
                .push(ScriptWorldCommand::ScreenEffect {
                    effect: "fade_out".to_string(),
                    duration,
                    color: color_arr,
                });
            Ok(())
        })?;
    world_tbl.set("screen_fade_out", screen_fade_out_fn)?;

    // world.screen_fade_in(duration, color?)
    let fade_in_commands_ref = pending_commands.clone();
    let screen_fade_in_fn =
        lua.create_function_mut(move |_lua, (duration, color): (f32, Option<mlua::Table>)| {
            let color_arr = if let Some(tbl) = color {
                let r: f32 = tbl.get(1).unwrap_or(0.0);
                let g: f32 = tbl.get(2).unwrap_or(0.0);
                let b: f32 = tbl.get(3).unwrap_or(0.0);
                Some([r, g, b])
            } else {
                None
            };
            fade_in_commands_ref
                .borrow_mut()
                .push(ScriptWorldCommand::ScreenEffect {
                    effect: "fade_in".to_string(),
                    duration,
                    color: color_arr,
                });
            Ok(())
        })?;
    world_tbl.set("screen_fade_in", screen_fade_in_fn)?;

    // world.set_ambient(intensity, color?)
    let ambient_commands_ref = pending_commands.clone();
    let set_ambient_fn =
        lua.create_function_mut(move |_lua, (intensity, color): (f32, Option<mlua::Table>)| {
            let color_arr = if let Some(tbl) = color {
                let r: f32 = tbl.get(1).unwrap_or(1.0);
                let g: f32 = tbl.get(2).unwrap_or(1.0);
                let b: f32 = tbl.get(3).unwrap_or(1.0);
                Some([r, g, b])
            } else {
                None
            };
            ambient_commands_ref
                .borrow_mut()
                .push(ScriptWorldCommand::SetAmbient {
                    intensity,
                    color: color_arr,
                });
            Ok(())
        })?;
    world_tbl.set("set_ambient", set_ambient_fn)?;

    let game_tbl = lua.create_table()?;
    game_tbl.set("state", game_state)?;
    let game_transition_events_ref = pending_events.clone();
    let game_transition_fn =
        lua.create_function_mut(move |lua_ctx, (to, opts): (String, Option<mlua::Table>)| {
            let mut payload = serde_json::json!({ "to": to });
            if let Some(opts) = opts {
                if let Ok(effect) = opts.get::<String>("effect") {
                    payload["effect"] = serde_json::json!(effect);
                }
                if let Ok(duration) = opts.get::<f32>("duration") {
                    payload["duration"] = serde_json::json!(duration);
                }
            }
            game_transition_events_ref.borrow_mut().push(ScriptEvent {
                name: "game_transition".to_string(),
                data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
                frame,
                source_entity,
            });
            Ok(())
        })?;
    game_tbl.set("transition", game_transition_fn)?;

    let game_pause_events_ref = pending_events.clone();
    let game_pause_fn = lua.create_function_mut(move |_lua, _: Option<mlua::Table>| {
        game_pause_events_ref.borrow_mut().push(ScriptEvent {
            name: "game_pause".to_string(),
            data: serde_json::json!({}),
            frame,
            source_entity,
        });
        Ok(())
    })?;
    game_tbl.set("pause", game_pause_fn)?;

    let game_resume_events_ref = pending_events.clone();
    let game_resume_fn = lua.create_function_mut(move |_lua, _: Option<mlua::Table>| {
        game_resume_events_ref.borrow_mut().push(ScriptEvent {
            name: "game_resume".to_string(),
            data: serde_json::json!({}),
            frame,
            source_entity,
        });
        Ok(())
    })?;
    game_tbl.set("resume", game_resume_fn)?;
    world_tbl.set("game", game_tbl)?;

    let pending_events_ref = pending_events.clone();
    let emit_fn = lua.create_function_mut(move |lua_ctx, (name, data): (String, Value)| {
        let json = match data {
            Value::Nil => serde_json::Value::Null,
            v => lua_ctx
                .from_value::<serde_json::Value>(v)
                .unwrap_or(serde_json::Value::Null),
        };
        pending_events_ref.borrow_mut().push(ScriptEvent {
            name,
            data: json,
            frame,
            source_entity,
        });
        Ok(())
    })?;
    world_tbl.set("emit", emit_fn)?;

    // Script logging: world.log(msg) or world.log(level, msg)
    let log_entries = lua.create_table()?;
    let log_idx = Rc::new(RefCell::new(0usize));
    let log_entries_ref = log_entries.clone();
    let log_script_name = script_name.to_string();
    let log_fn = lua.create_function_mut(move |lua_ctx, args: mlua::MultiValue| {
        let args_vec: Vec<Value> = args.into_iter().collect();
        let (level, message) = if args_vec.len() >= 2 {
            // Skip leading table arg from colon syntax
            let mut str_args = Vec::new();
            for val in &args_vec {
                if let Value::String(s) = val {
                    str_args.push(s.to_string_lossy().to_string());
                }
            }
            if str_args.len() >= 2 {
                (str_args[0].clone(), str_args[1].clone())
            } else if str_args.len() == 1 {
                ("info".to_string(), str_args[0].clone())
            } else {
                ("info".to_string(), format!("{:?}", args_vec))
            }
        } else if let Some(Value::String(s)) = args_vec.first() {
            ("info".to_string(), s.to_string_lossy().to_string())
        } else if let Some(val) = args_vec.first() {
            ("info".to_string(), format!("{:?}", val))
        } else {
            return Ok(());
        };
        let entry = lua_ctx.create_table()?;
        entry.set("level", level)?;
        entry.set("message", message)?;
        entry.set("script_name", log_script_name.as_str())?;
        entry.set("frame", frame)?;
        entry.set("entity_id", source_entity)?;
        let mut idx = log_idx.borrow_mut();
        *idx += 1;
        log_entries_ref.set(*idx, entry)?;
        Ok(())
    })?;
    world_tbl.set("log", log_fn)?;
    world_tbl.set("__axiom_pending_logs", log_entries)?;

    let on_events = world_events.clone();
    let on_fn =
        lua.create_function_mut(move |lua_ctx, (name, handler): (String, mlua::Function)| {
            let target = name.trim();
            if target.is_empty() {
                return Ok(0usize);
            }
            let mut called = 0usize;
            for ev in on_events.iter().filter(|ev| ev.name == target) {
                let payload = lua_ctx.to_value(&ev.data).unwrap_or(Value::Nil);
                handler.call::<()>(payload)?;
                called += 1;
            }
            Ok(called)
        })?;
    world_tbl.set("on", on_fn)?;

    let sfx_events_ref = pending_events.clone();
    let play_sfx_fn = lua.create_function_mut(
        move |lua_ctx, (name, opts): (String, Option<mlua::Table>)| {
            let mut payload = serde_json::json!({ "name": name });
            if let Some(opts) = opts {
                if let Ok(volume) = opts.get::<f32>("volume") {
                    payload["volume"] = serde_json::json!(volume);
                }
                if let Ok(pitch) = opts.get::<f32>("pitch") {
                    payload["pitch"] = serde_json::json!(pitch);
                }
            }
            sfx_events_ref.borrow_mut().push(ScriptEvent {
                name: "audio_play_sfx".to_string(),
                data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
                frame,
                source_entity,
            });
            Ok(())
        },
    )?;
    world_tbl.set("play_sfx", play_sfx_fn)?;

    let music_events_ref = pending_events.clone();
    let play_music_fn = lua.create_function_mut(
        move |lua_ctx, (name, opts): (String, Option<mlua::Table>)| {
            let mut payload = serde_json::json!({ "name": name });
            if let Some(opts) = opts {
                if let Ok(fade_in) = opts.get::<f32>("fade_in") {
                    payload["fade_in"] = serde_json::json!(fade_in);
                }
            }
            music_events_ref.borrow_mut().push(ScriptEvent {
                name: "audio_play_music".to_string(),
                data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
                frame,
                source_entity,
            });
            Ok(())
        },
    )?;
    world_tbl.set("play_music", play_music_fn)?;

    let stop_music_events_ref = pending_events.clone();
    let stop_music_fn = lua.create_function_mut(move |lua_ctx, opts: Option<mlua::Table>| {
        let mut payload = serde_json::json!({});
        if let Some(opts) = opts {
            if let Ok(fade_out) = opts.get::<f32>("fade_out") {
                payload["fade_out"] = serde_json::json!(fade_out);
            }
        }
        stop_music_events_ref.borrow_mut().push(ScriptEvent {
            name: "audio_stop_music".to_string(),
            data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
            frame,
            source_entity,
        });
        Ok(())
    })?;
    world_tbl.set("stop_music", stop_music_fn)?;

    let volume_events_ref = pending_events.clone();
    let set_volume_fn =
        lua.create_function_mut(move |lua_ctx, (channel, value): (String, f32)| {
            let payload = serde_json::json!({
                "channel": channel,
                "value": value,
            });
            volume_events_ref.borrow_mut().push(ScriptEvent {
                name: "audio_set_volume".to_string(),
                data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
                frame,
                source_entity,
            });
            Ok(())
        })?;
    world_tbl.set("set_volume", set_volume_fn)?;

    let ui_tbl = lua.create_table()?;
    let ui_show_ref = pending_events.clone();
    let ui_show_fn = lua.create_function_mut(move |lua_ctx, name: String| {
        let payload = serde_json::json!({ "name": name });
        ui_show_ref.borrow_mut().push(ScriptEvent {
            name: "ui_show_screen".to_string(),
            data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
            frame,
            source_entity,
        });
        Ok(())
    })?;
    ui_tbl.set("show_screen", ui_show_fn)?;

    let ui_hide_ref = pending_events.clone();
    let ui_hide_fn = lua.create_function_mut(move |lua_ctx, name: String| {
        let payload = serde_json::json!({ "name": name });
        ui_hide_ref.borrow_mut().push(ScriptEvent {
            name: "ui_hide_screen".to_string(),
            data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
            frame,
            source_entity,
        });
        Ok(())
    })?;
    ui_tbl.set("hide_screen", ui_hide_fn)?;

    let ui_text_ref = pending_events.clone();
    let ui_set_text_fn =
        lua.create_function_mut(move |lua_ctx, (id, text): (String, String)| {
            let payload = serde_json::json!({ "id": id, "text": text });
            ui_text_ref.borrow_mut().push(ScriptEvent {
                name: "ui_set_text".to_string(),
                data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
                frame,
                source_entity,
            });
            Ok(())
        })?;
    ui_tbl.set("set_text", ui_set_text_fn)?;

    let ui_progress_ref = pending_events.clone();
    let ui_set_progress_fn =
        lua.create_function_mut(move |lua_ctx, (id, value, max): (String, f32, f32)| {
            let payload = serde_json::json!({ "id": id, "value": value, "max": max });
            ui_progress_ref.borrow_mut().push(ScriptEvent {
                name: "ui_set_progress".to_string(),
                data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
                frame,
                source_entity,
            });
            Ok(())
        })?;
    ui_tbl.set("set_progress", ui_set_progress_fn)?;
    world_tbl.set("ui", ui_tbl)?;

    let dialogue_tbl = lua.create_table()?;
    let dialogue_start_ref = pending_events.clone();
    let dialogue_start_fn = lua.create_function_mut(move |lua_ctx, conversation: String| {
        let payload = serde_json::json!({ "conversation": conversation });
        dialogue_start_ref.borrow_mut().push(ScriptEvent {
            name: "dialogue_start".to_string(),
            data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
            frame,
            source_entity,
        });
        Ok(())
    })?;
    dialogue_tbl.set("start", dialogue_start_fn)?;
    let dialogue_choose_ref = pending_events.clone();
    let dialogue_choose_fn = lua.create_function_mut(move |lua_ctx, choice: u32| {
        let payload = serde_json::json!({ "choice": choice });
        dialogue_choose_ref.borrow_mut().push(ScriptEvent {
            name: "dialogue_choose".to_string(),
            data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
            frame,
            source_entity,
        });
        Ok(())
    })?;
    dialogue_tbl.set("choose", dialogue_choose_fn)?;
    world_tbl.set("dialogue", dialogue_tbl)?;

    let camera_tbl = lua.create_table()?;
    let camera_shake_ref = pending_events.clone();
    let camera_shake_fn =
        lua.create_function_mut(move |lua_ctx, (intensity, duration): (f32, Option<f32>)| {
            let payload = serde_json::json!({
                "intensity": intensity.max(0.0),
                "duration": duration.unwrap_or(0.25).max(0.0),
            });
            camera_shake_ref.borrow_mut().push(ScriptEvent {
                name: "camera_shake".to_string(),
                data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
                frame,
                source_entity,
            });
            Ok(())
        })?;
    camera_tbl.set("shake", camera_shake_fn)?;

    let camera_zoom_ref = pending_events.clone();
    let camera_zoom_fn = lua.create_function_mut(move |lua_ctx, zoom: f32| {
        let payload = serde_json::json!({
            "zoom": zoom.max(0.05),
        });
        camera_zoom_ref.borrow_mut().push(ScriptEvent {
            name: "camera_zoom".to_string(),
            data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
            frame,
            source_entity,
        });
        Ok(())
    })?;
    camera_tbl.set("zoom", camera_zoom_fn)?;

    let camera_look_ref = pending_events.clone();
    let camera_look_fn = lua.create_function_mut(move |lua_ctx, (x, y): (f32, f32)| {
        let payload = serde_json::json!({
            "x": x,
            "y": y,
        });
        camera_look_ref.borrow_mut().push(ScriptEvent {
            name: "camera_look_at".to_string(),
            data: lua_ctx.from_value(lua_ctx.to_value(&payload)?)?,
            frame,
            source_entity,
        });
        Ok(())
    })?;
    camera_tbl.set("look_at", camera_look_fn)?;
    world_tbl.set("camera", camera_tbl)?;

    // world.spawn_text(x, y, text, opts?)
    let spawn_text_cmds = pending_commands.clone();
    let spawn_text_fn = lua.create_function_mut(
        move |_lua, (x, y, text, opts): (f32, f32, String, Option<mlua::Table>)| {
            let mut font_size = 16.0f32;
            let mut color = [1.0f32, 1.0, 1.0, 1.0];
            let mut duration = None;
            let mut fade = false;
            let mut rise_speed = 0.0f32;
            let mut owner_id = None;
            if let Some(opts) = opts {
                if let Ok(fs) = opts.get::<f32>("font_size") { font_size = fs; }
                if let Ok(c) = opts.get::<mlua::Table>("color") {
                    color[0] = c.get(1).unwrap_or(1.0);
                    color[1] = c.get(2).unwrap_or(1.0);
                    color[2] = c.get(3).unwrap_or(1.0);
                    color[3] = c.get(4).unwrap_or(1.0);
                }
                if let Ok(d) = opts.get::<f32>("duration") { duration = Some(d); }
                if let Ok(f) = opts.get::<bool>("fade") { fade = f; }
                if let Ok(r) = opts.get::<f32>("rise_speed") { rise_speed = r; }
                if let Ok(o) = opts.get::<u64>("owner_id") { owner_id = Some(o); }
            }
            spawn_text_cmds
                .borrow_mut()
                .push(ScriptWorldCommand::SpawnText {
                    x, y, text, font_size, color, duration, fade, rise_speed, owner_id,
                });
            Ok(())
        },
    )?;
    world_tbl.set("spawn_text", spawn_text_fn)?;

    // world.set_weather(type, intensity, wind?)
    let weather_cmds = pending_commands.clone();
    let set_weather_fn = lua.create_function_mut(
        move |_lua, (weather_type, intensity, wind): (String, f32, Option<f32>)| {
            weather_cmds
                .borrow_mut()
                .push(ScriptWorldCommand::SetWeather {
                    weather_type,
                    intensity,
                    wind: wind.unwrap_or(0.0),
                });
            Ok(())
        },
    )?;
    world_tbl.set("set_weather", set_weather_fn)?;

    // world.clear_weather()
    let clear_weather_cmds = pending_commands.clone();
    let clear_weather_fn = lua.create_function_mut(move |_lua, _: Option<mlua::Table>| {
        clear_weather_cmds
            .borrow_mut()
            .push(ScriptWorldCommand::ClearWeather);
        Ok(())
    })?;
    world_tbl.set("clear_weather", clear_weather_fn)?;

    // world.set_parallax(layers)
    let parallax_cmds = pending_commands.clone();
    let set_parallax_fn = lua.create_function_mut(move |_lua, layers_tbl: mlua::Table| {
        let mut layers = Vec::new();
        for item in layers_tbl.sequence_values::<mlua::Table>() {
            if let Ok(layer_tbl) = item {
                let mut layer = crate::parallax::ParallaxLayerDef {
                    texture: layer_tbl.get("texture").ok(),
                    color: None,
                    scroll_factor: layer_tbl.get("scroll_factor").unwrap_or(0.5),
                    z_depth: layer_tbl.get("z_depth").unwrap_or(-10.0),
                    repeat_x: layer_tbl.get("repeat_x").unwrap_or(true),
                    repeat_y: layer_tbl.get("repeat_y").unwrap_or(false),
                    scale: layer_tbl.get("scale").ok(),
                };
                if let Ok(c) = layer_tbl.get::<mlua::Table>("color") {
                    let r: f32 = c.get(1).unwrap_or(0.0);
                    let g: f32 = c.get(2).unwrap_or(0.0);
                    let b: f32 = c.get(3).unwrap_or(0.0);
                    let a: f32 = c.get(4).unwrap_or(1.0);
                    layer.color = Some([r, g, b, a]);
                }
                layers.push(layer);
            }
        }
        parallax_cmds
            .borrow_mut()
            .push(ScriptWorldCommand::SetParallax { layers });
        Ok(())
    })?;
    world_tbl.set("set_parallax", set_parallax_fn)?;

    // world.cutscene(name) — play a defined cutscene
    let cutscene_play_cmds = pending_commands.clone();
    let cutscene_fn = lua.create_function_mut(move |_lua, name: String| {
        cutscene_play_cmds
            .borrow_mut()
            .push(ScriptWorldCommand::PlayCutscene { name });
        Ok(())
    })?;
    world_tbl.set("cutscene", cutscene_fn)?;

    // world.stop_cutscene()
    let cutscene_stop_cmds = pending_commands.clone();
    let stop_cutscene_fn = lua.create_function_mut(move |_lua, _: Option<mlua::Table>| {
        cutscene_stop_cmds
            .borrow_mut()
            .push(ScriptWorldCommand::StopCutscene);
        Ok(())
    })?;
    world_tbl.set("stop_cutscene", stop_cutscene_fn)?;

    // world.set_time_of_day(hour)
    let tod_cmds = pending_commands.clone();
    let set_tod_fn = lua.create_function_mut(move |_lua, hour: f32| {
        tod_cmds
            .borrow_mut()
            .push(ScriptWorldCommand::SetTimeOfDay { hour });
        Ok(())
    })?;
    world_tbl.set("set_time_of_day", set_tod_fn)?;

    // world.input.rebind(key, action)
    let rebind_cmds = pending_commands.clone();
    let rebind_fn = lua.create_function_mut(move |_lua, (key, action): (String, String)| {
        rebind_cmds
            .borrow_mut()
            .push(ScriptWorldCommand::RebindInput { key, action });
        Ok(())
    })?;
    {
        let input_tbl: mlua::Table = world_tbl.get("input")?;
        input_tbl.set("rebind", rebind_fn)?;
    }

    Ok((world_tbl, pending_events, pending_commands))
}

fn build_min_world_table(
    lua: &Lua,
    vars: &HashMap<String, serde_json::Value>,
    frame: u64,
    seconds: f64,
    dt: f32,
    game_state: &str,
) -> WorldTableBuildResult {
    let world_tbl = lua.create_table()?;
    let pending_events = Rc::new(RefCell::new(Vec::<ScriptEvent>::new()));
    let pending_commands = Rc::new(RefCell::new(Vec::<ScriptWorldCommand>::new()));
    world_tbl.set("frame", frame)?;
    world_tbl.set("time", seconds)?;
    world_tbl.set("dt", dt)?;

    let vars_tbl = lua.create_table()?;
    for (k, v) in vars {
        vars_tbl.set(k.as_str(), lua.to_value(v)?)?;
    }
    world_tbl.set("vars", vars_tbl.clone())?;
    let vars_get_tbl = vars_tbl.clone();
    let get_var_fn = lua.create_function(move |_lua, key: String| {
        Ok(vars_get_tbl.get::<Value>(key).unwrap_or(Value::Nil))
    })?;
    world_tbl.set("get_var", get_var_fn)?;
    let vars_set_tbl = vars_tbl.clone();
    let set_var_fn = lua.create_function_mut(move |_lua, (key, value): (String, Value)| {
        vars_set_tbl.set(key, value)?;
        Ok(())
    })?;
    world_tbl.set("set_var", set_var_fn)?;

    let game_tbl = lua.create_table()?;
    game_tbl.set("state", game_state)?;
    world_tbl.set("game", game_tbl)?;

    Ok((world_tbl, pending_events, pending_commands))
}

/// Despawn entities that have been marked with PendingDeath for at least 1 frame.
fn cleanup_pending_death(
    mut commands: Commands,
    frame: Res<ScriptFrame>,
    query: Query<(Entity, &PendingDeath)>,
) {
    for (entity, pending) in query.iter() {
        // Wait at least 1 frame so entity scripts can run on_death()
        if frame.frame > pending.frame_marked {
            commands.entity(entity).despawn();
        }
    }
}

/// Tick the script frame counter even when gameplay is paused.
/// This runs unconditionally so always-run scripts have an accurate frame count.
fn tick_script_frame_always(
    mut frame: ResMut<ScriptFrame>,
    time: Res<Time<Fixed>>,
    runtime_state: Res<crate::game_runtime::RuntimeState>,
) {
    // Only tick if gameplay systems did NOT already tick (to avoid double-counting)
    if !runtime_state.is_gameplay_active() {
        frame.frame = frame.frame.saturating_add(1);
        frame.seconds += time.delta_secs_f64();
    }
}

/// Run global scripts marked as always_run, even during pause/game_over.
/// Skips scripts that were already run by the normal gated system.
fn run_always_global_scripts(ctx: GlobalScriptSystemCtx<'_, '_>) {
    // Only run if gameplay is NOT enabled (otherwise the normal system already ran them)
    if ctx.runtime_state.is_gameplay_active() {
        return;
    }

    let GlobalScriptSystemCtx {
        mut commands,
        mut engine,
        mut errors,
        mut perf,
        frame,
        time,
        vinput,
        mouse_input,
        event_bus,
        mut tilemap,
        config,
        mut next_network_id,
        runtime_state,
        mut tilemap_cache,
        entity_cache,
        mut log_buffer,
        mut runtime,
    } = ctx;
    let start = std::time::Instant::now();
    let dt = time.delta_secs();
    let cache = &mut runtime.0;
    let shared_tilemap = tilemap_cache.tilemap.clone();
    let lua = &cache.lua;
    let input_snapshot = Arc::new(LuaInputSnapshot {
        active: vinput.active.clone(),
        just_pressed: vinput.just_pressed.clone(),
        mouse_x: mouse_input.world_x,
        mouse_y: mouse_input.world_y,
        mouse_left: mouse_input.left,
        mouse_right: mouse_input.right,
        mouse_middle: mouse_input.middle,
        mouse_left_just_pressed: mouse_input.left_just_pressed,
        mouse_right_just_pressed: mouse_input.right_just_pressed,
        mouse_middle_just_pressed: mouse_input.middle_just_pressed,
    });
    let bus_events = &event_bus.recent;
    let mut network_lookup = entity_cache.network_lookup.clone();
    let raycast_entities = entity_cache.entities.clone();
    let mut pending_world_commands = Vec::<ScriptWorldCommand>::new();

    let always_run_names: Vec<String> = engine
        .always_run_scripts
        .iter()
        .filter(|name| engine.global_scripts.contains(*name))
        .cloned()
        .collect();

    for script_name in always_run_names {
        lua.expire_registry_values();
        if engine.disabled_global_scripts.contains(&script_name) {
            continue;
        }
        let update = {
            let Some(source) = engine.scripts.get(&script_name) else {
                continue;
            };
            match get_or_compile_update(lua, &mut cache.compiled, &script_name, source) {
                Ok(f) => f,
                Err(err_msg) => {
                    let streak = engine
                        .global_error_streaks
                        .entry(script_name.clone())
                        .or_insert(0);
                    *streak = streak.saturating_add(1);
                    errors.push(ScriptError {
                        script_name: script_name.clone(),
                        entity_id: None,
                        error_message: err_msg,
                        frame: frame.frame,
                    });
                    if *streak >= MAX_GLOBAL_SCRIPT_ERROR_STREAK {
                        engine.disabled_global_scripts.insert(script_name.clone());
                    }
                    continue;
                }
            }
        };
        let last_event_frame = engine
            .global_event_cursors
            .get(&script_name)
            .copied()
            .unwrap_or(0);
        let world_events = Arc::new(
            bus_events
                .iter()
                .filter(|ev| ev.frame > last_event_frame)
                .cloned()
                .collect::<Vec<_>>(),
        );

        let globals = lua.globals();
        let _ = globals.set("__axiom_current_script", script_name.as_str());

        let world_tbl = build_world_table(
            lua,
            &WorldBuildArgs {
                vars: &engine.vars,
                script_name: &script_name,
                frame: frame.frame,
                seconds: frame.seconds,
                dt,
                source_entity: None,
                tilemap: &shared_tilemap,
                config: &config,
                entities: &raycast_entities,
                input: &input_snapshot,
                world_events: &world_events,
                game_state: &runtime_state.state,
            },
        );
        let (world_tbl, pending_events, pending_commands) = match world_tbl {
            Ok(v) => v,
            Err(err) => {
                let streak = engine
                    .global_error_streaks
                    .entry(script_name.clone())
                    .or_insert(0);
                *streak = streak.saturating_add(1);
                errors.push(ScriptError {
                    script_name: script_name.clone(),
                    entity_id: None,
                    error_message: err.to_string(),
                    frame: frame.frame,
                });
                if *streak >= MAX_GLOBAL_SCRIPT_ERROR_STREAK {
                    engine.disabled_global_scripts.insert(script_name.clone());
                }
                continue;
            }
        };
        let limits = script_execution_limits();
        let run_result = call_lua_with_budget(
            lua,
            Duration::from_millis(limits.global_budget_ms),
            limits.instruction_interval,
            || update.call::<()>((world_tbl.clone(), dt)),
        );
        if let Err(err) = run_result {
            let streak = engine
                .global_error_streaks
                .entry(script_name.clone())
                .or_insert(0);
            *streak = streak.saturating_add(1);
            errors.push(ScriptError {
                script_name: script_name.clone(),
                entity_id: None,
                error_message: err.to_string(),
                frame: frame.frame,
            });
            if *streak >= MAX_GLOBAL_SCRIPT_ERROR_STREAK {
                engine.disabled_global_scripts.insert(script_name.clone());
            }
            continue;
        }
        engine.global_error_streaks.insert(script_name.clone(), 0);

        if let Ok(vars_val) = world_tbl.get::<Value>("vars") {
            if let Ok(vars) = lua.from_value::<HashMap<String, serde_json::Value>>(vars_val) {
                engine.vars = vars;
            }
        }
        for event in pending_events.borrow().iter() {
            engine.push_event(event.clone());
        }
        for cmd in pending_commands.borrow().iter() {
            pending_world_commands.push(cmd.clone());
        }
        if let Ok(logs_tbl) = world_tbl.get::<Value>("__axiom_pending_logs") {
            if let Ok(logs) = lua.from_value::<Vec<ScriptLogEntry>>(logs_tbl) {
                for entry in logs {
                    log_buffer.push(entry);
                }
            }
        }
        engine
            .global_event_cursors
            .insert(script_name.clone(), frame.frame);
    }

    let tilemap_changed = apply_script_world_commands(
        &mut commands,
        &mut tilemap,
        &mut next_network_id,
        &mut network_lookup,
        &mut errors,
        frame.frame,
        pending_world_commands,
    );
    if tilemap_changed {
        tilemap_cache.tilemap = Arc::new(tilemap.clone());
    }

    let _ = runtime.0.lua.gc_collect();
    perf.script_time_ms += start.elapsed().as_secs_f32() * 1000.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_game_bindings_emit_runtime_events() {
        let lua = Lua::new();
        let vars = HashMap::new();
        let tilemap = Arc::new(Tilemap::test_level());
        let config = GameConfig::default();
        let raycast_entities = Arc::new(Vec::new());
        let input = Arc::new(LuaInputSnapshot::default());
        let world_events = Arc::new(Vec::<GameEvent>::new());
        let (world, pending, _commands) = build_world_table(
            &lua,
            &WorldBuildArgs {
                vars: &vars,
                script_name: "test_script",
                frame: 12,
                seconds: 1.0,
                dt: 1.0 / 60.0,
                source_entity: Some(7),
                tilemap: &tilemap,
                config: &config,
                entities: &raycast_entities,
                input: &input,
                world_events: &world_events,
                game_state: "Playing",
            },
        )
        .expect("build world table");

        let game: mlua::Table = world.get("game").expect("game table");
        let transition: mlua::Function = game.get("transition").expect("game.transition");
        let pause: mlua::Function = game.get("pause").expect("game.pause");
        let resume: mlua::Function = game.get("resume").expect("game.resume");

        transition
            .call::<()>(("Paused".to_string(), None::<mlua::Table>))
            .expect("transition call");
        pause.call::<()>(None::<mlua::Table>).expect("pause call");
        resume.call::<()>(None::<mlua::Table>).expect("resume call");

        let names = pending
            .borrow()
            .iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["game_transition", "game_pause", "game_resume"]);
    }

    #[test]
    fn script_engine_resets_event_cursors_on_reload_and_remove() {
        let mut engine = ScriptEngine::default();
        engine
            .entity_event_cursors
            .insert(("enemy_ai".to_string(), 10), 44);
        engine
            .global_event_cursors
            .insert("enemy_ai".to_string(), 44);

        engine
            .load_script(
                "enemy_ai".to_string(),
                "function update() end".to_string(),
                false,
            )
            .expect("load script");
        assert!(engine
            .entity_event_cursors
            .keys()
            .all(|(name, _)| name != "enemy_ai"));
        assert!(!engine.global_event_cursors.contains_key("enemy_ai"));

        engine
            .entity_event_cursors
            .insert(("enemy_ai".to_string(), 22), 100);
        engine
            .global_event_cursors
            .insert("enemy_ai".to_string(), 100);
        engine.remove_script("enemy_ai");
        assert!(engine
            .entity_event_cursors
            .keys()
            .all(|(name, _)| name != "enemy_ai"));
        assert!(!engine.global_event_cursors.contains_key("enemy_ai"));
    }

    #[test]
    fn world_tile_and_entity_raycast_bindings_work() {
        let lua = Lua::new();
        install_entity_snapshot_metatable(&lua).expect("install metatable");
        let vars = HashMap::new();
        let mut tiles = vec![crate::components::TileType::Empty as u8; 6 * 4];
        tiles[6 + 2] = crate::components::TileType::Solid as u8;
        let tilemap = Arc::new(Tilemap {
            width: 6,
            height: 4,
            tiles,
            player_spawn: (8.0, 8.0),
            goal: None,
            ..Default::default()
        });
        let config = GameConfig::default();
        let raycast_entities = Arc::new(vec![
            LuaEntitySnapshot {
                id: 101,
                x: 25.0,
                y: 10.0,
                vx: 0.0,
                vy: 0.0,
                grounded: false,
                alive: true,
                is_player: false,
                aabb: Some((Vec2::new(20.0, 0.0), Vec2::new(30.0, 20.0))),
                tags: HashSet::from(["enemy".to_string()]),
                health: Some(5.0),
                max_health: Some(5.0),
            },
            LuaEntitySnapshot {
                id: 202,
                x: 45.0,
                y: 10.0,
                vx: 0.0,
                vy: 0.0,
                grounded: false,
                alive: true,
                is_player: false,
                aabb: Some((Vec2::new(40.0, 0.0), Vec2::new(50.0, 20.0))),
                tags: HashSet::from(["pickup".to_string()]),
                health: None,
                max_health: None,
            },
        ]);
        let input = Arc::new(LuaInputSnapshot {
            active: HashSet::from(["left".to_string()]),
            just_pressed: HashSet::from(["jump".to_string()]),
            ..Default::default()
        });
        let world_events = Arc::new(Vec::<GameEvent>::new());
        let (world, _pending, _commands) = build_world_table(
            &lua,
            &WorldBuildArgs {
                vars: &vars,
                script_name: "test_script",
                frame: 0,
                seconds: 0.0,
                dt: 1.0 / 60.0,
                source_entity: None,
                tilemap: &tilemap,
                config: &config,
                entities: &raycast_entities,
                input: &input,
                world_events: &world_events,
                game_state: "Playing",
            },
        )
        .expect("build world table");

        let is_climbable: mlua::Function = world.get("is_climbable").expect("is_climbable");
        assert!(!is_climbable.call::<bool>((1i32, 1i32)).expect("call"));

        let tile_friction: mlua::Function = world.get("tile_friction").expect("tile_friction");
        let fr = tile_friction.call::<f32>((1i32, 1i32)).expect("call");
        assert!((0.0..=1.0).contains(&fr));

        let raycast_fn: mlua::Function = world.get("raycast").expect("raycast");
        let hit = raycast_fn
            .call::<Value>((0.0f32, 24.0f32, 1.0f32, 0.0f32, 100.0f32))
            .expect("raycast call");
        let hit_tbl = match hit {
            Value::Table(t) => t,
            _ => panic!("expected hit table"),
        };
        let hit_tx: i32 = hit_tbl.get("tile_x").expect("tile x");
        let hit_ty: i32 = hit_tbl.get("tile_y").expect("tile y");
        assert_eq!((hit_tx, hit_ty), (2, 1));

        let find_path_fn: mlua::Function = world.get("find_path").expect("find_path");
        let top_down_path: mlua::Table = find_path_fn
            .call((8.0f32, 8.0f32, 24.0f32, 8.0f32, None::<String>))
            .expect("find_path top_down");
        let _count_top_down = top_down_path.len().expect("path len");
        let platformer_path: mlua::Table = find_path_fn
            .call((
                8.0f32,
                8.0f32,
                24.0f32,
                8.0f32,
                Some("platformer".to_string()),
            ))
            .expect("find_path platformer");
        let _count_platformer = platformer_path.len().expect("path len");

        let raycast_entities_fn: mlua::Function =
            world.get("raycast_entities").expect("raycast_entities");
        let hits: mlua::Table = raycast_entities_fn
            .call((
                0.0f32,
                10.0f32,
                1.0f32,
                0.0f32,
                100.0f32,
                Some("enemy".to_string()),
            ))
            .expect("raycast call");
        let first: mlua::Table = hits.get(1).expect("first hit");
        let first_id: u64 = first.get("id").expect("first id");
        assert_eq!(first_id, 101);

        let find_nearest: mlua::Function = world.get("find_nearest").expect("find_nearest");
        let nearest = find_nearest
            .call::<Value>((0.0f32, 10.0f32, Some("enemy".to_string())))
            .expect("find nearest");
        let nearest_tbl = match nearest {
            Value::Table(t) => t,
            _ => panic!("expected nearest table"),
        };
        let nearest_id: u64 = nearest_tbl.get("id").expect("nearest id");
        assert_eq!(nearest_id, 101);

        let input_tbl: mlua::Table = world.get("input").expect("input table");
        let pressed: mlua::Function = input_tbl.get("pressed").expect("input.pressed");
        let just_pressed: mlua::Function =
            input_tbl.get("just_pressed").expect("input.just_pressed");
        assert!(pressed
            .call::<bool>("left".to_string())
            .expect("pressed call"));
        assert!(just_pressed
            .call::<bool>("jump".to_string())
            .expect("just_pressed call"));
    }

    #[test]
    fn world_on_dispatches_matching_events() {
        let lua = Lua::new();
        let vars = HashMap::new();
        let tilemap = Arc::new(Tilemap::test_level());
        let config = GameConfig::default();
        let entities = Arc::new(Vec::<LuaEntitySnapshot>::new());
        let input = Arc::new(LuaInputSnapshot::default());
        let world_events = Arc::new(vec![
            GameEvent {
                name: "player_hit".to_string(),
                data: serde_json::json!({"damage": 2}),
                frame: 10,
                source_entity: Some(1),
            },
            GameEvent {
                name: "other_event".to_string(),
                data: serde_json::json!({"value": 9}),
                frame: 10,
                source_entity: None,
            },
            GameEvent {
                name: "player_hit".to_string(),
                data: serde_json::json!({"damage": 3}),
                frame: 11,
                source_entity: Some(2),
            },
        ]);
        let (world, _pending, _commands) = build_world_table(
            &lua,
            &WorldBuildArgs {
                vars: &vars,
                script_name: "event_script",
                frame: 12,
                seconds: 0.0,
                dt: 1.0 / 60.0,
                source_entity: Some(7),
                tilemap: &tilemap,
                config: &config,
                entities: &entities,
                input: &input,
                world_events: &world_events,
                game_state: "Playing",
            },
        )
        .expect("build world table");

        let calls = Rc::new(RefCell::new(0usize));
        let total_damage = Rc::new(RefCell::new(0i64));
        let calls_ref = calls.clone();
        let total_damage_ref = total_damage.clone();
        let handler = lua
            .create_function_mut(move |_lua, payload: mlua::Table| {
                *calls_ref.borrow_mut() += 1;
                let damage = payload.get::<i64>("damage").unwrap_or(0);
                *total_damage_ref.borrow_mut() += damage;
                Ok(())
            })
            .expect("handler");

        let on_fn: mlua::Function = world.get("on").expect("world.on");
        let count = on_fn
            .call::<usize>(("player_hit".to_string(), handler))
            .expect("on call");
        assert_eq!(count, 2);
        assert_eq!(*calls.borrow(), 2);
        assert_eq!(*total_damage.borrow(), 5);
    }

    #[test]
    fn world_camera_bindings_emit_events() {
        let lua = Lua::new();
        let vars = HashMap::new();
        let tilemap = Arc::new(Tilemap::test_level());
        let config = GameConfig::default();
        let entities = Arc::new(Vec::<LuaEntitySnapshot>::new());
        let input = Arc::new(LuaInputSnapshot::default());
        let world_events = Arc::new(Vec::<GameEvent>::new());
        let (world, pending, _commands) = build_world_table(
            &lua,
            &WorldBuildArgs {
                vars: &vars,
                script_name: "camera_script",
                frame: 3,
                seconds: 0.0,
                dt: 1.0 / 60.0,
                source_entity: Some(7),
                tilemap: &tilemap,
                config: &config,
                entities: &entities,
                input: &input,
                world_events: &world_events,
                game_state: "Playing",
            },
        )
        .expect("build world table");

        let camera: mlua::Table = world.get("camera").expect("camera table");
        let shake: mlua::Function = camera.get("shake").expect("camera.shake");
        let zoom: mlua::Function = camera.get("zoom").expect("camera.zoom");
        let look_at: mlua::Function = camera.get("look_at").expect("camera.look_at");

        shake.call::<()>((0.75f32, Some(0.5f32))).expect("shake");
        zoom.call::<()>(1.25f32).expect("zoom");
        look_at.call::<()>((64.0f32, 48.0f32)).expect("look_at");

        let names = pending
            .borrow()
            .iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["camera_shake", "camera_zoom", "camera_look_at"]);
    }

    #[test]
    fn world_spawn_and_despawn_queue_commands() {
        let lua = Lua::new();
        let vars = HashMap::new();
        let tilemap = Arc::new(Tilemap::test_level());
        let config = GameConfig::default();
        let entities = Arc::new(Vec::<LuaEntitySnapshot>::new());
        let input = Arc::new(LuaInputSnapshot::default());
        let world_events = Arc::new(Vec::<GameEvent>::new());
        let (world, _pending_events, pending_commands) = build_world_table(
            &lua,
            &WorldBuildArgs {
                vars: &vars,
                script_name: "spawn_script",
                frame: 1,
                seconds: 0.0,
                dt: 1.0 / 60.0,
                source_entity: Some(99),
                tilemap: &tilemap,
                config: &config,
                entities: &entities,
                input: &input,
                world_events: &world_events,
                game_state: "Playing",
            },
        )
        .expect("build world table");

        let spawn_fn: mlua::Function = world.get("spawn").expect("spawn fn");
        let despawn_fn: mlua::Function = world.get("despawn").expect("despawn fn");
        let spec = lua.create_table().expect("spec");
        spec.set("x", 10.0f32).expect("x");
        spec.set("y", 20.0f32).expect("y");
        let comps = lua.create_table().expect("components");
        comps.set(1, "Collider").expect("component");
        spec.set("components", comps).expect("set components");
        spawn_fn.call::<()>(spec).expect("spawn call");
        despawn_fn.call::<()>(123u64).expect("despawn call");

        let commands = pending_commands.borrow().clone();
        assert_eq!(commands.len(), 2);
        match &commands[0] {
            ScriptWorldCommand::Spawn { request, .. } => {
                assert_eq!(request.x, 10.0);
                assert_eq!(request.y, 20.0);
                assert_eq!(request.components.len(), 1);
            }
            _ => panic!("expected spawn command"),
        }
        match &commands[1] {
            ScriptWorldCommand::Despawn { target_id, .. } => assert_eq!(*target_id, 123),
            _ => panic!("expected despawn command"),
        }
    }

    #[test]
    fn world_spawn_particles_queues_command_and_event() {
        let lua = Lua::new();
        let vars = HashMap::new();
        let tilemap = Arc::new(Tilemap::test_level());
        let config = GameConfig::default();
        let entities = Arc::new(Vec::<LuaEntitySnapshot>::new());
        let input = Arc::new(LuaInputSnapshot::default());
        let world_events = Arc::new(Vec::<GameEvent>::new());
        let (world, pending_events, pending_commands) = build_world_table(
            &lua,
            &WorldBuildArgs {
                vars: &vars,
                script_name: "particles_script",
                frame: 4,
                seconds: 0.0,
                dt: 1.0 / 60.0,
                source_entity: Some(321),
                tilemap: &tilemap,
                config: &config,
                entities: &entities,
                input: &input,
                world_events: &world_events,
                game_state: "Playing",
            },
        )
        .expect("build world table");

        let spawn_particles: mlua::Function =
            world.get("spawn_particles").expect("spawn_particles fn");
        spawn_particles
            .call::<()>(("dust".to_string(), 12.0f32, 34.0f32))
            .expect("spawn particles");

        let events = pending_events.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "spawn_particles");

        let commands = pending_commands.borrow();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            ScriptWorldCommand::SpawnParticles { preset, x, y, .. } => {
                assert_eq!(preset, "dust");
                assert!((*x - 12.0).abs() < 0.001);
                assert!((*y - 34.0).abs() < 0.001);
            }
            _ => panic!("expected spawn particles command"),
        }
    }

    #[test]
    fn world_spawn_supports_advanced_component_names_and_health_override() {
        let lua = Lua::new();
        let vars = HashMap::new();
        let tilemap = Arc::new(Tilemap::test_level());
        let config = GameConfig::default();
        let entities = Arc::new(Vec::<LuaEntitySnapshot>::new());
        let input = Arc::new(LuaInputSnapshot::default());
        let world_events = Arc::new(Vec::<GameEvent>::new());
        let (world, _pending_events, pending_commands) = build_world_table(
            &lua,
            &WorldBuildArgs {
                vars: &vars,
                script_name: "spawn_script",
                frame: 1,
                seconds: 0.0,
                dt: 1.0 / 60.0,
                source_entity: Some(99),
                tilemap: &tilemap,
                config: &config,
                entities: &entities,
                input: &input,
                world_events: &world_events,
                game_state: "Playing",
            },
        )
        .expect("build world table");

        let spawn_fn: mlua::Function = world.get("spawn").expect("spawn fn");
        let spec = lua.create_table().expect("spec");
        spec.set("x", 10.0f32).expect("x");
        spec.set("y", 20.0f32).expect("y");
        spec.set("health", 5.0f32).expect("health");
        spec.set("max_health", 9.0f32).expect("max_health");
        let comps = lua.create_table().expect("components");
        comps.set(1, "Health").expect("health component");
        comps.set(2, "ContactDamage").expect("contact component");
        comps.set(3, "AiBehavior").expect("ai component");
        comps.set(4, "PathFollower").expect("path component");
        spec.set("components", comps).expect("set components");
        spawn_fn.call::<()>(spec).expect("spawn call");

        let commands = pending_commands.borrow().clone();
        assert_eq!(commands.len(), 1);
        let request = match &commands[0] {
            ScriptWorldCommand::Spawn { request, .. } => request,
            _ => panic!("expected spawn command"),
        };

        let health_components = request
            .components
            .iter()
            .filter(|c| matches!(c, ComponentDef::Health { .. }))
            .count();
        assert_eq!(health_components, 1);
        let has_contact = request
            .components
            .iter()
            .any(|c| matches!(c, ComponentDef::ContactDamage { .. }));
        let has_ai = request
            .components
            .iter()
            .any(|c| matches!(c, ComponentDef::AiBehavior { .. }));
        let has_path = request
            .components
            .iter()
            .any(|c| matches!(c, ComponentDef::PathFollower { .. }));
        assert!(has_contact && has_ai && has_path);
        let health = request
            .components
            .iter()
            .find_map(|c| match c {
                ComponentDef::Health { current, max } => Some((*current, *max)),
                _ => None,
            })
            .expect("health component");
        assert_eq!(health, (5.0, 9.0));
    }

    #[test]
    fn world_spawn_projectile_queues_spawn_command() {
        let lua = Lua::new();
        let vars = HashMap::new();
        let tilemap = Arc::new(Tilemap::test_level());
        let config = GameConfig::default();
        let entities = Arc::new(Vec::<LuaEntitySnapshot>::new());
        let input = Arc::new(LuaInputSnapshot::default());
        let world_events = Arc::new(Vec::<GameEvent>::new());
        let (world, _pending_events, pending_commands) = build_world_table(
            &lua,
            &WorldBuildArgs {
                vars: &vars,
                script_name: "projectile_script",
                frame: 1,
                seconds: 0.0,
                dt: 1.0 / 60.0,
                source_entity: Some(77),
                tilemap: &tilemap,
                config: &config,
                entities: &entities,
                input: &input,
                world_events: &world_events,
                game_state: "Playing",
            },
        )
        .expect("build world table");

        let spawn_projectile: mlua::Function =
            world.get("spawn_projectile").expect("spawn_projectile fn");
        let spec = lua.create_table().expect("spec");
        spec.set("x", 32.0f32).expect("x");
        spec.set("y", 48.0f32).expect("y");
        spec.set("speed", 300.0f32).expect("speed");
        spec.set("damage", 2.0f32).expect("damage");
        spec.set("owner", 123u64).expect("owner");
        spec.set("damage_tag", "enemy").expect("damage_tag");
        let dir = lua.create_table().expect("direction");
        dir.set("x", 0.0f32).expect("dir x");
        dir.set("y", 1.0f32).expect("dir y");
        spec.set("direction", dir).expect("set direction");
        spawn_projectile.call::<()>(spec).expect("spawn projectile");

        let commands = pending_commands.borrow().clone();
        assert_eq!(commands.len(), 1);
        let request = match &commands[0] {
            ScriptWorldCommand::Spawn { request, .. } => request,
            _ => panic!("expected spawn command"),
        };
        assert_eq!(request.x, 32.0);
        assert_eq!(request.y, 48.0);
        assert!(request.tags.iter().any(|t| t == "projectile"));
        let mut saw_collider = false;
        let mut saw_projectile = false;
        for comp in &request.components {
            match comp {
                ComponentDef::Collider { .. } => saw_collider = true,
                ComponentDef::Projectile {
                    speed,
                    direction,
                    damage,
                    owner_id,
                    damage_tag,
                    ..
                } => {
                    saw_projectile = true;
                    assert_eq!(*speed, 300.0);
                    assert!((direction.x - 0.0).abs() < 0.001);
                    assert!((direction.y - 1.0).abs() < 0.001);
                    assert_eq!(*damage, 2.0);
                    assert_eq!(*owner_id, 123);
                    assert_eq!(damage_tag, "enemy");
                }
                _ => {}
            }
        }
        assert!(saw_collider && saw_projectile);
    }

    #[test]
    fn dry_run_script_supports_combat_and_ai_helpers() {
        let script = r#"
function update(entity, world, dt)
    entity.damage(1)
    entity.heal(0.5)
    entity.knockback(12, -4)
    entity.animation = "attack"
    entity.animation_frame = 2
    entity.flip_x = true
    entity.hitbox.active = true
    entity.hitbox.damage = 3
    local path = world.find_path(entity.x, entity.y, entity.x + 16, entity.y)
    entity.follow_path(path, 90)
    entity.ai.chase(42)
    entity.ai.idle()
    world.spawn_projectile({
        x = entity.x,
        y = entity.y,
        direction = { x = 1, y = 0 },
        speed = 240,
        damage = 1,
        damage_tag = "enemy",
        owner = entity.id
    })
    world.spawn_particles("dust", entity.x, entity.y)
end
"#;
        let result = dry_run_script(script);
        assert!(result.is_ok(), "dry run failed: {:?}", result.err());
    }

    #[test]
    fn backend_snapshot_roundtrip_and_source_lookup() {
        let mut engine = ScriptEngine::default();
        crate::scripting::ScriptBackend::load_script(
            &mut engine,
            "enemy_ai".to_string(),
            "function update(entity, world, dt) end".to_string(),
            true,
        )
        .expect("load script");
        crate::scripting::ScriptBackend::set_vars(
            &mut engine,
            std::collections::HashMap::from([("difficulty".to_string(), serde_json::json!(0.7))]),
        );

        let snapshot = crate::scripting::ScriptBackend::snapshot(&engine);
        assert!(snapshot.scripts.contains_key("enemy_ai"));
        assert!(snapshot.global_scripts.contains("enemy_ai"));
        assert_eq!(
            snapshot
                .vars
                .get("difficulty")
                .and_then(|v| v.as_f64())
                .unwrap_or_default(),
            0.7
        );

        let mut restored = ScriptEngine::default();
        crate::scripting::ScriptBackend::restore_snapshot(&mut restored, snapshot);

        let source = crate::scripting::ScriptBackend::get_script_source(&restored, "enemy_ai")
            .expect("script source");
        assert_eq!(source.name, "enemy_ai");
        assert!(source.global);
        assert!(source.source.contains("update"));
        assert_eq!(
            crate::scripting::ScriptBackend::vars(&restored)
                .get("difficulty")
                .and_then(|v| v.as_f64())
                .unwrap_or_default(),
            0.7
        );
    }

    #[test]
    fn restore_snapshot_drops_invalid_scripts() {
        let mut restored = ScriptEngine::default();
        crate::scripting::ScriptBackend::restore_snapshot(
            &mut restored,
            crate::scripting::ScriptRuntimeSnapshot {
                scripts: HashMap::from([
                    (
                        "ok_script".to_string(),
                        "function update(entity, world, dt) return end".to_string(),
                    ),
                    ("bad_script".to_string(), "function update(".to_string()),
                ]),
                global_scripts: HashSet::from(["ok_script".to_string(), "bad_script".to_string()]),
                vars: HashMap::new(),
            },
        );

        assert!(restored.scripts.contains_key("ok_script"));
        assert!(!restored.scripts.contains_key("bad_script"));
        assert!(restored.global_scripts.contains("ok_script"));
        assert!(!restored.global_scripts.contains("bad_script"));
    }

    #[test]
    fn push_event_tracks_overflow_drops() {
        let mut engine = ScriptEngine::default();
        for i in 0..(MAX_SCRIPT_EVENTS + 17) {
            engine.push_event(crate::scripting::ScriptEvent {
                name: "evt".to_string(),
                data: serde_json::json!({ "i": i }),
                frame: i as u64,
                source_entity: None,
            });
        }
        assert_eq!(engine.events.len(), MAX_SCRIPT_EVENTS);
        assert!(engine.dropped_events >= 17);
    }

    #[test]
    fn lua_budget_guard_interrupts_and_recovers() {
        let lua = Lua::new();
        lua.load(
            r#"
function busy(n)
  let_sum = 0
  for i = 1, n do
    let_sum = let_sum + i
  end
  return let_sum
end
function ok()
  return 42
end
"#,
        )
        .exec()
        .expect("load script");

        let busy: mlua::Function = lua.globals().get("busy").expect("busy");
        let err = call_lua_with_budget(&lua, Duration::ZERO, 1, || {
            busy.call::<i64>(50_000i64).map(|_| ())
        })
        .expect_err("busy script should exceed budget");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("budget") || msg.contains("instruction") || msg.contains("hook"),
            "unexpected error message: {msg}"
        );

        // Ensure the temporary hook is removed after an interrupted call.
        let ok: mlua::Function = lua.globals().get("ok").expect("ok");
        let out = ok.call::<i64>(()).expect("ok call");
        assert_eq!(out, 42);
    }
}
