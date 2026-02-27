use super::*;
use bevy::ecs::system::SystemParam;

#[derive(SystemParam)]
pub(in crate::api) struct ApplyLevelCtx<'w, 's> {
    commands: Commands<'w, 's>,
    physics: Res<'w, GameConfig>,
    headless: Res<'w, HeadlessMode>,
    sprite_assets: Option<Res<'w, SpriteAssets>>,
    asset_server: Res<'w, AssetServer>,
    atlas_layouts: ResMut<'w, Assets<TextureAtlasLayout>>,
    player_query: Query<'w, 's, (&'static mut GamePosition, &'static mut Velocity), With<Player>>,
}

pub(in crate::api) fn apply_level_change(
    mut pending: ResMut<PendingLevelChange>,
    mut tilemap: ResMut<Tilemap>,
    tile_entities: Query<Entity, With<TileEntity>>,
    mut ctx: ApplyLevelCtx,
) {
    let Some(req) = pending.0.take() else { return };

    tilemap.width = req.width;
    tilemap.height = req.height;
    tilemap.tiles = req.tiles;
    if let Some(spawn) = req.player_spawn {
        tilemap.player_spawn = spawn;
    }
    tilemap.goal = req.goal;
    tilemap.extra_layers = req.extra_layers;

    // Sync solid_ids from the tile type registry (materials registered before level load)
    for (id, def) in ctx.physics.tile_types.types.iter().enumerate() {
        if def.flags & crate::components::TILE_SOLID != 0 {
            tilemap.solid_ids.insert(id as u8);
        }
    }

    // Recalculate autotile visuals now that tiles are loaded
    tilemap.recalculate_auto_tiles();

    for entity in tile_entities.iter() {
        ctx.commands.entity(entity).despawn();
    }

    // Only spawn visual tile entities in windowed mode
    if !ctx.headless.0 {
        let ts = ctx.physics.tile_size;
        let tileset_data = crate::tilemap::prepare_tileset_data(
            &ctx.physics.tile_types,
            &ctx.asset_server,
            &mut ctx.atlas_layouts,
            ctx.physics.asset_path.as_deref(),
        );
        let sa = ctx.sprite_assets.as_deref();

        println!("[LevelLoad] tileset_data keys: {:?}, tile_visuals len={}, non-zero={}",
            tileset_data.keys().collect::<Vec<_>>(),
            tilemap.tile_visuals.len(),
            tilemap.tile_visuals.iter().filter(|&&v| v != 0).count());
        // Spawn main tilemap layer
        crate::tilemap::spawn_tile_layer(
            &mut ctx.commands,
            &tilemap.tiles,
            tilemap.width,
            tilemap.height,
            &tilemap.tile_visuals,
            0.0,
            &tileset_data,
            sa,
            &ctx.physics.tile_types,
            &ctx.physics.tile_mode,
            ts,
        );

        // Spawn extra decorative layers
        for layer in &tilemap.extra_layers {
            crate::tilemap::spawn_tile_layer(
                &mut ctx.commands,
                &layer.tiles,
                tilemap.width,
                tilemap.height,
                &[],
                layer.z_offset,
                &tileset_data,
                sa,
                &ctx.physics.tile_types,
                &ctx.physics.tile_mode,
                ts,
            );
        }
    }

    if let Ok((mut pos, mut vel)) = ctx.player_query.get_single_mut() {
        pos.x = tilemap.player_spawn.0;
        pos.y = tilemap.player_spawn.1;
        vel.x = 0.0;
        vel.y = 0.0;
    }
}

pub(in crate::api) fn sync_runtime_store_from_ecs(
    runtime_state: Option<Res<crate::game_runtime::RuntimeState>>,
    runtime_store: Option<Res<RuntimeStoreHandle>>,
) {
    let (Some(runtime_state), Some(runtime_store)) = (runtime_state, runtime_store) else {
        return;
    };

    let snapshot = runtime_state.snapshot();
    let mut store = runtime_store.0.write().unwrap();

    let now = std::time::Instant::now();
    let elapsed = std::time::Duration::from_secs_f32(snapshot.time_in_state_seconds.max(0.0));
    store.entered_at = now.checked_sub(elapsed).unwrap_or(now);

    if store.state != snapshot.state {
        let from = store.state.clone();
        let to = snapshot.state.clone();
        let effect = snapshot
            .active_transition
            .as_ref()
            .and_then(|t| t.effect.clone())
            .or(Some("Instant".to_string()));
        let duration = snapshot
            .active_transition
            .as_ref()
            .map(|t| t.duration)
            .unwrap_or(0.0);
        store.state = snapshot.state.clone();
        store.transitions.push(RuntimeTransition {
            from,
            to,
            effect,
            duration,
            at_unix_ms: unix_ms_now(),
        });
        if store.transitions.len() > 256 {
            let excess = store.transitions.len() - 256;
            store.transitions.drain(0..excess);
        }
    }

    store.active_transition = snapshot.active_transition.map(|t| {
        let started_at = now
            .checked_sub(std::time::Duration::from_secs_f32(
                t.elapsed_seconds.max(0.0),
            ))
            .unwrap_or(now);
        RuntimeActiveTransition {
            from: t.from,
            to: t.to,
            effect: t.effect,
            duration: t.duration,
            started_at,
        }
    });
}

pub(in crate::api) fn apply_physics_change(
    mut pending: ResMut<PendingPhysicsChange>,
    mut physics: ResMut<GameConfig>,
) {
    if let Some(new_config) = pending.0.take() {
        *physics = new_config;
    }
}

pub(in crate::api) fn take_screenshot(
    mut pending: ResMut<PendingScreenshot>,
    headless: Res<HeadlessMode>,
    mut commands: Commands,
) {
    if !pending.requested {
        return;
    }
    pending.requested = false;
    if headless.0 {
        pending.path = None;
        return;
    }
    if let Some(output_path) = pending.path.take() {
        if let Some(parent) = output_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        println!("[Axiom] Screenshot requested: {}", output_path.display());
        // Store save_path for the global observer to pick up when capture completes.
        pending.save_path = Some(output_path);
        // Spawn without entity-level observer — the global observer handles saving.
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window());
    }
}

/// Global observer for ScreenshotCaptured events. Registered once at app startup via
/// `app.add_observer(screenshot_captured_observer)`. This avoids entity-level observer
/// lifecycle issues — it fires whenever ANY screenshot is captured.
pub(in crate::api) fn screenshot_captured_observer(
    trigger: Trigger<bevy::render::view::screenshot::ScreenshotCaptured>,
    mut pending: ResMut<PendingScreenshot>,
) {
    let Some(save_path) = pending.save_path.take() else {
        println!("[Axiom] Screenshot captured but no save_path pending — ignoring");
        return;
    };
    println!("[Axiom] Screenshot captured, saving to: {}", save_path.display());
    let img = trigger.event().0.clone();
    match img.try_into_dynamic() {
        Ok(dyn_img) => {
            let rgb = dyn_img.to_rgb8();
            match rgb.save(&save_path) {
                Ok(()) => println!("[Axiom] Screenshot saved: {}", save_path.display()),
                Err(e) => eprintln!("[Axiom] Screenshot save failed: {e}"),
            }
        }
        Err(e) => eprintln!("[Axiom] Screenshot conversion failed: {e:?}"),
    }
}
