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
    /// 8-bit material autotile rules: name -> MaterialAutoTileRule.
    #[serde(default)]
    pub material_auto_tile_rules: std::collections::HashMap<String, crate::api::types::MaterialAutoTileRule>,
    /// Extra decorative tile layers (visual-only, no physics).
    #[serde(default)]
    pub extra_layers: Vec<TileLayer>,
    /// Additional tile IDs that should be treated as solid (from terrain materials).
    #[serde(default)]
    pub solid_ids: std::collections::HashSet<u8>,
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
        let id = self.get_tile(x, y);
        TileType::from_u8(id).is_solid() || self.solid_ids.contains(&id)
    }

    pub fn is_ground(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_ground_like()
    }

    /// Compute 8-bit neighbor bitmask for a cell.
    /// Bit layout: N=0, NE=1, E=2, SE=3, S=4, SW=5, W=6, NW=7.
    fn mask8_at(&self, x: i32, y: i32, base_id: u8) -> u8 {
        let mut m: u8 = 0;
        if self.get_tile(x, y - 1) == base_id { m |= 1; }   // N
        if self.get_tile(x + 1, y - 1) == base_id { m |= 2; }   // NE
        if self.get_tile(x + 1, y) == base_id { m |= 4; }   // E
        if self.get_tile(x + 1, y + 1) == base_id { m |= 8; }   // SE
        if self.get_tile(x, y + 1) == base_id { m |= 16; }  // S
        if self.get_tile(x - 1, y + 1) == base_id { m |= 32; }  // SW
        if self.get_tile(x - 1, y) == base_id { m |= 64; }  // W
        if self.get_tile(x - 1, y - 1) == base_id { m |= 128; } // NW
        m
    }

    /// Recalculate auto-tile visual variants for all tiles.
    /// Supports both legacy 4-bit rules and new 8-bit material rules.
    pub fn recalculate_auto_tiles(&mut self) {
        let has_legacy = !self.auto_tile_rules.is_empty();
        let has_material = !self.material_auto_tile_rules.is_empty();
        println!("[Autotile] recalculate: tiles={}, legacy_rules={}, material_rules={}, has_material={}",
            self.tiles.len(), self.auto_tile_rules.len(), self.material_auto_tile_rules.len(), has_material);
        if !has_legacy && !has_material {
            self.tile_visuals.clear();
            println!("[Autotile] No rules, clearing visuals");
            return;
        }
        if self.tile_visuals.len() != self.tiles.len() {
            self.tile_visuals = vec![0u16; self.tiles.len()];
        } else {
            self.tile_visuals.fill(0);
        }

        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let tile_id = self.get_tile(x, y);
                if tile_id == 0 { continue; }
                let idx = y as usize * self.width + x as usize;

                // 8-bit material autotile rules (higher priority)
                let mut matched = false;
                if has_material {
                    for rule in self.material_auto_tile_rules.values() {
                        if rule.base_tile_id == tile_id {
                            let mask = self.mask8_at(x, y, tile_id);
                            self.tile_visuals[idx] = rule.mask_to_frame[mask as usize];
                            matched = true;
                            break;
                        }
                    }
                }

                // Legacy 4-bit rules (fallback)
                if !matched && has_legacy {
                    for rule in self.auto_tile_rules.values() {
                        if rule.base_tile_id == tile_id {
                            let mut mask: u8 = 0;
                            if self.get_tile(x, y + 1) == tile_id { mask |= 1; } // N
                            if self.get_tile(x, y - 1) == tile_id { mask |= 2; } // S
                            if self.get_tile(x + 1, y) == tile_id { mask |= 4; } // E
                            if self.get_tile(x - 1, y) == tile_id { mask |= 8; } // W
                            self.tile_visuals[idx] = rule.variants.get(&mask).copied().unwrap_or(0) as u16;
                            break;
                        }
                    }
                }
            }
        }
        let nonzero = self.tile_visuals.iter().filter(|&&v| v != 0).count();
        println!("[Autotile] Done: {} tiles, {} non-zero visuals", self.tile_visuals.len(), nonzero);
        for rule in self.material_auto_tile_rules.values() {
            println!("[Autotile]   rule '{}': base_tile_id={}", rule.name, rule.base_tile_id);
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

// ── 8-bit border primitive classification ────────────────────────────

/// Classify an 8-bit neighbor mask into one of 20 border primitive slots.
/// Returns the atlas frame index (0-19) using the extended 20-slot layout:
///   0=fill, 1-4=edge(N/E/S/W), 5-8=outer(NW/NE/SE/SW), 9-12=inner(NW/NE/SE/SW),
///   13-16=endcap(N/E/S/W), 17-18=lane(NS/EW), 19=full_surround
/// Returns 0 (fill) for degenerate cases (orth_count==0, multiple missing diags with orth_count==4).
fn classify_mask8_to_frame(mask8: u8) -> u16 {
    let m = mask8;
    let n  = (m & 1) != 0;
    let ne = (m & 2) != 0;
    let e  = (m & 4) != 0;
    let se = (m & 8) != 0;
    let s  = (m & 16) != 0;
    let sw = (m & 32) != 0;
    let w  = (m & 64) != 0;
    let nw = (m & 128) != 0;
    let orth_count = n as u8 + e as u8 + s as u8 + w as u8;

    if orth_count == 4 {
        // All 4 orthogonal neighbors present — check diagonals
        let missing_diag = (!ne as u8) + (!se as u8) + (!sw as u8) + (!nw as u8);
        if missing_diag == 0 { return 19; } // fully surrounded → full_surround
        if missing_diag == 1 {
            // Inside corner at the missing diagonal position.
            // Atlas: inner_NW=9, inner_NE=10, inner_SE=11, inner_SW=12
            if !nw { return 9; }
            if !ne { return 10; }
            if !se { return 11; }
            if !sw { return 12; }
        }
        return 0; // degenerate (multiple diagonal gaps) → fill
    }

    if orth_count == 3 {
        // Edge: the missing orthogonal direction
        // Atlas: edge_N=1, edge_E=2, edge_S=3, edge_W=4
        if !n { return 1; }
        if !e { return 2; }
        if !s { return 3; }
        if !w { return 4; }
    }

    if orth_count == 2 {
        // Outside corner: two adjacent orthogonal neighbors present
        // Atlas: outer_NW=5, outer_NE=6, outer_SE=7, outer_SW=8
        // Convention from Python: n&&e → outside_corner SW, etc.
        if n && e { return 8; } // outer_SW
        if e && s { return 5; } // outer_NW
        if s && w { return 6; } // outer_NE
        if w && n { return 7; } // outer_SE
        // Opposite pair → lane
        // Atlas: lane_NS=17, lane_EW=18
        if n && s { return 17; } // lane NS
        if e && w { return 18; } // lane EW
    }

    if orth_count == 1 {
        // Endcap: single orthogonal neighbor
        // Atlas: endcap_N=13, endcap_E=14, endcap_S=15, endcap_W=16
        if n { return 13; }
        if e { return 14; }
        if s { return 15; }
        if w { return 16; }
    }

    // orth_count == 0 → isolated tile, use fill
    0
}

/// Build the 256-entry mask8→frame lookup table using the extended 20-slot layout.
/// If custom `slots` are provided, they remap the standard frame indices.
/// `max_columns` clamps any frame index ≥ max_columns back to 0 (fill), providing
/// backward compatibility for 13-slot atlases that don't have endcap/lane/full_surround frames.
pub fn build_mask_to_frame_table(custom_slots: Option<&[u16]>, max_columns: u16) -> Vec<u16> {
    let mut table = vec![0u16; 256];
    for mask in 0u16..256 {
        let base_frame = classify_mask8_to_frame(mask as u8);
        let frame = if let Some(slots) = custom_slots {
            slots.get(base_frame as usize).copied().unwrap_or(base_frame)
        } else {
            base_frame
        };
        // Clamp frames beyond atlas capacity back to fill (slot 0)
        table[mask as usize] = if frame >= max_columns { 0 } else { frame };
    }
    table
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
