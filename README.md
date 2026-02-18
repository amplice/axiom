# Axiom Engine

An AI-native 2D game engine. Build games by sending HTTP requests — no IDE, no editor, no manual asset pipeline. Designed for AI agents to create complete games through a REST API.

## How It Works

```
AI Agent → HTTP API (port 3000) → Bevy ECS → Physics / Scripting / Rendering
```

1. **Agent sends HTTP requests** to configure physics, load tilemaps, spawn entities, and upload Lua scripts
2. **Bevy ECS processes everything** — collision detection, AI behaviors, script execution, sprite animation
3. **Engine renders the game** with pixel-art-friendly defaults, auto-colored sprites, and UI overlays

## Quick Start

```bash
# 1. Build and run the engine
cargo build
cargo run

# 2. Send a /build request to create a complete game in one call
curl -X POST http://127.0.0.1:3000/build \
  -H "Content-Type: application/json" \
  -d @my_game.json

# 3. Check it's working
curl http://127.0.0.1:3000/health

# 4. Check the API docs
curl http://127.0.0.1:3000/docs
```

## Features

- **Full REST API** — 150+ endpoints covering entities, physics, scripting, audio, UI, cameras, and more
- **Lua Scripting** — Per-entity and global scripts with access to physics, pathfinding, raycasting, and events
- **Two Game Modes** — Platformer (gravity, jumping, slopes) and top-down (4-directional, no gravity)
- **Procedural Generation** — 11 level templates with constraint validation
- **Combat System** — Contact damage, hitboxes, projectiles, knockback, invincibility frames
- **AI Behaviors** — Patrol, chase, flee, guard, wander, and custom script-driven AI
- **Sprite Animation** — Sheet-based animations with 8-directional facing and velocity-driven state transitions
- **UI System** — HUD screens with text, progress bars, panels, and anchored positioning
- **Audio** — SFX, music, event-triggered sounds, volume control
- **Self-Diagnosis** — `/health`, `/diagnose`, `/evaluate` endpoints for debugging
- **Testing** — AI playtester, scenario testing, headless simulation, deterministic replay
- **Atomic Build** — `POST /build` applies config + tilemap + entities + scripts in one call

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `AXIOM_API_TOKEN` | API auth token (if set, requires `Authorization: Bearer <token>`) | *(none)* |
| `AXIOM_API_RATE_LIMIT_PER_SEC` | Request rate limit | *(unlimited)* |
| `AXIOM_ASSETS_DIR` | Asset directory path | `assets/` |
| `AXIOM_SCREENSHOT_PATH` | Screenshot output directory | CWD |
| `AXIOM_SCRIPT_ENTITY_BUDGET_MS` | Per-entity script timeout | 1ms |
| `AXIOM_SCRIPT_GLOBAL_BUDGET_MS` | Global script timeout | 5ms |

## Documentation

- **[CLAUDE.md](CLAUDE.md)** — Comprehensive agent guide (start here if you're an AI)
- **[docs/COOKBOOK.md](docs/COOKBOOK.md)** — Copy-paste recipes for common tasks
- **[docs/API_REFERENCE.md](docs/API_REFERENCE.md)** — Complete endpoint reference
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — Internal architecture and module map
- **`GET /docs`** — Machine-readable API docs served by the engine itself

## Tech Stack

- **Rust** + **Bevy ECS** for the core engine
- **Axum** for the HTTP API server
- **mlua** (LuaJIT) for Lua scripting
- **Crossbeam** channels for API-to-ECS communication

## License

Proprietary. All rights reserved.
