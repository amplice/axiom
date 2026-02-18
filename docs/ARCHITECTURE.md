# Axiom Engine — Architecture

## Overview

Axiom is a 2D game engine built on Rust and Bevy ECS, with an HTTP API layer for external control. AI agents interact exclusively through the REST API; the engine handles physics, collision, rendering, scripting, and audio internally.

```
┌──────────────┐     HTTP/JSON      ┌──────────────┐
│   AI Agent   │ ──────────────────→ │  Axum Router │
│ (Python/curl)│ ←────────────────── │  (port 3000) │
└──────────────┘                     └──────┬───────┘
                                            │ ApiCommand via crossbeam channel
                                            ▼
                                     ┌──────────────┐
                                     │  Bevy ECS    │
                                     │  Game Loop   │
                                     │  (60Hz fixed)│
                                     └──────┬───────┘
                                            │
                          ┌─────────────────┼─────────────────┐
                          ▼                 ▼                 ▼
                   ┌────────────┐   ┌────────────┐   ┌────────────┐
                   │  Physics   │   │  Scripting  │   │ Rendering  │
                   │ Collision  │   │  Lua (mlua) │   │  Sprites   │
                   │ Pathfinding│   │  Per-entity  │   │  UI / VFX  │
                   └────────────┘   └────────────┘   └────────────┘
```

## Module Map

### Core Engine (`src/`)

| File | Purpose |
|------|---------|
| `main.rs` | App setup, Bevy plugin registration, CLI args |
| `components.rs` | All ECS component definitions (Health, Collider, TopDownMover, etc.) |
| `spawn.rs` | Entity spawning from `EntitySpawnRequest` — converts API types to ECS components |
| `interaction.rs` | Contact damage, pickup collection, hitbox detection, knockback, invincibility |
| `spatial_hash.rs` | Spatial hash grid for efficient broad-phase collision detection |
| `tilemap.rs` | Tile storage, tile queries (is_solid, is_platform), tile rendering |
| `sprites.rs` | Sprite sheet management, auto-colored fallback sprites, animation rendering |

### API Layer (`src/api/`)

| File | Purpose |
|------|---------|
| `router.rs` | Axum route registration — maps HTTP paths to handlers |
| `commands.rs` | `ApiCommand` enum — all 130+ commands the API can send to the ECS |
| `types.rs` | Request/response structs (1900+ lines) — every JSON body and return type |
| `routes_misc.rs` | Handler implementations for most endpoints (2100+ lines) |
| `docs.rs` | Built-in API documentation data |
| `helpers.rs` | Built-in example recipes and utility functions |
| `security.rs` | Token authentication, rate limiting middleware |
| `command_runtime.rs` | `process_api_commands` system — receives commands from channel, applies to ECS |
| `command_runtime/systems.rs` | Additional ECS systems for processing deferred operations |
| `command_runtime/tests.rs` | Integration tests for API command processing |

### Scripting (`src/scripting/`)

| File | Purpose |
|------|---------|
| `vm.rs` | Lua VM management — entity table construction, world table construction, script execution |

### Other Modules

| File | Purpose |
|------|---------|
| `animation.rs` | Animation graph definitions and state machine transitions |
| `camera.rs` | Camera following, shake, zoom, pixel snapping |
| `constraints.rs` | Level validation constraints (reachable, bounds_check, etc.) |
| `generation.rs` | Procedural level generation from templates |
| `simulation.rs` | Headless simulation, scenario testing, AI playtester |
| `ui.rs` | UI screen definitions and rendering |
| `audio.rs` | Audio playback, SFX/music management |
| `ai.rs` | AI behavior execution (patrol, chase, flee, guard, wander) |

## Command Flow

Every API request follows this path:

```
1. HTTP Request arrives at Axum handler
2. Handler deserializes JSON body into request struct
3. Handler creates an ApiCommand variant
4. Command sent via crossbeam channel (with oneshot response channel)
5. Bevy system `process_api_commands` receives command during FixedUpdate
6. System reads/modifies ECS world, sends response back via oneshot
7. Handler awaits response, serializes to JSON, returns HTTP response
```

### Why Channels?

Bevy's ECS world is single-threaded during system execution. The HTTP server runs on a Tokio async runtime. Crossbeam channels bridge these two worlds safely. The oneshot response channel lets the HTTP handler await the ECS result.

## System Execution Order

The engine runs at **60 Hz fixed timestep** (FixedUpdate):

1. **process_api_commands** — Handle pending HTTP commands
2. **AI systems** — Update AI behaviors (patrol, chase, etc.)
3. **Script execution** — Run Lua update() for each entity, then global scripts
4. **Physics** — Apply gravity, movement, collision detection
5. **Interaction** — Contact damage, pickup collection, hitbox checks
6. **Death processing** — Call on_death hooks, despawn dead entities
7. **Animation** — Update sprite animation frames
8. **Camera** — Update camera position, apply shake

Rendering runs every frame (not tied to fixed timestep) with interpolation for smooth visuals.

## Key Resources

Bevy resources are shared state accessible to all systems:

| Resource | Type | Purpose |
|----------|------|---------|
| `GameConfig` | Struct | Physics settings (gravity, tile_size, move_speed, etc.) |
| `Tilemap` | Arc\<Tilemap\> | Tile data, spatial queries |
| `ScriptEngine` | Struct | Lua VM, loaded scripts, error tracking |
| `SpatialHashGrid` | Struct | Broad-phase collision grid |
| `NetworkIdMap` | HashMap | NetworkId → Entity mapping |
| `GameEvents` | Vec | Queued game events |
| `GameRuntimeState` | Enum | Playing/Paused/GameOver/etc. |
| `ApiChannels` | Struct | Crossbeam receiver for API commands |
| `SharedSnapshot` | Arc\<RwLock\> | Thread-safe game state snapshot for API reads |

## NetworkId vs Entity

Bevy's internal `Entity` is an opaque handle that can be recycled. The API uses `NetworkId` (u64) as a stable external identifier:

- **NetworkId** — Assigned at spawn, never reused, used in all API requests/responses
- **Entity** — Bevy's internal handle, may be recycled after despawn
- **NetworkIdMap** — Bidirectional lookup between the two

When you `POST /entities`, the response returns the NetworkId. Use this ID for all subsequent operations (`GET /entities/{id}`, `DELETE /entities/{id}`, etc.).

## Script Execution Model

### Entity Scripts
1. For each entity with a script component:
   - Build `entity` table from ECS components (position, velocity, health, etc.)
   - Build `world` table with lazy-loaded API groups
   - Call `update(entity, world, dt)` in Lua
   - Read back modified properties (position, velocity, etc.) and apply to ECS
   - Process queued commands (spawns, despawns, events, etc.)
2. GC runs every 50 entities to prevent memory buildup

### Global Scripts
1. Build `world` table (no entity)
2. Call `update(world, dt)` in Lua
3. Process queued commands
4. Full GC after all global scripts

### Error Handling
- Scripts that error 8 consecutive times are auto-disabled
- `GET /scripts/errors` shows recent errors with script name and stack trace
- Re-uploading a script clears its errors and re-enables it

### Memory Management
- Entity tables use pure Lua closures (no Rust captures) to avoid GC cycles
- Entity snapshots use a shared metatable with a single dispatch closure
- World table groups are lazy-loaded on first access
- `expire_registry_values()` + `gc_collect()` after each entity batch

## Rendering Pipeline

1. **Tile rendering** — Tiles rendered from tilemap data, colored by type
2. **Entity rendering** — Each entity gets a sprite:
   - If animation_controller + sprite_sheet: render animated sprite
   - Otherwise: render colored rectangle (color from tags: enemy=red, pickup=green, etc.)
3. **UI rendering** — Bevy UI nodes for text, progress bars, panels
4. **Camera** — Transform-based following with optional pixel snapping

Rendering uses Bevy's standard 2D camera. Y-axis points up (Bevy convention), but tilemaps are stored top-to-bottom (row 0 = top).
