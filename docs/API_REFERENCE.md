# Axiom Engine — API Reference

Base URL: `http://127.0.0.1:3000`

All responses follow: `{"ok": true, "data": <result>, "error": null}`

Authentication (optional): Set `AXIOM_API_TOKEN` env var, then send `Authorization: Bearer <token>` or `X-API-Key: <token>`.

---

## Table of Contents

- [Core](#core)
- [Entities](#entities)
- [Entity Mutations](#entity-mutations)
- [Entity Visual Effects](#entity-visual-effects)
- [Scripts](#scripts)
- [Tilemap](#tilemap)
- [Generation](#generation)
- [Camera](#camera)
- [Audio](#audio)
- [UI Screens](#ui-screens)
- [Dialogue](#dialogue)
- [Sprites & Animation](#sprites--animation)
- [Visual Effects](#visual-effects)
- [Lighting](#lighting)
- [Physics & Raycasting](#physics--raycasting)
- [AI & Pathfinding](#ai--pathfinding)
- [Game State](#game-state)
- [Save / Load](#save--load)
- [Level Packs](#level-packs)
- [Export / Import](#export--import)
- [Input](#input)
- [Inventory & Items](#inventory--items)
- [Cutscenes](#cutscenes)
- [Presets & Pools](#presets--pools)
- [Testing & Simulation](#testing--simulation)
- [Assets](#assets)
- [Debug & Performance](#debug--performance)
- [Health & Diagnostics](#health--diagnostics)
- [Documentation](#documentation)
- [Appendix A: Component Types](#appendix-a-component-types)
- [Appendix B: Lua API Surface](#appendix-b-lua-api-surface)

---

## Core

### GET /state
Get world + player state.

**Response:**
```json
{"tilemap": {"width": 20, "height": 15, "tiles": [...]}, "player": {"x": 100, "y": 50, "vx": 0, "vy": 0, "grounded": true, "alive": true}}
```

### GET /player
Get player state only.

**Response:**
```json
{"x": 100, "y": 50, "vx": 0, "vy": 0, "grounded": true, "alive": true}
```

### GET /config
Get full game config.

### POST /config
Set config (partial merge — only supplied fields updated).

**Body:**
```json
{
  "gravity": {"x": 0, "y": -980},
  "tile_size": 16,
  "move_speed": 200,
  "jump_velocity": 400,
  "fall_multiplier": 1.5,
  "coyote_frames": 5,
  "jump_buffer_frames": 4,
  "pixel_snap": true,
  "interpolate_transforms": true,
  "max_fall_speed": 600,
  "debug_mode": false,
  "screenshot_path": "./screenshots",
  "asset_path": "./assets"
}
```

### POST /config/tile_types
Set custom tile type registry.

### POST /level
Load a tilemap.

**Body:**
```json
{
  "width": 30, "height": 25,
  "tiles": [0, 1, 1, ...],
  "player_spawn": [104, 184],
  "goal": [28, 1]
}
```

- `tiles`: flat array, length = width * height, row-major
- `player_spawn`: pixel coordinates [x, y]
- `goal`: tile coordinates [col, row]

### POST /player/position
Teleport player.

**Body:** `{"x": 100, "y": 200}`

### POST /build
Atomic build: apply config, tilemap, entities, scripts in one call.

**Body:**
```json
{
  "config": {},
  "tilemap": {"width": 20, "height": 15, "tiles": [...], "player_spawn": [48, 48]},
  "entities": [{"x": 48, "y": 48, "is_player": true, "components": [...]}],
  "scripts": {"my_script": "function update(entity, world, dt) end"},
  "global_scripts": ["wave_manager"],
  "game_vars": {"score": 0},
  "animation_graphs": {},
  "sprite_sheets": {},
  "presets": {},
  "validate_first": ["top_down_reachable"]
}
```

All fields optional. Returns `{"success": true, "errors": []}`.

### POST /validate/manifest
Validate build manifest without applying.

### POST /window
Set window title and background color.

**Body:** `{"title": "My Game", "background": [0.1, 0.1, 0.2]}`

---

## Entities

### GET /entities
List entities. Supports query filters:

| Parameter | Type | Description |
|-----------|------|-------------|
| `tag` | string | Filter by tag |
| `alive` | bool | Filter by alive status |
| `has_script` | bool | Filter by script presence |
| `near_x`, `near_y`, `near_radius` | f32 | Spatial proximity filter |
| `entity_state` | string | Filter by state machine state |
| `component` | string | Filter by component type |
| `limit` | usize | Max results |

**Response:** Array of EntityInfo with 50+ fields including position, velocity, health, AI state, animation, physics diagnostics, collision info, and interaction details.

### POST /entities
Spawn a custom entity.

**Body:**
```json
{
  "x": 100, "y": 50,
  "is_player": false,
  "invisible": false,
  "tags": ["enemy", "zombie"],
  "script": "zombie_ai",
  "components": [
    {"type": "collider", "width": 12, "height": 14},
    {"type": "health", "current": 3, "max": 3}
  ]
}
```

**Response:** `{"id": 42}` (NetworkId)

### POST /entities/preset
Spawn from a built-in or custom preset.

**Body:**
```json
{
  "preset": "chase_enemy",
  "x": 200, "y": 100,
  "config": {
    "health": 5,
    "speed": 120,
    "contact_damage": 2,
    "detection_radius": 250,
    "script": "zombie_ai",
    "tags": ["enemy", "zombie"]
  }
}
```

Built-in presets: `platformer_player`, `top_down_player`, `patrol_enemy`, `chase_enemy`, `guard_enemy`, `turret`, `flying_enemy`, `boss`, `health_pickup`, `moving_platform`

### GET /entities/{id}
Get entity by NetworkId. Returns full EntityInfo.

### DELETE /entities/{id}
Remove entity.

### POST /entities/{id}/damage
Apply damage.

**Body:** `{"amount": 5}`

### POST /entities/reset_non_player
Delete all non-player entities.

### POST /entities/bulk
Bulk mutate entities matching filter.

**Body:**
```json
{
  "filter": {
    "tag": "enemy",
    "component": "health",
    "alive": true,
    "has_script": true,
    "entity_state": "idle",
    "ids": [1, 2, 3]
  },
  "mutations": {
    "health_current": 5,
    "health_max": 10,
    "add_tags": ["buffed"],
    "remove_tags": ["weak"],
    "contact_damage": 3,
    "contact_knockback": 100,
    "hitbox_active": true,
    "hitbox_damage": 5,
    "alive": false
  }
}
```

**Response:** `{"matched": 5, "mutated": 5}`

---

## Entity Mutations

### POST /entities/{id}/position
**Body:** `{"x": 100, "y": 200}`

### POST /entities/{id}/velocity
**Body:** `{"vx": 100, "vy": -50}`

### POST /entities/{id}/tags
**Body:** `{"add": ["burning"], "remove": ["frozen"]}`

### POST /entities/{id}/health
**Body:** `{"current": 5, "max": 10}`

### POST /entities/{id}/contact_damage
**Body:** `{"amount": 2, "cooldown_frames": 15, "knockback": 100}`

### POST /entities/{id}/hitbox
**Body:** `{"active": true, "damage": 3, "width": 24, "height": 20}`

---

## Entity Visual Effects

### GET /entities/{id}/animation
Get entity animation runtime state.

### POST /entities/{id}/animation
Force animation state change.

**Body:** `{"state": "attack"}`

### POST /entities/{id}/particles
Attach or update particle emitter.

### POST /entities/{id}/tween
Start a property tween.

**Body:**
```json
{
  "property": "x",
  "to": 200,
  "from": 100,
  "duration": 0.5,
  "easing": "ease_out",
  "tween_id": "move_tween"
}
```

### POST /entities/{id}/tween_sequence
Start a sequential tween chain.

**Body:**
```json
{
  "steps": [
    {"property": "x", "to": 200, "duration": 0.3, "easing": "ease_in"},
    {"property": "y", "to": 100, "duration": 0.3, "easing": "ease_out"}
  ],
  "sequence_id": "my_sequence"
}
```

### POST /entities/{id}/tint
Set sprite color tint.

**Body:** `{"color": [1, 0, 0, 1], "flash_color": [1, 1, 1, 1], "flash_frames": 6}`

### POST /entities/{id}/trail
Set or remove trail/afterimage effect.

**Body:** `{"interval": 3, "duration": 0.3, "alpha_start": 1.0, "alpha_end": 0.0}`

Send `null` to remove.

### GET /entities/{id}/state
Get entity state machine.

### POST /entities/{id}/state
Transition entity state machine.

**Body:** `{"state": "attacking"}`

### GET /entities/{id}/inventory
Get entity inventory contents.

### POST /entities/{id}/inventory
Add/remove/clear items in entity inventory.

**Body:** `{"action": "add", "item_id": "health_potion", "count": 1}`

---

## Scripts

### GET /scripts
List loaded scripts. Returns name, global flag, error count.

### POST /scripts
Upload or replace a script.

**Body:**
```json
{
  "name": "zombie_ai",
  "source": "function update(entity, world, dt)\n  -- script code\nend",
  "global": false
}
```

- Entity scripts: `function update(entity, world, dt)`
- Global scripts: `function update(world, dt)` with `"global": true`
- Optional hooks: `on_death(entity, world)`, `init(entity, world)`

### GET /scripts/{name}
Get script source code.

### DELETE /scripts/{name}
Delete a script.

### POST /scripts/{name}/test
Dry-run script with diagnostics. Returns entity state after one update() call.

**Response includes:** entity_health, entity_alive, entity_vx, entity_vy, entity_animation, defines_on_death, defines_init

### GET /scripts/errors
Recent script errors with script name, message, and stack trace.

### DELETE /scripts/errors
Clear all script errors.

### GET /scripts/logs
Get recent script log output (from `world.log()`).

### DELETE /scripts/logs
Clear script logs.

### GET /scripts/vars
Get all global game variables.

### POST /scripts/vars
Set global game variables.

**Body:** `{"score": 0, "wave": 1, "alive_enemies": 5}`

### GET /scripts/vars/diff
Track variable changes since last call. Returns `{changed, added, removed, snapshot_id}`. First call establishes baseline.

### GET /scripts/events
Recent script-emitted events (from `world.emit()`).

### GET /scripts/stats
Script runtime health counters: loaded count, disabled count, error counts, buffer sizes.

---

## Tilemap

### POST /tilemap/query
Query solid tiles in AABB region.

**Body:** `{"x1": 0, "y1": 0, "x2": 160, "y2": 80}`

**Response:** `{"solid_tiles": [{"col": 0, "row": 0, "tile_id": 1, "tile_type": "solid"}, ...]}`

### POST /tilemap/auto_tile
Set auto-tiling rules.

### GET /tilemap/layers
List decorative tile layers.

### POST /tilemap/layers
Add or update a decorative tile layer.

### DELETE /tilemap/layers/{name}
Delete a decorative tile layer.

---

## Generation

### POST /generate
Generate a procedural level.

**Body:**
```json
{
  "template": "top_down_dungeon",
  "difficulty": 0.3,
  "seed": 42,
  "width": 30,
  "height": 25,
  "constraints": ["top_down_reachable", "bounds_check"]
}
```

**Templates:** `platformer`, `top_down_dungeon`, `rts_arena`, `fighting_arena`, `metroidvania`, `roguelike_floor`, `puzzle_platformer`, `arena_waves`, `side_scroller`, `tower_defense_map`, `boss_arena`

**Constraints:** `reachable`, `top_down_reachable`, `bounds_check`, `has_ground`, `no_softlock`

**Response:**
```json
{
  "tilemap": {"width": 30, "height": 25, "tiles": [...]},
  "player_spawn": [48, 48],
  "goal": [28, 1],
  "validation": {"passed": true, "results": []},
  "difficulty_metrics": {}
}
```

### POST /validate
Validate constraints on current tilemap.

**Body:** `{"constraints": ["reachable", "bounds_check"]}`

### POST /solve
Auto-solve current level (finds input sequence to reach goal).

### POST /game/load_level
Generate + load a level from template and difficulty.

---

## Camera

### POST /camera/config
Configure camera behavior.

**Body:**
```json
{
  "zoom": 3.0,
  "follow_speed": 5.0,
  "follow_target": 1,
  "deadzone": [0, 0],
  "bounds": {"min_x": 0, "max_x": 800, "min_y": 0, "max_y": 600}
}
```

Omit `bounds` for auto-bounds from tilemap. Set `follow_target` to a NetworkId.

### POST /camera/shake
Trigger camera shake.

**Body:** `{"intensity": 5.0, "duration": 0.3, "decay": true}`

### POST /camera/look_at
Set camera look-at target.

**Body:** `{"x": 200, "y": 100, "speed": 5.0}`

### GET /camera/state
Get current camera runtime state (position, zoom, shake state).

---

## Audio

### POST /audio/sfx
Define named sound effects.

**Body:**
```json
{
  "effects": {
    "hit": {"path": "assets/hit.ogg", "volume": 1.0, "pitch_variance": 0.1},
    "death": {"path": "assets/death.ogg"}
  }
}
```

### POST /audio/music
Define named music tracks.

**Body:**
```json
{
  "tracks": {
    "theme": {"path": "assets/theme.ogg", "volume": 0.8, "looping": true},
    "boss": {"path": "assets/boss.ogg", "looping": true}
  }
}
```

### POST /audio/play
Play a sound effect or music track.

**Body (SFX):** `{"sfx": "hit"}`
**Body (Music):** `{"music": "theme", "fade_in": 2.0}`

### POST /audio/stop
Stop currently playing music.

**Body:** `{"fade_out": 1.0}`

### POST /audio/config
Set volume levels.

**Body:** `{"master": 0.8, "sfx": 1.0, "music": 0.6}`

### POST /audio/triggers
Map gameplay events to auto-play SFX.

**Body:** `{"mappings": {"entity_died": "death", "entity_damaged": "hit"}}`

### GET /audio/state
Get audio definitions, playing tracks, and recent audio events.

---

## UI Screens

### POST /ui/screens
Define or replace a UI screen.

**Body:**
```json
{
  "name": "hud",
  "layer": 0,
  "nodes": [
    {
      "id": "health_bar",
      "node_type": {"type": "progress_bar", "value": 10, "max": 10, "color": "red", "bg_color": "dark_red"},
      "position": {"Anchored": {"anchor": "top_left", "offset": [16, 16]}},
      "size": {"fixed": [200, 20]}
    },
    {
      "id": "score",
      "node_type": {"type": "text", "text": "Score: 0", "font_size": 24, "color": "white"},
      "position": {"Anchored": {"anchor": "top_right", "offset": [-16, 16]}}
    }
  ]
}
```

**Node types:** `text`, `progress_bar`, `panel`, `container`, `image`, `button`, `dialogue_box`, `slot`
(Only `text`, `progress_bar`, `panel`, `container` render visually)

**Anchors:** `top_left`, `top_right`, `bottom_left`, `bottom_right`, `center`

**Colors:** Named (`white`, `black`, `red`, `green`, `blue`, `yellow`, `dark_red`, `dark_green`, `gray`) or hex (`#FF0000`, `#FF000080`)

### POST /ui/screens/{name}/show
Show a named UI screen.

### POST /ui/screens/{name}/hide
Hide a named UI screen.

### POST /ui/screens/{name}/nodes/{id}
Update a node within a screen.

**Body:** `{"node_type": {"type": "text", "text": "Score: 42", "font_size": 24, "color": "white"}}`

### GET /ui/state
Get full UI manager state (all screens and their nodes).

---

## Dialogue

### POST /dialogue/conversations
Define a dialogue conversation tree.

### POST /dialogue/start
Start a dialogue conversation.

**Body:** `{"conversation": "shopkeeper_intro"}`

### GET /dialogue/state
Get current dialogue node state.

### POST /dialogue/choose
Choose an option in active dialogue.

**Body:** `{"choice": 0}`

---

## Sprites & Animation

### GET /sprites
Get sprite manifest (entity-to-sprite mappings).

### POST /sprites
Set sprite manifest.

### GET /sprites/sheets
List named sprite sheet definitions.

### POST /sprites/sheets
Define or update a sprite sheet with animation graph.

**Body:**
```json
{
  "name": "hero",
  "path": "assets/hero.png",
  "frame_width": 32,
  "frame_height": 32,
  "columns": 8,
  "rows": 1,
  "anchor_y": -0.15,
  "animations": {
    "idle": {"frames": [0,1,2,3], "fps": 8, "looping": true},
    "walk": {"frames": [4,5,6,7], "fps": 12, "looping": true},
    "attack": {"frames": [8,9,10], "fps": 15, "looping": false},
    "die": {"frames": [11,12,13], "fps": 10, "looping": false}
  },
  "direction_map": {}
}
```

`anchor_y` (default -0.15) controls vertical sprite anchor offset for collider alignment.

### GET /animations
List animation graphs.

### GET /animations/{name}
Get animation graph by name.

### POST /animations/{name}
Create or replace animation graph.

### DELETE /animations/{name}
Delete animation graph.

### GET /animations/state
List entity animation runtime states.

---

## Visual Effects

### POST /particles/presets
Define named particle presets.

### POST /screen/effect
Trigger a screen effect.

**Body:** `{"effect": "fade_out", "duration": 1.0, "color": [0, 0, 0, 1], "alpha": 1.0}`

Effects: `fade_out`, `fade_in`, `flash`, `tint`

### GET /screen/state
Get current screen effect state.

### GET /parallax/layers
Get parallax background layers.

### POST /parallax/layers
Set parallax background layers.

### GET /weather
Get current weather state.

### POST /weather
Set weather effect.

**Body:** `{"type": "rain", "intensity": 0.8, "wind": 0.3}`

Weather types: `rain`, `snow`, `dust`

### DELETE /weather
Clear active weather effect.

### POST /world_text
Spawn floating world-space text.

**Body:** `{"x": 100, "y": 50, "text": "Critical!", "font_size": 24, "color": "red", "duration": 1.0}`

---

## Lighting

### POST /lighting/config
Set lighting configuration.

**Body:** `{"enabled": true, "ambient": 0.3}`

### GET /lighting/state
Get current lighting state.

### GET /lighting/day_night
Get day/night cycle state.

### POST /lighting/day_night
Configure day/night cycle.

---

## Physics & Raycasting

### GET /physics
Get legacy physics config.

### POST /physics
Set legacy physics config.

### POST /physics/raycast
Tilemap raycast query.

**Body:** `{"origin": {"x": 0, "y": 100}, "direction": {"x": 1, "y": 0}, "max_distance": 500}`

**Response:** `{"hit": true, "x": 160, "y": 100, "tile_x": 10, "tile_y": 6, "distance": 160, "normal_x": -1, "normal_y": 0}`

### POST /physics/raycast_entities
Entity AABB raycast query.

**Body:** `{"origin": {"x": 0, "y": 100}, "direction": {"x": 1, "y": 0}, "max_distance": 500, "tag": "enemy"}`

**Response:** `{"hits": [{"id": 42, "x": 150, "y": 100, "distance": 150}]}`

---

## AI & Pathfinding

### POST /ai/pathfind
Find path waypoints between world points.

**Body:** `{"from": {"x": 50, "y": 50}, "to": {"x": 200, "y": 150}, "path_type": "top_down"}`

**Response:** `{"path": [{"x": 50, "y": 50}, {"x": 100, "y": 100}, {"x": 200, "y": 150}]}`

### POST /ai/line_of_sight
Check line-of-sight between two world points.

**Body:** `{"from": {"x": 50, "y": 50}, "to": {"x": 200, "y": 150}}`

**Response:** `{"clear": true}`

---

## Game State

### GET /game/state
Get current runtime game state.

**Response:** `{"state": "Playing"}`

States: `Loading`, `Menu`, `Playing`, `Paused`, `GameOver`, `LevelTransition`, `Cutscene`

### POST /game/state
Set runtime game state.

**Body:** `{"state": "Playing"}`

### POST /game/transition
Transition to another runtime game state with optional visual effect.

**Body:** `{"to": "GameOver", "effect": "FadeBlack", "duration": 0.5}`

### GET /game/transitions
List recent runtime game state transitions.

### POST /game/restart
Restart last level loaded via game flow APIs.

---

## Save / Load

### POST /save
Save current game state to slot.

**Body:** `{"slot": "save1"}`

### POST /load
Load game state from slot.

**Body:** `{"slot": "save1"}`

### GET /saves
List available save slots.

---

## Level Packs

### POST /levels/pack
Define a multi-level campaign.

### POST /levels/pack/{name}/start
Start campaign at level 0.

### POST /levels/pack/{name}/next
Advance to next level.

### GET /levels/pack/{name}/progress
Get campaign progress.

### POST /levels/export
Export current level + entities as JSON.

### POST /levels/import
Import a previously exported level JSON.

---

## Export / Import

### POST /game/export
Export full game/project snapshot as JSON.

### POST /game/import
Import full game/project snapshot from JSON.

### POST /export/web
Build a web export bundle (WASM + HTML).

### POST /export/desktop
Build a desktop export bundle.

### GET /examples
List built-in example game recipes.

### POST /examples/{name}/load
Generate and load a built-in example.

---

## Input

### GET /input/gamepad
Get gamepad configuration and connected count.

### POST /input/gamepad
Set gamepad configuration.

### GET /input/bindings
Get current input key bindings.

### POST /input/bindings
Set input key bindings.

**Body:**
```json
{
  "keyboard": {"left": ["ArrowLeft", "A"], "right": ["ArrowRight", "D"]},
  "gamepad": {"jump": "South", "attack": "West"}
}
```

Default keyboard bindings:
- Movement: Arrow keys or WASD
- Jump: Space
- Attack: Z, X, or Enter
- Sprint: Shift

---

## Inventory & Items

### POST /items/define
Define item types for inventory system.

### GET /entities/{id}/inventory
Get entity inventory.

### POST /entities/{id}/inventory
Add/remove/clear items.

**Body:** `{"action": "add", "item_id": "health_potion", "count": 1}`

---

## Cutscenes

### POST /cutscene/define
Define a cutscene sequence.

### POST /cutscene/play
Play a defined cutscene.

**Body:** `{"name": "intro"}`

### POST /cutscene/stop
Stop the active cutscene.

### GET /cutscene/state
Get cutscene playback state.

---

## Presets & Pools

### GET /presets
List custom spawn presets.

### POST /presets
Define custom spawn presets.

**Body:**
```json
{
  "fast_zombie": {
    "tags": ["enemy", "zombie"],
    "components": [
      {"type": "collider", "width": 12, "height": 14},
      {"type": "top_down_mover", "speed": 120},
      {"type": "health", "current": 2, "max": 2},
      {"type": "contact_damage", "amount": 1, "cooldown_frames": 20, "knockback": 80, "damage_tag": "player"}
    ]
  }
}
```

### POST /pool/init
Initialize entity pool with preset.

**Body:** `{"preset": "fast_zombie", "size": 20}`

### POST /pool/acquire
Acquire entity from pool.

**Body:** `{"pool": "fast_zombie", "x": 100, "y": 50}`

### POST /pool/release/{id}
Release entity back to pool.

### GET /pool/status
Get entity pool statistics.

---

## Testing & Simulation

### POST /simulate
Run headless physics simulation.

**Body:**
```json
{
  "inputs": [
    {"frame": 0, "action": "right", "duration": 60},
    {"frame": 30, "action": "attack", "duration": 5}
  ],
  "max_frames": 300,
  "record_interval": 10,
  "goal_position": [400, 56],
  "goal_radius": 12
}
```

**Response:** `{outcome, frames_elapsed, trace: [{x,y,vx,vy,grounded,frame}], events, entity_events, entity_states}`

### POST /simulate/world
Run world simulation with full game loop (scripts, AI, physics, interactions).

**Body:** `{"frames": 300, "real": true, "inputs": [...]}`

Set `real: true` for full game loop, `false` for physics-only.

### POST /test/scenario
Run scenario test with setup, simulation, and assertions.

### POST /test/playtest
Run AI playtester agent.

**Body:**
```json
{
  "frames": 600,
  "mode": "top_down",
  "goal": "survive"
}
```

Modes: `platformer`, `top_down`
Goals: `survive`, `reach_goal`, `explore`

**Response:** Difficulty rating, events, damage taken, exploration stats.

### POST /replay/record
Start replay recording.

### POST /replay/stop
Stop replay recording and save.

### POST /replay/play
Play a saved replay.

### GET /replay/list
List saved replays.

### GET /telemetry
Get gameplay telemetry (deaths, inputs, entity counts, damage).

### DELETE /telemetry
Reset gameplay telemetry counters.

---

## Assets

### POST /assets/upload
Upload base64-encoded image asset.

**Body:** `{"name": "hero.png", "data": "<base64>", "asset_type": "sprite"}`

### POST /assets/generate
Generate a colored rectangle sprite asset.

**Body:** `{"name": "enemy_sprite", "width": 32, "height": 32, "color": [200, 50, 50], "label": "E", "style": "flat"}`

Defaults: 32x32, gray [128,128,128]

### GET /assets/list
List image assets in assets directory.

### GET /screenshot
Take screenshot. Auto-numbered in screenshot_path directory.

**Response:** `{"path": "screenshot_001.png", "data": "<base64>", "width": 800, "height": 600}`

### POST /screenshot/baseline
Save current screenshot as visual regression baseline.

### POST /screenshot/diff
Compare current screenshot against baseline.

---

## Debug & Performance

### GET /perf
Runtime performance metrics (FPS, entity count, frame times).

### GET /perf/history
Recent performance samples (ring buffer).

### GET /debug/overlay
Get debug overlay state.

### POST /debug/overlay
Set debug overlay visibility and features.

**Body:** `{"show": true, "features": ["colliders", "paths", "spatial_hash"]}`

### GET /debug/input
Get current virtual input state.

### GET /scene/describe
Comprehensive scene snapshot: game state, entities, tilemap, UI, audio, camera, vars, perf.

Optional `?grid=N` parameter divides world into NxN cells for spatial analysis.

### GET /events
Get recent game events.

### GET /events/subscribe
SSE stream of game events (real-time).

---

## Health & Diagnostics

### GET /health
Unified health check.

**Response:**
```json
{
  "status": "healthy",
  "has_player": true,
  "entity_count": 15,
  "script_error_count": 0,
  "game_state": "Playing",
  "game_vars_count": 3,
  "tilemap_set": true,
  "issues": []
}
```

Status: `healthy`, `warning`, `unhealthy`

### GET /diagnose
Diagnose entities with missing companion components.

**Response:**
```json
{
  "entity_count": 15,
  "issues_count": 2,
  "entities": [
    {
      "id": 5,
      "tags": ["enemy"],
      "issues": [
        {"component": "ContactDamage", "severity": "error", "message": "ContactDamage requires Collider for collision detection", "missing": ["Collider"]}
      ]
    }
  ]
}
```

### POST /evaluate
Holistic game evaluation.

**Response:**
```json
{
  "scores": {
    "has_player": true,
    "has_enemies": true,
    "has_scripts": true,
    "script_errors": 0,
    "entity_count": 12,
    "tile_variety": 3,
    "has_goal": false,
    "game_vars_count": 5
  },
  "issues": ["No goal tile found"],
  "overall": "good"
}
```

### POST /evaluate/screenshot
Take screenshot + analyze + describe scene in one call.

---

## Documentation

### GET /docs
Full API docs. Use `?for=category` to filter.

Categories: `core`, `platformer`, `top_down`, `combat`, `visual`, `audio`, `narrative`, `testing`

### GET /docs/html
HTML API documentation page.

### GET /docs/quickstart
5-step hello world guide.

### GET /docs/workflow
Common task recipes.

### GET /docs/endpoints
Endpoint list.

### GET /docs/components
Component schema list.

### GET /docs/presets
Entity preset list.

### GET /docs/templates
Generation template list.

### GET /docs/constraints
Validation constraint list.

### GET /docs/scripts
Scripting API surface.

### GET /docs/examples
Built-in example recipes.

### GET /docs/security
API auth and rate-limit configuration.

---

## Appendix A: Component Types

### collider
```json
{"type": "collider", "width": 14, "height": 16}
```

### circle_collider
```json
{"type": "circle_collider", "radius": 8}
```

### gravity_body
```json
{"type": "gravity_body"}
```

### horizontal_mover
```json
{"type": "horizontal_mover", "speed": 200, "left_action": "left", "right_action": "right"}
```

### jumper
```json
{
  "type": "jumper",
  "velocity": 400,
  "action": "jump",
  "fall_multiplier": 1.5,
  "variable_height": true,
  "coyote_frames": 5,
  "buffer_frames": 4
}
```

### top_down_mover
```json
{
  "type": "top_down_mover",
  "speed": 200,
  "up_action": "up",
  "down_action": "down",
  "left_action": "left",
  "right_action": "right"
}
```

### health
```json
{"type": "health", "current": 10, "max": 10}
```

### contact_damage
```json
{
  "type": "contact_damage",
  "amount": 1,
  "cooldown_frames": 12,
  "knockback": 80,
  "damage_tag": "player"
}
```
**Requires: collider.** `damage_tag` = who gets hurt (entities with this tag).

### hitbox
```json
{
  "type": "hitbox",
  "width": 24,
  "height": 20,
  "offset": {"x": 10, "y": 0},
  "active": false,
  "damage": 2,
  "damage_tag": "enemy"
}
```

### pickup
```json
{
  "type": "pickup",
  "pickup_tag": "player",
  "effect": {"type": "heal", "amount": 1}
}
```
**Requires: collider.** Effects: `heal`, `score_add`, `custom`.

### trigger_zone
```json
{
  "type": "trigger_zone",
  "radius": 32,
  "trigger_tag": "player",
  "event_name": "entered_zone",
  "one_shot": true
}
```

### projectile
```json
{
  "type": "projectile",
  "speed": 400,
  "direction": {"x": 1, "y": 0},
  "lifetime_frames": 90,
  "damage": 1,
  "owner_id": 0,
  "damage_tag": "enemy"
}
```

### ai_behavior
```json
{"type": "ai_behavior", "behavior": {"type": "chase", "target_tag": "player", "speed": 100, "detection_radius": 200, "give_up_radius": 400}}
```

Behavior types: `patrol`, `chase`, `flee`, `guard`, `wander`, `custom`

### path_follower
```json
{
  "type": "path_follower",
  "target": {"x": 200, "y": 100},
  "recalculate_interval": 20,
  "path_type": "top_down",
  "speed": 100
}
```

### moving_platform
```json
{
  "type": "moving_platform",
  "waypoints": [{"x": 100, "y": 50}, {"x": 300, "y": 50}],
  "speed": 60,
  "loop_mode": "ping_pong",
  "pause_frames": 30,
  "carry_riders": true
}
```

### animation_controller
```json
{
  "type": "animation_controller",
  "graph": "hero",
  "state": "idle",
  "speed": 1.0,
  "playing": true,
  "facing_right": true,
  "auto_from_velocity": true,
  "facing_direction": 5
}
```

### render_layer
```json
{"type": "render_layer", "layer": 5}
```

### collision_layer
```json
{"type": "collision_layer", "layer": 1, "mask": 65535}
```

### point_light
```json
{"type": "point_light", "radius": 3.0, "intensity": 1.0, "color": [1, 0.9, 0.7]}
```

### sprite_color_tint
```json
{"type": "sprite_color_tint", "color": [1, 0, 0, 1], "flash_color": [1, 1, 1, 1], "flash_frames": 6}
```

### trail_effect
```json
{"type": "trail_effect", "interval": 3, "duration": 0.3, "alpha_start": 1.0, "alpha_end": 0.0}
```

### state_machine
```json
{"type": "state_machine", "states": {"idle": {}, "attacking": {}, "hurt": {}}, "initial": "idle"}
```

### inventory
```json
{"type": "inventory", "max_slots": 20}
```

### velocity_damping
```json
{"type": "velocity_damping", "factor": 0.1}
```

### knockback_impulse
```json
{"type": "knockback_impulse", "vx": 200, "vy": -100}
```

### solid_body
```json
{"type": "solid_body"}
```

### invisible
```json
{"type": "invisible"}
```

### particle_emitter
Complex particle definition. See `/docs/components` for full schema.

---

## Appendix B: Lua API Surface

### Entity Object (per-entity scripts)

**Properties (read/write):** `id`, `x`, `y`, `vx`, `vy`, `grounded`, `alive`, `visible`, `health`, `max_health`, `speed`, `animation`, `animation_frame`, `flip_x`, `render_layer`, `collision_layer`, `collision_mask`, `machine_state`, `facing_direction`, `state`, `tags`

**Methods:** `damage(amount)`, `heal(amount)`, `knockback(dx, dy)`, `has_tag(tag)`, `add_tag(tag)`, `remove_tag(tag)`, `follow_path(path, speed)`, `flash(color, frames)`, `transition_state(state)`

**Hitbox sub-table:** `hitbox.active`, `hitbox.damage`, `hitbox.width`, `hitbox.height`, `hitbox.damage_tag`

**AI sub-table:** `ai.state`, `ai.target_id`, `ai.chase(id)`, `ai.idle()`

**Inventory methods:** `add_item(id, count)`, `remove_item(id, count)`, `has_item(id)`, `count_item(id)`

### World Object

**Properties:** `frame`, `time`, `dt`, `game_state`

**Variables:** `get_var(key)`, `set_var(key, value)`

**Entity queries:** `player()`, `get_entity(id)`, `find_all(tag?)`, `find_in_radius(x, y, r, tag?)`, `find_nearest(x, y, tag?)`

**Spawning:** `spawn(spec)`, `spawn_projectile(spec)`, `spawn_particles(preset, x, y)`, `despawn(id)`

**Tiles:** `is_solid(x, y)`, `is_platform(x, y)`, `is_climbable(x, y)`, `get_tile(x, y)`, `set_tile(x, y, id)`, `tile_friction(x, y)`

**Raycasting:** `raycast(ox, oy, dx, dy, max_dist)`, `raycast_entities(ox, oy, dx, dy, max_dist, tag?)`, `find_path(fx, fy, tx, ty, type?)`, `line_of_sight(x1, y1, x2, y2)`

**Input:** `input.pressed(action)`, `input.just_pressed(action)`, `input.mouse_x`, `input.mouse_y`, `input.mouse_pressed(btn)`, `input.mouse_just_pressed(btn)`

**Audio:** `play_sfx(name, opts?)`, `play_music(name, opts?)`, `stop_music(opts?)`, `set_volume(channel, val)`

**Camera:** `camera.shake(intensity, duration)`, `camera.zoom(factor)`, `camera.look_at(x, y)`

**UI:** `ui.show_screen(name)`, `ui.hide_screen(name)`, `ui.set_text(id, text)`, `ui.set_progress(id, val, max)`

**Dialogue:** `dialogue.start(conv)`, `dialogue.choose(idx)`

**Game state:** `game.pause()`, `game.resume()`, `game.transition(state, opts?)`

**Events:** `emit(name, data)`, `on(name, handler)`, `log(level?, message)`

**Effects:** `screen_flash(dur, color)`, `screen_fade_out(dur, color)`, `screen_fade_in(dur)`, `set_ambient(intensity, color)`

**Misc:** `spawn_text(x, y, text, opts)`, `tween(entity_id, opts)`, `tween_sequence(entity_id, steps)`, `set_weather(type, intensity, wind)`, `clear_weather()`
