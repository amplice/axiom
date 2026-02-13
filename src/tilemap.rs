use bevy::prelude::*;
use crate::components::*;

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
    pub fn get(&self, x: i32, y: i32) -> TileType {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return TileType::Empty;
        }
        TileType::from_u8(self.tiles[y as usize * self.width + x as usize])
    }

    pub fn set(&mut self, x: i32, y: i32, tile: TileType) {
        if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
            self.tiles[y as usize * self.width + x as usize] = tile as u8;
        }
    }

    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_solid()
    }

    /// A simple test level for development
    pub fn test_level() -> Self {
        let width = 60;
        let height = 20;
        let mut tiles = vec![0u8; width * height];

        // Ground floor (y=0, bottom row)
        for x in 0..width {
            tiles[0 * width + x] = TileType::Solid as u8;
        }

        // Gap in ground (x=15..18)
        for x in 15..18 {
            tiles[0 * width + x] = TileType::Empty as u8;
        }

        // Spikes at bottom of gap
        // (gap is at y=0, spikes don't make sense at y=0 since it's the floor)
        // Instead, make the gap deeper and add spikes below - but we only have y>=0
        // So let's put spikes at the gap positions as the floor
        for x in 15..18 {
            tiles[0 * width + x] = TileType::Spike as u8;
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
            tiles[1 * width + x] = TileType::Solid as u8;
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
    physics: Res<PhysicsConfig>,
) {
    let ts = physics.tile_size;
    for y in 0..tilemap.height {
        for x in 0..tilemap.width {
            let tile_type = tilemap.get(x as i32, y as i32);
            if tile_type == TileType::Empty {
                continue;
            }

            let color = match tile_type {
                TileType::Solid => Color::srgb(0.4, 0.4, 0.45),
                TileType::Spike => Color::srgb(0.9, 0.15, 0.15),
                TileType::Goal => Color::srgb(0.15, 0.9, 0.3),
                TileType::Empty => unreachable!(),
            };

            commands.spawn((
                TileEntity,
                Tile { tile_type },
                GridPosition { x: x as i32, y: y as i32 },
                Sprite::from_color(color, Vec2::new(ts, ts)),
                Transform::from_xyz(
                    x as f32 * ts + ts / 2.0,
                    y as f32 * ts + ts / 2.0,
                    0.0,
                ),
            ));
        }
    }
}
