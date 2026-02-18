use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::components::*;
use crate::events::GameEventBus;
use crate::spatial_hash::SpatialHash;
use crate::tilemap::Tilemap;

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                entity_collision_system,
                projectile_system,
                trigger_zone_system,
                contact_damage_system,
                hitbox_system,
                pickup_system,
                invincibility_system,
                death_system,
            )
                .chain()
                .run_if(crate::game_runtime::gameplay_systems_enabled),
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CollisionShape: unified shape abstraction
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
enum CollisionShape {
    Aabb { x: f32, y: f32, w: f32, h: f32 },
    Circle { x: f32, y: f32, r: f32 },
}

impl CollisionShape {
    fn bounding_rect(&self) -> (f32, f32, f32, f32) {
        match *self {
            CollisionShape::Aabb { x, y, w, h } => (x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0),
            CollisionShape::Circle { x, y, r } => (x - r, y - r, x + r, y + r),
        }
    }
}

fn extract_shape(
    pos: &GamePosition,
    collider: Option<&Collider>,
    circle: Option<&CircleCollider>,
) -> Option<CollisionShape> {
    if let Some(c) = collider {
        Some(CollisionShape::Aabb {
            x: pos.x,
            y: pos.y,
            w: c.width,
            h: c.height,
        })
    } else if let Some(cc) = circle {
        Some(CollisionShape::Circle {
            x: pos.x,
            y: pos.y,
            r: cc.radius,
        })
    } else {
        None
    }
}

fn shapes_overlap(a: &CollisionShape, b: &CollisionShape) -> bool {
    match (a, b) {
        (
            CollisionShape::Aabb { x: ax, y: ay, w: aw, h: ah },
            CollisionShape::Aabb { x: bx, y: by, w: bw, h: bh },
        ) => {
            let a_min_x = ax - aw / 2.0;
            let a_max_x = ax + aw / 2.0;
            let a_min_y = ay - ah / 2.0;
            let a_max_y = ay + ah / 2.0;
            let b_min_x = bx - bw / 2.0;
            let b_max_x = bx + bw / 2.0;
            let b_min_y = by - bh / 2.0;
            let b_max_y = by + bh / 2.0;
            a_max_x > b_min_x && a_min_x < b_max_x && a_max_y > b_min_y && a_min_y < b_max_y
        }
        (
            CollisionShape::Circle { x: ax, y: ay, r: ar },
            CollisionShape::Circle { x: bx, y: by, r: br },
        ) => {
            let dx = bx - ax;
            let dy = by - ay;
            let sum_r = ar + br;
            dx * dx + dy * dy <= sum_r * sum_r
        }
        (circle @ CollisionShape::Circle { .. }, aabb @ CollisionShape::Aabb { .. }) => {
            circle_aabb_overlap(circle, aabb)
        }
        (aabb @ CollisionShape::Aabb { .. }, circle @ CollisionShape::Circle { .. }) => {
            circle_aabb_overlap(circle, aabb)
        }
    }
}

fn circle_aabb_overlap(circle: &CollisionShape, aabb: &CollisionShape) -> bool {
    let CollisionShape::Circle { x: cx, y: cy, r } = *circle else {
        return false;
    };
    let CollisionShape::Aabb { x, y, w, h } = *aabb else {
        return false;
    };
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    let closest_x = cx.clamp(x - half_w, x + half_w);
    let closest_y = cy.clamp(y - half_h, y + half_h);
    let dx = cx - closest_x;
    let dy = cy - closest_y;
    dx * dx + dy * dy <= r * r
}

fn compute_push_vector(a: &CollisionShape, b: &CollisionShape) -> (f32, f32) {
    match (a, b) {
        (
            CollisionShape::Aabb { x: ax, y: ay, w: aw, h: ah },
            CollisionShape::Aabb { x: bx, y: by, w: bw, h: bh },
        ) => {
            let overlap_x = (aw / 2.0 + bw / 2.0) - (bx - ax).abs();
            let overlap_y = (ah / 2.0 + bh / 2.0) - (by - ay).abs();
            if overlap_x <= 0.0 || overlap_y <= 0.0 {
                return (0.0, 0.0);
            }
            if overlap_x < overlap_y {
                let sign = if bx >= ax { 1.0 } else { -1.0 };
                (sign * overlap_x, 0.0)
            } else {
                let sign = if by >= ay { 1.0 } else { -1.0 };
                (0.0, sign * overlap_y)
            }
        }
        (
            CollisionShape::Circle { x: ax, y: ay, r: ar },
            CollisionShape::Circle { x: bx, y: by, r: br },
        ) => {
            let dx = bx - ax;
            let dy = by - ay;
            let dist = (dx * dx + dy * dy).sqrt();
            let overlap = ar + br - dist;
            if overlap <= 0.0 {
                return (0.0, 0.0);
            }
            if dist < 0.0001 {
                return (overlap, 0.0);
            }
            let nx = dx / dist;
            let ny = dy / dist;
            (nx * overlap, ny * overlap)
        }
        (circle @ CollisionShape::Circle { .. }, aabb @ CollisionShape::Aabb { .. }) => {
            circle_aabb_push(circle, aabb, false)
        }
        (aabb @ CollisionShape::Aabb { .. }, circle @ CollisionShape::Circle { .. }) => {
            circle_aabb_push(circle, aabb, true)
        }
    }
}

fn circle_aabb_push(
    circle: &CollisionShape,
    aabb: &CollisionShape,
    invert: bool,
) -> (f32, f32) {
    let CollisionShape::Circle { x: cx, y: cy, r } = *circle else {
        return (0.0, 0.0);
    };
    let CollisionShape::Aabb { x, y, w, h } = *aabb else {
        return (0.0, 0.0);
    };
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    let closest_x = cx.clamp(x - half_w, x + half_w);
    let closest_y = cy.clamp(y - half_h, y + half_h);
    let dx = cx - closest_x;
    let dy = cy - closest_y;
    let dist = (dx * dx + dy * dy).sqrt();
    let overlap = r - dist;
    if overlap <= 0.0 {
        return (0.0, 0.0);
    }
    let (nx, ny) = if dist < 0.0001 {
        (1.0, 0.0)
    } else {
        (dx / dist, dy / dist)
    };
    // Push vector points from a→b; circle pushes away from aabb
    // If circle is `a`, push = circle-away = (nx, ny) direction
    // If aabb is `a` (invert=true), push direction reverses
    let sign = if invert { -1.0 } else { 1.0 };
    (sign * nx * overlap, sign * ny * overlap)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Shared helpers and types
// ═══════════════════════════════════════════════════════════════════════════════

fn collision_layers_compatible(
    a: Option<&crate::components::CollisionLayer>,
    b: Option<&crate::components::CollisionLayer>,
) -> bool {
    let a = a.copied().unwrap_or_default();
    let b = b.copied().unwrap_or_default();
    (a.layer & b.mask) != 0 && (b.layer & a.mask) != 0
}

#[derive(Clone)]
struct CollisionView {
    entity: Entity,
    shape: CollisionShape,
    tags: Vec<String>,
    network_id: u64,
    contact_damage: Option<ContactDamage>,
    pickup: Option<Pickup>,
    collision_layer: Option<crate::components::CollisionLayer>,
}

type ContactInteractionQueryItem<'a> = (
    Entity,
    &'a GamePosition,
    Option<&'a Collider>,
    Option<&'a CircleCollider>,
    Option<&'a Tags>,
    &'a NetworkId,
    Option<&'a ContactDamage>,
    Option<&'a crate::components::CollisionLayer>,
);

type ProjectileQueryItem<'a> = (
    Entity,
    &'a mut GamePosition,
    &'a mut Projectile,
    Option<&'a Collider>,
    Option<&'a CircleCollider>,
    &'a NetworkId,
    Option<&'a crate::components::CollisionLayer>,
);

type TargetableEntityItem<'a> = (
    Entity,
    &'a GamePosition,
    Option<&'a Collider>,
    Option<&'a CircleCollider>,
    Option<&'a Tags>,
    &'a NetworkId,
    Option<&'a crate::components::CollisionLayer>,
);

type TargetView = (CollisionShape, Vec<String>, u64, Option<crate::components::CollisionLayer>);

type PickupInteractionQueryItem<'a> = (
    Entity,
    &'a GamePosition,
    Option<&'a Collider>,
    Option<&'a CircleCollider>,
    Option<&'a Tags>,
    &'a NetworkId,
    Option<&'a Pickup>,
    Option<&'a crate::components::CollisionLayer>,
);

type ActorView = (f32, f32, u64, Vec<String>, Option<CollisionShape>);

#[derive(SystemParam)]
struct InteractionIo<'w, 's> {
    commands: Commands<'w, 's>,
    events: ResMut<'w, GameEventBus>,
}

#[derive(SystemParam)]
struct DamageAccess<'w, 's> {
    health_q: Query<'w, 's, &'static mut Health>,
    vel_q: Query<'w, 's, &'static mut Velocity>,
    alive_q: Query<'w, 's, &'static mut Alive>,
    inv_q: Query<'w, 's, &'static Invincibility>,
    network_q: Query<'w, 's, &'static NetworkId>,
}

#[derive(SystemParam)]
struct CombatAccess<'w, 's> {
    health_q: Query<'w, 's, &'static mut Health>,
    alive_q: Query<'w, 's, &'static mut Alive>,
    inv_q: Query<'w, 's, &'static Invincibility>,
}

#[derive(SystemParam)]
struct PickupAccess<'w, 's> {
    health_q: Query<'w, 's, &'static mut Health>,
}

#[derive(SystemParam)]
struct InteractionPhysics<'w> {
    time: Res<'w, Time<Fixed>>,
    tilemap: Res<'w, Tilemap>,
    config: Res<'w, GameConfig>,
}

struct DamageApplication<'a> {
    target: Entity,
    source_network_id: u64,
    damage: &'a ContactDamage,
    knockback_dir_x: f32,
}

struct PickupApplication<'a> {
    pickup: &'a Pickup,
    pickup_entity: Entity,
    collector_entity: Entity,
    pickup_network_id: u64,
    collector_network_id: u64,
}

// ═══════════════════════════════════════════════════════════════════════════════
// entity_collision_system — SolidBody push-back
// ═══════════════════════════════════════════════════════════════════════════════

fn entity_collision_system(
    spatial: Res<SpatialHash>,
    mut positions: Query<(&mut GamePosition, Option<&Collider>, Option<&CircleCollider>)>,
    solid_entities: Query<(Entity, Option<&crate::components::CollisionLayer>), With<SolidBody>>,
) {
    // Snapshot all SolidBody entities (read positions immutably via get())
    struct SolidEntry {
        entity: Entity,
        shape: CollisionShape,
        collision_layer: Option<crate::components::CollisionLayer>,
    }
    let solids: Vec<SolidEntry> = solid_entities
        .iter()
        .filter_map(|(entity, cl)| {
            let (pos, col, circle) = positions.get(entity).ok()?;
            let shape = extract_shape(&pos, col, circle)?;
            Some(SolidEntry {
                entity,
                shape,
                collision_layer: cl.copied(),
            })
        })
        .collect();

    if solids.len() < 2 {
        return;
    }

    // Build entity→index lookup
    let entity_to_idx: std::collections::HashMap<Entity, usize> = solids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.entity, i))
        .collect();

    // Accumulate displacements
    let mut displacements: std::collections::HashMap<Entity, (f32, f32)> =
        std::collections::HashMap::new();

    for (i, a) in solids.iter().enumerate() {
        let (min_x, min_y, max_x, max_y) = a.shape.bounding_rect();
        let candidates = spatial.query_rect(min_x, min_y, max_x, max_y);
        for cand in candidates {
            let Some(&j) = entity_to_idx.get(&cand) else {
                continue;
            };
            if j <= i {
                continue; // dedupe: only process pair once
            }
            let b = &solids[j];
            if !collision_layers_compatible(
                a.collision_layer.as_ref(),
                b.collision_layer.as_ref(),
            ) {
                continue;
            }
            if !shapes_overlap(&a.shape, &b.shape) {
                continue;
            }
            let (px, py) = compute_push_vector(&a.shape, &b.shape);
            // Split 50/50: a moves -half, b moves +half
            let half_x = px * 0.5;
            let half_y = py * 0.5;
            {
                let d = displacements.entry(a.entity).or_insert((0.0, 0.0));
                d.0 -= half_x;
                d.1 -= half_y;
            }
            {
                let d = displacements.entry(b.entity).or_insert((0.0, 0.0));
                d.0 += half_x;
                d.1 += half_y;
            }
        }
    }

    // Apply all displacements
    for (entity, (dx, dy)) in displacements {
        if let Ok((mut pos, _, _)) = positions.get_mut(entity) {
            pos.x += dx;
            pos.y += dy;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// contact_damage_system
// ═══════════════════════════════════════════════════════════════════════════════

fn contact_damage_system(
    mut io: InteractionIo,
    spatial: Res<SpatialHash>,
    query: Query<ContactInteractionQueryItem<'_>>,
    mut damage_access: DamageAccess,
) {
    let items: Vec<CollisionView> = query
        .iter()
        .filter_map(|(entity, pos, col, circle, tags, nid, contact, cl)| {
            let shape = extract_shape(pos, col, circle)?;
            Some(CollisionView {
                entity,
                shape,
                tags: tags
                    .map(|t| t.0.iter().cloned().collect())
                    .unwrap_or_default(),
                network_id: nid.0,
                contact_damage: contact.cloned(),
                pickup: None,
                collision_layer: cl.copied(),
            })
        })
        .collect();
    let by_entity: std::collections::HashMap<Entity, &CollisionView> =
        items.iter().map(|i| (i.entity, i)).collect();

    for a in &items {
        let (min_x, min_y, max_x, max_y) = a.shape.bounding_rect();
        let candidates = spatial.query_rect(min_x, min_y, max_x, max_y);
        for cand in candidates {
            let Some(b) = by_entity.get(&cand).copied() else {
                continue;
            };
            if b.network_id <= a.network_id {
                continue;
            }
            if !shapes_overlap(&a.shape, &b.shape) {
                continue;
            }
            if !collision_layers_compatible(a.collision_layer.as_ref(), b.collision_layer.as_ref()) {
                continue;
            }
            let (a_cx, b_cx) = match (&a.shape, &b.shape) {
                (CollisionShape::Aabb { x: ax, .. }, CollisionShape::Aabb { x: bx, .. }) => (*ax, *bx),
                (CollisionShape::Circle { x: ax, .. }, CollisionShape::Circle { x: bx, .. }) => (*ax, *bx),
                (CollisionShape::Aabb { x: ax, .. }, CollisionShape::Circle { x: bx, .. }) => (*ax, *bx),
                (CollisionShape::Circle { x: ax, .. }, CollisionShape::Aabb { x: bx, .. }) => (*ax, *bx),
            };
            if let Some(cd) = &a.contact_damage {
                if tag_matches(&b.tags, &cd.damage_tag) {
                    apply_damage(
                        &mut io,
                        &mut damage_access,
                        DamageApplication {
                            target: b.entity,
                            source_network_id: a.network_id,
                            damage: cd,
                            knockback_dir_x: b_cx - a_cx,
                        },
                    );
                }
            }
            if let Some(cd) = &b.contact_damage {
                if tag_matches(&a.tags, &cd.damage_tag) {
                    apply_damage(
                        &mut io,
                        &mut damage_access,
                        DamageApplication {
                            target: a.entity,
                            source_network_id: b.network_id,
                            damage: cd,
                            knockback_dir_x: a_cx - b_cx,
                        },
                    );
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// projectile_system
// ═══════════════════════════════════════════════════════════════════════════════

fn projectile_system(
    mut io: InteractionIo,
    physics: InteractionPhysics,
    spatial: Res<SpatialHash>,
    mut projectiles: Query<ProjectileQueryItem<'_>>,
    targets: Query<TargetableEntityItem<'_>, Without<Projectile>>,
    mut combat: CombatAccess,
) {
    let dt = physics.time.delta_secs();
    let ts = physics.config.tile_size;

    let target_view: std::collections::HashMap<Entity, TargetView> = targets
        .iter()
        .filter_map(|(e, pos, col, circle, tags, nid, cl)| {
            let shape = extract_shape(pos, col, circle)?;
            Some((
                e,
                (
                    shape,
                    tags.map(|t| t.0.iter().cloned().collect())
                        .unwrap_or_default(),
                    nid.0,
                    cl.copied(),
                ),
            ))
        })
        .collect();

    for (entity, mut pos, mut proj, collider, circle, nid, proj_cl) in projectiles.iter_mut() {
        let dir = if proj.direction.length_squared() > 0.0 {
            proj.direction.normalize()
        } else {
            Vec2::X
        };
        pos.x += dir.x * proj.speed * dt;
        pos.y += dir.y * proj.speed * dt;

        if proj.lifetime_frames > 0 {
            proj.lifetime_frames -= 1;
        }
        if proj.lifetime_frames == 0 {
            io.events.emit(
                "projectile_expired",
                serde_json::json!({ "projectile": nid.0 }),
                Some(nid.0),
            );
            io.commands.entity(entity).despawn();
            continue;
        }

        let tx = (pos.x / ts).floor() as i32;
        let ty = (pos.y / ts).floor() as i32;
        if physics.tilemap.is_solid(tx, ty) {
            io.events.emit(
                "projectile_hit_wall",
                serde_json::json!({ "projectile": nid.0, "tile": [tx, ty] }),
                Some(nid.0),
            );
            io.commands.entity(entity).despawn();
            continue;
        }

        // Projectile shape: use its own collider, or 4x4 point AABB
        let proj_shape = extract_shape(&pos, collider, circle).unwrap_or(
            CollisionShape::Aabb {
                x: pos.x,
                y: pos.y,
                w: 4.0,
                h: 4.0,
            },
        );
        let (min_x, min_y, max_x, max_y) = proj_shape.bounding_rect();
        let candidates = spatial.query_rect(min_x, min_y, max_x, max_y);
        let mut hit_target = None;
        for cand in candidates {
            if cand == entity {
                continue;
            }
            let Some((target_shape, tags, target_nid, target_cl)) = target_view.get(&cand) else {
                continue;
            };
            if *target_nid == proj.owner_id {
                continue;
            }
            if !collision_layers_compatible(proj_cl, target_cl.as_ref()) {
                continue;
            }
            if !tag_matches(tags, &proj.damage_tag) {
                continue;
            }
            if !shapes_overlap(&proj_shape, target_shape) {
                continue;
            }
            hit_target = Some((cand, *target_nid));
            break;
        }

        if let Some((target, target_nid)) = hit_target {
            if let Ok(inv) = combat.inv_q.get(target) {
                if inv.frames_remaining > 0 {
                    io.commands.entity(entity).despawn();
                    continue;
                }
            }
            if let Ok(mut health) = combat.health_q.get_mut(target) {
                health.current -= proj.damage;
                io.events.emit(
                    "projectile_hit",
                    serde_json::json!({
                        "projectile": nid.0,
                        "target": target_nid,
                        "damage": proj.damage,
                        "health": health.current.max(0.0),
                    }),
                    Some(nid.0),
                );
                if health.current <= 0.0 {
                    if let Ok(mut alive) = combat.alive_q.get_mut(target) {
                        alive.0 = false;
                    }
                }
            }
            io.commands.entity(entity).despawn();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// hitbox_system
// ═══════════════════════════════════════════════════════════════════════════════

fn hitbox_system(
    mut io: InteractionIo,
    spatial: Res<SpatialHash>,
    attackers: Query<(Entity, &GamePosition, &Hitbox, &NetworkId, Option<&crate::components::CollisionLayer>)>,
    targets: Query<TargetableEntityItem<'_>>,
    mut combat: CombatAccess,
) {
    let target_view: std::collections::HashMap<Entity, TargetView> = targets
        .iter()
        .filter_map(|(e, pos, col, circle, tags, nid, cl)| {
            let shape = extract_shape(pos, col, circle)?;
            Some((
                e,
                (
                    shape,
                    tags.map(|t| t.0.iter().cloned().collect())
                        .unwrap_or_default(),
                    nid.0,
                    cl.copied(),
                ),
            ))
        })
        .collect();

    for (attacker, pos, hitbox, attacker_nid, attacker_cl) in attackers.iter() {
        if !hitbox.active {
            continue;
        }
        let hx = pos.x + hitbox.offset.x;
        let hy = pos.y + hitbox.offset.y;
        let hitbox_shape = CollisionShape::Aabb {
            x: hx,
            y: hy,
            w: hitbox.width,
            h: hitbox.height,
        };
        let candidates = spatial.query_rect(
            hx - hitbox.width / 2.0,
            hy - hitbox.height / 2.0,
            hx + hitbox.width / 2.0,
            hy + hitbox.height / 2.0,
        );
        for cand in candidates {
            if cand == attacker {
                continue;
            }
            let Some((target_shape, tags, target_nid, target_cl)) = target_view.get(&cand) else {
                continue;
            };
            if !collision_layers_compatible(attacker_cl, target_cl.as_ref()) {
                continue;
            }
            if !tag_matches(tags, &hitbox.damage_tag) {
                continue;
            }
            if attacker_nid.0 == *target_nid {
                continue;
            }
            if !shapes_overlap(&hitbox_shape, target_shape) {
                continue;
            }

            if let Ok(inv) = combat.inv_q.get(cand) {
                if inv.frames_remaining > 0 {
                    continue;
                }
            }

            if let Ok(mut health) = combat.health_q.get_mut(cand) {
                health.current -= hitbox.damage;
                io.commands.entity(cand).insert(Invincibility {
                    frames_remaining: 8,
                });
                io.events.emit(
                    "hitbox_hit",
                    serde_json::json!({
                        "attacker": attacker_nid.0,
                        "target": target_nid,
                        "damage": hitbox.damage,
                        "health": health.current.max(0.0),
                    }),
                    Some(attacker_nid.0),
                );
                if health.current <= 0.0 {
                    if let Ok(mut alive) = combat.alive_q.get_mut(cand) {
                        alive.0 = false;
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// apply_damage
// ═══════════════════════════════════════════════════════════════════════════════

fn apply_damage(
    io: &mut InteractionIo<'_, '_>,
    access: &mut DamageAccess<'_, '_>,
    app: DamageApplication<'_>,
) {
    if let Ok(inv) = access.inv_q.get(app.target) {
        if inv.frames_remaining > 0 {
            return;
        }
    }

    let Ok(mut health) = access.health_q.get_mut(app.target) else {
        return;
    };
    let Ok(target_network_id) = access.network_q.get(app.target).map(|n| n.0) else {
        return;
    };

    health.current -= app.damage.amount;
    if let Ok(mut vel) = access.vel_q.get_mut(app.target) {
        vel.x += app.knockback_dir_x.signum() * app.damage.knockback;
    }
    io.commands.entity(app.target).insert(Invincibility {
        frames_remaining: app.damage.cooldown_frames,
    });

    io.events.emit(
        "entity_damaged",
        serde_json::json!({
            "target": target_network_id,
            "amount": app.damage.amount,
            "health": health.current.max(0.0),
        }),
        Some(app.source_network_id),
    );

    if health.current <= 0.0 {
        if let Ok(mut alive) = access.alive_q.get_mut(app.target) {
            alive.0 = false;
        }
        io.events.emit(
            "entity_died",
            serde_json::json!({
                "target": target_network_id,
            }),
            Some(app.source_network_id),
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// pickup_system
// ═══════════════════════════════════════════════════════════════════════════════

fn pickup_system(
    mut io: InteractionIo,
    spatial: Res<SpatialHash>,
    query: Query<PickupInteractionQueryItem<'_>>,
    mut pickup_access: PickupAccess,
) {
    let items: Vec<CollisionView> = query
        .iter()
        .filter_map(|(entity, pos, col, circle, tags, nid, pickup, cl)| {
            let shape = extract_shape(pos, col, circle)?;
            Some(CollisionView {
                entity,
                shape,
                tags: tags
                    .map(|t| t.0.iter().cloned().collect())
                    .unwrap_or_default(),
                network_id: nid.0,
                contact_damage: None,
                pickup: pickup.cloned(),
                collision_layer: cl.copied(),
            })
        })
        .collect();
    let by_entity: std::collections::HashMap<Entity, &CollisionView> =
        items.iter().map(|i| (i.entity, i)).collect();

    for a in &items {
        let (min_x, min_y, max_x, max_y) = a.shape.bounding_rect();
        let candidates = spatial.query_rect(min_x, min_y, max_x, max_y);
        for cand in candidates {
            let Some(b) = by_entity.get(&cand).copied() else {
                continue;
            };
            if b.network_id <= a.network_id {
                continue;
            }
            if !shapes_overlap(&a.shape, &b.shape) {
                continue;
            }
            if !collision_layers_compatible(a.collision_layer.as_ref(), b.collision_layer.as_ref()) {
                continue;
            }
            if let Some(pickup) = &a.pickup {
                if tag_matches(&b.tags, &pickup.pickup_tag) {
                    apply_pickup(
                        &mut io,
                        &mut pickup_access,
                        PickupApplication {
                            pickup,
                            pickup_entity: a.entity,
                            collector_entity: b.entity,
                            pickup_network_id: a.network_id,
                            collector_network_id: b.network_id,
                        },
                    );
                }
            }
            if let Some(pickup) = &b.pickup {
                if tag_matches(&a.tags, &pickup.pickup_tag) {
                    apply_pickup(
                        &mut io,
                        &mut pickup_access,
                        PickupApplication {
                            pickup,
                            pickup_entity: b.entity,
                            collector_entity: a.entity,
                            pickup_network_id: b.network_id,
                            collector_network_id: a.network_id,
                        },
                    );
                }
            }
        }
    }
}

fn apply_pickup(
    io: &mut InteractionIo<'_, '_>,
    access: &mut PickupAccess<'_, '_>,
    app: PickupApplication<'_>,
) {
    match &app.pickup.effect {
        PickupEffect::Heal(amount) => {
            if let Ok(mut health) = access.health_q.get_mut(app.collector_entity) {
                health.current = (health.current + *amount).min(health.max);
            }
            io.events.emit(
                "pickup_collected",
                serde_json::json!({
                    "pickup": app.pickup_network_id,
                    "collector": app.collector_network_id,
                    "effect": "heal",
                    "amount": amount,
                }),
                Some(app.pickup_network_id),
            );
        }
        PickupEffect::ScoreAdd(amount) => {
            io.events.emit(
                "pickup_collected",
                serde_json::json!({
                    "pickup": app.pickup_network_id,
                    "collector": app.collector_network_id,
                    "effect": "score_add",
                    "amount": amount,
                }),
                Some(app.pickup_network_id),
            );
        }
        PickupEffect::Custom(name) => {
            io.events.emit(
                "pickup_collected",
                serde_json::json!({
                    "pickup": app.pickup_network_id,
                    "collector": app.collector_network_id,
                    "effect": "custom",
                    "name": name,
                }),
                Some(app.pickup_network_id),
            );
        }
    }
    io.commands.entity(app.pickup_entity).despawn();
}

// ═══════════════════════════════════════════════════════════════════════════════
// trigger_zone_system
// ═══════════════════════════════════════════════════════════════════════════════

fn trigger_zone_system(
    mut commands: Commands,
    mut events: ResMut<GameEventBus>,
    spatial: Res<SpatialHash>,
    triggers: Query<(Entity, &GamePosition, &TriggerZone, &NetworkId)>,
    actors: Query<(
        Entity,
        &GamePosition,
        &NetworkId,
        Option<&Tags>,
        Option<&Collider>,
        Option<&CircleCollider>,
    )>,
) {
    let actor_view: std::collections::HashMap<Entity, ActorView> = actors
        .iter()
        .map(|(e, pos, nid, tags, col, circle)| {
            let shape = extract_shape(pos, col, circle);
            (
                e,
                (
                    pos.x,
                    pos.y,
                    nid.0,
                    tags.map(|t| t.0.iter().cloned().collect())
                        .unwrap_or_default(),
                    shape,
                ),
            )
        })
        .collect();

    for (trigger_entity, trigger_pos, trigger, nid) in triggers.iter() {
        let trigger_shape = CollisionShape::Circle {
            x: trigger_pos.x,
            y: trigger_pos.y,
            r: trigger.radius,
        };
        let mut fired = false;
        let candidates = spatial.query_radius(trigger_pos.x, trigger_pos.y, trigger.radius);
        for actor_entity in candidates {
            if actor_entity == trigger_entity {
                continue;
            }
            let Some((ax, ay, actor_network_id, actor_tags, actor_shape)) =
                actor_view.get(&actor_entity)
            else {
                continue;
            };
            if !tag_matches(actor_tags, &trigger.trigger_tag) {
                continue;
            }
            // Use shape-based overlap if actor has a collider, else point-in-circle
            let overlaps = if let Some(shape) = actor_shape {
                shapes_overlap(&trigger_shape, shape)
            } else {
                let d2 = (ax - trigger_pos.x).powi(2) + (ay - trigger_pos.y).powi(2);
                d2 <= trigger.radius * trigger.radius
            };
            if overlaps {
                events.emit(
                    trigger.event_name.clone(),
                    serde_json::json!({
                        "trigger": nid.0,
                        "actor": actor_network_id,
                    }),
                    Some(nid.0),
                );
                fired = true;
            }
        }
        if fired && trigger.one_shot {
            commands.entity(trigger_entity).despawn();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// invincibility_system & death_system
// ═══════════════════════════════════════════════════════════════════════════════

fn invincibility_system(mut commands: Commands, mut query: Query<(Entity, &mut Invincibility)>) {
    for (entity, mut inv) in query.iter_mut() {
        if inv.frames_remaining > 0 {
            inv.frames_remaining -= 1;
        }
        if inv.frames_remaining == 0 {
            commands.entity(entity).remove::<Invincibility>();
        }
    }
}

fn death_system(
    mut commands: Commands,
    tilemap: Res<Tilemap>,
    frame: Res<crate::scripting::vm::ScriptFrame>,
    mut query: Query<(
        Entity,
        &mut GamePosition,
        &mut Velocity,
        &mut Alive,
        Option<&mut Health>,
        Option<&Player>,
        Option<&PendingDeath>,
    )>,
) {
    for (entity, mut pos, mut vel, mut alive, health, player, pending) in query.iter_mut() {
        if alive.0 {
            continue;
        }
        // Already marked for deferred death — skip
        if pending.is_some() {
            continue;
        }
        if player.is_some() {
            // Respawn player at spawn point with full health
            pos.x = tilemap.player_spawn.0;
            pos.y = tilemap.player_spawn.1;
            vel.x = 0.0;
            vel.y = 0.0;
            if let Some(mut health) = health {
                health.current = health.max;
            }
            alive.0 = true;
        } else {
            // Mark for deferred death so scripts can react via on_death()
            commands.entity(entity).insert(PendingDeath {
                frame_marked: frame.frame,
            });
        }
    }
}

fn tag_matches(tags: &[String], required: &str) -> bool {
    tags.iter().any(|t| t == required)
}
