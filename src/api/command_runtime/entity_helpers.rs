use super::*;

pub(super) fn apply_tint_to_entity(world: &mut World, entity_id: u64, req: TintRequest) -> Result<(), String> {
    let entity = find_entity_by_network_id(world, entity_id)
        .ok_or_else(|| format!("Entity {} not found", entity_id))?;
    let tint = crate::components::SpriteColorTint {
        color: req.color,
        flash_color: req.flash_color,
        flash_frames: req.flash_frames,
    };
    world.entity_mut(entity).insert(tint);
    Ok(())
}

pub(super) fn apply_trail_to_entity(world: &mut World, entity_id: u64, req: Option<TrailRequest>) -> Result<(), String> {
    let entity = find_entity_by_network_id(world, entity_id)
        .ok_or_else(|| format!("Entity {} not found", entity_id))?;
    match req {
        Some(r) => {
            world.entity_mut(entity).insert(crate::trail::TrailEffect {
                interval: r.interval,
                duration: r.duration,
                alpha_start: r.alpha_start,
                alpha_end: r.alpha_end,
                frame_counter: 0,
            });
        }
        None => {
            world.entity_mut(entity).remove::<crate::trail::TrailEffect>();
        }
    }
    Ok(())
}

pub(super) fn spawn_world_text_command(world: &mut World, req: WorldTextRequest) -> Result<u64, String> {
    let text_id = {
        let mut counter = world.resource_mut::<crate::world_text::WorldTextIdCounter>();
        let id = counter.0;
        counter.0 += 1;
        id
    };
    let z = if req.owner_id.is_some() { 10.5 } else { 10.0 + (-req.y * 0.001) + 0.5 };
    world.spawn((
        crate::world_text::WorldText {
            text_id,
            text: req.text,
            font_size: req.font_size,
            color: req.color,
            offset: Vec2::new(0.0, 0.0),
            owner_entity: req.owner_id,
            duration: req.duration,
            elapsed: 0.0,
            fade: req.fade,
            rise_speed: req.rise_speed,
        },
        Transform::from_xyz(req.x, req.y, z),
    ));
    Ok(text_id)
}

pub(super) fn get_entity_state_machine(world: &mut World, entity_id: u64) -> Option<StateMachineResponse> {
    let entity = find_entity_by_network_id(world, entity_id)?;
    let sm = world.get::<crate::state_machine::EntityStateMachine>(entity)?;
    Some(StateMachineResponse {
        current: sm.current.clone(),
        previous: sm.previous.clone(),
        entered_at_frame: sm.entered_at_frame,
        states: sm.states.keys().cloned().collect(),
    })
}

pub(super) fn transition_entity_state(world: &mut World, entity_id: u64, new_state: String) -> Result<(), String> {
    let entity = find_entity_by_network_id(world, entity_id)
        .ok_or_else(|| format!("Entity {} not found", entity_id))?;
    let frame = world.get_resource::<crate::scripting::vm::ScriptFrame>().map(|f| f.frame).unwrap_or(0);
    // Need to get the state machine and event bus
    let sm = world.get_mut::<crate::state_machine::EntityStateMachine>(entity)
        .ok_or_else(|| format!("Entity {} has no state machine", entity_id))?;
    // Clone to avoid borrow issues
    let mut sm_clone = sm.clone();
    let result = {
        let mut events = world.resource_mut::<GameEventBus>();
        sm_clone.transition(&new_state, entity_id, frame, &mut events)
    };
    if result.is_ok() {
        if let Some(mut sm) = world.get_mut::<crate::state_machine::EntityStateMachine>(entity) {
            *sm = sm_clone;
        }
    }
    result
}

pub(super) fn get_entity_inventory(world: &mut World, entity_id: u64) -> Option<InventoryResponse> {
    let entity = find_entity_by_network_id(world, entity_id)?;
    let inv = world.get::<crate::inventory::Inventory>(entity)?;
    Some(InventoryResponse {
        slots: inv.slots.clone(),
        max_slots: inv.max_slots,
    })
}

pub(super) fn entity_inventory_action(world: &mut World, entity_id: u64, req: InventoryActionRequest) -> Result<(), String> {
    let entity = find_entity_by_network_id(world, entity_id)
        .ok_or_else(|| format!("Entity {} not found", entity_id))?;

    match req.action.as_str() {
        "add" => {
            let item_id = req.item_id.ok_or("item_id required for add")?;
            let registry = world.get_resource::<crate::inventory::ItemRegistry>().cloned().unwrap_or_default();
            let nid = entity_id;
            let mut inv = world.get_mut::<crate::inventory::Inventory>(entity)
                .ok_or_else(|| format!("Entity {} has no inventory", entity_id))?;
            let added = inv.add_item(&item_id, req.count, &registry);
            let new_total = inv.count_item(&item_id);
            drop(inv);
            let mut events = world.resource_mut::<GameEventBus>();
            events.emit("item_added", serde_json::json!({
                "entity": nid, "item_id": item_id, "count": added, "new_total": new_total
            }), Some(nid));
            Ok(())
        }
        "remove" => {
            let item_id = req.item_id.ok_or("item_id required for remove")?;
            let nid = entity_id;
            let mut inv = world.get_mut::<crate::inventory::Inventory>(entity)
                .ok_or_else(|| format!("Entity {} has no inventory", entity_id))?;
            let removed = inv.remove_item(&item_id, req.count);
            let new_total = inv.count_item(&item_id);
            drop(inv);
            let mut events = world.resource_mut::<GameEventBus>();
            events.emit("item_removed", serde_json::json!({
                "entity": nid, "item_id": item_id, "count": removed, "new_total": new_total
            }), Some(nid));
            Ok(())
        }
        "clear" => {
            let mut inv = world.get_mut::<crate::inventory::Inventory>(entity)
                .ok_or_else(|| format!("Entity {} has no inventory", entity_id))?;
            inv.clear();
            Ok(())
        }
        _ => Err(format!("Unknown inventory action: {}", req.action)),
    }
}

pub(super) fn find_entity_by_network_id(world: &mut World, network_id: u64) -> Option<Entity> {
    let mut query = world.query::<(Entity, &NetworkId)>();
    query.iter(world).find(|(_, nid)| nid.0 == network_id).map(|(e, _)| e)
}

#[derive(Default)]
pub(super) struct EntityInfoExtras {
    pub health_current: Option<f32>,
    pub health_max: Option<f32>,
    pub has_contact: bool,
    pub has_trigger: bool,
    pub has_pickup: bool,
    pub has_projectile: bool,
    pub has_hitbox: bool,
    pub has_moving_platform: bool,
    pub has_animation_controller: bool,
    pub has_path_follower: bool,
    pub has_ai_behavior: bool,
    pub has_particle_emitter: bool,
    pub ai_behavior: Option<String>,
    pub ai_state: Option<String>,
    pub ai_target_id: Option<u64>,
    pub path_target: Option<Vec2Def>,
    pub path_len: Option<usize>,
    pub animation_graph: Option<String>,
    pub animation_state: Option<String>,
    pub animation_frame: Option<usize>,
    pub animation_facing_right: Option<bool>,
    pub render_layer: Option<i32>,
    pub collision_layer: Option<u16>,
    pub collision_mask: Option<u16>,
    pub machine_state: Option<String>,
    pub inventory_slots: Option<usize>,
    // Physics diagnostics
    pub coyote_frames: Option<u32>,
    pub jump_buffer_frames: Option<u32>,
    pub invincibility_frames: Option<u32>,
    pub grounded: Option<bool>,
    // Interaction details
    pub contact_damage: Option<f32>,
    pub contact_knockback: Option<f32>,
    pub pickup_effect: Option<String>,
    pub trigger_event: Option<String>,
    pub projectile_damage: Option<f32>,
    pub projectile_speed: Option<f32>,
    pub hitbox_active: Option<bool>,
    pub hitbox_damage: Option<f32>,
    pub visible: Option<bool>,
}

pub(super) struct EntityInfoSource<'a> {
    pub id: u64,
    pub pos: &'a GamePosition,
    pub vel: Option<&'a Velocity>,
    pub collider: Option<&'a Collider>,
    pub player: Option<&'a Player>,
    pub gravity: Option<&'a GravityBody>,
    pub hmover: Option<&'a HorizontalMover>,
    pub jumper: Option<&'a Jumper>,
    pub tdmover: Option<&'a TopDownMover>,
    pub alive: Option<&'a Alive>,
    pub network_id: Option<&'a NetworkId>,
    pub tags: Option<&'a Tags>,
    pub script: Option<&'a LuaScript>,
}

pub(super) fn build_entity_info(source: EntityInfoSource<'_>, extras: EntityInfoExtras) -> EntityInfo {
    let EntityInfoSource {
        id,
        pos,
        vel,
        collider,
        player,
        gravity,
        hmover,
        jumper,
        tdmover,
        alive,
        network_id,
        tags,
        script,
    } = source;
    let mut components = Vec::new();
    if player.is_some() {
        components.push("Player".into());
    }
    if gravity.is_some() {
        components.push("GravityBody".into());
    }
    if collider.is_some() {
        components.push("Collider".into());
    }
    if hmover.is_some() {
        components.push("HorizontalMover".into());
    }
    if jumper.is_some() {
        components.push("Jumper".into());
    }
    if tdmover.is_some() {
        components.push("TopDownMover".into());
    }
    if extras.health_current.is_some() {
        components.push("Health".into());
    }
    if extras.has_contact {
        components.push("ContactDamage".into());
    }
    if extras.has_trigger {
        components.push("TriggerZone".into());
    }
    if extras.has_pickup {
        components.push("Pickup".into());
    }
    if extras.has_projectile {
        components.push("Projectile".into());
    }
    if extras.has_hitbox {
        components.push("Hitbox".into());
    }
    if extras.has_moving_platform {
        components.push("MovingPlatform".into());
    }
    if extras.has_animation_controller {
        components.push("AnimationController".into());
    }
    if extras.has_path_follower {
        components.push("PathFollower".into());
    }
    if extras.has_ai_behavior {
        components.push("AiBehavior".into());
    }
    if extras.has_particle_emitter {
        components.push("ParticleEmitter".into());
    }

    let mut sorted_tags = tags
        .map(|t| t.0.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    sorted_tags.sort();

    EntityInfo {
        id,
        network_id: network_id.map(|n| n.0),
        x: pos.x,
        y: pos.y,
        vx: vel.map_or(0.0, |v| v.x),
        vy: vel.map_or(0.0, |v| v.y),
        components,
        script: script.map(|s| s.script_name.clone()),
        tags: sorted_tags,
        health: extras.health_current,
        max_health: extras.health_max,
        alive: alive.map(|a| a.0),
        ai_behavior: extras.ai_behavior,
        ai_state: extras.ai_state,
        ai_target_id: extras.ai_target_id,
        path_target: extras.path_target,
        path_len: extras.path_len,
        animation_graph: extras.animation_graph,
        animation_state: extras.animation_state,
        animation_frame: extras.animation_frame,
        animation_facing_right: extras.animation_facing_right,
        render_layer: extras.render_layer,
        collision_layer: extras.collision_layer,
        collision_mask: extras.collision_mask,
        machine_state: extras.machine_state,
        inventory_slots: extras.inventory_slots,
        coyote_frames: extras.coyote_frames,
        jump_buffer_frames: extras.jump_buffer_frames,
        invincibility_frames: extras.invincibility_frames,
        grounded: extras.grounded,
        contact_damage: extras.contact_damage,
        contact_knockback: extras.contact_knockback,
        pickup_effect: extras.pickup_effect,
        trigger_event: extras.trigger_event,
        projectile_damage: extras.projectile_damage,
        projectile_speed: extras.projectile_speed,
        hitbox_active: extras.hitbox_active,
        hitbox_damage: extras.hitbox_damage,
        visible: extras.visible,
    }
}
