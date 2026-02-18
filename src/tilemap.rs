use crate::components::*;
use crate::sprites::SpriteAssets;
use bevy::prelude::*;
use std::collections::HashMap;

pub struct TilemapPlugin;

impl Plugin for TilemapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Tilemap::test_level())
            .add_systems(Startup, spawn_tilemap);
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TileLayer {
    pub name: String,
    pub tiles: Vec<u8>,
    pub z_offset: f32,
}

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Tilemap {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub player_spawn: (f32, f32),
    pub goal: Option<(i32, i32)>,
    /// Auto-tile rules: name -> AutoTileSetDef (visual variant mapping).
    #[serde(default)]
    pub auto_tile_rules: std::collections::HashMap<String, crate::api::types::AutoTileSetDef>,
    /// Visual variant indices parallel to `tiles`. Used by auto-tiling.
    #[serde(default)]
    pub tile_visuals: Vec<u16>,
    /// Extra decorative tile layers (visual-only, no physics).
    #[serde(default)]
    pub extra_layers: Vec<TileLayer>,
}

impl Tilemap {
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn get_tile(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width() as i32 || y >= self.height() as i32 {
            return TileType::Empty as u8;
        }
        self.tiles[y as usize * self.width() + x as usize]
    }

    pub fn tile_id(&self, x: i32, y: i32) -> u8 {
        self.get_tile(x, y)
    }

    pub fn get(&self, x: i32, y: i32) -> TileType {
        TileType::from_u8(self.tile_id(x, y))
    }

    pub fn set_tile(&mut self, x: i32, y: i32, tile_id: u8) {
        if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
            self.tiles[y as usize * self.width + x as usize] = tile_id;
        }
    }

    pub fn set(&mut self, x: i32, y: i32, tile: TileType) {
        self.set_tile(x, y, tile as u8);
    }

    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_solid()
    }

    pub fn is_ground(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_ground_like()
    }

    /// Recalculate auto-tile visual variants for all tiles.
    /// Uses 4-bit neighbor bitmask (N=1, S=2, E=4, W=8).
    pub fn recalculate_auto_tiles(&mut self) {
        if self.auto_tile_rules.is_empty() {
            self.tile_visuals.clear();
            return;
        }
        self.tile_visuals = vec![0u16; self.tiles.len()];
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let tile_id = self.get_tile(x, y);
                // Find matching rule by base_tile_id
                let mut visual = 0u16;
                for rule in self.auto_tile_rules.values() {
                    if rule.base_tile_id == tile_id {
                        let mut mask: u8 = 0;
                        if self.get_tile(x, y + 1) == tile_id { mask |= 1; } // N
                        if self.get_tile(x, y - 1) == tile_id { mask |= 2; } // S
                        if self.get_tile(x + 1, y) == tile_id { mask |= 4; } // E
                        if self.get_tile(x - 1, y) == tile_id { mask |= 8; } // W
                        visual = rule.variants.get(&mask).copied().unwrap_or(0) as u16;
                        break;
                    }
                }
                self.tile_visuals[y as usize * self.width + x as usize] = visual;
            }
        }
    }

    /// A simple test level for development
    pub fn test_level() -> Self {
        let width = 60;
        let height = 20;
        let mut tiles = vec![0u8; width * height];

        // Ground floor (y=0, bottom row)
        for tile in tiles.iter_mut().take(width) {
            *tile = TileType::Solid as u8;
        }

        // Gap in ground (x=15..18)
        for tile in tiles.iter_mut().take(18).skip(15) {
            *tile = TileType::Empty as u8;
        }

        // Spikes at bottom of gap
        // (gap is at y=0, spikes don't make sense at y=0 since it's the floor)
        // Instead, make the gap deeper and add spikes below - but we only have y>=0
        // So let's put spikes at the gap positions as the floor
        for tile in tiles.iter_mut().take(18).skip(15) {
            *tile = TileType::Spike as u8;
        }

        // Platforms
        // Platform at y=3, x=8..12
        for x in 8..12 {
            tiles[3 * width + x] = TileType::Solid as u8;
        }

        // Platform at y=5, x=20..26
        for x in 20..26 {
            tiles[5 * width + x] = TileType::Solid as u8;
        }

        // Staircase platforms (y=2..5, x=30..35)
        for i in 0..4 {
            let x = 30 + i * 2;
            let y = 2 + i;
            for dx in 0..2 {
                if x + dx < width {
                    tiles[y * width + x + dx] = TileType::Solid as u8;
                }
            }
        }

        // Wall (x=40, y=1..4)
        for y in 1..4 {
            tiles[y * width + 40] = TileType::Solid as u8;
        }

        // Higher platform to jump over wall (y=5, x=38..42)
        for x in 38..43 {
            tiles[5 * width + x] = TileType::Solid as u8;
        }

        // Goal platform and goal tile
        for x in 50..55 {
            tiles[width + x] = TileType::Solid as u8;
        }
        tiles[2 * width + 52] = TileType::Goal as u8;

        Tilemap {
            width,
            height,
            tiles,
            player_spawn: (2.0 * 16.0 + 8.0, 1.0 * 16.0 + 8.0), // above ground
            goal: Some((52, 2)),
            ..Default::default()
        }
    }
}

/// Marker for tile visual entities (so we can despawn them when reloading)
#[derive(Component)]
pub struct TileEntity;

/// Cached render data for a tileset (texture + atlas layout).
pub struct TilesetRenderData {
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub tile_width: f32,
    pub tile_height: f32,
}

/// Prepare tileset render data for all tile types that have a TilesetDef.
pub fn prepare_tileset_data(
    registry: &TileTypeRegistry,
    asset_server: &AssetServer,
    atlas_layouts: &mut Assets<TextureAtlasLayout>,
    asset_path: Option<&str>,
) -> HashMap<u8, TilesetRenderData> {
    let mut data = HashMap::new();
    for (idx, def) in registry.types.iter().enumerate() {
        if let Some(ref tileset) = def.tileset {
            let resolved = crate::sprites::resolve_sprite_asset_path(&tileset.path, asset_path);
            let texture: Handle<Image> = asset_server.load(&resolved);
            let frame_size = UVec2::new(tileset.tile_width, tileset.tile_height);
            let layout = TextureAtlasLayout::from_grid(
                frame_size,
                tileset.columns,
                tileset.rows,
                None,
                None,
            );
            let layout_handle = atlas_layouts.add(layout);
            data.insert(idx as u8, TilesetRenderData {
                texture,
                layout: layout_handle,
                tile_width: tileset.tile_width as f32,
                tile_height: tileset.tile_height as f32,
            });
        }
    }
    data
}

/// Build a Sprite for a tile, using tileset data when available.
/// When a tileset is found, sets `sprite.texture_atlas` for atlas-based rendering.
pub fn build_tile_sprite(
    tile_id: u8,
    visual_index: u16,
    tileset_data: &HashMap<u8, TilesetRenderData>,
    sprite_assets: Option<&SpriteAssets>,
    registry: &TileTypeRegistry,
    ts: f32,
) -> Sprite {
    // Priority 1: TilesetDef with texture atlas
    if let Some(td) = tileset_data.get(&tile_id) {
        let atlas_index = if visual_index > 0 {
            visual_index as usize
        } else if let Some(def) = registry.types.get(tile_id as usize) {
            if let Some(ref tileset) = def.tileset {
                if let Some(ref vmap) = tileset.variant_map {
                    vmap.get(&tile_id).copied().unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };
        return Sprite {
            image: td.texture.clone(),
            custom_size: Some(Vec2::new(td.tile_width, td.tile_height)),
            texture_atlas: Some(TextureAtlas {
                layout: td.layout.clone(),
                index: atlas_index,
            }),
            ..default()
        };
    }

    // Priority 2: SpriteAssets built-in tile sprites
    let tile_type = TileType::from_u8(tile_id);
    if let Some(sa) = sprite_assets {
        if let Some(handle) = sa.get_tile(tile_type) {
            return Sprite {
                image: handle.clone(),
                custom_size: Some(Vec2::new(ts, ts)),
                ..default()
            };
        }
    }

    // Priority 3: Colored rectangle fallback
    tile_color_sprite_by_id(tile_id, registry, ts)
}

/// Spawn tile entities for a tile grid (main tilemap or extra layer).
pub fn spawn_tile_layer(
    commands: &mut Commands,
    tiles: &[u8],
    width: usize,
    height: usize,
    tile_visuals: &[u16],
    z_offset: f32,
    tileset_data: &HashMap<u8, TilesetRenderData>,
    sprite_assets: Option<&SpriteAssets>,
    registry: &TileTypeRegistry,
    tile_mode: &TileMode,
    ts: f32,
) {
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let tile_id = tiles[idx];
            if tile_id == 0 {
                continue;
            }
            let visual_index = tile_visuals.get(idx).copied().unwrap_or(0);
            let sprite = build_tile_sprite(
                tile_id, visual_index, tileset_data, sprite_assets, registry, ts,
            );
            let tile_type = TileType::from_u8(tile_id);
            let (wx, wy) = tile_mode.grid_to_world(x as f32, y as f32, ts);
            let z = match tile_mode {
                TileMode::Isometric { depth_sort: true, .. } => z_offset - wy * 0.001,
                _ => z_offset,
            };

            commands.spawn((
                TileEntity,
                Tile { tile_type },
                GridPosition {
                    x: x as i32,
                    y: y as i32,
                },
                sprite,
                Transform::from_xyz(wx, wy, z),
            ));
        }
    }
}

fn spawn_tilemap(
    mut commands: Commands,
    tilemap: Res<Tilemap>,
    physics: Res<GameConfig>,
    headless: Res<HeadlessMode>,
    sprite_assets: Option<Res<SpriteAssets>>,
    asset_server: Res<AssetServer>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    if headless.0 {
        return;
    }
    let ts = physics.tile_size;
    let tileset_data = prepare_tileset_data(&physics.tile_types, &asset_server, &mut atlas_layouts, physics.asset_path.as_deref());
    let sa = sprite_assets.as_deref();

    // Spawn main tilemap layer
    spawn_tile_layer(
        &mut commands,
        &tilemap.tiles,
        tilemap.width,
        tilemap.height,
        &tilemap.tile_visuals,
        0.0,
        &tileset_data,
        sa,
        &physics.tile_types,
        &physics.tile_mode,
        ts,
    );

    // Spawn extra decorative layers
    for layer in &tilemap.extra_layers {
        spawn_tile_layer(
            &mut commands,
            &layer.tiles,
            tilemap.width,
            tilemap.height,
            &[],  // Extra layers don't have auto-tile visuals
            layer.z_offset,
            &tileset_data,
            sa,
            &physics.tile_types,
            &physics.tile_mode,
            ts,
        );
    }
}

/// Look up color from TileTypeRegistry, falling back to legacy TileType colors then gray.
pub fn tile_color_sprite_by_id(tile_id: u8, registry: &TileTypeRegistry, ts: f32) -> Sprite {
    if let Some(def) = registry.types.get(tile_id as usize) {
        if let Some([r, g, b]) = def.color {
            return Sprite::from_color(Color::srgb(r, g, b), Vec2::new(ts, ts));
        }
    }
    // Fallback to legacy hardcoded colors for built-in types
    tile_color_sprite(TileType::from_u8(tile_id), ts)
}

pub fn tile_color_sprite(tile_type: TileType, ts: f32) -> Sprite {
    let color = match tile_type {
        TileType::Solid => Color::srgb(0.4, 0.4, 0.45),
        TileType::Spike => Color::srgb(0.9, 0.15, 0.15),
        TileType::Goal => Color::srgb(0.15, 0.9, 0.3),
        TileType::Platform => Color::srgb(0.85, 0.7, 0.2),
        TileType::SlopeUp => Color::srgb(0.2, 0.65, 0.9),
        TileType::SlopeDown => Color::srgb(0.18, 0.52, 0.82),
        TileType::Ladder => Color::srgb(0.72, 0.47, 0.2),
        TileType::Empty => Color::srgb(0.5, 0.5, 0.5),
    };
    Sprite::from_color(color, Vec2::new(ts, ts))
}
