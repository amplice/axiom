use crate::components::*;
use crate::sprites::SpriteAssets;
use bevy::prelude::*;

pub struct TilemapPlugin;

impl Plugin for TilemapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Tilemap::test_level())
            .add_systems(Startup, spawn_tilemap);
    }
}

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct Tilemap {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub player_spawn: (f32, f32),
    pub goal: Option<(i32, i32)>,
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
        }
    }
}

/// Marker for tile visual entities (so we can despawn them when reloading)
#[derive(Component)]
pub struct TileEntity;

fn spawn_tilemap(
    mut commands: Commands,
    tilemap: Res<Tilemap>,
    physics: Res<GameConfig>,
    headless: Res<HeadlessMode>,
    sprite_assets: Option<Res<SpriteAssets>>,
) {
    if headless.0 {
        return;
    } // No visual tiles in headless mode
    let ts = physics.tile_size;
    for y in 0..tilemap.height {
        for x in 0..tilemap.width {
            let tile_type = tilemap.get(x as i32, y as i32);
            if tile_type == TileType::Empty {
                continue;
            }

            let sprite = if let Some(ref sa) = sprite_assets {
                if let Some(handle) = sa.get_tile(tile_type) {
                    Sprite {
                        image: handle.clone(),
                        custom_size: Some(Vec2::new(ts, ts)),
                        ..default()
                    }
                } else {
                    tile_color_sprite(tile_type, ts)
                }
            } else {
                tile_color_sprite(tile_type, ts)
            };

            commands.spawn((
                TileEntity,
                Tile { tile_type },
                GridPosition {
                    x: x as i32,
                    y: y as i32,
                },
                sprite,
                Transform::from_xyz(x as f32 * ts + ts / 2.0, y as f32 * ts + ts / 2.0, 0.0),
            ));
        }
    }
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
        TileType::Empty => Color::NONE,
    };
    Sprite::from_color(color, Vec2::new(ts, ts))
}
