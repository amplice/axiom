# Axiom Engine — Master Development Plan

## Vision

Axiom is an **AI-native 2D game engine**. The AI (Claude, GPT, etc.) builds games autonomously through an HTTP API. The human provides sprites, audio assets, and taste-tests the result. The engine provides a complete closed feedback loop: generate → validate → simulate → measure → iterate.

Every feature must be:
1. **API-controllable** — the AI can configure it via HTTP
2. **API-observable** — the AI can query its state and verify correctness
3. **Simulatable** — the AI can test it headlessly without a human in the loop
4. **Measurable** — the AI can extract metrics to drive iteration

## Architecture Principles

### Multiplayer-Ready from Day One
The engine must support future networking (up to MMO-scale, 100s-1000s of players) without architectural rewrites. This means:
- **Server-authoritative**: all game logic runs in `FixedUpdate` at a deterministic tick rate (60Hz)
- **All game state in ECS**: no global mutable state outside Bevy resources/components
- **Serializable entities**: every gameplay component must derive `Serialize`/`Deserialize`
- **Input-driven**: all gameplay mutations flow from `VirtualInput`, never from direct state manipulation
- **Entity IDs are stable**: use a `NetworkId(u64)` component (not raw Bevy `Entity`) for cross-session/cross-network entity references. **Every system that references entities across frames, in saves, in scripts, or over the network MUST use `NetworkId`, never raw Bevy `Entity`.** The `NetworkId` is assigned at spawn via a global atomic counter and is unique for the lifetime of the engine process. Save/load must preserve and restore `NetworkId` values and the counter.
- **Authority model**: headless mode = server, windowed mode = client+server. Future networking just adds input replication + state sync
- **No client-only gameplay state**: rendering/audio/UI are client-only, but all gameplay data lives in ECS and could be replicated

### No Player Singleton
`Player` is a **tag**, not a special case. Every physics, movement, and collision system must operate on **any entity that has the relevant components**, not just entities with a `Player` component. Specifically:
- `apply_gravity` runs on all entities with `GravityBody` + `Velocity`, NOT `With<Player>`
- `horizontal_movement` runs on all entities with `HorizontalMover` + `Velocity` + `GamePosition`, NOT `With<Player>`
- `top_down_movement` runs on all entities with `TopDownMover` + `Velocity` + `GamePosition`, NOT `With<Player>`
- `jump_system` runs on all entities with `Jumper` + `Velocity` + `Grounded`, NOT `With<Player>`
- `apply_velocity` (with tile collision resolution via `physics_core::resolve_motion`) runs on all entities with `Velocity` + `GamePosition` + `Collider`, NOT `With<Player>`
- `check_grounded` runs on all entities with `GamePosition` + `Collider` + `Grounded`, NOT `With<Player>`

The `Player` component exists only to identify which entity the camera follows and which entity keyboard input is routed to. It has NO effect on physics, collision, or movement system queries. If a system currently uses `With<Player>` as a filter for physics/movement, that filter must be removed.

Input routing: `VirtualInput` actions are read by movement systems, but only entities whose `NetworkId` matches the local player's ID (or which have the `Player` tag) consume keyboard-driven `VirtualInput`. NPC entities with `HorizontalMover` get their movement from AI/scripts setting their velocity directly, NOT from `VirtualInput`. The movement systems apply velocity and resolve collisions for ALL entities equally — the difference is only in what SETS the velocity (keyboard for player, AI/scripts for NPCs).

### Single Source of Truth for Physics
`simulation.rs` must NOT re-implement physics logic. It MUST call the same functions from `physics_core.rs` that the live ECS systems use. Specifically:
- `physics_core::resolve_motion` for collision resolution
- `physics_core::compute_grounded` for grounding checks
- `physics_core::apply_gravity` for gravity
- `physics_core::try_jump` for jump initiation

If `simulation.rs` currently contains its own gravity, collision, or grounding code that duplicates `physics_core.rs`, that code must be deleted and replaced with calls to the `physics_core` functions. Any physics behavior difference between `/simulate` and live gameplay is a bug.

### Tilemap Abstraction
All systems that read tile data must go through query functions, not access the `Tilemap` struct fields directly. This prepares for future chunked/streaming worlds without rewriting every consumer. Specifically:
- All tile lookups must use `tilemap.get_tile(x, y)` or `tilemap.is_solid(x, y)` methods
- No system should directly index into `tilemap.tiles[...]` except the `Tilemap` implementation itself
- Pathfinding, physics, generation, and rendering all use these methods
- The `Tilemap` struct may change its internal representation (to chunks, streaming, etc.) in the future, but the method signatures (`get_tile`, `is_solid`, `set_tile`, `width`, `height`) must remain stable

This does NOT mean building chunked worlds now. It means accessing tiles through methods so the internals can change later without rewriting consumers.

### Platform-Agnostic Scripting Interface
The scripting system must define a **Rust trait** that abstracts the scripting runtime. The trait defines what a script engine can do; the implementation can be `mlua`/LuaJIT (native), `rhai` (WASM), or any future backend. Specifically:

```rust
pub trait ScriptBackend: Send + Sync {
    fn load_script(&mut self, name: &str, source: &str) -> Result<(), String>;
    fn remove_script(&mut self, name: &str);
    fn run_entity_script(&mut self, name: &str, entity: &mut EntityScriptProxy) -> Result<(), ScriptError>;
    fn run_global_script(&mut self, name: &str, world: &mut WorldScriptProxy) -> Result<(), ScriptError>;
    fn set_var(&mut self, key: &str, value: ScriptValue);
    fn get_var(&self, key: &str) -> Option<ScriptValue>;
    fn list_scripts(&self) -> Vec<String>;
}
```

`ScriptEngine` holds a `Box<dyn ScriptBackend>`. On native builds, the backend is `LuaBackend` (using `mlua`). On WASM builds, the backend is `RhaiBackend` (using `rhai`) or a `NoopBackend` stub. No system outside of the backend implementation should import or reference `mlua` types directly. All script interaction goes through the trait.

This means:
- `src/scripting/vm.rs` implements `ScriptBackend` for `mlua` (native only, behind `#[cfg(not(target_arch = "wasm32"))]`)
- `src/scripting/vm_wasm.rs` implements `ScriptBackend` for the WASM target (currently a no-op stub, will become `rhai` later)
- `src/scripting/mod.rs` defines the `ScriptBackend` trait and `ScriptEngine` resource
- Entity script runner and global script runner systems call `ScriptEngine.backend.run_entity_script(...)` etc.
- No `mlua::` types appear outside of `vm.rs`

### WASM-Compatible
All code must compile to WASM for web export. This means:
- No `std::fs` in gameplay code (use Bevy's asset system)
- No `std::thread::spawn` in gameplay code (use Bevy's task system)
- No native-only crates in the gameplay path
- HTTP API server is desktop-only (WASM games run standalone without API)
- Feature-gate the API server behind `#[cfg(not(target_arch = "wasm32"))]`
- **Scripting runtime**: `mlua`/LuaJIT is native-only (lua-src cannot compile to `wasm32-unknown-unknown`). WASM builds use `rhai` (pure-Rust, compiles to WASM natively). Both backends implement the `ScriptBackend` trait (see "Platform-Agnostic Scripting Interface" above). Do NOT attempt to compile `mlua` with `lua54` feature for WASM — it will fail because `lua-src` panics for `wasm32-unknown-unknown`. Do NOT use `piccolo`. Use `rhai`.

### Lua Scripting as First-Class
Game logic lives in Lua scripts, not Rust. Rust provides the systems/components/API; Lua provides the game-specific behavior. The AI writes Lua, the engine runs it.

---

## Current State (Completed)

- Bevy 0.15 ECS foundation
- Composable behavior components (Collider, GravityBody, HorizontalMover, Jumper, TopDownMover)
- HTTP API: /state, /player, /level, /physics, /config, /simulate, /validate, /feel/*, /generate, /entities, /sprites, /screenshot
- Procedural generation (4 templates: platformer, top_down_dungeon, rts_arena, fighting_arena)
- Platformer physics (gravity, AABB collision, grounded detection, coyote time, jump buffering)
- Top-down physics (4-direction movement, wall collision)
- Constraint validation (reachability, top-down reachability, bounds check, no-softlock)
- Pathfinding solver (platformer bot, 97% solve rate)
- Feel measurement + auto-tuning (jump profiles, Celeste comparison)
- VirtualInput abstraction (keyboard → action names → systems)
- Headless + windowed modes
- Entity CRUD via API with presets
- Sprite rendering (basic colored rectangles + sprite loading)
- Camera follow system

### Resolved Architectural Issues

All five architectural issues identified during audit have been resolved by the implementation:

1. **Player singleton** — RESOLVED. Physics systems use `Option<&Player>` for input routing, not `With<Player>` for system filtering. All entities with relevant components get physics/movement/collision.
2. **Simulation physics deduplication** — RESOLVED. `simulation.rs` calls `physics_core::` functions exclusively (10 delegated calls, zero reimplemented logic).
3. **Tilemap abstraction** — RESOLVED. `Tilemap` has `get_tile()`, `set_tile()`, `is_solid()`, `is_ground()` methods. All consumers use them.
4. **Scripting trait abstraction** � RESOLVED. `ScriptBackend` trait defined in `scripting/mod.rs` with 9 methods. Implemented by `vm.rs` (native Lua) and `vm_wasm.rs` (WASM Rhai runtime with Lua-compat normalization). No `mlua::` types leak outside `vm.rs`.
5. **NetworkId consistency** — RESOLVED. Save/load preserves `NetworkId` values and the `next_network_id` counter.
6. **Simulation/physics parity** — RESOLVED. `simulation.rs` delegates all physics to `physics_core.rs` functions; no reimplemented logic.
7. **Scripting backend parity** — RESOLVED. Rhai backend (`vm_wasm.rs`) covers ~95%+ of the Lua backend API surface: entity read/write (position, velocity, health, tags, hitbox, animation), world functions (tile queries including `is_platform`/`is_climbable`/`tile_friction`, entity queries, pathfinding, raycasting, events), camera/UI/dialogue/input/game-state functions, and spawning/despawning. Lua-to-Rhai transpiler handles `for`/`while`/`repeat` loops, table constructors, string concatenation, and nested-to-flat API call rewriting (`world.camera.shake()` → `camera_shake()`, etc.).
8. **Velocity clamping** — RESOLVED. `MAX_FALL_SPEED` constant (800.0) in `physics_core.rs` prevents tunneling through floors at extreme velocities.
9. **SpawnEntity/DeleteEntity observability** — RESOLVED. Both commands respond via `commands.queue()` closure, ensuring the API response is sent only after the entity actually exists in (or is removed from) the World.

### Remaining Issues

None currently blocking. All engine-layer success criteria from Phases 1-12 are met. Remaining work is game-layer (audio/UI rendering wiring, example games) and CI polish (multi-seed fuzz testing, code splitting).

---

## Phase 1: Lua Scripting Engine

**Goal**: AI can write game logic as Lua scripts, uploaded via API, hot-reloaded without recompiling Rust. This is the foundation for everything — every future phase exposes Lua bindings.

**Unlocks**: Custom enemy behaviors, puzzle mechanics, game rules, win conditions, dialogue logic, ability systems — anything the AI can express in code.

**Crate**: `mlua` (with `luajit` feature for performance, `serialize` feature for serde bridge)

### 1.1 Core Lua VM

**New file: `src/scripting/mod.rs`**

```rust
pub mod vm;
pub mod bridge;
pub mod api;
```

**File: `src/scripting/mod.rs`** — defines the `ScriptBackend` trait (see Architecture Principles above) and `ScriptEngine` resource.

**File: `src/scripting/vm.rs`** (native only, `#[cfg(not(target_arch = "wasm32"))]`)

`LuaBackend` implements `ScriptBackend`:
- Holds `mlua::Lua` VM instance
- Stores scripts by name: `HashMap<String, String>` (name → source)
- Compiles scripts on load, caches compiled chunks
- Implements `load_script(name, source)`, `remove_script(name)`, `list_scripts()`, `run_entity_script(...)`, `run_global_script(...)`
- Error handling: script errors are captured, not panics. Stored in `ScriptErrors` resource for API query.
- No `mlua::` types appear in public interfaces — all interaction goes through `ScriptBackend` trait methods.

Resource `ScriptEngine`:
- Holds `backend: Box<dyn ScriptBackend>`
- On native: `backend = Box::new(LuaBackend::new())`
- On WASM: `backend = Box::new(RhaiBackend::new())` (or `NoopBackend` until rhai is implemented)

Resource `ScriptErrors`:
- `Vec<ScriptError>` with fields: `script_name`, `entity_id`, `error_message`, `frame`
- Capped at 100 errors (ring buffer)
- Queryable via API

Component `LuaScript`:
- `script_name: String` — references a script in ScriptEngine
- `state: mlua::Value` — per-entity Lua table for local state (e.g., patrol timer, cooldown)
- `enabled: bool`

System `run_entity_scripts` (runs in `FixedUpdate` after physics):
- For each entity with `LuaScript` component
- Calls the script's `update(entity, dt)` function
- Passes entity proxy + world proxy as arguments
- Catches errors, logs to `ScriptErrors`

System `run_global_scripts` (runs in `FixedUpdate` after entity scripts):
- For scripts registered as "global" (game rules, win conditions, scoring)
- Calls `update(world, dt)`

### 1.2 Entity Bridge (Lua API)

**New file: `src/scripting/bridge.rs`**

The Lua `entity` object exposes:
```lua
-- Position & movement
entity.x                    -- read/write
entity.y                    -- read/write
entity.vx                   -- read/write
entity.vy                   -- read/write
entity.speed                -- from HorizontalMover/TopDownMover

-- State
entity.grounded             -- bool, read-only
entity.alive                -- bool, read/write
entity.health               -- from Health component, read/write
entity.max_health            -- read-only
entity.id                   -- NetworkId, read-only

-- Tags / identity
entity.has_tag(tag)         -- check if entity has a string tag
entity.add_tag(tag)         -- add a tag
entity.remove_tag(tag)

-- Custom state (per-entity Lua table, persists between frames)
entity.state.my_variable = 42
entity.state.patrol_timer = entity.state.patrol_timer - dt

-- Component queries
entity.has("GravityBody")   -- check component existence
entity.has("Jumper")
```

The Lua `world` object exposes:
```lua
-- Tilemap
world.is_solid(x, y)           -- tile query
world.get_tile(x, y)           -- returns tile type id
world.set_tile(x, y, type)     -- modify tilemap at runtime

-- Input
world.input.pressed("left")    -- current frame input
world.input.just_pressed("jump")

-- Entity queries
world.find_nearest(x, y, tag)           -- find nearest entity with tag
world.find_in_radius(x, y, radius, tag) -- all entities in radius
world.find_all(tag)                      -- all entities with tag
world.get_entity(id)                     -- get by NetworkId
world.player()                           -- shortcut for player entity

-- Spawning
world.spawn({
    x = 100, y = 50,
    components = {"Collider", "GravityBody"},
    script = "enemy_patrol",
    tags = {"enemy", "skeleton"},
    health = 3,
})
world.despawn(entity_id)

-- Events (write to event bus, read by other scripts / engine systems)
world.emit("player_hit", { damage = 1, source = entity.id })
world.on("player_hit", function(data) ... end)  -- register handler

-- Game state
world.get_var("score")         -- global game variables
world.set_var("score", 100)
world.get_var("level")

-- Audio (added in Phase 4, but bridge slot defined now)
-- world.play_sfx("hit")
-- world.play_music("dungeon_theme")

-- Camera (added in Phase 5, but bridge slot defined now)
-- world.camera.shake(0.5)
-- world.camera.zoom(2.0)

-- Time
world.dt                       -- delta time (fixed timestep)
world.frame                    -- current frame number
world.time                     -- total elapsed time
```

### 1.3 Script API Endpoints

**New file: `src/scripting/api.rs`** (or extend `src/api/mod.rs`)

```
POST   /scripts              — upload script { name: "enemy_patrol", source: "function update(e, dt)..." }
GET    /scripts              — list all loaded scripts
GET    /scripts/{name}       — get script source
DELETE /scripts/{name}       — remove script
POST   /scripts/{name}/test  — dry-run script against a mock entity, return errors if any

GET    /scripts/errors       — get recent script errors
POST   /scripts/vars         — set global game variables { "score": 0, "level": 1 }
GET    /scripts/vars         — get all global game variables
GET    /scripts/events       — get recent event bus history (for debugging)
```

### 1.4 Script Component in Entity API

Extend `POST /entities` and `POST /entities/preset` to accept `script` field:
```json
{
    "x": 100, "y": 50,
    "components": [{"type": "collider", "width": 12, "height": 14}],
    "script": "enemy_patrol",
    "tags": ["enemy"],
    "is_player": false
}
```

New component: `Tags(HashSet<String>)` — arbitrary string tags for entity classification.

### 1.5 Testing Criteria

1. Upload a Lua script via API that makes an entity patrol left-right
2. Spawn entity with that script
3. `/simulate` for 120 frames, verify entity position oscillates
4. Upload a broken script, verify error appears in `/scripts/errors`
5. Hot-reload: upload modified script, verify behavior changes next frame without restart
6. Global script: upload a "win condition" script that emits "level_complete" when player reaches goal tile
7. `/simulate` reports the emitted event in its output

### 1.6 Files

| File | Action | Description |
|------|--------|-------------|
| `Cargo.toml` | Modify | Add `mlua` dependency with `luajit` + `serialize` features (native only). Add `rhai` dependency (WASM only, can be deferred to Phase 12). |
| `src/scripting/mod.rs` | Create | `ScriptBackend` trait definition, `ScriptEngine` resource (holds `Box<dyn ScriptBackend>`), `LuaScript` component, `ScriptErrors` resource, runner systems that call through the trait |
| `src/scripting/vm.rs` | Create | `LuaBackend` implementing `ScriptBackend` using `mlua`. Native only (`#[cfg(not(target_arch = "wasm32"))]`). No `mlua::` types in public API. |
| `src/scripting/vm_wasm.rs` | Create | `NoopBackend` implementing `ScriptBackend` as a stub. WASM only (`#[cfg(target_arch = "wasm32")]`). Will be replaced with `RhaiBackend` in Phase 12. |
| `src/scripting/bridge.rs` | Create | Entity proxy, world proxy — these are backend-agnostic data structures passed to `ScriptBackend` methods |
| `src/scripting/api.rs` | Create | HTTP endpoints for script management |
| `src/components.rs` | Modify | Add `Tags`, `NetworkId` components |
| `src/api/mod.rs` | Modify | Register script routes, extend entity spawn to accept `script`/`tags` |
| `src/api/types.rs` | Modify | Add script fields to EntitySpawnRequest |
| `src/main.rs` | Modify | Register scripting plugin |
| `src/simulation.rs` | Modify | Run scripts during simulation, report script events. **Physics logic must call `physics_core.rs` functions, not re-implement them.** |

### 1.7 Multiplayer Considerations

- Scripts run server-side only (in headless/authority mode)
- Script state is part of entity state (serializable for replication)
- `NetworkId` component assigned at spawn, stable across sessions
- Global game variables stored in a `GameVars` resource (serializable)

### 1.8 WASM Considerations

- `mlua` with `luajit` feature does NOT compile to WASM. `lua-src` panics with "don't know how to build Lua for wasm32-unknown-unknown". This is a fundamental blocker, not a configuration issue.
- Do NOT attempt `mlua` with `lua54` for WASM. Do NOT use `piccolo`.
- For WASM builds, use `rhai` (pure-Rust scripting engine, compiles to WASM natively).
- Both `mlua` (native) and `rhai` (WASM) implement the `ScriptBackend` trait defined in `src/scripting/mod.rs`.
- Feature-gate: `#[cfg(not(target_arch = "wasm32"))]` uses `mlua`/LuaJIT via `LuaBackend`. `#[cfg(target_arch = "wasm32")]` uses `rhai` via `RhaiBackend`.
- Since Axiom is AI-native, script source code is generated by the AI. The AI targets whichever scripting language the platform requires. Lua syntax and rhai syntax differ, but the scripting API surface (entity table, world functions, events) is identical across both backends.
- The `ScriptBackend` trait is the contract. No code outside `vm.rs` or `vm_wasm.rs` should depend on the scripting language syntax.

---

## Phase 2: Entity Interaction System

**Goal**: Entities can collide with each other, deal damage, trigger events, collect pickups. This is what turns Axiom from a tilemap simulator into a game engine.

**Unlocks**: Combat (melee, ranged, projectiles), collectibles, triggers/switches, hazards, NPCs that react to the player, boss fights.

**Depends on**: Phase 1 (Lua scripts define interaction behaviors)

### 2.1 New Components

All in `src/components.rs`:

```rust
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct ContactDamage {
    pub amount: f32,
    pub cooldown_frames: u32,       // prevent rapid re-hits
    pub knockback: f32,             // knockback force on hit
    pub damage_tag: String,         // only damages entities with this tag (e.g., "player")
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Invincibility {
    pub frames_remaining: u32,      // i-frames after taking damage
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct TriggerZone {
    pub radius: f32,                // circular trigger area
    pub trigger_tag: String,        // what activates it (e.g., "player")
    pub event_name: String,         // event emitted when triggered
    pub one_shot: bool,             // destroy after triggering?
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Pickup {
    pub pickup_tag: String,         // what can pick it up (e.g., "player")
    pub effect: PickupEffect,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum PickupEffect {
    Heal(f32),
    ScoreAdd(i32),
    Custom(String),                 // fires a Lua event
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Projectile {
    pub speed: f32,
    pub direction: Vec2,
    pub lifetime_frames: u32,
    pub damage: f32,
    pub owner_id: u64,              // NetworkId of who fired it (no self-damage)
    pub damage_tag: String,         // what it damages
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Hitbox {
    pub width: f32,
    pub height: f32,
    pub offset: Vec2,               // offset from entity center
    pub active: bool,               // only checks collision when active
    pub damage: f32,
    pub damage_tag: String,
}
```

### 2.2 New Systems

**New file: `src/interaction.rs`**

System `entity_collision_detection` (runs in `FixedUpdate` after `apply_velocity`):
- For every pair of entities with `Collider`, do AABB overlap test
- Use spatial hash grid for performance (required for MMO-scale: O(n) instead of O(n²))
- Output: `CollisionEvents` resource — `Vec<CollisionEvent>` with `entity_a`, `entity_b`, `overlap`
- Spatial hash cell size = max entity collider size (e.g., 32px)

System `contact_damage_system`:
- For each collision where one entity has `ContactDamage` and the other has matching tag + `Health`
- Apply damage, respecting `Invincibility` i-frames and cooldown
- Apply knockback to velocity
- Emit "entity_damaged" event to script event bus
- If health <= 0, emit "entity_died" event, set `Alive(false)`

System `trigger_zone_system`:
- For each entity with `TriggerZone`, check distance to all entities with matching tag
- When entity enters radius: emit the trigger's event
- If `one_shot`, mark for despawn

System `pickup_system`:
- For each collision where one entity has `Pickup` and the other has matching tag
- Apply effect (heal, score, or fire custom Lua event)
- Despawn pickup entity
- Emit "pickup_collected" event

System `projectile_system`:
- Move projectiles along their direction at their speed
- Decrement lifetime, despawn at 0
- Check collision with tilemap (despawn on solid) and entities (apply damage, despawn)

System `hitbox_system`:
- For entities with `Hitbox` where `active == true`
- Check AABB overlap with other entities (offset from parent position)
- Apply damage on overlap
- Used for: melee attacks, sword swings, etc. (Lua script sets `hitbox.active = true` for attack frames)

System `invincibility_system`:
- Decrement `Invincibility.frames_remaining` each frame
- Remove component when it reaches 0

System `death_system`:
- For entities with `Alive(false)`:
  - If the entity has the `"player"` string tag (checked via `Tags` component, NOT via a `Player` component filter): respawn at spawn point instead of despawning
  - Otherwise: despawn after a configurable delay (for death animation)
- Note: this is NOT checking for a `Player` ECS component. It checks for the string tag `"player"` in the entity's `Tags` set. This is consistent with the "No Player Singleton" principle.

### 2.3 Spatial Hash Grid

**New file: `src/spatial_hash.rs`**

Resource `SpatialHash`:
- `HashMap<(i32, i32), Vec<Entity>>` — cell coordinates → entities in that cell
- Rebuilt every frame from entity positions
- `query_radius(x, y, radius) -> Vec<Entity>`
- `query_rect(min_x, min_y, max_x, max_y) -> Vec<Entity>`
- Cell size configurable (default: 64px)
- This is essential for MMO-scale: 1000s of entities need O(n) collision, not O(n²)

### 2.4 Event Bus

**New file: `src/events.rs`**

Resource `GameEventBus`:
- `Vec<GameEvent>` with fields: `name: String`, `data: serde_json::Value`, `frame: u64`, `source_entity: Option<u64>`
- Systems and Lua scripts can emit events
- Systems and Lua scripts can subscribe to events
- Events are cleared each frame after processing
- Recent events stored in ring buffer for API query (debugging)

### 2.5 API Extensions

```
# Existing endpoints gain new fields:
POST /entities  — accepts health, contact_damage, trigger_zone, pickup, projectile, hitbox components
GET  /entities  — returns health, alive status, tags

# New endpoints:
GET  /events              — recent game events (for debugging/AI observation)
GET  /events/subscribe    — SSE stream of events in real-time (for live AI monitoring)
POST /entities/{id}/damage — manually deal damage to entity (testing)

# Simulation extensions:
POST /simulate — response now includes:
  - entity_events: [{frame, type: "damaged"|"died"|"pickup"|"trigger", entities: [...], data: {...}}]
  - entity_states: final state of all entities (health, alive, position)
```

### 2.6 Lua Bindings

```lua
-- In entity scripts:
entity.health           -- current health (read/write)
entity.max_health       -- max health (read-only)
entity.alive            -- alive status
entity.damage(amount)   -- deal damage to this entity
entity.heal(amount)
entity.knockback(dx, dy)

-- Hitbox control (for melee attacks):
entity.hitbox.active = true
entity.hitbox.damage = 2

-- Spawning projectiles:
world.spawn_projectile({
    x = entity.x, y = entity.y,
    direction = {1, 0},     -- rightward
    speed = 300,
    damage = 1,
    damage_tag = "enemy",
    owner = entity.id,
    lifetime = 60,
})

-- Events:
world.emit("door_opened", { door_id = 5 })
world.on("player_died", function(data)
    world.set_var("lives", world.get_var("lives") - 1)
end)
```

### 2.7 Testing Criteria

1. Spawn player + enemy with `ContactDamage`, walk player into enemy, verify health decreases
2. `/simulate`: player walks through 3 enemies, verify damage events and final health
3. Spawn pickup, player walks over it, verify health restored / score increased
4. Lua script enemy that fires projectiles at player every 60 frames
5. `/simulate`: verify projectile hits player, deals damage
6. Trigger zone at door: player enters radius, "door_open" event fires
7. 500 entities with collision: verify no performance regression (spatial hash)
8. Player dies (health=0), respawns at spawn point

### 2.8 Files

| File | Action | Description |
|------|--------|-------------|
| `src/components.rs` | Modify | Health, ContactDamage, Invincibility, TriggerZone, Pickup, Projectile, Hitbox |
| `src/interaction.rs` | Create | All interaction systems |
| `src/spatial_hash.rs` | Create | Spatial hash grid for O(n) collision |
| `src/events.rs` | Create | GameEventBus |
| `src/physics.rs` | Modify | Integrate with spatial hash, add collision output. **Remove all `With<Player>` filters** — systems must run for all entities with relevant components. |
| `src/simulation.rs` | Modify | Run interaction systems, report entity events. **All physics must call `physics_core.rs` functions.** |
| `src/scripting/bridge.rs` | Modify | Add health/damage/hitbox/projectile/event Lua bindings |
| `src/api/mod.rs` | Modify | Event endpoints, entity damage endpoint |
| `src/api/types.rs` | Modify | New component types in EntitySpawnRequest |
| `src/spawn.rs` | Modify | Handle new component types |

---

## Phase 3: Animation System

**Goal**: Sprite sheet animations with state machines. Entities visually animate (run, idle, attack, hurt, die). AI can define animations via API, observe animation state.

**Unlocks**: Visual feedback for all gameplay. Entities look alive. Screenshots become meaningful for AI evaluation.

**Depends on**: Phase 2 (attack animations trigger hitboxes)

### 3.1 New Components

```rust
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct SpriteSheet {
    pub texture_path: String,           // e.g., "assets/player.png"
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,                   // frames per row in sheet
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct AnimationController {
    pub animations: HashMap<String, AnimationDef>,  // "idle", "run", "attack", "hurt", "die"
    pub current: String,
    pub frame: usize,
    pub timer: f32,
    pub playing: bool,
    pub flip_x: bool,                   // face left/right
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AnimationDef {
    pub frames: Vec<usize>,             // indices into sprite sheet
    pub fps: f32,
    pub looping: bool,
    pub next: Option<String>,           // animation to play after this one finishes (if not looping)
    pub events: Vec<AnimFrameEvent>,    // events triggered at specific frames
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AnimFrameEvent {
    pub frame: usize,                   // which frame of the animation
    pub event: String,                  // "hitbox_on", "hitbox_off", "sfx:slash", "spawn_particle"
}
```

### 3.2 Systems

System `animation_update` (runs in `Update`, NOT `FixedUpdate` — visual only):
- For each `AnimationController`: advance timer, advance frame
- When animation ends (non-looping): transition to `next` animation or stop
- Process `AnimFrameEvent`s: emit events to event bus at the right frames
- This enables: attack animations that activate hitboxes at specific frames

System `sprite_sheet_render`:
- For each entity with `SpriteSheet` + `AnimationController`
- Set the `Sprite` atlas index based on current frame
- Handle `flip_x` for directional facing

System `auto_animation`:
- Automatically sets animation based on entity state:
  - Moving + grounded → "run"
  - Not moving + grounded → "idle"
  - Airborne + vy > 0 → "jump"
  - Airborne + vy < 0 → "fall"
  - Recently damaged → "hurt" (override for N frames)
  - Dead → "die" (one-shot)
- Can be overridden by Lua scripts setting `entity.animation = "attack"`

### 3.3 API

```
POST /sprites/sheets         — define a sprite sheet
{
    "name": "player",
    "path": "assets/player.png",
    "frame_width": 32,
    "frame_height": 32,
    "columns": 8,
    "animations": {
        "idle": { "frames": [0,1,2,3], "fps": 8, "looping": true },
        "run":  { "frames": [4,5,6,7,8,9], "fps": 12, "looping": true },
        "attack": {
            "frames": [10,11,12,13],
            "fps": 15,
            "looping": false,
            "next": "idle",
            "events": [
                { "frame": 2, "event": "hitbox_on" },
                { "frame": 3, "event": "hitbox_off" }
            ]
        },
        "hurt": { "frames": [14,15], "fps": 10, "looping": false, "next": "idle" },
        "die":  { "frames": [16,17,18,19], "fps": 8, "looping": false }
    }
}

GET  /sprites/sheets         — list defined sheets
POST /entities/{id}/animation — force animation state { "animation": "attack" }
GET  /entities/{id}/animation — current animation state
```

### 3.4 Lua Bindings

```lua
entity.animation = "attack"              -- set current animation
entity.animation_frame                   -- current frame index (read-only)
entity.flip_x = true                     -- face left
entity.sprite_sheet = "player"           -- assign sprite sheet

world.on("anim:hitbox_on", function(data)
    data.entity.hitbox.active = true
end)
```

### 3.5 API Observation

Extend `/scene/describe` (new endpoint):
```json
{
    "camera": {"x": 120, "y": 80, "zoom": 2.0},
    "visible_entities": [
        {
            "id": 1,
            "pos": [120, 80],
            "tags": ["player"],
            "animation": "run",
            "animation_frame": 3,
            "flip_x": false,
            "health": 5,
            "alive": true,
            "sprite_sheet": "player"
        }
    ],
    "tile_summary": {"solid": 45, "empty": 120, "spike": 3, "goal": 1}
}
```

This endpoint is how the AI "sees" the game structurally — more useful than screenshots for gameplay verification.

### 3.6 Particle Effects

Lightweight particle system for juice:

```rust
#[derive(Component)]
pub struct ParticleEmitter {
    pub texture_path: Option<String>,       // None = colored rectangles
    pub color_start: Color,
    pub color_end: Color,
    pub size_start: f32,
    pub size_end: f32,
    pub lifetime: f32,                      // seconds per particle
    pub emit_rate: f32,                     // particles per second
    pub spread_angle: f32,                  // emission cone
    pub speed_min: f32,
    pub speed_max: f32,
    pub gravity_multiplier: f32,            // 0 = no gravity on particles
    pub one_shot: bool,                     // burst then stop
    pub burst_count: u32,                   // particles per burst (if one_shot)
}
```

API:
```
POST /entities/{id}/particles — attach/configure particle emitter
POST /particles/presets       — define named presets: "blood_splash", "sparkle", "dust", "explosion"
```

Lua:
```lua
world.spawn_particles("blood_splash", entity.x, entity.y)
entity.particles = "dust_trail"     -- continuous emitter while moving
```

### 3.7 Testing Criteria

1. Upload sprite sheet, assign to player, verify `/screenshot` shows animated sprite
2. Player moves right → "run" animation plays, flipped correctly
3. Attack animation triggers hitbox at frame 2, deactivates at frame 3
4. `/scene/describe` returns correct animation state for all visible entities
5. Particle emitter spawns particles, visible in screenshot
6. `/simulate` reports animation-triggered events (hitbox_on, etc.)

### 3.8 Files

| File | Action | Description |
|------|--------|-------------|
| `src/animation.rs` | Create | AnimationController, SpriteSheet, systems |
| `src/particles.rs` | Create | ParticleEmitter, particle systems |
| `src/components.rs` | Modify | New component definitions |
| `src/render.rs` | Modify | Integrate sprite sheet rendering |
| `src/scripting/bridge.rs` | Modify | Animation + particle Lua bindings |
| `src/api/mod.rs` | Modify | Animation + particle + /scene/describe endpoints |

---

## Phase 4: Audio System

**Goal**: Sound effects and music, fully API-controlled. AI configures audio; human evaluates by ear during playtesting.

**Unlocks**: Game feel polish. Sound feedback for jumps, hits, pickups, deaths, ambient atmosphere.

**Depends on**: Phase 2 (audio triggered by gameplay events)

### 4.1 Architecture

Use `bevy_audio` (built-in) + `kira` backend for advanced features (crossfade, spatial audio).

Resource `AudioManager`:
- `sfx: HashMap<String, Handle<AudioSource>>` — named sound effects
- `music_current: Option<String>` — currently playing music track
- `music_volume: f32`
- `sfx_volume: f32`
- `master_volume: f32`

### 4.2 API

```
POST /audio/sfx             — define sound effects
{
    "effects": {
        "jump":     { "path": "assets/audio/jump.ogg", "volume": 0.8 },
        "hit":      { "path": "assets/audio/hit.ogg", "volume": 1.0, "pitch_variance": 0.1 },
        "pickup":   { "path": "assets/audio/pickup.ogg" },
        "death":    { "path": "assets/audio/death.ogg" },
        "footstep": { "path": "assets/audio/step.ogg", "volume": 0.3, "pitch_variance": 0.2 }
    }
}

POST /audio/music            — define music tracks
{
    "tracks": {
        "menu":    { "path": "assets/audio/menu.ogg", "volume": 0.5, "looping": true },
        "dungeon": { "path": "assets/audio/dungeon.ogg", "volume": 0.4, "looping": true },
        "boss":    { "path": "assets/audio/boss.ogg", "volume": 0.6, "looping": true }
    }
}

POST /audio/play             — play a sound: { "sfx": "jump" } or { "music": "dungeon", "fade_in": 1.0 }
POST /audio/stop             — stop: { "music": true, "fade_out": 2.0 }
POST /audio/config           — { "master_volume": 0.8, "sfx_volume": 1.0, "music_volume": 0.5 }
GET  /audio/state            — currently playing music, volume levels, recent SFX triggers
```

### 4.3 Automatic SFX Triggers

System `auto_audio`:
- Listens to `GameEventBus` for gameplay events
- Plays appropriate SFX based on configurable mapping:

```
POST /audio/triggers          — map gameplay events to sounds
{
    "entity_damaged": "hit",
    "entity_died": "death",
    "pickup_collected": "pickup",
    "jump_start": "jump",
    "land": "footstep",
    "projectile_fire": "shoot",
    "trigger_activated": "switch"
}
```

### 4.4 Lua Bindings

```lua
world.play_sfx("hit")
world.play_sfx("hit", { volume = 0.5, pitch = 1.2 })
world.play_music("boss", { fade_in = 2.0 })
world.stop_music({ fade_out = 1.0 })
world.set_volume("master", 0.8)
```

### 4.5 Simulation Integration

`/simulate` response includes audio event log:
```json
{
    "audio_events": [
        { "frame": 10, "type": "sfx", "name": "jump" },
        { "frame": 45, "type": "sfx", "name": "hit" },
        { "frame": 45, "type": "music", "name": "boss", "action": "start" }
    ]
}
```

This lets the AI verify the right sounds play at the right times without hearing them.

### 4.6 WASM Considerations

- `bevy_audio` works with WASM (uses WebAudio API)
- Audio files must be loadable via Bevy's asset system (works in both native and WASM)
- No filesystem access in WASM — assets bundled at build time or fetched via HTTP

### 4.7 Testing Criteria

1. Define SFX via API, trigger manually, verify audio plays (human test)
2. Set up auto-triggers, jump in game, verify jump sound plays
3. Enemy hits player, verify hit sound plays
4. `/simulate` reports correct audio events at correct frames
5. Music crossfade: switch from "dungeon" to "boss" with 2s fade
6. `/audio/state` reports correct current state

### 4.8 Files

| File | Action | Description |
|------|--------|-------------|
| `src/audio.rs` | Create | AudioManager, systems, auto-trigger |
| `src/scripting/bridge.rs` | Modify | Audio Lua bindings |
| `src/api/mod.rs` | Modify | Audio endpoints |
| `src/simulation.rs` | Modify | Track audio events during simulation |

---

## Phase 5: Camera System

**Goal**: Configurable camera with zoom, bounds, smoothing, screen shake, split-screen support. AI-controllable for cinematic moments and gameplay tuning.

**Depends on**: None (can be done in parallel with Phase 3-4)

### 5.1 Components/Resources

```rust
#[derive(Resource, Serialize, Deserialize)]
pub struct CameraConfig {
    pub follow_target: Option<u64>,     // NetworkId of entity to follow
    pub follow_speed: f32,              // lerp speed (0-1)
    pub zoom: f32,                      // 1.0 = default, 2.0 = zoomed in, 0.5 = zoomed out
    pub bounds: Option<CameraBounds>,   // min/max world position
    pub offset: Vec2,                   // look-ahead offset
    pub deadzone: Vec2,                 // don't move camera until target moves this far
}

pub struct CameraBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

#[derive(Component)]
pub struct ScreenShake {
    pub intensity: f32,
    pub duration: f32,
    pub decay: f32,                     // how fast it reduces
    pub remaining: f32,
}
```

### 5.2 Systems

- `camera_follow`: follow target entity with smoothing, deadzone, look-ahead
- `camera_bounds`: clamp to level bounds
- `camera_zoom`: apply zoom (modify OrthographicProjection scale)
- `camera_shake`: apply random offset when ScreenShake active, decay over time
- Auto-calculate bounds from tilemap dimensions

### 5.3 API

```
POST /camera/config     — { "zoom": 2.0, "follow_speed": 0.1, "bounds": "auto" }
POST /camera/shake      — { "intensity": 5.0, "duration": 0.3 }
POST /camera/look_at    — { "x": 200, "y": 100, "speed": 0.05 }  (cinematic pan)
GET  /camera/state      — current position, zoom, target
```

### 5.4 Lua Bindings

```lua
world.camera.shake(5.0, 0.3)           -- intensity, duration
world.camera.zoom(2.0)
world.camera.look_at(200, 100)
world.camera.follow(entity.id)
world.camera.bounds = { min_x = 0, max_x = 480, min_y = 0, max_y = 400 }
```

### 5.5 Testing Criteria

1. Set zoom to 2.0, verify screenshot shows zoomed view
2. Camera follows player smoothly, doesn't go outside bounds
3. Screen shake on damage, verify it decays
4. `/simulate` trace includes camera position at each sample point
5. Cinematic: camera pans from point A to B, then returns to player

---

## Phase 6: UI System

**Goal**: API-driven UI for menus, HUD, dialogue boxes, inventory screens. The AI creates UI by posting JSON, not by writing rendering code.

**Unlocks**: Title screens, pause menus, health bars, score displays, dialogue systems, inventory, shops.

**Depends on**: Phase 3 (sprites for UI elements)

### 6.1 Architecture

UI is a tree of **UI nodes**, each defined by JSON. The engine renders them. The AI builds UIs by posting node trees. Player interaction (button clicks, menu navigation) generates input events that Lua scripts handle.

This is NOT a visual editor. The AI constructs UIs programmatically.

### 6.2 UI Node Types

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct UiNode {
    pub id: String,
    pub node_type: UiNodeType,
    pub position: UiPosition,           // anchor + offset
    pub size: UiSize,                   // fixed, percentage, or auto
    pub visible: bool,
    pub children: Vec<UiNode>,
    pub style: UiStyle,
}

pub enum UiNodeType {
    Panel { color: Color },
    Text { text: String, font_size: f32, color: Color, align: TextAlign },
    Image { path: String },
    Button { text: String, action: String },       // action = event name emitted on click
    ProgressBar { value: f32, max: f32, color: Color, bg_color: Color },
    Container { direction: Direction, gap: f32 },   // flex layout
    DialogueBox { speaker: String, text: String, choices: Vec<DialogueChoice> },
    Slot { index: usize },                          // inventory slot
}

pub enum UiPosition {
    Anchored { anchor: Anchor, offset: Vec2 },     // relative to screen
    WorldSpace { x: f32, y: f32 },                  // follows world position (health bars)
    FollowEntity { entity_id: u64, offset: Vec2 },  // health bar above entity
}
```

### 6.3 UI Screens

UI is organized into **screens** (layers that can be shown/hidden):

```
POST /ui/screens            — define a screen
{
    "name": "hud",
    "layer": 0,
    "nodes": [
        {
            "id": "health_bar",
            "node_type": { "ProgressBar": { "value": 5, "max": 5, "color": "red", "bg_color": "dark_red" }},
            "position": { "Anchored": { "anchor": "top_left", "offset": [16, 16] }},
            "size": { "fixed": [200, 20] }
        },
        {
            "id": "score",
            "node_type": { "Text": { "text": "Score: 0", "font_size": 24, "color": "white" }},
            "position": { "Anchored": { "anchor": "top_right", "offset": [-16, 16] }}
        }
    ]
}

POST /ui/screens/{name}/show
POST /ui/screens/{name}/hide
POST /ui/screens/{name}/nodes/{id}  — update a specific node
{
    "node_type": { "Text": { "text": "Score: 42", "font_size": 24, "color": "white" }}
}
```

### 6.4 Dialogue System

Built on top of the UI system:

```
POST /dialogue/conversations    — define dialogue trees
{
    "name": "shopkeeper_intro",
    "nodes": [
        { "id": "start", "speaker": "Shopkeeper", "text": "Welcome, traveler! Need supplies?",
          "choices": [
            { "text": "Show me what you have", "next": "shop_menu" },
            { "text": "Just looking", "next": "goodbye" }
          ]
        },
        { "id": "shop_menu", "speaker": "Shopkeeper", "text": "Take your pick!",
          "event": "open_shop",
          "choices": [{ "text": "Thanks", "next": "goodbye" }]
        },
        { "id": "goodbye", "speaker": "Shopkeeper", "text": "Safe travels!" }
    ]
}

POST /dialogue/start    — { "conversation": "shopkeeper_intro" }
GET  /dialogue/state    — current dialogue node, choices available
POST /dialogue/choose   — { "choice": 0 }
```

### 6.5 Lua Bindings

```lua
-- HUD updates
world.ui.set_text("score", "Score: " .. world.get_var("score"))
world.ui.set_progress("health_bar", entity.health, entity.max_health)
world.ui.show_screen("game_over")
world.ui.hide_screen("hud")

-- Dialogue
world.dialogue.start("shopkeeper_intro")
world.on("dialogue_choice", function(data)
    if data.choice == "open_shop" then
        world.ui.show_screen("shop")
    end
end)

-- Dynamic UI creation
world.ui.create_floating_text(entity.x, entity.y + 20, "-1 HP", { color = "red", duration = 1.0 })
```

### 6.6 API Observation

`/scene/describe` includes UI state:
```json
{
    "ui": {
        "active_screens": ["hud"],
        "dialogue_active": false,
        "hud_state": {
            "health_bar": {"value": 3, "max": 5},
            "score": "Score: 42"
        }
    }
}
```

### 6.7 Testing Criteria

1. Create HUD with health bar + score, verify visible in screenshot
2. Update score via Lua, verify `/scene/describe` shows new value
3. Start dialogue, make choice, verify event fires
4. Show game_over screen, verify it overlays gameplay
5. Floating damage numbers appear and fade
6. `/simulate` can trigger UI events and report dialogue choices

---

## Phase 7: Game State Management

**Goal**: Clean game states (menu, playing, paused, game_over, loading) with transition logic. AI can control state flow.

**Depends on**: Phase 6 (UI screens for menus)

### 7.1 Architecture

Use Bevy's built-in `States` system:

```rust
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash, Serialize, Deserialize)]
pub enum GameState {
    #[default]
    Loading,
    Menu,
    Playing,
    Paused,
    GameOver,
    LevelTransition,
    Cutscene,
    Custom(String),         // Lua-defined custom states
}
```

Each state controls which systems run:
- `Loading`: asset loading only
- `Menu`: UI input only, no physics
- `Playing`: full physics + scripts + interaction
- `Paused`: render but no physics/scripts
- `GameOver`: show results, accept restart input
- `LevelTransition`: fade out → load level → fade in
- `Cutscene`: scripted camera movement + dialogue, no player input

### 7.2 Transition Effects

```rust
pub struct StateTransition {
    pub from: GameState,
    pub to: GameState,
    pub effect: TransitionEffect,
}

pub enum TransitionEffect {
    Instant,
    FadeBlack { duration: f32 },
    FadeWhite { duration: f32 },
    SlideLeft { duration: f32 },
    Custom(String),             // Lua script name
}
```

### 7.3 API

```
POST /game/state        — { "state": "Playing" } or { "state": "Paused" }
GET  /game/state        — current state + time in state
POST /game/transition   — { "to": "GameOver", "effect": "FadeBlack", "duration": 0.5 }
POST /game/restart      — restart current level (reset entities, keep config)
POST /game/load_level   — { "template": "dungeon", "difficulty": 0.5 } (generate + transition)
```

### 7.4 Lua Bindings

```lua
world.game.state                    -- current state (read-only)
world.game.transition("GameOver", { effect = "FadeBlack", duration = 0.5 })
world.game.restart()
world.game.pause()
world.game.resume()

-- Level progression
world.on("goal_reached", function()
    local next_diff = world.get_var("difficulty") + 0.1
    world.set_var("difficulty", next_diff)
    world.game.load_level({ template = "dungeon", difficulty = next_diff })
end)
```

### 7.5 Testing Criteria

1. Start in Menu state, transition to Playing, verify physics starts
2. Pause game, verify entities frozen, unpause, verify they resume
3. Player dies → GameOver state → restart → back to Playing at spawn
4. Level transition with fade: verify smooth visual transition
5. `/simulate` can set initial game state and test state transitions

---

## Phase 8: Advanced Physics

**Goal**: Physics features needed for diverse 2D genres beyond basic AABB platforming.

**Depends on**: Phase 2 (entity interaction uses physics)

### 8.1 Features

**One-Way Platforms** (new tile flag: `PLATFORM = 0x10`):
- Entity can jump through from below but stands on top
- Only blocks downward movement (vy < 0)
- Critical for: platformers, metroidvanias

**Slopes** (new tile type with angle):
- Tiles with diagonal collision surface
- Entity slides along slope surface
- Critical for: platformers, sonic-likes

**Raycasting**:
```rust
pub fn raycast(tilemap: &Tilemap, origin: Vec2, direction: Vec2, max_distance: f32) -> Option<RayHit> {
    // DDA algorithm against tilemap
    // Returns: hit position, tile coordinate, surface normal, distance
}

pub fn raycast_entities(spatial_hash: &SpatialHash, origin: Vec2, direction: Vec2, max_distance: f32, filter_tag: Option<&str>) -> Vec<EntityRayHit> {
    // Ray vs entity AABBs
    // Returns: entity id, hit position, distance
}
```
- Critical for: line-of-sight, laser weapons, grappling hooks, enemy AI vision

**Moving Platforms**:
```rust
#[derive(Component, Serialize, Deserialize)]
pub struct MovingPlatform {
    pub waypoints: Vec<Vec2>,
    pub speed: f32,
    pub pause_frames: u32,      // pause at each waypoint
    pub current_waypoint: usize,
}
```
- Entities standing on platform move with it
- Critical for: platformers, puzzle games

**Friction / Ice / Conveyor**:
- Tile-based surface properties
- `TileTypeDef` gets `friction: f32` field (0 = ice, 1 = normal)
- Conveyor tiles add velocity to entities standing on them

**Ladders / Climbable surfaces**:
- Ladder tile flag: `CLIMBABLE = 0x20`
- When entity overlaps climbable tile, gravity is suppressed, up/down input moves vertically
- Critical for: platformers, metroidvanias

### 8.2 Lua Bindings

```lua
-- Raycasting
local hit = world.raycast(entity.x, entity.y, 1, 0, 200)  -- origin, direction, max_dist
if hit then
    -- hit.x, hit.y, hit.tile_x, hit.tile_y, hit.distance, hit.normal
end

local enemies = world.raycast_entities(entity.x, entity.y, 1, 0, 200, "enemy")

-- Platform queries
world.is_platform(x, y)
world.is_climbable(x, y)
world.tile_friction(x, y)
```

### 8.3 API

```
POST /config/tile_types — now supports friction, platform, climbable flags
POST /physics/raycast   — { "origin": [100, 50], "direction": [1, 0], "max_distance": 200 }
```

### 8.4 Testing Criteria

1. One-way platform: entity jumps through from below, lands on top
2. Slope: entity walks up/down slope smoothly
3. Raycast: line-of-sight check between two entities through tilemap
4. Moving platform: entity rides platform between waypoints
5. Ice tile: entity slides with reduced friction
6. Ladder: entity climbs up, gravity resumes at top
7. `/simulate` with all physics features active, verify no regressions

---

## Phase 9: NPC AI & Pathfinding

**Goal**: Built-in AI behaviors for NPCs. Pathfinding, state machines, perception. The AI (Claude) designs enemy behaviors; the engine executes them efficiently.

**Depends on**: Phase 1 (Lua for custom behaviors), Phase 2 (entity interaction), Phase 8 (raycasting for perception)

### 9.1 Pathfinding

**New file: `src/ai/pathfinding.rs`**

A* pathfinding on the tilemap:
- Top-down: 4/8-direction grid pathfinding
- Platformer: jump-aware pathfinding (existing `/solve` logic, generalized)
- Returns: `Vec<Vec2>` waypoints
- Caches paths, recalculates on tilemap change or every N frames

Component:
```rust
#[derive(Component, Serialize, Deserialize)]
pub struct PathFollower {
    pub target: Vec2,
    pub path: Vec<Vec2>,
    pub recalculate_interval: u32,  // frames between recalculation
    pub path_type: PathType,        // TopDown or Platformer
}

pub enum PathType {
    TopDown,                // A* grid
    Platformer,             // jump-aware
}
```

### 9.2 Behavior Components

```rust
#[derive(Component, Serialize, Deserialize)]
pub struct AiBehavior {
    pub behavior: BehaviorType,
    pub state: AiState,
}

pub enum BehaviorType {
    Patrol { waypoints: Vec<Vec2>, speed: f32 },
    Chase { target_tag: String, speed: f32, detection_radius: f32, give_up_radius: f32 },
    Flee { threat_tag: String, speed: f32, detection_radius: f32 },
    Guard { position: Vec2, radius: f32, chase_radius: f32 },
    Wander { speed: f32, radius: f32, pause_frames: Range<u32> },
    Custom(String),         // Lua script name
}

pub enum AiState {
    Idle,
    Patrolling { waypoint_index: usize },
    Chasing { target_id: u64 },
    Fleeing,
    Attacking,
    Returning,
}
```

### 9.3 Perception System

System `perception_system`:
- For each AI entity, check detection radius (using spatial hash)
- Optionally check line-of-sight via raycast
- Update `AiBehavior.state` based on what's detected

### 9.4 Preset Enemies

```
POST /entities/preset — enhanced presets:
{
    "preset": "patrol_enemy",
    "x": 100, "y": 50,
    "config": {
        "waypoints": [[100, 50], [200, 50]],
        "speed": 100,
        "health": 3,
        "contact_damage": 1,
        "sprite_sheet": "skeleton"
    }
}
```

Presets: `patrol_enemy`, `chase_enemy`, `guard_enemy`, `turret`, `flying_enemy`, `boss`

### 9.5 Lua Bindings

```lua
-- Custom AI in Lua
entity.ai.state                     -- current state
entity.ai.target                    -- current target entity (if chasing)

-- Pathfinding
local path = world.find_path(entity.x, entity.y, target_x, target_y, "top_down")
entity.follow_path(path)

-- Perception
local nearby = world.find_in_radius(entity.x, entity.y, 200, "player")
if #nearby > 0 then
    local can_see = world.line_of_sight(entity.x, entity.y, nearby[1].x, nearby[1].y)
    if can_see then
        entity.ai.chase(nearby[1])
    end
end
```

### 9.6 Testing Criteria

1. Patrol enemy walks between waypoints, reverses at each end
2. Chase enemy detects player within radius, pathfinds toward them
3. Guard enemy chases player near its post, returns when player is far
4. Line-of-sight: enemy doesn't detect player behind wall
5. `/simulate`: spawn 10 enemies, simulate 300 frames, verify sensible behaviors
6. `/validate` can check "fairness" — player can reach goal despite enemies

---

## Phase 10: Persistence & Levels

**Goal**: Save/load game state. Level management (multiple levels, progression). Export/import.

**Depends on**: Phase 7 (game states for level transitions)

### 10.1 Save/Load

All gameplay components are already `Serialize`/`Deserialize`. Save = serialize entire ECS state.

```
POST /save      — { "slot": "save1" } → saves to JSON file
POST /load      — { "slot": "save1" } → restores ECS state
GET  /saves     — list available save slots

Save format (JSON):
{
    "version": 1,
    "config": { GameConfig },
    "tilemap": { Tilemap },
    "entities": [
        {
            "network_id": 42,
            "x": 100.0, "y": 50.0,
            "vx": 0.0, "vy": 0.0,
            "components": [ ... all serialized components ... ],
            "script": "enemy_patrol",
            "script_state": { ... per-entity Lua state ... },
            "tags": ["enemy", "skeleton"],
            "alive": true
        }
    ],
    "next_network_id": 143,
    "game_vars": { "score": 42, "level": 3 },
    "game_state": "Playing"
}
```

**Critical**: The save MUST include:
- Every entity's full component state (position, velocity, health, AI state, etc.)
- Each entity's `NetworkId` value (NOT the Bevy Entity id)
- `next_network_id`: the current value of the NetworkId counter, so new entities spawned after load don't collide with existing IDs
- Script per-entity state (the Lua/rhai table that persists between frames)
- All global game variables from `ScriptEngine`

On load: despawn all existing gameplay entities, recreate them from the save data with their original `NetworkId` values, restore the NetworkId counter to `next_network_id`, restore game variables and game state.

### 10.2 Level Pack System

```
POST /levels/pack — define a level pack (campaign)
{
    "name": "dark_dungeon",
    "levels": [
        { "template": "top_down_dungeon", "difficulty": 0.1, "seed": 1, "config_overrides": {} },
        { "template": "top_down_dungeon", "difficulty": 0.3, "seed": 2 },
        { "template": "top_down_dungeon", "difficulty": 0.5, "seed": 3, "config_overrides": { "move_speed": 250 } },
        { "template": "fighting_arena", "difficulty": 0.7, "seed": 4 }
    ]
}

POST /levels/pack/{name}/start     — begin at level 0
POST /levels/pack/{name}/next      — advance to next level
GET  /levels/pack/{name}/progress  — current level, scores, times
```

### 10.3 Level Export/Import

```
POST /levels/export     — export current level + entities + scripts as standalone JSON
POST /levels/import     — load a previously exported level
POST /game/export       — export entire game (levels, scripts, assets, config) as a project file
POST /game/import       — load an exported project
```

### 10.4 Testing Criteria

1. Save game mid-play, load, verify exact state restored — entity positions, health, AI states, script per-entity state, game variables, score, NetworkId values all match
2. After load, spawn a new entity — its NetworkId must NOT collide with any restored entity's NetworkId
3. Define 3-level pack, progress through all levels
4. Export level, import on fresh engine instance, verify identical

---

## Phase 11: Generation System Expansion

**Goal**: Rich procedural generation for every supported genre. The AI's primary content creation tool.

**Depends on**: Phase 2 (entities to place), Phase 9 (enemies to spawn)

### 11.1 Enhanced Templates

Each template should generate not just tilemaps but complete levels with entity placements:

```rust
pub struct GenerateResult {
    pub tilemap: GeneratedTilemap,
    pub player_spawn: (f32, f32),
    pub goal: (i32, i32),
    pub entities: Vec<EntityPlacement>,     // NEW: enemies, pickups, NPCs
    pub scripts: Vec<ScriptAssignment>,     // NEW: scripts for generated entities
    pub validation: serde_json::Value,
    pub difficulty_metrics: DifficultyMetrics,
}

pub struct EntityPlacement {
    pub preset: String,             // e.g., "patrol_enemy"
    pub x: f32,
    pub y: f32,
    pub config: serde_json::Value,  // preset-specific config
}
```

### 11.2 New Templates

Add templates for underserved genres:

- `metroidvania` — interconnected rooms, ability gates, backtracking paths
- `roguelike_floor` — rooms connected by corridors with enemy density scaling
- `puzzle_platformer` — switches, doors, push blocks, timed platforms
- `arena_waves` — central arena with wave spawn points around edges
- `side_scroller` — auto-scrolling level with obstacles and enemies
- `tower_defense_map` — path from spawn to base with tower placement zones
- `boss_arena` — large room with platforms and hazard zones

### 11.3 Generation Constraints

Expand the constraint system:

```
"difficulty_range"      — ensure difficulty metrics fall within a range
"enemy_fairness"        — player can reach goal while avoiding/defeating all enemies
"item_reachability"     — all pickups are reachable
"pacing"                — alternates between combat and rest areas
"no_dead_ends"          — every path leads somewhere useful (except intentional secret rooms)
"ability_gating"        — certain areas require specific abilities
```

### 11.4 Testing Criteria

1. Generate a metroidvania with 10 rooms, verify interconnected and backtrackable
2. Generate roguelike floor, verify enemy placement scales with difficulty
3. Generate puzzle platformer, verify switches connect to doors
4. All generated levels pass their constraints
5. `/simulate` bot can complete generated levels

---

## Phase 12: WASM / Web Export

**Goal**: Export games as standalone web pages playable in any browser.

**Depends on**: All gameplay features complete (Phases 1-11)

### 12.1 Architecture

The web build is a standalone game that does NOT use the HTTP API. Instead:
- Level data is baked into the binary (or loaded from bundled JSON)
- Scripts are bundled as assets
- No server needed — it's a self-contained .wasm + .html

The HTTP API is only for development (AI building the game). The export strips it out.

### 12.2 Build Pipeline

```
POST /export/web — export game as web build
{
    "title": "Dark Dungeon",
    "width": 960,
    "height": 540,
    "levels": "all",            // or specific level pack name
    "embed_assets": true        // bundle sprites + audio into binary
}
```

This triggers:
1. Serialize current game state (levels, scripts, config, assets) to `export/`
2. Generate a `game_data.rs` that embeds the serialized data
3. Compile with `cargo build --target wasm32-unknown-unknown --features web_export`
4. Run `wasm-bindgen` to generate JS glue
5. Generate `index.html` with canvas + input handling
6. Output: `export/web/` directory ready to deploy

### 12.3 Desktop Export

```
POST /export/desktop — export as native executable
{
    "title": "Dark Dungeon",
    "target": "windows"         // or "linux", "macos"
}
```

Similar process but compiles native binary with embedded game data.

### 12.4 Feature Gating

```rust
#[cfg(not(target_arch = "wasm32"))]
mod api;                        // HTTP API only in native builds

#[cfg(target_arch = "wasm32")]
mod web_bootstrap;              // load embedded game data on startup
```

Scripting:
```rust
// In src/scripting/mod.rs:
#[cfg(not(target_arch = "wasm32"))]
mod vm;         // LuaBackend using mlua/LuaJIT

#[cfg(target_arch = "wasm32")]
mod vm_wasm;    // RhaiBackend using rhai

// ScriptEngine is initialized with the platform-appropriate backend:
#[cfg(not(target_arch = "wasm32"))]
let backend: Box<dyn ScriptBackend> = Box::new(vm::LuaBackend::new());

#[cfg(target_arch = "wasm32")]
let backend: Box<dyn ScriptBackend> = Box::new(vm_wasm::RhaiBackend::new());
```

### 12.5 Testing Criteria

1. Export web build, open in browser, game plays correctly
2. All gameplay features work in WASM (physics, scripts via rhai, audio, UI)
3. Scripts written in rhai syntax produce identical gameplay behavior to their Lua equivalents (same entity behaviors, same events emitted, same game logic)
4. Performance: 60fps in browser with 200 entities
5. Desktop export produces standalone executable

---

## Phase 13: Developer Experience & Polish

**Goal**: Make Axiom pleasant to use. Documentation, error messages, examples, debugging tools.

**Depends on**: All other phases

### 13.1 API Documentation

Auto-generated API docs:
```
GET /docs — returns full API documentation as JSON/HTML
GET /docs/endpoints — list all endpoints with parameters
GET /docs/components — list all available components with fields
GET /docs/presets — list all entity presets
GET /docs/templates — list all generation templates with parameters
```

### 13.2 Debug Overlay

In windowed mode, toggleable debug overlay showing:
- Entity colliders (green rectangles)
- Trigger zones (blue circles)
- Pathfinding paths (yellow lines)
- Spatial hash grid
- FPS counter
- Entity count

```
POST /debug/overlay     — { "show": true, "features": ["colliders", "paths", "spatial_hash"] }
```

### 13.3 Replay System

Record and replay gameplay sessions for debugging:

```
POST /replay/record     — start recording inputs + events
POST /replay/stop       — stop recording, save to file
POST /replay/play       — play back a recorded session
GET  /replay/list       — available recordings
```

Replays are deterministic because all gameplay runs in `FixedUpdate` with input-driven logic.

### 13.4 Performance Profiling

```
GET /perf — returns performance metrics
{
    "fps": 60,
    "frame_time_ms": 2.1,
    "entity_count": 347,
    "physics_time_ms": 0.4,
    "script_time_ms": 0.8,
    "render_time_ms": 0.6,
    "collision_checks": 1200,
    "spatial_hash_cells": 64
}

GET /perf/history â€” returns a downsampled ring buffer of recent perf samples
```

### 13.5 Example Games

Build 3-5 complete example games that ship with Axiom:

1. **Platformer** — Mario-like, 5 levels, enemies, pickups, boss
2. **Top-Down RPG** — Zelda-like, dungeon with enemies, keys, doors, health
3. **Roguelike** — procedural floors, permadeath, item pickups, escalating difficulty
4. **Bullet Hell** — arena shooter, wave enemies, projectile patterns
5. **Puzzle Platformer** — switches, doors, push blocks, timed challenges

Each example is a set of Lua scripts + assets + a generation config. They serve as both tests and documentation.

### 13.6 Testing Criteria

1. `/docs` returns complete, accurate API documentation
2. Debug overlay shows correct collider positions
3. Record 10 seconds of gameplay, replay produces identical output
4. All 5 example games play correctly from generation through completion
5. `/perf` reports reasonable metrics

---

## Phase Dependency Graph

```
Phase 1: Lua Scripting     ─────────────────────────────┐
    │                                                    │
    ├──► Phase 2: Entity Interaction ──► Phase 9: NPC AI │
    │        │                              │            │
    │        ├──► Phase 3: Animation        │            │
    │        │        │                     │            │
    │        │        ├──► Phase 6: UI      │            │
    │        │        │        │            │            │
    │        │        │        ├──► Phase 7: Game States  │
    │        │        │        │        │                │
    │        │        │        │        ├──► Phase 10: Persistence
    │        │        │        │        │                │
    │        │        │        │        └──► Phase 11: Generation Expansion
    │        │        │        │                         │
    │        ├──► Phase 4: Audio                         │
    │        │                                           │
    │        └──► Phase 8: Advanced Physics              │
    │                                                    │
    ├──► Phase 5: Camera (independent)                   │
    │                                                    │
    └────────────────────────────────────────────────────┘
                                                         │
                                    Phase 12: WASM Export ◄┘
                                         │
                                    Phase 13: Polish
```

**Parallelizable**: Phases 3, 4, 5 can run in parallel. Phase 8 can parallel with 3-4.

---

## Genre Coverage

After all phases, Axiom supports:

| Genre | Key Features Used |
|-------|-------------------|
| Platformer | HorizontalMover, Jumper, GravityBody, one-way platforms, moving platforms |
| Metroidvania | Above + ability gating, backtracking, interconnected rooms |
| Top-Down RPG | TopDownMover, NPC AI, dialogue, inventory UI, health |
| Roguelike | Procedural generation, permadeath (save deletion), item system |
| Fighter | GravityBody, Hitbox, animation-driven attacks, health, knockback |
| Bullet Hell | Projectile spawning, Lua patterns, spatial hash for many entities |
| Puzzle Platformer | Triggers, switches, moving platforms, Lua puzzle logic |
| Tower Defense | Path generation, wave spawning, Lua tower logic, UI for placement |
| Side-Scroller (Shmup) | Auto-scroll, projectiles, wave spawning, Lua patterns |
| Visual Novel | Dialogue system, UI screens, game state branching |
| RTS (basic) | Top-down, entity selection (UI), pathfinding, Lua commands |
| Survival | Health, inventory UI, crafting (Lua), day/night (Lua), enemy waves |
| Racing (top-down) | TopDownMover, checkpoints (triggers), lap counting (Lua) |
| Card Game | UI system, Lua game rules, animation for card movement |

"Creative/unusual" designs work because Lua scripting lets the AI define arbitrary game rules that the engine doesn't need to anticipate.

---

## Estimated Timeline

| Phase | Effort | Parallel? |
|-------|--------|-----------|
| Phase 1: Lua Scripting | 5 days | — |
| Phase 2: Entity Interaction | 5 days | After 1 |
| Phase 3: Animation | 4 days | After 2, parallel with 4,5 |
| Phase 4: Audio | 3 days | After 2, parallel with 3,5 |
| Phase 5: Camera | 2 days | After 1, parallel with 2,3,4 |
| Phase 6: UI | 5 days | After 3 |
| Phase 7: Game States | 3 days | After 6 |
| Phase 8: Advanced Physics | 4 days | After 2, parallel with 3,4 |
| Phase 9: NPC AI | 4 days | After 1,2,8 |
| Phase 10: Persistence | 3 days | After 7 |
| Phase 11: Generation Expansion | 4 days | After 2,9 |
| Phase 12: WASM Export | 4 days | After all gameplay phases |
| Phase 13: Polish | 5 days | After all |
| **Total (sequential)** | **~51 days** | |
| **Total (with parallelism)** | **~30-35 days** | |

---

## Success Criteria

Axiom is "production-plausible" when:

1. **An AI can build a complete game** (generation through polish) using only the HTTP API, with the human only providing sprites/audio and taste-testing
2. **5 example games** of different genres are playable and feel good
3. **Web export** produces a playable game in the browser
4. **1000 entities** with physics + scripts runs at 60fps
5. **The /simulate endpoint** can fully test any generated level (entity interactions, damage, pickups, win/lose conditions) without a human pressing any keys
6. **Script hot-reload** lets the AI iterate on game logic in <1 second cycles
7. **No Rust recompilation** needed for game-specific logic (all in Lua)
8. An indie dev with sprites and a Claude API key could say "make me a roguelike" and get a playable game



