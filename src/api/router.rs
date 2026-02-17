use super::*;

pub(super) fn build_router(state: AppState, security: ApiSecurity) -> Router {
    Router::new()
        .route("/state", get(get_state))
        .route("/player", get(get_player))
        .route("/level", post(set_level))
        .route("/player/position", post(teleport_player))
        .route("/physics", get(get_physics))
        .route("/physics", post(set_physics))
        .route("/physics/raycast", post(physics_raycast))
        .route("/physics/raycast_entities", post(physics_raycast_entities))
        .route("/ai/pathfind", post(ai_pathfind))
        .route("/ai/line_of_sight", post(ai_line_of_sight))
        .route("/simulate", post(simulate))
        .route("/save", post(save_game))
        .route("/load", post(load_game))
        .route("/saves", get(list_saves))
        .route("/levels/pack", post(define_level_pack))
        .route("/levels/pack/{name}/start", post(start_level_pack))
        .route("/levels/pack/{name}/next", post(next_level_pack))
        .route("/levels/pack/{name}/progress", get(level_pack_progress))
        .route("/levels/export", post(export_level))
        .route("/levels/import", post(import_level))
        .route("/game/export", post(export_game))
        .route("/game/import", post(import_game))
        .route("/export/web", post(export_web))
        .route("/export/desktop", post(export_desktop))
        .route("/examples", get(list_examples))
        .route("/examples/{name}/load", post(load_example))
        .route("/game/state", get(get_game_state).post(set_game_state))
        .route("/game/transitions", get(get_game_transitions))
        .route("/game/transition", post(transition_game_state))
        .route("/game/restart", post(restart_game_level))
        .route("/game/load_level", post(load_game_level))
        .route("/replay/record", post(replay_record))
        .route("/replay/stop", post(replay_stop))
        .route("/replay/play", post(replay_play))
        .route("/replay/list", get(replay_list))
        .route(
            "/debug/overlay",
            get(get_debug_overlay).post(set_debug_overlay),
        )
        .route("/debug/input", get(get_debug_input))
        .route("/audio/sfx", post(set_audio_sfx))
        .route("/audio/music", post(set_audio_music))
        .route("/audio/play", post(play_audio))
        .route("/audio/stop", post(stop_audio))
        .route("/audio/config", post(set_audio_config))
        .route("/audio/triggers", post(set_audio_triggers))
        .route("/particles/presets", post(set_particle_presets))
        .route("/audio/state", get(get_audio_state))
        .route("/camera/config", post(set_camera_config))
        .route("/camera/shake", post(camera_shake))
        .route("/camera/look_at", post(camera_look_at))
        .route("/camera/state", get(get_camera_state))
        .route("/ui/screens", post(ui_define_screen))
        .route("/ui/screens/{name}/show", post(ui_show_screen))
        .route("/ui/screens/{name}/hide", post(ui_hide_screen))
        .route("/ui/screens/{name}/nodes/{id}", post(ui_update_node))
        .route("/ui/state", get(get_ui_state))
        .route("/dialogue/conversations", post(set_dialogue_conversation))
        .route("/dialogue/start", post(start_dialogue))
        .route("/dialogue/state", get(get_dialogue_state))
        .route("/dialogue/choose", post(choose_dialogue))
        .route("/scene/describe", get(scene_describe))
        .route("/validate", post(validate))
        .route("/feel/jump", get(get_feel_jump))
        .route("/feel/compare", get(compare_feel))
        .route("/feel/tune", post(tune_feel))
        .route("/generate", post(generate_level))
        .route("/sprites", get(get_sprites))
        .route("/sprites", post(set_sprites))
        .route(
            "/sprites/sheets",
            get(get_sprite_sheets).post(upsert_sprite_sheet),
        )
        .route("/screenshot", get(take_screenshot_api))
        .route("/solve", post(solve_level))
        .route("/config", get(get_config).post(set_config))
        .route("/config/tile_types", post(set_tile_types))
        .route("/entities", get(list_entities).post(create_entity))
        .route("/entities/preset", post(create_entity_preset))
        .route(
            "/entities/reset_non_player",
            post(reset_non_player_entities),
        )
        .route(
            "/entities/{id}",
            get(get_entity).delete(delete_entity).post(damage_entity),
        )
        .route("/entities/{id}/damage", post(damage_entity))
        .route(
            "/entities/{id}/animation",
            get(get_entity_animation).post(set_entity_animation),
        )
        .route("/entities/{id}/particles", post(set_entity_particles))
        .route("/entities/{id}/tween", post(tween_entity))
        .route("/entities/{id}/tween_sequence", post(tween_sequence_entity))
        .route("/events", get(get_events))
        .route("/events/subscribe", get(subscribe_events))
        .route("/perf", get(get_perf))
        .route("/perf/history", get(get_perf_history))
        .route("/scripts", get(list_scripts).post(upsert_script))
        .route("/scripts/{name}", get(get_script).delete(delete_script))
        .route("/scripts/{name}/test", post(test_named_script))
        .route("/scripts/errors", get(get_script_errors).delete(clear_script_errors))
        .route("/scripts/vars", get(get_script_vars).post(set_script_vars))
        .route("/scripts/events", get(get_script_events))
        .route("/scripts/stats", get(get_script_stats))
        .route("/scripts/logs", get(get_script_logs).delete(clear_script_logs))
        .route("/animations", get(list_animation_graphs))
        .route(
            "/animations/{name}",
            get(get_animation_graph)
                .post(upsert_animation_graph)
                .delete(delete_animation_graph),
        )
        .route("/animations/state", get(get_animation_states))
        .route(
            "/input/gamepad",
            get(get_gamepad_config).post(set_gamepad_config),
        )
        .route("/screen/effect", post(trigger_screen_effect))
        .route("/screen/state", get(get_screen_state))
        .route(
            "/lighting/config",
            post(set_lighting_config),
        )
        .route("/lighting/state", get(get_lighting_state))
        .route("/entities/{id}/tint", post(set_entity_tint))
        .route("/entities/{id}/trail", post(set_entity_trail))
        .route(
            "/input/bindings",
            get(get_input_bindings).post(set_input_bindings),
        )
        .route(
            "/lighting/day_night",
            get(get_day_night).post(set_day_night),
        )
        .route("/world_text", post(spawn_world_text))
        .route(
            "/entities/{id}/state",
            get(get_entity_state).post(transition_entity_state),
        )
        .route("/tilemap/auto_tile", post(set_auto_tile))
        .route("/tilemap/layers", get(get_tile_layers).post(set_tile_layer))
        .route("/tilemap/layers/{name}", delete(delete_tile_layer))
        .route(
            "/parallax/layers",
            get(get_parallax).post(set_parallax),
        )
        .route(
            "/weather",
            get(get_weather).post(set_weather).delete(clear_weather),
        )
        .route("/items/define", post(define_items))
        .route(
            "/entities/{id}/inventory",
            get(get_entity_inventory).post(entity_inventory_action),
        )
        .route("/cutscene/define", post(define_cutscene))
        .route("/cutscene/play", post(play_cutscene))
        .route("/cutscene/stop", post(stop_cutscene))
        .route("/cutscene/state", get(get_cutscene_state))
        .route("/presets", get(list_presets).post(define_presets))
        .route("/pool/init", post(init_pool))
        .route("/pool/acquire", post(acquire_from_pool))
        .route("/pool/release/{id}", post(release_to_pool))
        .route("/pool/status", get(get_pool_status))
        // Telemetry
        .route("/telemetry", get(get_telemetry).delete(reset_telemetry))
        // World simulation & scenario testing
        .route("/simulate/world", post(simulate_world))
        .route("/test/scenario", post(run_scenario))
        .route("/test/playtest", post(run_playtest))
        // Screenshot extras
        .route("/screenshot/baseline", post(screenshot_baseline))
        .route("/screenshot/diff", post(screenshot_diff))
        // Atomic build
        .route("/build", post(atomic_build))
        // Manifest validation
        .route("/validate/manifest", post(validate_manifest))
        // Asset pipeline
        .route("/assets/upload", post(upload_asset))
        .route("/assets/generate", post(generate_asset))
        .route("/assets/list", get(list_assets))
        .route("/docs", get(get_docs))
        .route("/docs/html", get(get_docs_html))
        .route("/docs/endpoints", get(get_docs_endpoints))
        .route("/docs/components", get(get_docs_components))
        .route("/docs/presets", get(get_docs_presets))
        .route("/docs/templates", get(get_docs_templates))
        .route("/docs/constraints", get(get_docs_constraints))
        .route("/docs/scripts", get(get_docs_scripts))
        .route("/docs/examples", get(get_docs_examples))
        .route("/docs/security", get(get_docs_security))
        .with_state(state)
        .layer(middleware::from_fn_with_state(security, api_guard))
}
