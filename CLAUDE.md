# Axiom Engine — Agent Guide

Axiom is an AI-native 2D game engine. You build games by sending HTTP requests to `http://127.0.0.1:3000` and uploading Lua scripts. The engine handles physics, collision, rendering, audio, and scripting. You never touch Rust code.

## Quick Start — Zero to Game in 5 Calls

```bash
# 1. Set config (top-down game with no gravity)
curl -X POST http://127.0.0.1:3000/config \
  -H "Content-Type: application/json" \
  -d '{"gravity": {"x": 0, "y": 0}, "tile_size": 16}'

# 2. Load a tilemap (30x25, all empty)
curl -X POST http://127.0.0.1:3000/level \
  -H "Content-Type: application/json" \
  -d '{"width": 30, "height": 25, "tiles": [0,0,0,...], "player_spawn": [100, 200]}'

# 3. Spawn the player
curl -X POST http://127.0.0.1:3000/entities \
  -H "Content-Type: application/json" \
  -d '{"x": 100, "y": 200, "is_player": true, "components": [{"type": "collider", "width": 14, "height": 14}, {"type": "top_down_mover", "speed": 200}, {"type": "health", "current": 10, "max": 10}]}'

# 4. Spawn an enemy
curl -X POST http://127.0.0.1:3000/entities \
  -H "Content-Type: application/json" \
  -d '{"x": 200, "y": 150, "tags": ["enemy"], "components": [{"type": "collider", "width": 12, "height": 12}, {"type": "top_down_mover", "speed": 80}, {"type": "health", "current": 3, "max": 3}, {"type": "contact_damage", "amount": 1, "cooldown_frames": 30, "knockback": 100, "damage_tag": "player"}]}'

# 5. Upload a Lua script for the enemy
curl -X POST http://127.0.0.1:3000/scripts \
  -H "Content-Type: application/json" \
  -d '{"name": "chase_ai", "source": "function update(entity, world, dt)\n  local p = world.player()\n  if not p then return end\n  local dx = p.x - entity.x\n  local dy = p.y - entity.y\n  local dist = math.sqrt(dx*dx + dy*dy)\n  if dist > 1 then\n    entity.vx = (dx/dist) * 80\n    entity.vy = (dy/dist) * 80\n  end\nend"}'
```

### The /build Shortcut — Everything in One Call

`POST /build` applies config, tilemap, entities, and scripts atomically:

```json
{
  "config": {"gravity": {"x": 0, "y": 0}, "tile_size": 16},
  "tilemap": {
    "width": 20, "height": 15,
    "tiles": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,1,1,1,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
              1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
    "player_spawn": [48, 48]
  },
  "entities": [
    {"x": 48, "y": 48, "is_player": true, "components": [
      {"type": "collider", "width": 14, "height": 14},
      {"type": "top_down_mover", "speed": 200},
      {"type": "health", "current": 10, "max": 10}
    ]},
    {"x": 200, "y": 100, "tags": ["enemy"], "script": "chase_ai", "components": [
      {"type": "collider", "width": 12, "height": 12},
      {"type": "top_down_mover", "speed": 80},
      {"type": "health", "current": 3, "max": 3},
      {"type": "contact_damage", "amount": 1, "cooldown_frames": 30, "knockback": 100, "damage_tag": "player"}
    ]}
  ],
  "scripts": {
    "chase_ai": "function update(entity, world, dt)\n  local p = world.player()\n  if not p then return end\n  local dx = p.x - entity.x\n  local dy = p.y - entity.y\n  local dist = math.sqrt(dx*dx + dy*dy)\n  if dist > 1 then\n    entity.vx = (dx/dist) * 80\n    entity.vy = (dy/dist) * 80\n  end\nend"
  }
}
```

## Game Modes

### Top-Down (Zelda, Vampire Survivors)
- Config: `"gravity": {"x": 0, "y": 0}`
- Player uses: `top_down_mover` component
- Movement: WASD/arrow keys move in all 4 directions

### Platformer (Mario, Celeste)
- Config: `"gravity": {"x": 0, "y": -980}`
- Player uses: `horizontal_mover` + `jumper` + `gravity_body` components
- Movement: left/right + jump, gravity pulls down

**Gravity gotcha:** Y is **negative** for downward gravity (`-980`, not `980`).

## Entity System

Every game object is an entity with a unique `NetworkId` (u64). Entities have:
- Position (`x`, `y`) and velocity (`vx`, `vy`)
- Optional components (collider, health, AI, etc.)
- Optional tags (strings like `"enemy"`, `"pickup"`)
- Optional Lua script (runs `update()` every frame)

### Spawning Entities

```json
POST /entities
{
  "x": 100, "y": 50,
  "is_player": false,
  "tags": ["enemy", "zombie"],
  "script": "zombie_ai",
  "components": [
    {"type": "collider", "width": 12, "height": 14},
    {"type": "top_down_mover", "speed": 100},
    {"type": "health", "current": 3, "max": 3},
    {"type": "contact_damage", "amount": 1, "cooldown_frames": 20, "knockback": 80, "damage_tag": "player"}
  ]
}
```

### Component Reference

| Component | Fields | Notes |
|-----------|--------|-------|
| `collider` | `width`, `height` | **Required for physics/combat.** Axis-aligned bounding box |
| `circle_collider` | `radius` | Alternative circular collider |
| `gravity_body` | *(none)* | Enables gravity. Platformer only |
| `horizontal_mover` | `speed`, `left_action?`, `right_action?` | Left/right movement. Defaults: actions="left"/"right" |
| `jumper` | `velocity`, `action?`, `fall_multiplier?`, `variable_height?`, `coyote_frames?`, `buffer_frames?` | Jump physics. Defaults: action="jump", fall_multiplier=1.5, variable_height=true, coyote=5, buffer=4 |
| `top_down_mover` | `speed`, `up_action?`, `down_action?`, `left_action?`, `right_action?` | 4-directional movement |
| `health` | `current`, `max` | Hit points. Entity dies when current reaches 0 |
| `contact_damage` | `amount`, `cooldown_frames?`, `knockback?`, `damage_tag` | Deals damage on collision. **Requires collider.** Default cooldown=12 |
| `hitbox` | `width`, `height`, `offset?`, `active?`, `damage`, `damage_tag` | Attack hitbox. Toggle `active` to swing. Default active=false. Offset is `{"x":N,"y":N}` |
| `pickup` | `pickup_tag`, `effect` | Collectible. **Requires collider.** Effect: `{"type":"heal","amount":1}`, `{"type":"score_add","amount":10}`, or `{"type":"custom","name":"key"}` |
| `trigger_zone` | `radius`, `trigger_tag`, `event_name`, `one_shot?` | Emits event when tagged entity enters radius |
| `projectile` | `speed`, `direction`, `lifetime_frames`, `damage`, `owner_id`, `damage_tag` | Moving bullet. Auto-despawns after lifetime |
| `ai_behavior` | `behavior` | AI controller. See AI Behaviors below |
| `path_follower` | `target`, `recalculate_interval?`, `path_type?`, `speed` | Pathfinding. path_type: "top_down" or "platformer" |
| `moving_platform` | `waypoints`, `speed`, `loop_mode?`, `pause_frames?`, `carry_riders?` | Moving solid. loop_mode: "loop" or "ping_pong" |
| `animation_controller` | `graph`, `state?`, `frame?`, `speed?`, `playing?`, `facing_right?`, `auto_from_velocity?` | Sprite animation. Links to animation graph |
| `particle_emitter` | *(complex)* | Particle effect attached to entity |
| `render_layer` | `layer` | Z-order. Higher = on top. Default 0 |
| `collision_layer` | `layer`, `mask` | Collision filtering. Default layer=1, mask=0xFFFF |
| `point_light` | `radius`, `intensity`, `color` | Dynamic light source |
| `sprite_color_tint` | `color`, `flash_color?`, `flash_frames?` | Color tint [r,g,b,a]. Flash for hit feedback |
| `trail_effect` | `interval`, `duration`, `alpha_start`, `alpha_end` | Afterimage trail |
| `state_machine` | `states`, `initial` | Finite state machine |
| `inventory` | `max_slots` | Item container. Default 20 slots |
| `velocity_damping` | `factor` | Friction. Default 0.1 |
| `knockback_impulse` | `vx`, `vy` | One-shot velocity impulse |
| `solid_body` | *(none)* | Solid obstacle (non-player) |
| `invisible` | *(none)* | Hidden from rendering |

### Component Gotchas

- **ContactDamage and Pickup REQUIRE a collider.** Without a collider, they silently do nothing. Use `GET /diagnose` to find these issues.
- **damage_tag** controls WHO gets damaged. `"damage_tag": "player"` means "I damage entities tagged `player`". An enemy that should hurt the player uses `"damage_tag": "player"`. A player attack that should hurt enemies uses `"damage_tag": "enemy"`.
- **Hitbox vs ContactDamage:** ContactDamage is passive (damages on any collision). Hitbox is active (toggle `active` on/off for attacks).

### AI Behaviors

```json
{"type": "patrol", "waypoints": [{"x":100,"y":50}, {"x":300,"y":50}], "speed": 80}
{"type": "chase", "target_tag": "player", "speed": 100, "detection_radius": 200, "give_up_radius": 400}
{"type": "flee", "threat_tag": "player", "speed": 120, "detection_radius": 150, "give_up_radius": 300}
{"type": "guard", "position": {"x":200,"y":100}, "radius": 50, "chase_radius": 150, "speed": 90, "target_tag": "player"}
{"type": "wander", "speed": 60, "radius": 100, "pause_frames": 30}
{"type": "custom", "script": "my_ai_script"}
```

### Entity Presets

Spawn preconfigured entities with `POST /entities/preset`:

| Preset | Description | Key Components |
|--------|-------------|----------------|
| `platformer_player` | Side-scrolling player | Collider, GravityBody, HorizontalMover, Jumper, Health(3) |
| `top_down_player` | Top-down player | Collider, TopDownMover(200), Health(3) |
| `patrol_enemy` | Walks between waypoints | HorizontalMover, AiBehavior(Patrol), Health(2), ContactDamage(1) |
| `chase_enemy` | Chases player | TopDownMover, AiBehavior(Chase), Health(2), ContactDamage(1) |
| `guard_enemy` | Guards a position | TopDownMover, AiBehavior(Guard), Health(3), ContactDamage(1) |
| `turret` | Stationary shooter | AiBehavior(Guard, speed=0), Health(3), ContactDamage(1) |
| `flying_enemy` | Fast aerial enemy | AiBehavior(Chase, speed=165), Health(1), ContactDamage(1) |
| `boss` | Large boss entity | Collider(22x26), Health(20), ContactDamage(2), Hitbox(28x24) |
| `health_pickup` | Heals on contact | Pickup(heal: 1.0) |
| `moving_platform` | Moving solid surface | MovingPlatform(ping_pong) |

```json
POST /entities/preset
{
  "preset": "chase_enemy",
  "x": 200, "y": 100,
  "config": {
    "health": 5, "speed": 120, "contact_damage": 2,
    "detection_radius": 250, "script": "zombie_ai",
    "tags": ["enemy", "zombie"]
  }
}
```

## Lua Scripting

### Entity Scripts vs Global Scripts

**Entity scripts** run once per entity per frame. Set `"global": false` (default):
```lua
function update(entity, world, dt)
  -- entity = the entity this script is attached to
  -- world = global game state and utilities
  -- dt = delta time in seconds
end
```

**Global scripts** run once per frame. Set `"global": true`:
```lua
function update(world, dt)
  -- No entity parameter — use world queries instead
end
```

Upload: `POST /scripts {"name": "my_script", "source": "...", "global": false}`

### Lifecycle Hooks

- `update(entity, world, dt)` — Called every frame (required)
- `on_death(entity, world)` — Called once when entity dies (optional)
- `init(entity, world)` — Called on first load (optional)
- `always_run` scripts execute even during pause/game_over

### Entity Properties (read/write in update)

```lua
entity.id            -- NetworkId (read-only)
entity.x, entity.y   -- position
entity.vx, entity.vy -- velocity
entity.grounded      -- on ground (read-only)
entity.alive         -- alive status (set false to kill)
entity.visible       -- rendering visibility
entity.health        -- current health
entity.max_health    -- max health
entity.speed         -- TopDownMover speed (writable)
entity.animation     -- current animation state name
entity.animation_frame -- current frame index
entity.flip_x        -- sprite facing direction
entity.render_layer  -- z-order
entity.state         -- persistent per-entity table (survives between frames)
entity.tags          -- {tag_name = true} table
entity.machine_state -- state machine current state
entity.facing_direction -- 8-dir index: 0=NW,1=N,2=NE,3=E,4=SE,5=S,6=SW,7=W
entity.collision_layer  -- collision layer bitmask
entity.collision_mask   -- collision mask bitmask
```

### Entity Methods

```lua
entity.damage(amount)        -- deal damage, returns new health
entity.heal(amount)          -- heal, returns new health
entity.knockback(dx, dy)     -- velocity impulse
entity.has_tag("enemy")      -- check tag
entity.add_tag("burning")    -- add tag
entity.remove_tag("burning") -- remove tag
entity.follow_path(path, speed) -- follow waypoint array [{x,y}, ...]
entity.flash({1,0,0,1}, 6)  -- red flash for 6 frames (hit feedback)
entity.transition_state("attacking") -- state machine transition
```

Dot or colon syntax both work: `entity.damage(5)` or `entity:damage(5)`.

### Entity Hitbox (if hitbox component exists)

```lua
entity.hitbox.active = true   -- activate attack
entity.hitbox.damage = 2
entity.hitbox.width, entity.hitbox.height
entity.hitbox.damage_tag = "enemy"
```

### Entity AI (if ai_behavior component exists)

```lua
entity.ai.state              -- "idle", "chasing", "patrolling", etc.
entity.ai.target_id          -- current chase target
entity.ai.chase(target_id)   -- override: start chasing
entity.ai.idle()             -- override: go idle
```

### Entity Inventory (if inventory component exists)

```lua
entity.add_item("health_potion", 1)
entity.remove_item("health_potion", 1)
entity.has_item("key")       -- bool
entity.count_item("arrows")  -- number
```

### World Object Reference

#### Core Properties
```lua
world.frame      -- current simulation frame (u64)
world.time       -- total elapsed time (seconds)
world.dt         -- delta time this frame
world.game_state -- current state: "Playing", "Paused", "GameOver"
```

#### Variables (persist across frames)
```lua
world.get_var("score")        -- read global variable
world.set_var("score", 100)   -- write global variable
```

#### Entity Queries
```lua
world.player()                          -- player snapshot or nil
world.get_entity(id)                    -- entity by NetworkId or nil
world.find_all("zombie")               -- all entities with tag
world.find_all()                        -- all entities
world.find_in_radius(x, y, r, "enemy") -- entities in radius with tag
world.find_in_radius(x, y, r)          -- entities in radius (any)
world.find_nearest(x, y, "enemy")      -- nearest entity with tag
world.find_nearest(x, y)               -- nearest entity
```

**Snapshot methods** (on results from queries):
```lua
local target = world.find_nearest(entity.x, entity.y, "enemy")
if target then
    target.id, target.x, target.y       -- read-only properties
    target.health, target.alive          -- health info
    target.has_tag("zombie")             -- check tag
    target.damage(5)                     -- apply damage
    target.heal(2)                       -- heal
    target.knockback(100, 0)             -- push
    target.set_position(x, y)            -- teleport
    target.set_velocity(vx, vy)          -- set velocity
    target.set_alive(false)              -- kill
end
```

#### Spawning
```lua
-- Spawn entity with component tables
world.spawn({
    x = 100, y = 50,
    tags = {"enemy", "zombie"},
    script = "zombie_ai",
    health = 3,
    components = {
        {type = "collider", width = 12, height = 14},
        {type = "top_down_mover", speed = 90},
        {type = "contact_damage", amount = 1, cooldown_frames = 20, knockback = 0, damage_tag = "player"},
    }
})

-- Spawn projectile
world.spawn_projectile({
    x = entity.x, y = entity.y,
    speed = 300,
    direction = {x = 1, y = 0},
    damage = 1,
    damage_tag = "enemy",
    owner = entity.id,
    lifetime = 60,
})

world.despawn(entity_id)             -- remove entity
world.spawn_particles("explosion", x, y) -- particle effect
```

#### Tile Queries
```lua
world.is_solid(x, y)        -- solid tile check
world.is_platform(x, y)     -- one-way platform check
world.is_climbable(x, y)    -- ladder check
world.get_tile(x, y)        -- raw tile ID (0-7)
world.set_tile(x, y, id)    -- modify tilemap at runtime
world.tile_friction(x, y)   -- friction value
```

#### Raycasting & Pathfinding
```lua
-- Tilemap raycast
local hit = world.raycast(ox, oy, dx, dy, max_dist)
-- hit = {x, y, tile_x, tile_y, distance, normal_x, normal_y} or nil

-- Entity raycast
local hits = world.raycast_entities(ox, oy, dx, dy, max_dist, "enemy")
-- hits = [{id, x, y, distance}]

-- Pathfinding
local path = world.find_path(from_x, from_y, to_x, to_y, "top_down")
-- path = [{x, y}] or empty

-- Line of sight
local clear = world.line_of_sight(x1, y1, x2, y2)  -- bool
```

#### Input
```lua
world.input.pressed("left")        -- held down this frame
world.input.just_pressed("attack")  -- pressed this frame only
world.input.mouse_x, world.input.mouse_y   -- world-space mouse position
world.input.mouse_pressed("left")           -- mouse button held
world.input.mouse_just_pressed("left")      -- mouse button just pressed
```

Actions: `"left"`, `"right"`, `"up"`, `"down"`, `"jump"`, `"attack"`, `"sprint"`
Keys: Attack = Z/X/Enter, Sprint = Shift, Jump = Space

#### Events
```lua
world.emit("zombie_killed", {id = entity.id, score = 10})

world.on("zombie_killed", function(data)
    -- Called for each matching event this frame
end)
```

#### Audio
```lua
world.play_sfx("hit")
world.play_sfx("hit", {volume = 0.5, pitch = 1.2})
world.play_music("dungeon", {fade_in = 2.0})
world.stop_music({fade_out = 1.0})
world.set_volume("master", 0.8)  -- channels: "master", "sfx", "music"
```

#### Camera
```lua
world.camera.shake(5.0, 0.3)     -- intensity, duration
world.camera.zoom(2.0)           -- zoom factor
world.camera.look_at(200, 100)   -- look at position
```

#### UI
```lua
world.ui.show_screen("hud")
world.ui.hide_screen("game_over")
world.ui.set_text("score_label", "Score: 42")
world.ui.set_progress("health_bar", 3, 10)  -- value, max
```

#### Dialogue
```lua
world.dialogue.start("shopkeeper_intro")
world.dialogue.choose(0)  -- select option index
```

#### Game State
```lua
world.game.pause()
world.game.resume()
world.game.transition("GameOver", {effect = "FadeBlack", duration = 0.5})
```

#### Screen Effects
```lua
world.screen_flash(0.3, {1, 1, 1, 0.5})    -- white flash
world.screen_fade_out(1.0, {0, 0, 0, 1})    -- fade to black
world.screen_fade_in(1.0)                     -- fade from black
```

#### Misc
```lua
world.log("info", "Debug message")     -- script log (or just world.log("msg"))
world.spawn_text(x, y, "Critical!", {font_size = 24, color = "red", duration = 1.0})
world.tween(entity_id, {property = "x", to = 200, duration = 0.5, easing = "ease_out"})
```

## Tilemap

### Tile Types

| ID | Type | Notes |
|----|------|-------|
| 0 | Empty | Walkable floor |
| 1 | Solid | Wall / ground |
| 2 | Spike | Damage on contact |
| 3 | Goal | Triggers goal event |
| 4 | Platform | One-way (platformer only) |
| 5 | SlopeUp | Diagonal surface |
| 6 | SlopeDown | Diagonal surface |
| 7 | Ladder | Climbable |

### Tilemap Format

```json
POST /level
{
  "width": 20, "height": 15,
  "tiles": [1,1,1,...],
  "player_spawn": [100, 200],
  "goal": [18, 1]
}
```

- `tiles`: flat array of length `width * height`, row-major (top-left to bottom-right)
- `player_spawn`: **world coordinates** in pixels `[x, y]`
- `goal`: **tile coordinates** `[col, row]` (not pixels)

**Gotcha:** `player_spawn` is in pixels but `goal` is in tile indices. A tile at column 5, row 3 with tile_size 16 is at pixel position (80, 48).

## Procedural Generation

```json
POST /generate
{
  "template": "top_down_dungeon",
  "difficulty": 0.3,
  "seed": 42,
  "width": 30, "height": 25,
  "constraints": ["top_down_reachable", "bounds_check"]
}
```

**Templates:** `platformer`, `top_down_dungeon`, `rts_arena`, `fighting_arena`, `metroidvania`, `roguelike_floor`, `puzzle_platformer`, `arena_waves`, `side_scroller`, `tower_defense_map`, `boss_arena`

**Constraints:** `reachable`, `top_down_reachable`, `bounds_check`, `has_ground`, `no_softlock`

Returns `{tilemap, player_spawn, goal, validation, difficulty_metrics}` — feed tilemap directly to `POST /level`.

## Visual Rendering

Entities automatically get colored rectangle sprites based on tags:

| Tag | Color |
|-----|-------|
| *(player)* | Blue |
| `enemy` | Red |
| `pickup` / `health` | Green |
| `projectile` | Yellow |
| `npc` | Cyan |
| *(default)* | Gray |

Size matches the collider dimensions. Upload sprite sheets with `POST /sprites/sheets` to override.

### Sprite Sheets

```json
POST /sprites/sheets
{
  "name": "zombie",
  "path": "assets/zombie.png",
  "frame_width": 32, "frame_height": 32, "columns": 8,
  "animations": {
    "idle": {"frames": [0,1,2,3], "fps": 8, "looping": true},
    "walk": {"frames": [4,5,6,7], "fps": 12, "looping": true},
    "die": {"frames": [8,9,10], "fps": 10, "looping": false}
  }
}
```

## UI Screens

```json
POST /ui/screens
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

Anchors: `top_left`, `top_right`, `bottom_left`, `bottom_right`, `center`
Node types that render: `text`, `progress_bar`, `panel`, `container`
Colors: Named (`white`, `red`, `green`, `blue`, `yellow`, `dark_red`, `dark_green`, `gray`, `black`) or hex (`#FF0000`, `#FF000080`)

## Camera

```json
POST /camera/config
{
  "zoom": 3.0,
  "follow_speed": 5.0,
  "follow_target": 1,
  "deadzone": [0, 0]
}
```

Camera auto-follows the player. Set `pixel_snap: true` and `interpolate_transforms: true` in config for smooth pixel art. Keep `deadzone: [0, 0]` to avoid jitter.

## Self-Diagnosis

When something isn't working, use these endpoints:

| Endpoint | What It Checks |
|----------|---------------|
| `GET /health` | Player exists, entity count, script errors, game state. Returns "healthy"/"warning"/"unhealthy" |
| `GET /diagnose` | Missing companion components (e.g., ContactDamage without Collider) |
| `POST /evaluate` | Entity census, script health, tilemap quality. Returns scores + issues + rating |
| `GET /scripts/errors` | Recent Lua errors with script name and line number |
| `GET /perf` | FPS, entity count, frame times |
| `GET /scripts/stats` | Loaded/disabled script counts, error counts |
| `POST /evaluate/screenshot` | Screenshot + visual analysis + scene description in one call |

**Diagnosis workflow:**
1. `GET /health` — Is anything obviously broken?
2. `GET /diagnose` — Are components wired up correctly?
3. `GET /scripts/errors` — Are scripts crashing?
4. `GET /entities?tag=enemy` — Are entities where you expect?
5. `POST /evaluate` — Overall game quality check

## Common Workflows

### Add an Enemy
1. Spawn entity with `collider`, `health`, `contact_damage` (damage_tag: "player"), and an AI behavior or script
2. Add tag `"enemy"` so it renders red
3. Upload a Lua script for custom behavior (or use `ai_behavior` component)

### Add Combat
1. Player needs: `collider`, `health`, `hitbox` (damage_tag: "enemy")
2. Enemies need: `collider`, `health`, `contact_damage` (damage_tag: "player")
3. In player script: toggle `entity.hitbox.active = true` on attack input
4. Optional: add `knockback` to contact_damage, add `entity.flash()` for hit feedback

### Add a Pickup
1. Spawn entity with `collider` and `pickup` component
2. Set `pickup_tag` to who can collect it (e.g., "player")
3. Set effect: `{"type": "heal", "amount": 1}` or `{"type": "score_add", "amount": 10}`
4. Add tag `"pickup"` so it renders green

### Setup Camera
```json
POST /camera/config
{"zoom": 3.0, "follow_speed": 5.0, "deadzone": [0, 0]}
```
In config: `{"pixel_snap": true, "interpolate_transforms": true}`

### Test with Playtest
```json
POST /test/playtest
{"frames": 600, "mode": "top_down", "goal": "survive"}
```
Returns difficulty rating, events, damage taken, exploration stats.

## Audio

```json
POST /audio/sfx
{"effects": {"hit": {"path": "assets/hit.ogg"}, "death": {"path": "assets/death.ogg"}}}

POST /audio/music
{"tracks": {"theme": {"path": "assets/theme.ogg", "looping": true}}}

POST /audio/triggers
{"mappings": {"entity_died": "death", "entity_damaged": "hit"}}
```

## API Response Format

All responses follow:
```json
{"ok": true, "data": <result>, "error": null}
```

On error:
```json
{"ok": false, "data": null, "error": "description of what went wrong"}
```

Use `GET /docs` for the full machine-readable API reference. Use `GET /docs?for=combat` to filter by category.

Categories: `core`, `platformer`, `top_down`, `combat`, `visual`, `audio`, `narrative`, `testing`

## Key Gotchas

1. **Gravity is negative Y.** Use `{"x": 0, "y": -980}` for platformers, not positive.
2. **ContactDamage and Pickup need a collider.** Without one, they silently fail. Use `/diagnose` to catch this.
3. **damage_tag = who gets hurt.** `"damage_tag": "player"` on an enemy means "I hurt the player."
4. **tiles array is flat.** Length must equal `width * height`. Row-major order.
5. **player_spawn is pixels, goal is tile coords.** Different coordinate systems.
6. **Scripts auto-disable after 8 consecutive errors.** Check `/scripts/errors` and re-upload to re-enable.
7. **Config is partial merge.** `POST /config` only updates fields you send, preserving the rest.
8. **Camera jitter fix:** Set `pixel_snap: true`, `interpolate_transforms: true`, `deadzone: [0, 0]` in config.
9. **Collider required for all physics interactions.** No collider = no collisions, damage, or pickups.
10. **Entity scripts must define `function update(entity, world, dt)`.** Missing this = script does nothing.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `AXIOM_API_TOKEN` | API authentication token. If set, all requests need `Authorization: Bearer <token>` or `X-API-Key: <token>` |
| `AXIOM_API_RATE_LIMIT_PER_SEC` | Rate limit per second |
| `AXIOM_ASSETS_DIR` | Custom assets directory |
| `AXIOM_SCREENSHOT_PATH` | Screenshot output directory |
| `AXIOM_SCRIPT_ENTITY_BUDGET_MS` | Per-entity script time limit (default 1ms) |
| `AXIOM_SCRIPT_GLOBAL_BUDGET_MS` | Global script time limit (default 5ms) |
