use super::helpers::builtin_examples;
use super::security::DEFAULT_API_RATE_LIMIT_PER_SEC;

pub fn docs_endpoints() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"method":"GET","path":"/state","description":"Get world + player state"}),
        serde_json::json!({"method":"GET","path":"/player","description":"Get player state"}),
        serde_json::json!({"method":"POST","path":"/level","description":"Set current level tilemap"}),
        serde_json::json!({"method":"POST","path":"/player/position","description":"Teleport player"}),
        serde_json::json!({"method":"GET","path":"/physics","description":"Get legacy physics config"}),
        serde_json::json!({"method":"POST","path":"/physics","description":"Set legacy physics config"}),
        serde_json::json!({"method":"POST","path":"/physics/raycast","description":"Tilemap raycast query"}),
        serde_json::json!({"method":"POST","path":"/physics/raycast_entities","description":"Entity AABB raycast query"}),
        serde_json::json!({"method":"POST","path":"/ai/pathfind","description":"Find path waypoints between world points"}),
        serde_json::json!({"method":"POST","path":"/ai/line_of_sight","description":"Check line-of-sight between two world points"}),
        serde_json::json!({"method":"GET","path":"/config","description":"Get full config"}),
        serde_json::json!({"method":"POST","path":"/config","description":"Set full config (includes pixel_snap, interpolate_transforms). Set AXIOM_TEXTURE_FILTER=nearest env var for pixel-art filtering."}),
        serde_json::json!({"method":"POST","path":"/config/tile_types","description":"Set tile type registry"}),
        serde_json::json!({"method":"POST","path":"/simulate","description":"Run headless simulation (supports optional goal_position/goal_radius)"}),
        serde_json::json!({"method":"POST","path":"/save","description":"Save current game state to slot"}),
        serde_json::json!({"method":"POST","path":"/load","description":"Load game state from slot"}),
        serde_json::json!({"method":"GET","path":"/saves","description":"List available save slots"}),
        serde_json::json!({"method":"POST","path":"/levels/pack","description":"Define or replace a level pack/campaign"}),
        serde_json::json!({"method":"POST","path":"/levels/pack/{name}/start","description":"Start a level pack at level 0"}),
        serde_json::json!({"method":"POST","path":"/levels/pack/{name}/next","description":"Advance to the next level in a started pack"}),
        serde_json::json!({"method":"GET","path":"/levels/pack/{name}/progress","description":"Get current progress for a level pack"}),
        serde_json::json!({"method":"POST","path":"/levels/export","description":"Export current level + entities as JSON"}),
        serde_json::json!({"method":"POST","path":"/levels/import","description":"Import a previously exported level JSON"}),
        serde_json::json!({"method":"POST","path":"/game/export","description":"Export full game/project snapshot as JSON"}),
        serde_json::json!({"method":"POST","path":"/game/import","description":"Import full game/project snapshot from JSON"}),
        serde_json::json!({"method":"POST","path":"/export/web","description":"Build a web export bundle (wasm + data + html)"}),
        serde_json::json!({"method":"POST","path":"/export/desktop","description":"Build a desktop export bundle for a target platform"}),
        serde_json::json!({"method":"GET","path":"/examples","description":"List built-in example game recipes"}),
        serde_json::json!({"method":"POST","path":"/examples/{name}/load","description":"Generate and load a built-in example recipe"}),
        serde_json::json!({"method":"GET","path":"/game/state","description":"Get current runtime game state"}),
        serde_json::json!({"method":"POST","path":"/game/state","description":"Set runtime game state"}),
        serde_json::json!({"method":"POST","path":"/game/transition","description":"Transition to another runtime game state"}),
        serde_json::json!({"method":"GET","path":"/game/transitions","description":"List recent runtime game state transitions"}),
        serde_json::json!({"method":"POST","path":"/game/restart","description":"Restart last level loaded via game flow APIs"}),
        serde_json::json!({"method":"POST","path":"/game/load_level","description":"Generate+load a level from template and difficulty"}),
        serde_json::json!({"method":"POST","path":"/replay/record","description":"Start replay recording for /simulate calls"}),
        serde_json::json!({"method":"POST","path":"/replay/stop","description":"Stop replay recording and save to file"}),
        serde_json::json!({"method":"POST","path":"/replay/play","description":"Play a saved replay and compare deterministic outcomes"}),
        serde_json::json!({"method":"GET","path":"/replay/list","description":"List saved replays and recording status"}),
        serde_json::json!({"method":"GET","path":"/debug/overlay","description":"Get debug overlay visibility and feature flags"}),
        serde_json::json!({"method":"POST","path":"/debug/overlay","description":"Set debug overlay visibility and feature flags"}),
        serde_json::json!({"method":"POST","path":"/audio/sfx","description":"Define named sound effects"}),
        serde_json::json!({"method":"POST","path":"/audio/music","description":"Define named music tracks"}),
        serde_json::json!({"method":"POST","path":"/audio/play","description":"Play a sound effect or music track"}),
        serde_json::json!({"method":"POST","path":"/audio/stop","description":"Stop currently playing music"}),
        serde_json::json!({"method":"POST","path":"/audio/config","description":"Set master/sfx/music volume"}),
        serde_json::json!({"method":"POST","path":"/audio/triggers","description":"Map gameplay events to auto-play SFX"}),
        serde_json::json!({"method":"POST","path":"/particles/presets","description":"Define named particle presets"}),
        serde_json::json!({"method":"GET","path":"/audio/state","description":"Get audio definitions and recent audio events"}),
        serde_json::json!({"method":"POST","path":"/camera/config","description":"Set camera follow/zoom/deadzone/bounds config"}),
        serde_json::json!({"method":"POST","path":"/camera/shake","description":"Trigger camera shake"}),
        serde_json::json!({"method":"POST","path":"/camera/look_at","description":"Set explicit camera look-at target"}),
        serde_json::json!({"method":"GET","path":"/camera/state","description":"Get current camera runtime state"}),
        serde_json::json!({"method":"POST","path":"/ui/screens","description":"Define or replace a UI screen"}),
        serde_json::json!({"method":"POST","path":"/ui/screens/{name}/show","description":"Show a named UI screen"}),
        serde_json::json!({"method":"POST","path":"/ui/screens/{name}/hide","description":"Hide a named UI screen"}),
        serde_json::json!({"method":"POST","path":"/ui/screens/{name}/nodes/{id}","description":"Update a UI node within a screen"}),
        serde_json::json!({"method":"GET","path":"/ui/state","description":"Get full UI manager state"}),
        serde_json::json!({"method":"POST","path":"/dialogue/conversations","description":"Define or replace a dialogue conversation tree"}),
        serde_json::json!({"method":"POST","path":"/dialogue/start","description":"Start a dialogue conversation"}),
        serde_json::json!({"method":"GET","path":"/dialogue/state","description":"Get current dialogue node state"}),
        serde_json::json!({"method":"POST","path":"/dialogue/choose","description":"Choose an option in the active dialogue"}),
        serde_json::json!({"method":"GET","path":"/scene/describe","description":"Describe current scene snapshot (ui/audio/camera/vars/perf)"}),
        serde_json::json!({"method":"POST","path":"/validate","description":"Validate constraints"}),
        serde_json::json!({"method":"POST","path":"/solve","description":"Auto-solve current level (platformer + top-down modes)"}),
        serde_json::json!({"method":"POST","path":"/generate","description":"Generate level from template"}),
        serde_json::json!({"method":"GET","path":"/feel/jump","description":"Measure jump profile"}),
        serde_json::json!({"method":"GET","path":"/feel/compare","description":"Compare feel to target"}),
        serde_json::json!({"method":"POST","path":"/feel/tune","description":"Auto-tune feel"}),
        serde_json::json!({"method":"GET","path":"/sprites","description":"Get sprite manifest"}),
        serde_json::json!({"method":"POST","path":"/sprites","description":"Set sprite manifest"}),
        serde_json::json!({"method":"GET","path":"/sprites/sheets","description":"List named sprite sheet definitions"}),
        serde_json::json!({"method":"POST","path":"/sprites/sheets","description":"Define/update a sprite sheet and linked animation graph"}),
        serde_json::json!({"method":"GET","path":"/screenshot","description":"Take screenshot in windowed mode (saved to AXIOM_SCREENSHOT_PATH or screenshot.png)"}),
        serde_json::json!({"method":"GET","path":"/entities","description":"List entities"}),
        serde_json::json!({"method":"POST","path":"/entities","description":"Spawn entity"}),
        serde_json::json!({"method":"POST","path":"/entities/preset","description":"Spawn preset entity"}),
        serde_json::json!({"method":"POST","path":"/entities/reset_non_player","description":"Delete all non-player entities"}),
        serde_json::json!({"method":"GET","path":"/entities/{id}","description":"Get entity by id"}),
        serde_json::json!({"method":"DELETE","path":"/entities/{id}","description":"Delete entity"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}","description":"Apply damage to entity"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/damage","description":"Apply damage to entity"}),
        serde_json::json!({"method":"GET","path":"/entities/{id}/animation","description":"Get entity animation runtime state"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/animation","description":"Force entity animation state"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/particles","description":"Attach or update particle emitter on entity"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/tween","description":"Start a property tween on entity"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/tween_sequence","description":"Start a sequential tween chain on entity"}),
        serde_json::json!({"method":"GET","path":"/events","description":"Get recent game events"}),
        serde_json::json!({"method":"GET","path":"/events/subscribe","description":"SSE stream of events"}),
        serde_json::json!({"method":"GET","path":"/perf","description":"Runtime performance metrics"}),
        serde_json::json!({"method":"GET","path":"/perf/history","description":"Recent perf samples ring buffer"}),
        serde_json::json!({"method":"GET","path":"/scripts","description":"List scripts"}),
        serde_json::json!({"method":"POST","path":"/scripts","description":"Upload/replace script"}),
        serde_json::json!({"method":"GET","path":"/scripts/{name}","description":"Get script source"}),
        serde_json::json!({"method":"DELETE","path":"/scripts/{name}","description":"Delete script"}),
        serde_json::json!({"method":"POST","path":"/scripts/{name}/test","description":"Dry-run script"}),
        serde_json::json!({"method":"GET","path":"/scripts/errors","description":"Recent script errors"}),
        serde_json::json!({"method":"GET","path":"/scripts/logs","description":"Get recent script log output"}),
        serde_json::json!({"method":"GET","path":"/scripts/vars","description":"Get script global vars"}),
        serde_json::json!({"method":"POST","path":"/scripts/vars","description":"Set script global vars"}),
        serde_json::json!({"method":"GET","path":"/scripts/events","description":"Recent script-emitted events"}),
        serde_json::json!({"method":"GET","path":"/scripts/stats","description":"Script runtime health counters (loaded scripts, disabled scripts, dropped events, buffer sizes)"}),
        serde_json::json!({"method":"GET","path":"/animations","description":"List animation graphs"}),
        serde_json::json!({"method":"GET","path":"/animations/{name}","description":"Get animation graph by name"}),
        serde_json::json!({"method":"POST","path":"/animations/{name}","description":"Create/replace animation graph"}),
        serde_json::json!({"method":"DELETE","path":"/animations/{name}","description":"Delete animation graph"}),
        serde_json::json!({"method":"GET","path":"/animations/state","description":"List entity animation runtime states"}),
        serde_json::json!({"method":"GET","path":"/input/gamepad","description":"Get gamepad configuration and connected count"}),
        serde_json::json!({"method":"POST","path":"/input/gamepad","description":"Set gamepad configuration"}),
        serde_json::json!({"method":"POST","path":"/screen/effect","description":"Trigger a screen effect (fade, flash, tint)"}),
        serde_json::json!({"method":"GET","path":"/screen/state","description":"Get current screen effect state"}),
        serde_json::json!({"method":"POST","path":"/lighting/config","description":"Set lighting configuration (ambient, enabled)"}),
        serde_json::json!({"method":"GET","path":"/lighting/state","description":"Get current lighting state"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/tint","description":"Set sprite color tint on entity"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/trail","description":"Set or remove trail/afterimage effect on entity"}),
        serde_json::json!({"method":"GET","path":"/input/bindings","description":"Get current input key bindings"}),
        serde_json::json!({"method":"POST","path":"/input/bindings","description":"Set input key bindings"}),
        serde_json::json!({"method":"GET","path":"/lighting/day_night","description":"Get day/night cycle state"}),
        serde_json::json!({"method":"POST","path":"/lighting/day_night","description":"Configure day/night cycle"}),
        serde_json::json!({"method":"POST","path":"/world_text","description":"Spawn floating world-space text"}),
        serde_json::json!({"method":"GET","path":"/entities/{id}/state","description":"Get entity state machine"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/state","description":"Transition entity state machine"}),
        serde_json::json!({"method":"POST","path":"/tilemap/auto_tile","description":"Set auto-tiling rules"}),
        serde_json::json!({"method":"GET","path":"/tilemap/layers","description":"List decorative tile layers"}),
        serde_json::json!({"method":"POST","path":"/tilemap/layers","description":"Add or update a decorative tile layer"}),
        serde_json::json!({"method":"DELETE","path":"/tilemap/layers/{name}","description":"Delete a decorative tile layer"}),
        serde_json::json!({"method":"GET","path":"/parallax/layers","description":"Get parallax background layers"}),
        serde_json::json!({"method":"POST","path":"/parallax/layers","description":"Set parallax background layers"}),
        serde_json::json!({"method":"GET","path":"/weather","description":"Get current weather state"}),
        serde_json::json!({"method":"POST","path":"/weather","description":"Set weather effect (rain, snow, dust)"}),
        serde_json::json!({"method":"DELETE","path":"/weather","description":"Clear active weather effect"}),
        serde_json::json!({"method":"POST","path":"/items/define","description":"Define item types for inventory system"}),
        serde_json::json!({"method":"GET","path":"/entities/{id}/inventory","description":"Get entity inventory"}),
        serde_json::json!({"method":"POST","path":"/entities/{id}/inventory","description":"Add/remove/clear items in entity inventory"}),
        serde_json::json!({"method":"POST","path":"/cutscene/define","description":"Define a cutscene sequence"}),
        serde_json::json!({"method":"POST","path":"/cutscene/play","description":"Play a defined cutscene"}),
        serde_json::json!({"method":"POST","path":"/cutscene/stop","description":"Stop the active cutscene"}),
        serde_json::json!({"method":"GET","path":"/cutscene/state","description":"Get cutscene playback state"}),
        serde_json::json!({"method":"GET","path":"/presets","description":"List custom spawn presets"}),
        serde_json::json!({"method":"POST","path":"/presets","description":"Define custom spawn presets"}),
        serde_json::json!({"method":"POST","path":"/pool/init","description":"Initialize entity pool with preset"}),
        serde_json::json!({"method":"POST","path":"/pool/acquire","description":"Acquire entity from pool"}),
        serde_json::json!({"method":"POST","path":"/pool/release/{id}","description":"Release entity back to pool"}),
        serde_json::json!({"method":"GET","path":"/pool/status","description":"Get entity pool statistics"}),
        // Telemetry
        serde_json::json!({"method":"GET","path":"/telemetry","description":"Get gameplay telemetry (deaths, inputs, entity counts, damage)"}),
        serde_json::json!({"method":"DELETE","path":"/telemetry","description":"Reset gameplay telemetry counters"}),
        // World simulation & scenario testing
        serde_json::json!({"method":"POST","path":"/simulate/world","description":"Run world simulation. Set real:true for full game loop (scripts, AI, physics, interactions) with virtual inputs, or false for deterministic physics-only sim."}),
        serde_json::json!({"method":"POST","path":"/test/scenario","description":"Run scenario test with setup, simulation, and assertions"}),
        serde_json::json!({"method":"POST","path":"/test/playtest","description":"Run AI playtester agent that plays the game with heuristic inputs. Returns difficulty rating, events, damage taken, exploration stats. Body: {frames?, mode? (platformer|top_down), goal? (survive|reach_goal|explore)}"}),
        // Screenshot extras
        serde_json::json!({"method":"POST","path":"/screenshot/baseline","description":"Save current screenshot as visual regression baseline"}),
        serde_json::json!({"method":"POST","path":"/screenshot/diff","description":"Compare current screenshot against baseline"}),
        // Atomic build
        serde_json::json!({"method":"POST","path":"/build","description":"Atomic build: apply config, tilemap, entities, scripts in one call"}),
        // Manifest validation
        serde_json::json!({"method":"POST","path":"/validate/manifest","description":"Validate build manifest without applying"}),
        // Asset pipeline
        serde_json::json!({"method":"POST","path":"/assets/upload","description":"Upload base64-encoded image asset"}),
        serde_json::json!({"method":"POST","path":"/assets/generate","description":"Generate colored rectangle sprite asset"}),
        serde_json::json!({"method":"GET","path":"/assets/list","description":"List image assets in assets directory"}),
        serde_json::json!({"method":"GET","path":"/debug/input","description":"Get current virtual input state"}),
        serde_json::json!({"method":"GET","path":"/docs","description":"Full API docs"}),
        serde_json::json!({"method":"GET","path":"/docs/html","description":"HTML API documentation page"}),
        serde_json::json!({"method":"GET","path":"/docs/endpoints","description":"Endpoint list"}),
        serde_json::json!({"method":"GET","path":"/docs/components","description":"Component schema list"}),
        serde_json::json!({"method":"GET","path":"/docs/presets","description":"Entity preset list"}),
        serde_json::json!({"method":"GET","path":"/docs/templates","description":"Generation template list"}),
        serde_json::json!({"method":"GET","path":"/docs/constraints","description":"Validation constraint list"}),
        serde_json::json!({"method":"GET","path":"/docs/scripts","description":"Scripting API surface (Lua + Rhai helpers)"}),
        serde_json::json!({"method":"GET","path":"/docs/examples","description":"Built-in example recipes and defaults"}),
        serde_json::json!({"method":"GET","path":"/docs/security","description":"API auth and rate-limit configuration"}),
        // Window config
        serde_json::json!({"method":"POST","path":"/window","description":"Set window title and/or background color. Body: {title?: string, background?: [r,g,b]}"}),
        // Evaluation
        serde_json::json!({"method":"POST","path":"/evaluate","description":"Holistic game evaluation: entity census, script health, tilemap quality, game vars. Returns scores + issues + overall rating"}),
        serde_json::json!({"method":"POST","path":"/evaluate/screenshot","description":"Take screenshot + analyze + describe scene in one call. Returns screenshot_path, analysis (quadrant colors, overlaps), and scene (entities, vars, game_state)"}),
    ]
}

pub fn docs_components() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"name":"collider","fields":{"width":"f32","height":"f32"}}),
        serde_json::json!({"name":"gravity_body","fields":{}}),
        serde_json::json!({"name":"horizontal_mover","fields":{"speed":"f32","left_action":"string","right_action":"string"}}),
        serde_json::json!({"name":"jumper","fields":{"velocity":"f32","action":"string","fall_multiplier":"f32","variable_height":"bool","coyote_frames":"u32","buffer_frames":"u32"}}),
        serde_json::json!({"name":"top_down_mover","fields":{"speed":"f32","up_action":"string","down_action":"string","left_action":"string","right_action":"string"}}),
        serde_json::json!({"name":"health","fields":{"current":"f32","max":"f32"}}),
        serde_json::json!({"name":"contact_damage","fields":{"amount":"f32","cooldown_frames":"u32","knockback":"f32","damage_tag":"string"}}),
        serde_json::json!({"name":"trigger_zone","fields":{"radius":"f32","trigger_tag":"string","event_name":"string","one_shot":"bool"}}),
        serde_json::json!({"name":"pickup","fields":{"pickup_tag":"string","effect":"{type: heal|score_add|custom, ...}"}}),
        serde_json::json!({"name":"projectile","fields":{"speed":"f32","direction":"{x,y}","lifetime_frames":"u32","damage":"f32","owner_id":"u64","damage_tag":"string"}}),
        serde_json::json!({"name":"hitbox","fields":{"width":"f32","height":"f32","offset":"{x,y}","active":"bool","damage":"f32","damage_tag":"string"}}),
        serde_json::json!({"name":"moving_platform","fields":{"waypoints":"[{x,y},...]","speed":"f32","loop_mode":"loop|ping_pong","pause_frames":"u32","carry_riders":"bool","current_waypoint":"usize","direction":"i8"}}),
        serde_json::json!({"name":"animation_controller","fields":{"graph":"string","state":"string","frame":"usize","timer":"f32","speed":"f32","playing":"bool","facing_right":"bool","auto_from_velocity":"bool"}}),
        serde_json::json!({"name":"particle_emitter","fields":{"preset":"string?","emit_rate":"f32","lifetime":"f32","spread_angle":"f32","speed_min":"f32","speed_max":"f32","one_shot":"bool","burst_count":"u32"}}),
        serde_json::json!({"name":"path_follower","fields":{"target":"{x,y}","recalculate_interval":"u32","path_type":"top_down|platformer","speed":"f32"}}),
        serde_json::json!({"name":"ai_behavior","fields":{"behavior":"{type: patrol|chase|flee|guard|wander|custom, ...}"}}),
        serde_json::json!({"name":"collision_layer","fields":{"layer":"u16 (bitmask)","mask":"u16 (bitmask)"}}),
        serde_json::json!({"name":"sprite_color_tint","fields":{"color":"[f32;4] RGBA","flash_color":"[f32;4]?","flash_frames":"u32"}}),
        serde_json::json!({"name":"trail_effect","fields":{"interval":"u32 (frames between ghosts)","duration":"f32 (ghost lifetime)","alpha_start":"f32","alpha_end":"f32"}}),
        serde_json::json!({"name":"state_machine","fields":{"initial":"string","states":"{ name: { allowed_transitions, on_enter_event?, on_exit_event? } }"}}),
        serde_json::json!({"name":"inventory","fields":{"max_slots":"usize"}}),
    ]
}

pub fn docs_presets() -> Vec<&'static str> {
    vec![
        "platformer_player",
        "top_down_player",
        "patrol_enemy",
        "chase_enemy",
        "guard_enemy",
        "turret",
        "flying_enemy",
        "boss",
        "health_pickup",
        "projectile",
        "moving_platform",
    ]
}

pub fn docs_templates() -> Vec<&'static str> {
    vec![
        "platformer",
        "top_down_dungeon",
        "rts_arena",
        "fighting_arena",
        "metroidvania",
        "roguelike_floor",
        "puzzle_platformer",
        "arena_waves",
        "side_scroller",
        "tower_defense_map",
        "boss_arena",
    ]
}

pub fn docs_constraints() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"name":"reachable","description":"Platformer reachability from spawn to goal"}),
        serde_json::json!({"name":"completable","description":"Alias of reachable"}),
        serde_json::json!({"name":"top_down_reachable","description":"4-dir top-down reachability"}),
        serde_json::json!({"name":"bounds_check","description":"Tile buffer length matches width*height"}),
        serde_json::json!({"name":"has_ground","description":"At least one solid tile on y=0"}),
        serde_json::json!({"name":"no_softlock","description":"Basic softlock proxy via reachability"}),
        serde_json::json!({"name":"difficulty_range:min:max","description":"Estimated difficulty score must be in range [min,max]"}),
        serde_json::json!({"name":"enemy_fairness","description":"Enemy pressure along spawn->goal path stays under threshold"}),
        serde_json::json!({"name":"item_reachability","description":"All pickups are reachable from spawn"}),
        serde_json::json!({"name":"pacing","description":"Enemy placement includes meaningful rest gaps"}),
        serde_json::json!({"name":"no_dead_ends[:max_ratio]","description":"Top-down reachable region dead-end ratio under max_ratio (default 0.35)"}),
        serde_json::json!({"name":"ability_gating","description":"Any requires_ability has a matching grants_ability in entity metadata"}),
        serde_json::json!({"name":"entity_overlap[:threshold_px]","description":"Flags entities within threshold (default 8px) of each other"}),
        serde_json::json!({"name":"spawn_in_solid","description":"Flags entities whose position is inside a solid tile"}),
        serde_json::json!({"name":"script_errors","description":"Fails if script_errors array in request is non-empty"}),
        serde_json::json!({"name":"performance[:fps_min=N]","description":"Checks perf_fps in request meets minimum (default 30)"}),
        serde_json::json!({"name":"asset_missing","description":"Checks entity sprite_sheet refs exist in available_assets list"}),
    ]
}

pub fn docs_scripts() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "group": "entity",
            "description": "Per-entity helpers available in script update(entity, world, dt).",
            "fields": ["id", "x", "y", "vx", "vy", "grounded", "alive", "health", "max_health", "state", "tags"],
            "methods": ["damage(amount)", "heal(amount)", "knockback(dx, dy)", "has_tag(tag)", "add_tag(tag)", "remove_tag(tag)"]
        }),
        serde_json::json!({
            "group": "world_core",
            "description": "Core world helpers shared by native Lua and wasm Rhai backends.",
            "fields": ["frame", "time", "dt", "game_state", "vars"],
            "methods": ["get_var(name)", "set_var(name, value)", "emit(name, payload)", "on(name)", "pressed(action)", "just_pressed(action)", "get_tile(x, y)", "is_solid(x, y)", "set_tile(x, y, tile_id)"]
        }),
        serde_json::json!({
            "group": "world_entities",
            "description": "Entity query helpers exposed to scripts.",
            "methods": [
                "player()",
                "get_entity(id)",
                "find_all([tag])",
                "find_in_radius(x, y, radius[, tag])",
                "find_nearest(x, y[, tag])",
                "find_path(start_x, start_y, goal_x, goal_y[, path_type])",
                "line_of_sight(x1, y1, x2, y2)",
                "spawn(spec)",
                "despawn(id)",
                "spawn_projectile(spec)",
                "spawn_particles(preset, x, y)"
            ]
        }),
        serde_json::json!({
            "group": "world_gameplay",
            "description": "High-level event helpers that map to engine systems.",
            "methods": [
                "pause()",
                "resume()",
                "transition(to[, effect[, duration]])",
                "play_sfx(name)",
                "play_music(name)",
                "stop_music()",
                "set_volume(channel, value)",
                "camera_shake(intensity, duration)",
                "camera_zoom(zoom)",
                "camera_look_at(x, y)",
                "show_screen(name)",
                "hide_screen(name)"
            ]
        }),
    ]
}

pub fn docs_examples() -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = builtin_examples()
        .into_iter()
        .map(|ex| {
            serde_json::json!({
                "name": ex.name,
                "description": ex.description,
                "genre": ex.genre,
                "template": ex.template,
                "default_difficulty": ex.difficulty,
                "default_seed": ex.seed,
                "constraints": ex.constraints,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.cmp(bn)
    });
    out
}

pub fn docs_security() -> serde_json::Value {
    serde_json::json!({
        "authentication": {
            "summary": "Optional token auth for all API routes.",
            "env_var": "AXIOM_API_TOKEN",
            "accepted_headers": [
                "Authorization: Bearer <token>",
                "X-API-Key: <token>"
            ]
        },
        "rate_limit": {
            "summary": "Per-client request limit (per second).",
            "env_var": "AXIOM_API_RATE_LIMIT_PER_SEC",
            "default": DEFAULT_API_RATE_LIMIT_PER_SEC
        },
        "script_runtime": {
            "summary": "Per-script execution watchdog limits (native Lua and wasm Rhai).",
            "env_vars": [
                "AXIOM_SCRIPT_ENTITY_BUDGET_MS",
                "AXIOM_SCRIPT_GLOBAL_BUDGET_MS",
                "AXIOM_SCRIPT_HOOK_INSTRUCTION_INTERVAL",
                "AXIOM_RHAI_MAX_OPERATIONS",
                "AXIOM_RHAI_MAX_CALL_LEVELS"
            ]
        },
        "notes": [
            "When AXIOM_API_TOKEN is unset, token auth is disabled.",
            "Rate limiting is always active and keyed by client address."
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn docs_include_export_endpoints() {
        let endpoints = docs_endpoints();
        let paths: Vec<String> = endpoints
            .into_iter()
            .filter_map(|v| {
                v.get("path")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        assert!(paths.iter().any(|p| p == "/export/web"));
        assert!(paths.iter().any(|p| p == "/export/desktop"));
        assert!(paths.iter().any(|p| p == "/physics/raycast_entities"));
        assert!(paths.iter().any(|p| p == "/audio/state"));
        assert!(paths.iter().any(|p| p == "/camera/state"));
        assert!(paths.iter().any(|p| p == "/ui/state"));
        assert!(paths.iter().any(|p| p == "/dialogue/state"));
        assert!(paths.iter().any(|p| p == "/scene/describe"));
        assert!(paths.iter().any(|p| p == "/animations"));
        assert!(paths.iter().any(|p| p == "/animations/state"));
        assert!(paths.iter().any(|p| p == "/sprites/sheets"));
        assert!(paths.iter().any(|p| p == "/entities/{id}/animation"));
        assert!(paths.iter().any(|p| p == "/entities/reset_non_player"));
        assert!(paths.iter().any(|p| p == "/particles/presets"));
        assert!(paths.iter().any(|p| p == "/entities/{id}/particles"));
        assert!(paths.iter().any(|p| p == "/docs/scripts"));
        assert!(paths.iter().any(|p| p == "/docs/examples"));
        assert!(paths.iter().any(|p| p == "/docs/security"));
    }

    #[test]
    fn docs_cover_all_registered_routes() {
        let router_src = include_str!("router.rs");
        let mut registered_paths = BTreeSet::new();
        for line in router_src.lines() {
            if let Some(start) = line.find(".route(\"") {
                let remainder = &line[start + ".route(\"".len()..];
                if let Some(end) = remainder.find('"') {
                    registered_paths.insert(remainder[..end].to_string());
                }
            }
        }
        assert!(
            !registered_paths.is_empty(),
            "Failed to discover any registered routes from router.rs"
        );

        let documented_paths: BTreeSet<String> = docs_endpoints()
            .into_iter()
            .filter_map(|v| {
                v.get("path")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        let missing: Vec<String> = registered_paths
            .difference(&documented_paths)
            .cloned()
            .collect();
        assert!(
            missing.is_empty(),
            "Missing docs entries for routes: {missing:?}"
        );
    }

    #[test]
    fn docs_scripts_contains_expected_groups() {
        let groups: BTreeSet<String> = docs_scripts()
            .into_iter()
            .filter_map(|v| {
                v.get("group")
                    .and_then(|g| g.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        assert!(groups.contains("entity"));
        assert!(groups.contains("world_core"));
        assert!(groups.contains("world_gameplay"));
    }

    #[test]
    fn docs_examples_covers_builtin_example_set() {
        let items = docs_examples();
        assert!(items.len() >= 5);
        let names: BTreeSet<String> = items
            .iter()
            .filter_map(|v| {
                v.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        for required in [
            "platformer_campaign",
            "top_down_rpg_dungeon",
            "roguelike_floor_run",
            "bullet_hell_arena",
            "puzzle_platformer_trials",
        ] {
            assert!(names.contains(required), "missing docs example: {required}");
        }
    }
}
