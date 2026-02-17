use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::HashMap;

pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SpriteAssets::default())
            .insert_resource(SpriteSheetRegistry::default())
            .insert_resource(PlayerAnimations::default())
            .insert_resource(SpriteSheetRenderCache::default())
            .add_systems(PreStartup, init_sprites)
            .add_systems(
                Update,
                (
                    setup_player_sprite,
                    sync_sprite_sheet_cache,
                    ensure_sprite_for_sheet_entities,
                    apply_sprite_sheet_rendering,
                    apply_player_animation_from_controller,
                )
                    .chain(),
            );
    }
}

/// When a Player entity exists without AnimationController, set up its sprite.
/// Runs every frame so it picks up API-spawned players (not just startup-spawned).
fn setup_player_sprite(
    mut commands: Commands,
    player_anims: Res<PlayerAnimations>,
    query: Query<
        Entity,
        (
            With<crate::components::Player>,
            Without<crate::components::AnimationController>,
        ),
    >,
) {
    let Ok(entity) = query.get_single() else {
        return;
    };
    let mut entity_cmd = commands.entity(entity);
    entity_cmd.insert(crate::components::AnimationController {
        graph: "samurai_player".to_string(),
        state: "idle".to_string(),
        frame: 0,
        timer: 0.0,
        speed: 1.0,
        playing: true,
        facing_right: true,
        auto_from_velocity: true,
        facing_direction: 5,
    });
    // If samurai sprites loaded, set up the initial sprite
    if let Some(ref idle_data) = player_anims.idle {
        let mut s = Sprite::from_atlas_image(
            idle_data.texture.clone(),
            TextureAtlas {
                layout: idle_data.layout.clone(),
                index: 0,
            },
        );
        s.custom_size = Some(Vec2::new(140.0, 140.0));
        s.anchor = bevy::sprite::Anchor::Custom(Vec2::new(0.0, -0.29));
        entity_cmd.insert(s);
    }
}

/// Holds all loaded sprite handles, keyed by name
#[derive(Resource, Default)]
pub struct SpriteAssets {
    pub textures: HashMap<String, Handle<Image>>,
    pub enabled: bool,
    pub manifest: SpriteManifest,
}

impl SpriteAssets {
    pub fn get(&self, name: &str) -> Option<&Handle<Image>> {
        if self.enabled {
            self.textures.get(name)
        } else {
            None
        }
    }

    pub fn get_tile(&self, tile_type: crate::components::TileType) -> Option<&Handle<Image>> {
        let name = match tile_type {
            crate::components::TileType::Solid => "tile_solid",
            crate::components::TileType::Spike => "tile_spike",
            crate::components::TileType::Goal => "tile_goal",
            crate::components::TileType::Platform => "tile_platform",
            crate::components::TileType::SlopeUp => "tile_slope_up",
            crate::components::TileType::SlopeDown => "tile_slope_down",
            crate::components::TileType::Ladder => "tile_ladder",
            crate::components::TileType::Empty => return None,
        };
        self.get(name)
    }
}

/// Manifest describing sprite file mappings
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SpriteManifest {
    pub player: Option<SpriteEntry>,
    pub tiles: HashMap<String, SpriteEntry>,
}

#[derive(Resource, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SpriteSheetRegistry {
    pub sheets: HashMap<String, SpriteSheetDef>,
    #[serde(skip)]
    pub version: u64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SpriteSheetDef {
    pub path: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
    #[serde(default = "default_one_u32")]
    pub rows: u32,
    #[serde(default)]
    pub animations: HashMap<String, SpriteSheetAnimationDef>,
    #[serde(default)]
    pub direction_map: Option<Vec<u8>>,
    #[serde(default = "default_anchor_y")]
    pub anchor_y: f32,
}

fn default_anchor_y() -> f32 {
    -0.15
}

fn default_one_u32() -> u32 {
    1
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SpriteSheetAnimationDef {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub frames: Vec<usize>,
    pub fps: f32,
    #[serde(default = "default_true")]
    pub looping: bool,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub events: Vec<crate::animation::AnimFrameEventDef>,
}

fn default_true() -> bool {
    true
}

/// Cached texture + atlas layout for a specific sprite sheet (or per-animation override).
struct CachedSheetEntry {
    texture: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
}

/// Runtime cache that bridges SpriteSheetRegistry (data) to actual Bevy texture handles.
#[derive(Resource, Default)]
pub struct SpriteSheetRenderCache {
    /// Key: (sheet_name, anim_state) for per-animation paths, or (sheet_name, "") for sheet-level fallback
    entries: HashMap<(String, String), CachedSheetEntry>,
    /// Track which sheet names we've already loaded so we only load once
    loaded_sheets: std::collections::HashSet<String>,
    /// Last registry version we synced from
    last_synced_version: u64,
    /// Last asset_path we synced with; if it changes we need to reload everything.
    last_asset_path: Option<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SpriteEntry {
    pub path: String,
    pub columns: Option<u32>,
    pub rows: Option<u32>,
    pub frame_width: Option<u32>,
    pub frame_height: Option<u32>,
}

/// Player animation data (atlas layouts + textures for each state)
#[derive(Resource, Default)]
pub struct PlayerAnimations {
    pub idle: Option<AnimationData>,
    pub run: Option<AnimationData>,
    pub attack: Option<AnimationData>,
    pub hurt: Option<AnimationData>,
}

pub struct AnimationData {
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub frame_count: usize,
}

fn init_sprites(
    mut images: ResMut<Assets<Image>>,
    mut sprite_assets: ResMut<SpriteAssets>,
    mut player_anims: ResMut<PlayerAnimations>,
    asset_server: Res<AssetServer>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // Try loading samurai sprites from assets/samurai/
    let samurai_idle = std::path::Path::new("assets/samurai/idle.png");
    if samurai_idle.exists() {
        load_samurai_sprites(&asset_server, &mut atlas_layouts, &mut player_anims);
        println!("[Axiom Sprites] Loaded samurai sprite sheets");
    }

    // Try loading manifest
    let manifest_path = "assets/sprites.json";
    if let Ok(manifest_str) = std::fs::read_to_string(manifest_path) {
        if let Ok(manifest) = serde_json::from_str::<SpriteManifest>(&manifest_str) {
            load_from_manifest(&manifest, &asset_server, &mut sprite_assets);
            sprite_assets.manifest = manifest;
            sprite_assets.enabled = true;
            println!("[Axiom Sprites] Loaded manifest from {}", manifest_path);
            return;
        }
    }

    // Generate placeholder tile sprites
    generate_placeholders(&mut images, &mut sprite_assets);
    sprite_assets.enabled = true;
    println!("[Axiom Sprites] Generated placeholder sprites");
}

fn load_samurai_sprites(
    asset_server: &AssetServer,
    atlas_layouts: &mut Assets<TextureAtlasLayout>,
    player_anims: &mut PlayerAnimations,
) {
    let frame_size = UVec2::new(96, 96);

    // IDLE: 960x96 = 10 frames
    let idle_tex = asset_server.load("samurai/idle.png");
    let idle_layout = TextureAtlasLayout::from_grid(frame_size, 10, 1, None, None);
    player_anims.idle = Some(AnimationData {
        texture: idle_tex,
        layout: atlas_layouts.add(idle_layout),
        frame_count: 10,
    });

    // RUN: 1536x96 = 16 frames
    let run_tex = asset_server.load("samurai/run.png");
    let run_layout = TextureAtlasLayout::from_grid(frame_size, 16, 1, None, None);
    player_anims.run = Some(AnimationData {
        texture: run_tex,
        layout: atlas_layouts.add(run_layout),
        frame_count: 16,
    });

    // ATTACK: 672x96 = 7 frames
    let attack_tex = asset_server.load("samurai/attack.png");
    let attack_layout = TextureAtlasLayout::from_grid(frame_size, 7, 1, None, None);
    player_anims.attack = Some(AnimationData {
        texture: attack_tex,
        layout: atlas_layouts.add(attack_layout),
        frame_count: 7,
    });

    // HURT: 384x96 = 4 frames
    let hurt_tex = asset_server.load("samurai/hurt.png");
    let hurt_layout = TextureAtlasLayout::from_grid(frame_size, 4, 1, None, None);
    player_anims.hurt = Some(AnimationData {
        texture: hurt_tex,
        layout: atlas_layouts.add(hurt_layout),
        frame_count: 4,
    });
}

/// Insert a default Sprite on entities that have AnimationController with a registered
/// sprite sheet graph but no Sprite component yet. Without this, the rendering system
/// can't write texture/atlas data (it queries for `&mut Sprite`).
fn ensure_sprite_for_sheet_entities(
    mut commands: Commands,
    registry: Res<SpriteSheetRegistry>,
    query: Query<
        (Entity, &crate::components::AnimationController),
        (Without<Sprite>, Without<crate::components::Invisible>),
    >,
) {
    for (entity, anim) in query.iter() {
        if registry.sheets.contains_key(&anim.graph) {
            commands.entity(entity).insert(Sprite {
                color: Color::WHITE,
                ..default()
            });
        }
    }
}

/// Resolve a sprite path against the game's configured asset_path.
/// If asset_path is set and the sprite path is relative, joins them to create
/// an absolute path that Bevy's AssetServer can load regardless of the startup
/// asset root.  If asset_path is None, returns the path unchanged (relative to
/// Bevy's default asset root).
fn resolve_sprite_asset_path(relative: &str, asset_path: Option<&str>) -> String {
    if let Some(base) = asset_path.filter(|s| !s.is_empty()) {
        let p = std::path::Path::new(relative);
        if p.is_absolute() {
            return relative.to_string();
        }
        let resolved = std::path::Path::new(base).join(relative);
        resolved.to_string_lossy().to_string()
    } else {
        relative.to_string()
    }
}

/// Sync SpriteSheetRegistry into the render cache: load textures + create atlas layouts.
fn sync_sprite_sheet_cache(
    registry: Res<SpriteSheetRegistry>,
    config: Res<crate::components::GameConfig>,
    asset_server: Res<AssetServer>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut cache: ResMut<SpriteSheetRenderCache>,
) {
    // Invalidate cache if the registry version changed or the asset_path changed.
    let asset_path_changed = cache.last_asset_path != config.asset_path;
    if registry.version != cache.last_synced_version || asset_path_changed {
        cache.loaded_sheets.clear();
        cache.entries.clear();
        cache.last_synced_version = registry.version;
        cache.last_asset_path = config.asset_path.clone();
    }
    let base = config.asset_path.as_deref();
    for (name, sheet) in &registry.sheets {
        if cache.loaded_sheets.contains(name) {
            continue;
        }
        cache.loaded_sheets.insert(name.clone());
        let frame_size = UVec2::new(sheet.frame_width, sheet.frame_height);

        // Load the sheet-level fallback texture + layout
        let resolved = resolve_sprite_asset_path(&sheet.path, base);
        let texture: Handle<Image> = asset_server.load(&resolved);
        let layout = TextureAtlasLayout::from_grid(
            frame_size,
            sheet.columns,
            sheet.rows,
            None,
            None,
        );
        let layout_handle = atlas_layouts.add(layout);
        cache.entries.insert(
            (name.clone(), String::new()),
            CachedSheetEntry {
                texture,
                layout: layout_handle,
            },
        );

        // Load per-animation override textures (separate PNG per animation state)
        for (anim_name, anim_def) in &sheet.animations {
            if let Some(ref anim_path) = anim_def.path {
                if anim_path != &sheet.path {
                    let resolved_anim = resolve_sprite_asset_path(anim_path, base);
                    let anim_texture: Handle<Image> = asset_server.load(&resolved_anim);
                    let anim_layout = TextureAtlasLayout::from_grid(
                        frame_size,
                        sheet.columns,
                        sheet.rows,
                        None,
                        None,
                    );
                    let anim_layout_handle = atlas_layouts.add(anim_layout);
                    cache.entries.insert(
                        (name.clone(), anim_name.clone()),
                        CachedSheetEntry {
                            texture: anim_texture,
                            layout: anim_layout_handle,
                        },
                    );
                }
            }
        }
    }
}

/// Render entities that have AnimationController graphs registered in SpriteSheetRegistry.
fn apply_sprite_sheet_rendering(
    registry: Res<SpriteSheetRegistry>,
    cache: Res<SpriteSheetRenderCache>,
    library: Option<Res<crate::animation::AnimationLibrary>>,
    mut query: Query<(&mut Sprite, &crate::components::AnimationController)>,
) {
    for (mut sprite, anim) in query.iter_mut() {
        let Some(sheet) = registry.sheets.get(&anim.graph) else {
            continue; // Not a registered sprite sheet, skip (handled by samurai fallback)
        };

        // Resolve per-animation texture or fall back to sheet-level
        let entry = cache
            .entries
            .get(&(anim.graph.clone(), anim.state.clone()))
            .or_else(|| cache.entries.get(&(anim.graph.clone(), String::new())));

        let Some(entry) = entry else {
            continue;
        };

        // Compute the frame index within the animation
        let render_frame = library
            .as_ref()
            .and_then(|lib| lib.graphs.get(&anim.graph))
            .and_then(|graph| graph.states.get(&anim.state))
            .map(|clip| crate::animation::resolve_clip_frame(clip, anim.frame))
            .unwrap_or(anim.frame % sheet.columns.max(1) as usize);

        // Compute atlas index: for multi-row sheets, row = facing_direction (or mapped via direction_map)
        let atlas_index = if sheet.rows > 1 {
            let row = if let Some(ref map) = sheet.direction_map {
                *map.get(anim.facing_direction as usize).unwrap_or(&anim.facing_direction) as u32
            } else {
                anim.facing_direction as u32
            };
            let dir = row.min(sheet.rows - 1);
            (dir * sheet.columns + render_frame as u32) as usize
        } else {
            render_frame
        };

        sprite.image = entry.texture.clone();
        sprite.color = Color::WHITE; // Clear any tint from fallback sprites
        sprite.texture_atlas = Some(TextureAtlas {
            layout: entry.layout.clone(),
            index: atlas_index,
        });
        // Display at full frame size — character body (~30% of cell) scales with world
        let display_size = sheet.frame_width.max(sheet.frame_height) as f32;
        sprite.custom_size = Some(Vec2::new(display_size, display_size));
        sprite.anchor = bevy::sprite::Anchor::Custom(Vec2::new(0.0, sheet.anchor_y));
        // Don't flip for 8-directional sprites
        if sheet.rows > 1 {
            sprite.flip_x = false;
        }
    }
}

fn apply_player_animation_from_controller(
    registry: Res<SpriteSheetRegistry>,
    player_anims: Res<PlayerAnimations>,
    library: Option<Res<crate::animation::AnimationLibrary>>,
    mut query: Query<
        (&mut Sprite, &crate::components::AnimationController),
        With<crate::components::Player>,
    >,
) {
    for (mut sprite, anim) in query.iter_mut() {
        // Skip if this player is using a registered sprite sheet (handled by apply_sprite_sheet_rendering)
        if registry.sheets.contains_key(&anim.graph) {
            continue;
        }
        let anim_data = match anim.state.as_str() {
            "run" => player_anims.run.as_ref(),
            "attack" => player_anims.attack.as_ref(),
            "hurt" => player_anims.hurt.as_ref(),
            _ => player_anims.idle.as_ref(),
        };

        let Some(data) = anim_data else { continue };
        let render_frame = library
            .as_ref()
            .and_then(|lib| lib.graphs.get(&anim.graph))
            .and_then(|graph| graph.states.get(&anim.state))
            .map(|clip| crate::animation::resolve_clip_frame(clip, anim.frame))
            .unwrap_or(anim.frame % data.frame_count.max(1));

        // Character is ~23x34 px at y=47-81 in 96x96 frame (91% padding)
        // Anchor offset puts character feet (image y=81) at hitbox bottom
        sprite.image = data.texture.clone();
        sprite.custom_size = Some(Vec2::new(140.0, 140.0));
        sprite.anchor = bevy::sprite::Anchor::Custom(Vec2::new(0.0, -0.29));
        sprite.texture_atlas = Some(TextureAtlas {
            layout: data.layout.clone(),
            index: render_frame % data.frame_count.max(1),
        });
        sprite.flip_x = !anim.facing_right;
    }
}

fn load_from_manifest(
    manifest: &SpriteManifest,
    asset_server: &AssetServer,
    sprite_assets: &mut SpriteAssets,
) {
    if let Some(ref entry) = manifest.player {
        sprite_assets
            .textures
            .insert("player".to_string(), asset_server.load(&entry.path));
    }

    for (name, entry) in &manifest.tiles {
        let key = format!("tile_{}", name);
        sprite_assets
            .textures
            .insert(key, asset_server.load(&entry.path));
    }
}

fn generate_placeholders(images: &mut Assets<Image>, sprite_assets: &mut SpriteAssets) {
    let ts = 16u32;

    sprite_assets
        .textures
        .insert("tile_solid".to_string(), images.add(make_brick_tile(ts)));

    sprite_assets
        .textures
        .insert("tile_spike".to_string(), images.add(make_spike_tile(ts)));

    sprite_assets
        .textures
        .insert("tile_goal".to_string(), images.add(make_goal_tile(ts)));
}

fn make_image(width: u32, height: u32, data: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

#[derive(Clone, Copy)]
struct Rgba(u8, u8, u8, u8);

fn set_pixel(data: &mut [u8], width: u32, x: u32, y: u32, color: Rgba) {
    let idx = ((y * width + x) * 4) as usize;
    if idx + 3 < data.len() {
        data[idx] = color.0;
        data[idx + 1] = color.1;
        data[idx + 2] = color.2;
        data[idx + 3] = color.3;
    }
}

fn make_brick_tile(size: u32) -> Image {
    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let brick_h = 4u32;
            let brick_w = 8u32;
            let row = y / brick_h;
            let offset = if row % 2 == 0 { 0 } else { brick_w / 2 };
            let bx = (x + offset) % brick_w;
            let by = y % brick_h;
            let is_mortar = bx == 0 || by == 0;
            if is_mortar {
                set_pixel(&mut data, size, x, y, Rgba(60, 58, 55, 255));
            } else {
                let base = 90 + ((row * 7) % 20) as u8;
                set_pixel(&mut data, size, x, y, Rgba(base + 10, base, base - 5, 255));
            }
        }
    }
    make_image(size, size, data)
}

fn make_spike_tile(size: u32) -> Image {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let s = size as i32;
    let mid = s / 2;
    for y in 0..size {
        for x in 0..size {
            let ix = x as i32;
            let iy = y as i32;
            let inv_y = s - 1 - iy;
            let half_width = (inv_y * mid) / s;
            if ix >= mid - half_width && ix <= mid + half_width && inv_y >= 0 {
                let t = iy as f32 / size as f32;
                let r = (220.0 - t * 80.0) as u8;
                let g = (40.0 - t * 30.0).max(10.0) as u8;
                set_pixel(&mut data, size, x, y, Rgba(r, g, 15, 255));
            } else {
                set_pixel(&mut data, size, x, y, Rgba(30, 20, 20, 255));
            }
        }
    }
    make_image(size, size, data)
}

fn make_goal_tile(size: u32) -> Image {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    let mid = s / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - mid + 0.5;
            let dy = y as f32 - mid + 0.5;
            let dist = dx.abs() + dy.abs();
            if dist <= mid - 1.0 {
                let t = dist / mid;
                let g = (255.0 - t * 80.0) as u8;
                let r = (80.0 + t * 40.0) as u8;
                set_pixel(&mut data, size, x, y, Rgba(r, g, 60, 255));
            } else if dist <= mid {
                set_pixel(&mut data, size, x, y, Rgba(200, 255, 200, 255));
            } else {
                set_pixel(&mut data, size, x, y, Rgba(0, 0, 0, 0));
            }
        }
    }
    make_image(size, size, data)
}

pub fn reload_from_manifest(
    manifest: &SpriteManifest,
    asset_server: &AssetServer,
    sprite_assets: &mut SpriteAssets,
) {
    sprite_assets.textures.clear();
    load_from_manifest(manifest, asset_server, sprite_assets);
    sprite_assets.manifest = manifest.clone();
    sprite_assets.enabled = true;
}
