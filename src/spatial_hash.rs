use std::collections::HashMap;

use bevy::prelude::*;

use crate::components::{Collider, GamePosition};

type SpatialChangedItem<'a> = (Entity, &'a GamePosition, &'a Collider);
type SpatialChangedFilter = Or<(
    Changed<GamePosition>,
    Changed<Collider>,
    Added<GamePosition>,
    Added<Collider>,
)>;

#[derive(Resource, Default)]
pub struct SpatialHash {
    pub cell_size: f32,
    pub cells: HashMap<(i32, i32), Vec<Entity>>,
}

#[derive(Resource, Default)]
struct SpatialOccupancy {
    entity_cells: HashMap<Entity, Vec<(i32, i32)>>,
}

impl SpatialHash {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    pub fn query_radius(&self, x: f32, y: f32, radius: f32) -> Vec<Entity> {
        let min_x = ((x - radius) / self.cell_size).floor() as i32;
        let max_x = ((x + radius) / self.cell_size).floor() as i32;
        let min_y = ((y - radius) / self.cell_size).floor() as i32;
        let max_y = ((y + radius) / self.cell_size).floor() as i32;
        let mut out = Vec::new();
        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                if let Some(entities) = self.cells.get(&(cx, cy)) {
                    out.extend_from_slice(entities);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn query_rect(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Vec<Entity> {
        let min_cx = (min_x / self.cell_size).floor() as i32;
        let max_cx = (max_x / self.cell_size).floor() as i32;
        let min_cy = (min_y / self.cell_size).floor() as i32;
        let max_cy = (max_y / self.cell_size).floor() as i32;
        let mut out = Vec::new();
        for cy in min_cy..=max_cy {
            for cx in min_cx..=max_cx {
                if let Some(entities) = self.cells.get(&(cx, cy)) {
                    out.extend_from_slice(entities);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

pub struct SpatialHashPlugin;

impl Plugin for SpatialHashPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SpatialHash::new(64.0))
            .insert_resource(SpatialOccupancy::default())
            .add_systems(
                FixedPreUpdate,
                rebuild_spatial_hash.run_if(crate::game_runtime::gameplay_systems_enabled),
            );
    }
}

fn rebuild_spatial_hash(
    mut hash: ResMut<SpatialHash>,
    mut occupancy: ResMut<SpatialOccupancy>,
    changed: Query<SpatialChangedItem<'_>, SpatialChangedFilter>,
    mut removed_pos: RemovedComponents<GamePosition>,
    mut removed_col: RemovedComponents<Collider>,
) {
    let mut removed_entities = std::collections::HashSet::new();
    for entity in removed_pos.read() {
        removed_entities.insert(entity);
    }
    for entity in removed_col.read() {
        removed_entities.insert(entity);
    }

    if changed.is_empty() && removed_entities.is_empty() {
        return;
    }

    for entity in removed_entities {
        remove_entity_cells(&mut hash, &mut occupancy, entity);
    }

    for (entity, pos, collider) in changed.iter() {
        let cells = compute_covered_cells(pos, collider, hash.cell_size);
        upsert_entity_cells(&mut hash, &mut occupancy, entity, cells);
    }
}

fn compute_covered_cells(
    pos: &GamePosition,
    collider: &Collider,
    cell_size: f32,
) -> Vec<(i32, i32)> {
    let min_x = ((pos.x - collider.width / 2.0) / cell_size).floor() as i32;
    let max_x = ((pos.x + collider.width / 2.0) / cell_size).floor() as i32;
    let min_y = ((pos.y - collider.height / 2.0) / cell_size).floor() as i32;
    let max_y = ((pos.y + collider.height / 2.0) / cell_size).floor() as i32;
    let mut out = Vec::new();
    for cy in min_y..=max_y {
        for cx in min_x..=max_x {
            out.push((cx, cy));
        }
    }
    out
}

fn remove_entity_cells(hash: &mut SpatialHash, occupancy: &mut SpatialOccupancy, entity: Entity) {
    let Some(old_cells) = occupancy.entity_cells.remove(&entity) else {
        return;
    };
    for cell in old_cells {
        if let Some(list) = hash.cells.get_mut(&cell) {
            list.retain(|e| *e != entity);
            if list.is_empty() {
                hash.cells.remove(&cell);
            }
        }
    }
}

fn upsert_entity_cells(
    hash: &mut SpatialHash,
    occupancy: &mut SpatialOccupancy,
    entity: Entity,
    new_cells: Vec<(i32, i32)>,
) {
    if let Some(old_cells) = occupancy.entity_cells.get(&entity) {
        if *old_cells == new_cells {
            return;
        }
    }
    remove_entity_cells(hash, occupancy, entity);
    for cell in &new_cells {
        hash.cells.entry(*cell).or_default().push(entity);
    }
    occupancy.entity_cells.insert(entity, new_cells);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_updates_move_entity_between_cells() {
        let mut hash = SpatialHash::new(16.0);
        let mut occupancy = SpatialOccupancy::default();
        let entity = Entity::from_raw(42);
        let collider = Collider {
            width: 8.0,
            height: 8.0,
        };

        let cells_a = compute_covered_cells(&GamePosition { x: 8.0, y: 8.0 }, &collider, 16.0);
        upsert_entity_cells(&mut hash, &mut occupancy, entity, cells_a);
        assert!(hash.cells.contains_key(&(0, 0)));

        let cells_b = compute_covered_cells(&GamePosition { x: 40.0, y: 8.0 }, &collider, 16.0);
        upsert_entity_cells(&mut hash, &mut occupancy, entity, cells_b);

        assert!(!hash
            .cells
            .get(&(0, 0))
            .is_some_and(|entities| entities.contains(&entity)));
        assert!(hash
            .cells
            .get(&(2, 0))
            .is_some_and(|entities| entities.contains(&entity)));
    }
}
