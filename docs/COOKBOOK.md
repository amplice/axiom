# Axiom Engine — Cookbook

Copy-paste ready recipes. All examples use `curl` but any HTTP client works.

## 1. Hello World (5 calls)

```bash
# Config
curl -X POST http://127.0.0.1:3000/config -H "Content-Type: application/json" \
  -d '{"gravity": {"x": 0, "y": 0}, "tile_size": 16}'

# Level (10x10 bordered room)
curl -X POST http://127.0.0.1:3000/level -H "Content-Type: application/json" \
  -d '{"width":10,"height":10,"tiles":[1,1,1,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,1,1,1],"player_spawn":[48,48]}'

# Player
curl -X POST http://127.0.0.1:3000/entities -H "Content-Type: application/json" \
  -d '{"x":48,"y":48,"is_player":true,"components":[{"type":"collider","width":12,"height":12},{"type":"top_down_mover","speed":150},{"type":"health","current":5,"max":5}]}'

# Enemy
curl -X POST http://127.0.0.1:3000/entities -H "Content-Type: application/json" \
  -d '{"x":100,"y":100,"tags":["enemy"],"components":[{"type":"collider","width":12,"height":12},{"type":"health","current":3,"max":3},{"type":"contact_damage","amount":1,"cooldown_frames":30,"knockback":80,"damage_tag":"player"},{"type":"ai_behavior","behavior":{"type":"chase","target_tag":"player","speed":60,"detection_radius":200,"give_up_radius":400}}]}'

# Verify
curl http://127.0.0.1:3000/health
```

## 2. Hello World with /build (1 call)

```bash
curl -X POST http://127.0.0.1:3000/build -H "Content-Type: application/json" -d '{
  "config": {"gravity": {"x": 0, "y": 0}, "tile_size": 16},
  "tilemap": {
    "width": 10, "height": 10,
    "tiles": [1,1,1,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,1,1,1],
    "player_spawn": [48, 48]
  },
  "entities": [
    {"x":48,"y":48,"is_player":true,"components":[
      {"type":"collider","width":12,"height":12},
      {"type":"top_down_mover","speed":150},
      {"type":"health","current":5,"max":5}
    ]},
    {"x":100,"y":100,"tags":["enemy"],"components":[
      {"type":"collider","width":12,"height":12},
      {"type":"health","current":3,"max":3},
      {"type":"contact_damage","amount":1,"cooldown_frames":30,"knockback":80,"damage_tag":"player"},
      {"type":"ai_behavior","behavior":{"type":"chase","target_tag":"player","speed":60,"detection_radius":200,"give_up_radius":400}}
    ]}
  ],
  "scripts": {}
}'
```

## 3. Platformer Player

```json
POST /entities
{
  "x": 48, "y": 200,
  "is_player": true,
  "components": [
    {"type": "collider", "width": 14, "height": 16},
    {"type": "gravity_body"},
    {"type": "horizontal_mover", "speed": 200},
    {"type": "jumper", "velocity": 400, "fall_multiplier": 1.5, "coyote_frames": 5, "buffer_frames": 4},
    {"type": "health", "current": 3, "max": 3}
  ]
}
```

Config for platformer: `POST /config {"gravity": {"x": 0, "y": -980}}`

## 4. Top-Down Player

```json
POST /entities
{
  "x": 100, "y": 100,
  "is_player": true,
  "components": [
    {"type": "collider", "width": 14, "height": 14},
    {"type": "top_down_mover", "speed": 200},
    {"type": "health", "current": 10, "max": 10}
  ]
}
```

Config for top-down: `POST /config {"gravity": {"x": 0, "y": 0}}`

## 5. Chase Enemy with AI

```json
POST /entities
{
  "x": 250, "y": 150,
  "tags": ["enemy", "zombie"],
  "script": "zombie_ai",
  "components": [
    {"type": "collider", "width": 12, "height": 14},
    {"type": "top_down_mover", "speed": 80},
    {"type": "health", "current": 5, "max": 5},
    {"type": "contact_damage", "amount": 1, "cooldown_frames": 20, "knockback": 100, "damage_tag": "player"},
    {"type": "ai_behavior", "behavior": {
      "type": "chase", "target_tag": "player", "speed": 80,
      "detection_radius": 250, "give_up_radius": 400
    }}
  ]
}
```

Or use the preset shortcut:
```json
POST /entities/preset
{
  "preset": "chase_enemy",
  "x": 250, "y": 150,
  "config": {"health": 5, "speed": 80, "contact_damage": 1, "tags": ["enemy", "zombie"], "script": "zombie_ai"}
}
```

## 6. Combat System (Hitbox + Contact Damage + Audio)

**Player with attack hitbox:**
```json
POST /entities
{
  "x": 100, "y": 100,
  "is_player": true,
  "script": "player_combat",
  "components": [
    {"type": "collider", "width": 14, "height": 14},
    {"type": "top_down_mover", "speed": 200},
    {"type": "health", "current": 10, "max": 10},
    {"type": "hitbox", "width": 24, "height": 20, "offset": {"x": 10, "y": 0}, "active": false, "damage": 2, "damage_tag": "enemy"}
  ]
}
```

**Player combat script:**
```json
POST /scripts
{
  "name": "player_combat",
  "source": "function update(entity, world, dt)\n  if world.input.just_pressed('attack') then\n    entity.hitbox.active = true\n    entity.state.attack_timer = 12\n    world.play_sfx('swing')\n  end\n  if entity.state.attack_timer then\n    entity.state.attack_timer = entity.state.attack_timer - 1\n    if entity.state.attack_timer <= 0 then\n      entity.hitbox.active = false\n      entity.state.attack_timer = nil\n    end\n  end\nend"
}
```

**Audio triggers for combat:**
```json
POST /audio/sfx
{"effects": {"swing": {"path": "assets/swing.ogg"}, "hit": {"path": "assets/hit.ogg"}, "death": {"path": "assets/death.ogg"}}}

POST /audio/triggers
{"mappings": {"entity_damaged": "hit", "entity_died": "death"}}
```

## 7. Wave Spawner (Global Lua Script)

```json
POST /scripts
{
  "name": "wave_spawner",
  "source": "function update(world, dt)\n  local wave = world.get_var('wave') or 1\n  local alive = world.get_var('alive_enemies') or 0\n  \n  if alive <= 0 then\n    world.set_var('wave', wave + 1)\n    local count = wave * 3\n    for i = 1, count do\n      local angle = (i / count) * math.pi * 2\n      local px = world.player() and world.player().x or 100\n      local py = world.player() and world.player().y or 100\n      local dist = 150 + wave * 20\n      world.spawn({\n        x = px + math.cos(angle) * dist,\n        y = py + math.sin(angle) * dist,\n        tags = {'enemy', 'zombie'},\n        script = 'zombie_ai',\n        health = 2 + wave,\n        components = {\n          {type = 'collider', width = 12, height = 14},\n          {type = 'top_down_mover', speed = 60 + wave * 5},\n          {type = 'contact_damage', amount = 1, cooldown_frames = 20, knockback = 80, damage_tag = 'player'},\n        }\n      })\n    end\n    world.set_var('alive_enemies', count)\n    world.ui.set_text('wave_label', 'Wave ' .. (wave + 1))\n  end\nend",
  "global": true
}
```

## 8. HUD with Health Bar

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
      "id": "health_text",
      "node_type": {"type": "text", "text": "HP: 10/10", "font_size": 16, "color": "white"},
      "position": {"Anchored": {"anchor": "top_left", "offset": [16, 40]}}
    },
    {
      "id": "score_text",
      "node_type": {"type": "text", "text": "Score: 0", "font_size": 24, "color": "yellow"},
      "position": {"Anchored": {"anchor": "top_right", "offset": [-16, 16]}}
    },
    {
      "id": "wave_label",
      "node_type": {"type": "text", "text": "Wave 1", "font_size": 20, "color": "white"},
      "position": {"Anchored": {"anchor": "top_right", "offset": [-16, 48]}}
    }
  ]
}
```

Then show it: `POST /ui/screens/hud/show`

Update from Lua:
```lua
world.ui.set_progress("health_bar", entity.health, entity.max_health)
world.ui.set_text("health_text", "HP: " .. entity.health .. "/" .. entity.max_health)
world.ui.set_text("score_text", "Score: " .. (world.get_var("score") or 0))
```

## 9. Camera Setup

```json
POST /camera/config
{
  "zoom": 3.0,
  "follow_speed": 5.0,
  "deadzone": [0, 0]
}
```

Important config settings for smooth pixel art:
```json
POST /config
{
  "pixel_snap": true,
  "interpolate_transforms": true
}
```

Camera shake on hit (from Lua):
```lua
world.camera.shake(3.0, 0.2)
```

## 10. Sprite Animations

```json
POST /sprites/sheets
{
  "name": "hero",
  "path": "assets/hero.png",
  "frame_width": 32,
  "frame_height": 32,
  "columns": 8,
  "animations": {
    "idle": {"frames": [0,1,2,3], "fps": 8, "looping": true},
    "walk": {"frames": [8,9,10,11], "fps": 12, "looping": true},
    "attack": {"frames": [16,17,18,19], "fps": 15, "looping": false},
    "die": {"frames": [24,25,26,27], "fps": 10, "looping": false}
  }
}
```

Then add `animation_controller` to an entity:
```json
{"type": "animation_controller", "graph": "hero", "state": "idle", "auto_from_velocity": true}
```

With `auto_from_velocity: true`, the engine auto-switches between "idle" and "walk" based on entity velocity.

## 11. Projectile System

**From Lua script (player fires on attack):**
```lua
function update(entity, world, dt)
  if world.input.just_pressed("attack") then
    local dir_x = entity.flip_x and -1 or 1
    world.spawn_projectile({
      x = entity.x + dir_x * 10,
      y = entity.y,
      speed = 400,
      direction = {x = dir_x, y = 0},
      damage = 1,
      damage_tag = "enemy",
      owner = entity.id,
      lifetime = 90,
    })
    world.play_sfx("shoot")
  end
end
```

**From API (spawn a projectile entity):**
```json
POST /entities
{
  "x": 100, "y": 100,
  "tags": ["projectile"],
  "components": [
    {"type": "collider", "width": 8, "height": 4},
    {"type": "projectile", "speed": 400, "direction": {"x": 1, "y": 0}, "lifetime_frames": 90, "damage": 1, "owner_id": 0, "damage_tag": "enemy"}
  ]
}
```

## 12. Self-Testing Loop

```bash
# 1. Build your game
curl -X POST http://127.0.0.1:3000/build -d @game.json

# 2. Check health
curl http://127.0.0.1:3000/health
# → {"ok":true,"data":{"status":"healthy","has_player":true,...}}

# 3. Diagnose missing components
curl http://127.0.0.1:3000/diagnose
# → Shows entities with ContactDamage but no Collider, etc.

# 4. Check for script errors
curl http://127.0.0.1:3000/scripts/errors
# → Shows Lua errors with script name and line number

# 5. Run AI playtest
curl -X POST http://127.0.0.1:3000/test/playtest \
  -H "Content-Type: application/json" \
  -d '{"frames": 600, "mode": "top_down", "goal": "survive"}'
# → Returns difficulty rating, damage taken, exploration stats

# 6. Evaluate overall quality
curl -X POST http://127.0.0.1:3000/evaluate
# → Returns scores, issues, overall rating

# 7. Fix issues and re-build
# Iterate until healthy + no issues
```

## 13. Complete Game Template (Top-Down Survival)

This is a full `/build` body for a complete top-down survival game:

```json
{
  "config": {
    "gravity": {"x": 0, "y": 0},
    "tile_size": 16,
    "pixel_snap": true,
    "interpolate_transforms": true
  },
  "tilemap": {
    "width": 20, "height": 15,
    "tiles": [
      1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,0,0,0,0,1,1,0,0,0,0,0,0,1,1,0,0,0,0,1,
      1,0,0,0,0,1,1,0,0,0,0,0,0,1,1,0,0,0,0,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,0,0,0,0,1,1,0,0,0,0,0,0,1,1,0,0,0,0,1,
      1,0,0,0,0,1,1,0,0,0,0,0,0,1,1,0,0,0,0,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
      1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1
    ],
    "player_spawn": [160, 120]
  },
  "entities": [
    {
      "x": 160, "y": 120,
      "is_player": true,
      "script": "player_combat",
      "components": [
        {"type": "collider", "width": 14, "height": 14},
        {"type": "top_down_mover", "speed": 200},
        {"type": "health", "current": 10, "max": 10},
        {"type": "hitbox", "width": 22, "height": 18, "offset": {"x": 8, "y": 0}, "active": false, "damage": 2, "damage_tag": "enemy"}
      ]
    },
    {
      "x": 60, "y": 60, "tags": ["enemy", "zombie"],
      "script": "enemy_death",
      "components": [
        {"type": "collider", "width": 12, "height": 14},
        {"type": "top_down_mover", "speed": 60},
        {"type": "health", "current": 3, "max": 3},
        {"type": "contact_damage", "amount": 1, "cooldown_frames": 30, "knockback": 100, "damage_tag": "player"},
        {"type": "ai_behavior", "behavior": {"type": "chase", "target_tag": "player", "speed": 60, "detection_radius": 200, "give_up_radius": 400}}
      ]
    },
    {
      "x": 260, "y": 60, "tags": ["enemy", "zombie"],
      "script": "enemy_death",
      "components": [
        {"type": "collider", "width": 12, "height": 14},
        {"type": "top_down_mover", "speed": 70},
        {"type": "health", "current": 3, "max": 3},
        {"type": "contact_damage", "amount": 1, "cooldown_frames": 30, "knockback": 100, "damage_tag": "player"},
        {"type": "ai_behavior", "behavior": {"type": "chase", "target_tag": "player", "speed": 70, "detection_radius": 200, "give_up_radius": 400}}
      ]
    },
    {
      "x": 160, "y": 200, "tags": ["pickup", "health"],
      "components": [
        {"type": "collider", "width": 10, "height": 10},
        {"type": "pickup", "pickup_tag": "player", "effect": {"type": "heal", "amount": 2}}
      ]
    }
  ],
  "scripts": {
    "player_combat": "function update(entity, world, dt)\n  -- Attack on input\n  if world.input.just_pressed('attack') then\n    entity.hitbox.active = true\n    entity.state.attack_timer = 10\n    world.camera.shake(2.0, 0.1)\n  end\n  if entity.state.attack_timer then\n    entity.state.attack_timer = entity.state.attack_timer - 1\n    if entity.state.attack_timer <= 0 then\n      entity.hitbox.active = false\n      entity.state.attack_timer = nil\n    end\n  end\n  -- Update HUD\n  world.ui.set_progress('health_bar', entity.health, entity.max_health)\n  world.ui.set_text('score_text', 'Score: ' .. (world.get_var('score') or 0))\nend",
    "enemy_death": "function on_death(entity, world)\n  local score = (world.get_var('score') or 0) + 10\n  world.set_var('score', score)\n  local alive = (world.get_var('alive_enemies') or 1) - 1\n  world.set_var('alive_enemies', alive)\nend\n\nfunction update(entity, world, dt)\nend"
  },
  "game_vars": {"score": 0, "wave": 1, "alive_enemies": 2}
}
```

After building, set up the camera and HUD:

```bash
# Camera
curl -X POST http://127.0.0.1:3000/camera/config \
  -H "Content-Type: application/json" \
  -d '{"zoom": 3.0, "follow_speed": 5.0, "deadzone": [0, 0]}'

# HUD
curl -X POST http://127.0.0.1:3000/ui/screens \
  -H "Content-Type: application/json" \
  -d '{"name":"hud","layer":0,"nodes":[{"id":"health_bar","node_type":{"type":"progress_bar","value":10,"max":10,"color":"red","bg_color":"dark_red"},"position":{"Anchored":{"anchor":"top_left","offset":[16,16]}},"size":{"fixed":[200,20]}},{"id":"score_text","node_type":{"type":"text","text":"Score: 0","font_size":24,"color":"yellow"},"position":{"Anchored":{"anchor":"top_right","offset":[-16,16]}}}]}'

curl -X POST http://127.0.0.1:3000/ui/screens/hud/show
```
