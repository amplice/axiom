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

#[derive(Clone)]
struct CollisionView {
    entity: Entity,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    tags: Vec<String>,
    network_id: u64,
    contact_damage: Option<ContactDamage>,
    pickup: Option<Pickup>,
}

#[derive(Clone, Copy)]
struct Aabb {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

type ContactInteractionQueryItem<'a> = (
    Entity,
    &'a GamePosition,
    &'a Collider,
    Option<&'a Tags>,
    &'a NetworkId,
    Option<&'a ContactDamage>,
);

type ProjectileQueryItem<'a> = (
    Entity,
    &'a mut GamePosition,
    &'a mut Projectile,
    Option<&'a Collider>,
    &'a NetworkId,
);

type TargetableEntityItem<'a> = (
    Entity,
    &'a GamePosition,
    &'a Collider,
    Option<&'a Tags>,
    &'a NetworkId,
);

type TargetView = (f32, f32, f32, f32, Vec<String>, u64);

type PickupInteractionQueryItem<'a> = (
    Entity,
    &'a GamePosition,
    &'a Collider,
    Option<&'a Tags>,
    &'a NetworkId,
    Option<&'a Pickup>,
);

type ActorView = (f32, f32, u64, Vec<String>);

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

fn contact_damage_system(
    mut io: InteractionIo,
    spatial: Res<SpatialHash>,
    query: Query<ContactInteractionQueryItem<'_>>,
    mut damage_access: DamageAccess,
) {
    let items: Vec<CollisionView> = query
        .iter()
        .map(|(entity, pos, col, tags, nid, contact)| CollisionView {
            entity,
            x: pos.x,
            y: pos.y,
            w: col.width,
            h: col.height,
            tags: tags
                .map(|t| t.0.iter().cloned().collect())
                .unwrap_or_default(),
            network_id: nid.0,
            contact_damage: contact.cloned(),
            pickup: None,
        })
        .collect();
    let by_entity: std::collections::HashMap<Entity, &CollisionView> =
        items.iter().map(|i| (i.entity, i)).collect();

    for a in &items {
        let candidates = spatial.query_rect(
            a.x - a.w / 2.0,
            a.y - a.h / 2.0,
            a.x + a.w / 2.0,
            a.y + a.h / 2.0,
        );
        for cand in candidates {
            let Some(b) = by_entity.get(&cand).copied() else {
                continue;
            };
            if b.network_id <= a.network_id {
                continue;
            }
            if !overlap(
                Aabb {
                    x: a.x,
                    y: a.y,
                    w: a.w,
                    h: a.h,
                },
                Aabb {
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                },
            ) {
                continue;
            }
            if let Some(cd) = &a.contact_damage {
                if tag_matches(&b.tags, &cd.damage_tag) {
                    apply_damage(
                        &mut io,
                        &mut damage_access,
                        DamageApplication {
                            target: b.entity,
                            source_network_id: a.network_id,
                            damage: cd,
                            knockback_dir_x: b.x - a.x,
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
                            knockback_dir_x: a.x - b.x,
                        },
                    );
                }
            }
        }
    }
}

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
        .map(|(e, pos, col, tags, nid)| {
            (
                e,
                (
                    pos.x,
                    pos.y,
                    col.width,
                    col.height,
                    tags.map(|t| t.0.iter().cloned().collect())
                        .unwrap_or_default(),
                    nid.0,
                ),
            )
        })
        .collect();

    for (entity, mut pos, mut proj, collider, nid) in projectiles.iter_mut() {
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

        let (pw, ph) = collider.map(|c| (c.width, c.height)).unwrap_or((4.0, 4.0));
        let candidates = spatial.query_rect(
            pos.x - pw / 2.0,
            pos.y - ph / 2.0,
            pos.x + pw / 2.0,
            pos.y + ph / 2.0,
        );
        let mut hit_target = None;
        for cand in candidates {
            if cand == entity {
                continue;
            }
            let Some((tx, ty, tw, th, tags, target_nid)) = target_view.get(&cand) else {
                continue;
            };
            if *target_nid == proj.owner_id {
                continue;
            }
            if !tag_matches(tags, &proj.damage_tag) {
                continue;
            }
            if !overlap(
                Aabb {
                    x: pos.x,
                    y: pos.y,
                    w: pw,
                    h: ph,
                },
                Aabb {
                    x: *tx,
                    y: *ty,
                    w: *tw,
                    h: *th,
                },
            ) {
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

fn hitbox_system(
    mut io: InteractionIo,
    spatial: Res<SpatialHash>,
    attackers: Query<(Entity, &GamePosition, &Hitbox, &NetworkId)>,
    targets: Query<TargetableEntityItem<'_>, Without<Hitbox>>,
    mut combat: CombatAccess,
) {
    let target_view: std::collections::HashMap<Entity, TargetView> = targets
        .iter()
        .map(|(e, pos, col, tags, nid)| {
            (
                e,
                (
                    pos.x,
                    pos.y,
                    col.width,
                    col.height,
                    tags.map(|t| t.0.iter().cloned().collect())
                        .unwrap_or_default(),
                    nid.0,
                ),
            )
        })
        .collect();

    for (attacker, pos, hitbox, attacker_nid) in attackers.iter() {
        if !hitbox.active {
            continue;
        }
        let hx = pos.x + hitbox.offset.x;
        let hy = pos.y + hitbox.offset.y;
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
            let Some((tx, ty, tw, th, tags, target_nid)) = target_view.get(&cand) else {
                continue;
            };
            if !tag_matches(tags, &hitbox.damage_tag) {
                continue;
            }
            if attacker_nid.0 == *target_nid {
                continue;
            }
            if !overlap(
                Aabb {
                    x: hx,
                    y: hy,
                    w: hitbox.width,
                    h: hitbox.height,
                },
                Aabb {
                    x: *tx,
                    y: *ty,
                    w: *tw,
                    h: *th,
                },
            ) {
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

fn pickup_system(
    mut io: InteractionIo,
    spatial: Res<SpatialHash>,
    query: Query<PickupInteractionQueryItem<'_>>,
    mut pickup_access: PickupAccess,
) {
    let items: Vec<CollisionView> = query
        .iter()
        .map(|(entity, pos, col, tags, nid, pickup)| CollisionView {
            entity,
            x: pos.x,
            y: pos.y,
            w: col.width,
            h: col.height,
            tags: tags
                .map(|t| t.0.iter().cloned().collect())
                .unwrap_or_default(),
            network_id: nid.0,
            contact_damage: None,
            pickup: pickup.cloned(),
        })
        .collect();
    let by_entity: std::collections::HashMap<Entity, &CollisionView> =
        items.iter().map(|i| (i.entity, i)).collect();

    for a in &items {
        let candidates = spatial.query_rect(
            a.x - a.w / 2.0,
            a.y - a.h / 2.0,
            a.x + a.w / 2.0,
            a.y + a.h / 2.0,
        );
        for cand in candidates {
            let Some(b) = by_entity.get(&cand).copied() else {
                continue;
            };
            if b.network_id <= a.network_id {
                continue;
            }
            if !overlap(
                Aabb {
                    x: a.x,
                    y: a.y,
                    w: a.w,
                    h: a.h,
                },
                Aabb {
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                },
            ) {
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

fn trigger_zone_system(
    mut commands: Commands,
    mut events: ResMut<GameEventBus>,
    spatial: Res<SpatialHash>,
    triggers: Query<(Entity, &GamePosition, &TriggerZone, &NetworkId)>,
    actors: Query<(Entity, &GamePosition, &NetworkId, Option<&Tags>)>,
) {
    let actor_view: std::collections::HashMap<Entity, ActorView> = actors
        .iter()
        .map(|(e, pos, nid, tags)| {
            (
                e,
                (
                    pos.x,
                    pos.y,
                    nid.0,
                    tags.map(|t| t.0.iter().cloned().collect())
                        .unwrap_or_default(),
                ),
            )
        })
        .collect();

    for (trigger_entity, trigger_pos, trigger, nid) in triggers.iter() {
        let mut fired = false;
        let candidates = spatial.query_radius(trigger_pos.x, trigger_pos.y, trigger.radius);
        for actor_entity in candidates {
            if actor_entity == trigger_entity {
                continue;
            }
            let Some((ax, ay, actor_network_id, actor_tags)) = actor_view.get(&actor_entity) else {
                continue;
            };
            if !tag_matches(actor_tags, &trigger.trigger_tag) {
                continue;
            }
            let d2 = (ax - trigger_pos.x).powi(2) + (ay - trigger_pos.y).powi(2);
            if d2 <= trigger.radius * trigger.radius {
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
    mut query: Query<(
        Entity,
        &mut GamePosition,
        &mut Velocity,
        &mut Alive,
        Option<&Player>,
    )>,
) {
    for (entity, mut pos, mut vel, mut alive, player) in query.iter_mut() {
        if alive.0 {
            continue;
        }
        if player.is_some() {
            pos.x = tilemap.player_spawn.0;
            pos.y = tilemap.player_spawn.1;
            vel.x = 0.0;
            vel.y = 0.0;
            alive.0 = true;
        } else {
            commands.entity(entity).despawn();
        }
    }
}

fn overlap(a: Aabb, b: Aabb) -> bool {
    let a_min_x = a.x - a.w / 2.0;
    let a_max_x = a.x + a.w / 2.0;
    let a_min_y = a.y - a.h / 2.0;
    let a_max_y = a.y + a.h / 2.0;
    let b_min_x = b.x - b.w / 2.0;
    let b_max_x = b.x + b.w / 2.0;
    let b_min_y = b.y - b.h / 2.0;
    let b_max_y = b.y + b.h / 2.0;
    a_max_x > b_min_x && a_min_x < b_max_x && a_max_y > b_min_y && a_min_y < b_max_y
}

fn tag_matches(tags: &[String], required: &str) -> bool {
    tags.iter().any(|t| t == required)
}
