use crate::components::*;
use crate::input::LocalPlayerId;
use crate::input::VirtualInput;
use crate::perf::PerfAccum;
use crate::physics_core::{self, PhysicsCounters};
use crate::tilemap::Tilemap;
use bevy::prelude::*;
use bevy::utils::Instant;

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                moving_platform_system,
                ladder_movement,
                apply_gravity,
                horizontal_movement,
                top_down_movement,
                jump_system,
                apply_drag,
                apply_knockback_impulse,
                apply_velocity,
                check_grounded,
                update_coyote_timer,
            )
                .chain()
                .run_if(crate::game_runtime::gameplay_systems_enabled),
        );
    }
}

type PlatformRiderQueryItem<'a> = (
    &'a mut GamePosition,
    &'a Collider,
    Option<&'a mut Velocity>,
    Option<&'a mut Grounded>,
);

type HorizontalMovementQueryItem<'a> = (
    &'a GamePosition,
    &'a Collider,
    &'a Grounded,
    &'a mut Velocity,
    &'a HorizontalMover,
    Option<&'a NetworkId>,
    Option<&'a Player>,
);

type LadderMovementQueryItem<'a> = (
    &'a GamePosition,
    &'a Collider,
    &'a mut Velocity,
    &'a mut Grounded,
    Option<&'a TopDownMover>,
    Option<&'a NetworkId>,
    Option<&'a Player>,
);

type JumpSystemQueryItem<'a> = (
    &'a mut Velocity,
    &'a Grounded,
    &'a mut CoyoteTimer,
    &'a mut JumpBuffer,
    &'a Jumper,
    Option<&'a NetworkId>,
    Option<&'a Player>,
);

fn moving_platform_system(
    time: Res<Time<Fixed>>,
    mut platforms: Query<(&mut GamePosition, &Collider, &mut MovingPlatform)>,
    mut riders: Query<PlatformRiderQueryItem<'_>, Without<MovingPlatform>>,
) {
    let dt = time.delta_secs();
    let mut motions = Vec::<physics_core::PlatformMotion>::new();

    for (mut pos, collider, mut platform) in platforms.iter_mut() {
        if platform.waypoints.len() < 2 {
            continue;
        }
        if platform.current_waypoint >= platform.waypoints.len() {
            platform.current_waypoint = 0;
        }
        if platform.direction == 0 {
            platform.direction = 1;
        }
        if platform.pause_timer > 0 {
            platform.pause_timer -= 1;
            continue;
        }

        let prev = Vec2::new(pos.x, pos.y);
        let target = platform.waypoints[platform.current_waypoint];
        let to_target = target - prev;
        let dist = to_target.length();
        let max_step = platform.speed.max(0.0) * dt;
        if max_step <= 0.0 {
            continue;
        }

        let reached = dist <= max_step + 0.001;
        let next = if reached {
            target
        } else {
            prev + to_target.normalize_or_zero() * max_step
        };
        let delta = next - prev;
        if delta.length_squared() > 0.000001 {
            pos.x = next.x;
            pos.y = next.y;
            if platform.carry_riders {
                motions.push(physics_core::PlatformMotion {
                    prev_x: prev.x,
                    prev_y: prev.y,
                    delta_x: delta.x,
                    delta_y: delta.y,
                    width: collider.width,
                    height: collider.height,
                });
            }
        }

        if reached {
            advance_platform_waypoint(&mut platform);
        }
    }

    if motions.is_empty() {
        return;
    }

    for (mut rider_pos, rider_collider, mut rider_vel, mut rider_grounded) in riders.iter_mut() {
        for motion in &motions {
            if !physics_core::rider_on_platform_top(
                rider_pos.x,
                rider_pos.y,
                rider_collider.width,
                rider_collider.height,
                motion,
            ) {
                continue;
            }

            rider_pos.x += motion.delta_x;
            rider_pos.y += motion.delta_y;
            if let Some(vel) = rider_vel.as_deref_mut() {
                if motion.delta_y > 0.0 {
                    vel.y = vel.y.max(0.0);
                }
            }
            if let Some(grounded) = rider_grounded.as_deref_mut() {
                grounded.0 = true;
            }
            break;
        }
    }
}

fn advance_platform_waypoint(platform: &mut MovingPlatform) {
    let len = platform.waypoints.len();
    if len <= 1 {
        platform.current_waypoint = 0;
        return;
    }
    match platform.loop_mode {
        PlatformLoopMode::Loop => {
            platform.current_waypoint = (platform.current_waypoint + 1) % len;
        }
        PlatformLoopMode::PingPong => {
            let dir = if platform.direction >= 0 {
                1isize
            } else {
                -1isize
            };
            let next = platform.current_waypoint as isize + dir;
            if next < 0 || next >= len as isize {
                platform.direction = -platform.direction;
                let new_dir = if platform.direction >= 0 {
                    1isize
                } else {
                    -1isize
                };
                let bounced = platform.current_waypoint as isize + new_dir;
                platform.current_waypoint = bounced.clamp(0, len as isize - 1) as usize;
            } else {
                platform.current_waypoint = next as usize;
            }
        }
    }
    if platform.pause_frames > 0 {
        platform.pause_timer = platform.pause_frames;
    }
}

fn apply_gravity(
    config: Res<GameConfig>,
    time: Res<Time<Fixed>>,
    mut query: Query<(&mut Velocity, &Grounded, Option<&Jumper>), With<GravityBody>>,
) {
    let dt = time.delta_secs();
    let grav = config.gravity_magnitude();
    for (mut vel, grounded, jumper) in query.iter_mut() {
        let fall_mult = jumper.map_or(config.fall_multiplier, |j| j.fall_multiplier);
        physics_core::apply_gravity_with_max(&mut vel.y, grounded.0, grav, fall_mult, dt, config.max_fall_speed);
    }
}

fn horizontal_movement(
    config: Res<GameConfig>,
    tilemap: Res<Tilemap>,
    vinput: Res<VirtualInput>,
    local_player: Res<LocalPlayerId>,
    mut query: Query<HorizontalMovementQueryItem<'_>>,
) {
    for (pos, collider, grounded, mut vel, mover, network_id, player) in query.iter_mut() {
        if !entity_uses_local_input(player, network_id, &local_player) {
            continue;
        }
        let left = vinput.pressed(&mover.left_action);
        let right = vinput.pressed(&mover.right_action);
        if left || right {
            vel.x = physics_core::horizontal_velocity(left, right, mover.speed);
            continue;
        }
        if grounded.0 {
            let friction = physics_core::surface_friction(
                &tilemap,
                &config,
                pos.x,
                pos.y,
                collider.width,
                collider.height,
            );
            physics_core::apply_surface_friction(&mut vel.x, friction);
        }
    }
}

fn ladder_movement(
    config: Res<GameConfig>,
    tilemap: Res<Tilemap>,
    vinput: Res<VirtualInput>,
    local_player: Res<LocalPlayerId>,
    mut query: Query<LadderMovementQueryItem<'_>>,
) {
    for (pos, collider, mut vel, mut grounded, top_down, network_id, player) in query.iter_mut() {
        if !entity_uses_local_input(player, network_id, &local_player) {
            continue;
        }
        if !overlaps_climbable(&tilemap, &config, pos, collider) {
            continue;
        }
        let climb_up = vinput.pressed("up") || vinput.pressed("jump");
        let climb_down = vinput.pressed("down");
        let speed = top_down.map(|m| m.speed).unwrap_or(config.move_speed * 0.8);
        if climb_up && !climb_down {
            vel.y = speed;
            grounded.0 = true;
        } else if climb_down && !climb_up {
            vel.y = -speed;
            grounded.0 = true;
        } else {
            vel.y = 0.0;
            grounded.0 = true;
        }
    }
}

fn top_down_movement(
    vinput: Res<VirtualInput>,
    local_player: Res<LocalPlayerId>,
    mut query: Query<(
        &mut Velocity,
        &TopDownMover,
        Option<&NetworkId>,
        Option<&Player>,
    )>,
) {
    for (mut vel, mover, network_id, player) in query.iter_mut() {
        if !entity_uses_local_input(player, network_id, &local_player) {
            continue;
        }
        let mut dx = 0.0f32;
        let mut dy = 0.0f32;
        if vinput.pressed(&mover.left_action) {
            dx -= 1.0;
        }
        if vinput.pressed(&mover.right_action) {
            dx += 1.0;
        }
        if vinput.pressed(&mover.up_action) {
            dy += 1.0;
        }
        if vinput.pressed(&mover.down_action) {
            dy -= 1.0;
        }
        let dir = Vec2::new(dx, dy);
        let dir = if dir.length() > 0.0 {
            dir.normalize()
        } else {
            dir
        };
        vel.x = dir.x * mover.speed;
        vel.y = dir.y * mover.speed;
    }
}

fn jump_system(
    vinput: Res<VirtualInput>,
    local_player: Res<LocalPlayerId>,
    mut query: Query<JumpSystemQueryItem<'_>>,
) {
    for (mut vel, grounded, mut coyote, mut jump_buf, jumper, network_id, player) in
        query.iter_mut()
    {
        if !entity_uses_local_input(player, network_id, &local_player) {
            continue;
        }
        let jump_just_pressed = vinput.just_pressed(&jumper.action);
        let jump_pressed = vinput.pressed(&jumper.action);
        physics_core::update_jump_buffer(jump_just_pressed, &mut jump_buf.0, jumper.buffer_frames);
        let _jumped = physics_core::try_jump(
            grounded.0,
            &mut coyote.0,
            &mut jump_buf.0,
            jump_just_pressed,
            jumper.velocity,
            &mut vel.y,
        );
        physics_core::apply_variable_jump(&mut vel.y, jump_pressed, jumper.variable_height);
    }
}

fn apply_drag(
    time: Res<Time<Fixed>>,
    mut query: Query<(&mut Velocity, &VelocityDamping)>,
) {
    let dt = time.delta_secs();
    for (mut vel, damping) in query.iter_mut() {
        let retain: f32 = (1.0 - damping.factor).max(0.0).powf(dt * 60.0);
        vel.x *= retain;
        vel.y *= retain;
    }
}

fn apply_knockback_impulse(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Velocity, &KnockbackImpulse)>,
) {
    for (entity, mut vel, impulse) in query.iter_mut() {
        vel.x += impulse.vx;
        vel.y += impulse.vy;
        commands.entity(entity).remove::<KnockbackImpulse>();
    }
}

fn apply_velocity(
    config: Res<GameConfig>,
    time: Res<Time<Fixed>>,
    tilemap: Res<Tilemap>,
    mut perf: ResMut<PerfAccum>,
    mut query: Query<(&mut GamePosition, &mut Velocity, &mut Alive, &Collider)>,
) {
    let start = Instant::now();
    let dt = time.delta_secs();
    let ts = config.tile_size;
    let mut counters = PhysicsCounters::default();

    for (mut pos, mut vel, mut alive, collider) in query.iter_mut() {
        let motion = physics_core::resolve_motion(
            &tilemap,
            physics_core::MotionParams {
                tile_size: ts,
                dt,
                x: pos.x,
                y: pos.y,
                vx: vel.x,
                vy: vel.y,
                width: collider.width,
                height: collider.height,
            },
            &mut counters,
        );
        pos.x = motion.x;
        pos.y = motion.y;
        vel.x = motion.vx;
        vel.y = motion.vy;

        // Check spikes
        if physics_core::collides_type(
            &tilemap,
            physics_core::CollisionQuery {
                x: pos.x,
                y: pos.y,
                width: collider.width,
                height: collider.height,
                tile_size: ts,
                target: TileType::Spike,
            },
            &mut counters,
        ) {
            alive.0 = false;
            pos.x = tilemap.player_spawn.0;
            pos.y = tilemap.player_spawn.1;
            vel.x = 0.0;
            vel.y = 0.0;
            alive.0 = true;
        }

        // Fall out of world -> respawn
        if pos.y < -100.0 {
            pos.x = tilemap.player_spawn.0;
            pos.y = tilemap.player_spawn.1;
            vel.x = 0.0;
            vel.y = 0.0;
        }
    }

    perf.physics_time_ms += start.elapsed().as_secs_f32() * 1000.0;
    perf.collision_checks = perf
        .collision_checks
        .saturating_add(counters.collision_checks);
}

fn check_grounded(
    config: Res<GameConfig>,
    tilemap: Res<Tilemap>,
    mut perf: ResMut<PerfAccum>,
    mut query: Query<(&GamePosition, &mut Grounded, &Collider)>,
) {
    let start = Instant::now();
    let ts = config.tile_size;
    let mut counters = PhysicsCounters::default();
    for (pos, mut grounded, collider) in query.iter_mut() {
        grounded.0 = physics_core::compute_grounded(
            &tilemap,
            ts,
            pos.x,
            pos.y,
            collider.width,
            collider.height,
            &mut counters,
        );
    }
    perf.physics_time_ms += start.elapsed().as_secs_f32() * 1000.0;
    perf.collision_checks = perf
        .collision_checks
        .saturating_add(counters.collision_checks);
}

fn update_coyote_timer(mut query: Query<(&Grounded, &mut CoyoteTimer, &Jumper)>) {
    for (grounded, mut coyote, jumper) in query.iter_mut() {
        physics_core::update_coyote_timer(grounded.0, &mut coyote.0, jumper.coyote_frames);
    }
}

fn entity_uses_local_input(
    player: Option<&Player>,
    network_id: Option<&NetworkId>,
    local_player: &LocalPlayerId,
) -> bool {
    if player.is_some() {
        return true;
    }
    if let (Some(network_id), Some(local_id)) = (network_id, local_player.0) {
        return network_id.0 == local_id;
    }
    false
}

fn overlaps_climbable(
    tilemap: &Tilemap,
    config: &GameConfig,
    pos: &GamePosition,
    collider: &Collider,
) -> bool {
    let ts = config.tile_size.max(0.001);
    let min_x = ((pos.x - collider.width / 2.0) / ts).floor() as i32;
    let max_x = ((pos.x + collider.width / 2.0 - 0.01) / ts).floor() as i32;
    let min_y = ((pos.y - collider.height / 2.0) / ts).floor() as i32;
    let max_y = ((pos.y + collider.height / 2.0 - 0.01) / ts).floor() as i32;
    for ty in min_y..=max_y {
        for tx in min_x..=max_x {
            if config.tile_types.is_climbable(tilemap.get_tile(tx, ty)) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    fn run_moving_platform_once(world: &mut World) {
        world
            .resource_mut::<Time<Fixed>>()
            .advance_by(Duration::from_secs_f32(1.0 / 60.0));
        world
            .run_system_once(moving_platform_system)
            .expect("run moving_platform_system");
    }

    #[test]
    fn moving_platform_carries_rider_when_enabled() {
        let mut world = World::new();
        world.insert_resource(Time::<Fixed>::from_hz(60.0));

        world.spawn((
            GamePosition { x: 0.0, y: 0.0 },
            Collider {
                width: 32.0,
                height: 8.0,
            },
            MovingPlatform {
                waypoints: vec![Vec2::new(0.0, 0.0), Vec2::new(16.0, 0.0)],
                speed: 960.0,
                loop_mode: PlatformLoopMode::PingPong,
                current_waypoint: 1,
                direction: 1,
                pause_frames: 0,
                pause_timer: 0,
                carry_riders: true,
            },
        ));

        let rider = world
            .spawn((
                GamePosition { x: 0.0, y: 11.0 },
                Collider {
                    width: 12.0,
                    height: 14.0,
                },
                Velocity::default(),
                Grounded(true),
            ))
            .id();

        run_moving_platform_once(&mut world);

        let rider_pos = world
            .get::<GamePosition>(rider)
            .expect("rider position after move");
        assert!((rider_pos.x - 16.0).abs() < 0.01);
    }

    #[test]
    fn moving_platform_does_not_carry_rider_when_disabled() {
        let mut world = World::new();
        world.insert_resource(Time::<Fixed>::from_hz(60.0));

        world.spawn((
            GamePosition { x: 0.0, y: 0.0 },
            Collider {
                width: 32.0,
                height: 8.0,
            },
            MovingPlatform {
                waypoints: vec![Vec2::new(0.0, 0.0), Vec2::new(16.0, 0.0)],
                speed: 960.0,
                loop_mode: PlatformLoopMode::PingPong,
                current_waypoint: 1,
                direction: 1,
                pause_frames: 0,
                pause_timer: 0,
                carry_riders: false,
            },
        ));

        let rider = world
            .spawn((
                GamePosition { x: 0.0, y: 11.0 },
                Collider {
                    width: 12.0,
                    height: 14.0,
                },
                Velocity::default(),
                Grounded(true),
            ))
            .id();

        run_moving_platform_once(&mut world);

        let rider_pos = world
            .get::<GamePosition>(rider)
            .expect("rider position after move");
        assert!((rider_pos.x - 0.0).abs() < 0.01);
    }

    #[test]
    fn horizontal_movement_only_consumes_input_for_local_player_entity() {
        let mut world = World::new();
        world.insert_resource(GameConfig::default());
        world.insert_resource(Tilemap::test_level());
        world.insert_resource(VirtualInput {
            active: std::collections::HashSet::from(["left".to_string()]),
            just_pressed: std::collections::HashSet::new(),
            just_released: std::collections::HashSet::new(),
        });
        world.insert_resource(LocalPlayerId(Some(1)));

        let local = world
            .spawn((
                NetworkId(1),
                GamePosition { x: 20.0, y: 20.0 },
                Collider {
                    width: 12.0,
                    height: 14.0,
                },
                Grounded(true),
                Velocity::default(),
                HorizontalMover {
                    speed: 120.0,
                    left_action: "left".to_string(),
                    right_action: "right".to_string(),
                },
            ))
            .id();

        let npc = world
            .spawn((
                NetworkId(2),
                GamePosition { x: 30.0, y: 20.0 },
                Collider {
                    width: 12.0,
                    height: 14.0,
                },
                Grounded(true),
                Velocity::default(),
                HorizontalMover {
                    speed: 120.0,
                    left_action: "left".to_string(),
                    right_action: "right".to_string(),
                },
            ))
            .id();

        world
            .run_system_once(horizontal_movement)
            .expect("run horizontal movement");

        let local_vel = world.get::<Velocity>(local).expect("local velocity");
        let npc_vel = world.get::<Velocity>(npc).expect("npc velocity");
        assert!(local_vel.x < -0.1, "local entity should receive input");
        assert!(npc_vel.x.abs() < 0.001, "npc should ignore keyboard input");
    }
}
