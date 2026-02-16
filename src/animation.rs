use std::collections::HashMap;

use bevy::prelude::*;

use crate::components::{AnimationController, NetworkId, Velocity};
use crate::events::GameEventBus;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AnimFrameEventDef {
    pub frame: usize,
    pub event: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AnimationClipDef {
    pub frame_count: usize,
    #[serde(default)]
    pub frames: Vec<usize>,
    pub fps: f32,
    #[serde(default = "default_true")]
    pub looping: bool,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub events: Vec<AnimFrameEventDef>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AnimationGraphDef {
    #[serde(default)]
    pub default_state: String,
    #[serde(default)]
    pub states: HashMap<String, AnimationClipDef>,
}

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AnimationLibrary {
    pub graphs: HashMap<String, AnimationGraphDef>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AnimationGraphInfo {
    pub name: String,
    pub states: Vec<String>,
    pub default_state: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AnimationEntityState {
    pub id: u64,
    pub graph: String,
    pub state: String,
    pub frame: usize,
    pub playing: bool,
    pub speed: f32,
    pub facing_right: bool,
}

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(default_animation_library())
            .add_systems(
                FixedUpdate,
                (
                    drive_animation_state_from_velocity,
                    advance_animation_frames,
                    normalize_missing_animation_states,
                )
                    .chain()
                    .run_if(crate::game_runtime::gameplay_systems_enabled),
            );
    }
}

fn default_animation_library() -> AnimationLibrary {
    let mut graphs = HashMap::new();
    graphs.insert(
        "basic_actor".to_string(),
        AnimationGraphDef {
            default_state: "idle".to_string(),
            states: HashMap::from([
                (
                    "idle".to_string(),
                    AnimationClipDef {
                        frame_count: 1,
                        frames: Vec::new(),
                        fps: 8.0,
                        looping: true,
                        next: None,
                        events: Vec::new(),
                    },
                ),
                (
                    "run".to_string(),
                    AnimationClipDef {
                        frame_count: 6,
                        frames: Vec::new(),
                        fps: 12.0,
                        looping: true,
                        next: None,
                        events: Vec::new(),
                    },
                ),
            ]),
        },
    );
    graphs.insert(
        "samurai_player".to_string(),
        AnimationGraphDef {
            default_state: "idle".to_string(),
            states: HashMap::from([
                (
                    "idle".to_string(),
                    AnimationClipDef {
                        frame_count: 10,
                        frames: Vec::new(),
                        fps: 8.0,
                        looping: true,
                        next: None,
                        events: Vec::new(),
                    },
                ),
                (
                    "run".to_string(),
                    AnimationClipDef {
                        frame_count: 16,
                        frames: Vec::new(),
                        fps: 12.0,
                        looping: true,
                        next: None,
                        events: Vec::new(),
                    },
                ),
                (
                    "attack".to_string(),
                    AnimationClipDef {
                        frame_count: 7,
                        frames: Vec::new(),
                        fps: 15.0,
                        looping: false,
                        next: Some("idle".to_string()),
                        events: Vec::new(),
                    },
                ),
                (
                    "hurt".to_string(),
                    AnimationClipDef {
                        frame_count: 4,
                        frames: Vec::new(),
                        fps: 10.0,
                        looping: false,
                        next: Some("idle".to_string()),
                        events: Vec::new(),
                    },
                ),
            ]),
        },
    );
    AnimationLibrary { graphs }
}

fn drive_animation_state_from_velocity(
    mut query: Query<(&Velocity, &mut AnimationController)>,
    library: Res<AnimationLibrary>,
) {
    for (vel, mut anim) in query.iter_mut() {
        if !anim.auto_from_velocity {
            continue;
        }
        if vel.x > 1.0 {
            anim.facing_right = true;
        } else if vel.x < -1.0 {
            anim.facing_right = false;
        }
        let Some(graph) = library.graphs.get(&anim.graph) else {
            continue;
        };
        let next_state = if vel.y > 1.0 && graph.states.contains_key("jump") {
            "jump"
        } else if vel.y < -1.0 && graph.states.contains_key("fall") {
            "fall"
        } else if vel.x.abs() > 1.0 && graph.states.contains_key("run") {
            "run"
        } else if graph.states.contains_key("idle") {
            "idle"
        } else {
            graph.default_state.as_str()
        };
        if !next_state.is_empty() && anim.state != next_state {
            anim.state = next_state.to_string();
            anim.frame = 0;
            anim.timer = 0.0;
            anim.playing = true;
        }
    }
}

fn normalize_missing_animation_states(
    library: Res<AnimationLibrary>,
    mut query: Query<&mut AnimationController>,
) {
    for mut anim in query.iter_mut() {
        let Some(graph) = library.graphs.get(&anim.graph) else {
            continue;
        };
        if graph.states.is_empty() {
            continue;
        }
        if !graph.states.contains_key(&anim.state) {
            if !graph.default_state.is_empty() && graph.states.contains_key(&graph.default_state) {
                anim.state = graph.default_state.clone();
            } else if let Some(first) = graph.states.keys().next() {
                anim.state = first.clone();
            }
            anim.frame = 0;
            anim.timer = 0.0;
        }
    }
}

fn advance_animation_frames(
    time: Res<Time<Fixed>>,
    library: Res<AnimationLibrary>,
    mut bus: ResMut<GameEventBus>,
    mut query: Query<(&mut AnimationController, Option<&NetworkId>)>,
) {
    let dt = time.delta_secs();
    for (mut anim, network_id) in query.iter_mut() {
        if !anim.playing {
            continue;
        }
        let Some(graph) = library.graphs.get(&anim.graph) else {
            continue;
        };
        let Some(clip) = graph.states.get(&anim.state) else {
            continue;
        };
        let frame_count = if clip.frames.is_empty() {
            clip.frame_count.max(1)
        } else {
            clip.frames.len().max(1)
        };
        let fps = clip.fps.max(0.001) * anim.speed.max(0.0);
        if fps <= 0.001 {
            continue;
        }

        anim.timer += dt;
        let frame_time = 1.0 / fps;
        let source_id = network_id.map(|n| n.0);
        while anim.timer >= frame_time {
            anim.timer -= frame_time;
            if clip.looping {
                anim.frame = (anim.frame + 1) % frame_count;
                if let Some(source_id) = source_id {
                    emit_animation_events(&mut bus, source_id, &anim, clip);
                }
            } else if anim.frame + 1 < frame_count {
                anim.frame += 1;
                if let Some(source_id) = source_id {
                    emit_animation_events(&mut bus, source_id, &anim, clip);
                }
            } else {
                let mut transitioned = false;
                if let Some(next) = clip.next.as_deref() {
                    if graph.states.contains_key(next) {
                        anim.state = next.to_string();
                        anim.frame = 0;
                        anim.timer = 0.0;
                        anim.playing = true;
                        transitioned = true;
                        if let Some(next_clip) = graph.states.get(next) {
                            if let Some(source_id) = source_id {
                                emit_animation_events(&mut bus, source_id, &anim, next_clip);
                            }
                        }
                    }
                }
                if !transitioned {
                    anim.playing = false;
                    anim.frame = frame_count - 1;
                }
                break;
            }
        }
    }
}

pub fn resolve_clip_frame(clip: &AnimationClipDef, frame: usize) -> usize {
    if clip.frames.is_empty() {
        frame % clip.frame_count.max(1)
    } else {
        clip.frames[frame % clip.frames.len().max(1)]
    }
}

fn emit_animation_events(
    bus: &mut GameEventBus,
    source_entity: u64,
    anim: &AnimationController,
    clip: &AnimationClipDef,
) {
    for ev in clip.events.iter().filter(|ev| ev.frame == anim.frame) {
        let name = ev.event.trim();
        if name.is_empty() {
            continue;
        }
        let full_name = if name.starts_with("anim:") {
            name.to_string()
        } else {
            format!("anim:{name}")
        };
        bus.emit(
            full_name,
            serde_json::json!({
                "entity_id": source_entity,
                "graph": anim.graph,
                "state": anim.state,
                "frame": anim.frame,
            }),
            Some(source_entity),
        );
    }
}

pub fn collect_animation_states(
    world: &World,
    query: &mut QueryState<(&AnimationController, &NetworkId)>,
) -> Vec<AnimationEntityState> {
    let mut out = Vec::new();
    for (anim, network_id) in query.iter(world) {
        out.push(AnimationEntityState {
            id: network_id.0,
            graph: anim.graph.clone(),
            state: anim.state.clone(),
            frame: anim.frame,
            playing: anim.playing,
            speed: anim.speed,
            facing_right: anim.facing_right,
        });
    }
    out.sort_by_key(|x| x.id);
    out
}

pub fn list_graph_infos(library: &AnimationLibrary) -> Vec<AnimationGraphInfo> {
    let mut infos = Vec::new();
    for (name, graph) in &library.graphs {
        let mut states: Vec<String> = graph.states.keys().cloned().collect();
        states.sort();
        infos.push(AnimationGraphInfo {
            name: name.clone(),
            states,
            default_state: graph.default_state.clone(),
        });
    }
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    infos
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    #[test]
    fn non_looping_clip_stops_at_last_frame() {
        let mut world = World::new();
        world.insert_resource(Time::<Fixed>::from_hz(60.0));
        world.insert_resource(default_animation_library());
        world.insert_resource(GameEventBus::default());
        world.spawn(AnimationController {
            graph: "test".to_string(),
            state: "attack".to_string(),
            frame: 0,
            timer: 0.0,
            speed: 1.0,
            playing: true,
            facing_right: true,
            auto_from_velocity: false,
        });
        {
            let mut lib = world.resource_mut::<AnimationLibrary>();
            lib.graphs.insert(
                "test".to_string(),
                AnimationGraphDef {
                    default_state: "attack".to_string(),
                    states: HashMap::from([(
                        "attack".to_string(),
                        AnimationClipDef {
                            frame_count: 3,
                            frames: Vec::new(),
                            fps: 60.0,
                            looping: false,
                            next: None,
                            events: Vec::new(),
                        },
                    )]),
                },
            );
        }
        for _ in 0..10 {
            world
                .resource_mut::<Time<Fixed>>()
                .advance_by(Duration::from_secs_f32(1.0 / 60.0));
            world
                .run_system_once(normalize_missing_animation_states)
                .expect("normalize states");
            world
                .run_system_once(advance_animation_frames)
                .expect("advance frames");
        }
        let anim = {
            let mut query = world.query::<&AnimationController>();
            query.single(&world).clone()
        };
        assert_eq!(anim.frame, 2);
        assert!(!anim.playing);
    }

    #[test]
    fn non_looping_clip_transitions_to_next_and_emits_events() {
        let mut world = World::new();
        world.insert_resource(Time::<Fixed>::from_hz(60.0));
        world.insert_resource(default_animation_library());
        world.insert_resource(GameEventBus::default());
        world.spawn((
            NetworkId(77),
            AnimationController {
                graph: "test".to_string(),
                state: "attack".to_string(),
                frame: 0,
                timer: 0.0,
                speed: 1.0,
                playing: true,
                facing_right: true,
                auto_from_velocity: false,
            },
        ));

        {
            let mut lib = world.resource_mut::<AnimationLibrary>();
            lib.graphs.insert(
                "test".to_string(),
                AnimationGraphDef {
                    default_state: "idle".to_string(),
                    states: HashMap::from([
                        (
                            "attack".to_string(),
                            AnimationClipDef {
                                frame_count: 2,
                                frames: Vec::new(),
                                fps: 60.0,
                                looping: false,
                                next: Some("idle".to_string()),
                                events: vec![AnimFrameEventDef {
                                    frame: 1,
                                    event: "hitbox_on".to_string(),
                                }],
                            },
                        ),
                        (
                            "idle".to_string(),
                            AnimationClipDef {
                                frame_count: 1,
                                frames: Vec::new(),
                                fps: 1.0,
                                looping: true,
                                next: None,
                                events: Vec::new(),
                            },
                        ),
                    ]),
                },
            );
        }

        for _ in 0..3 {
            world
                .resource_mut::<Time<Fixed>>()
                .advance_by(Duration::from_secs_f32(1.0 / 60.0));
            world
                .run_system_once(advance_animation_frames)
                .expect("advance frames");
        }

        let anim = {
            let mut query = world.query::<&AnimationController>();
            query.single(&world).clone()
        };
        assert_eq!(anim.state, "idle");
        assert!(anim.playing);

        let bus = world.resource::<GameEventBus>();
        assert!(bus.recent.iter().any(|e| {
            e.name == "anim:hitbox_on"
                && e.source_entity == Some(77)
                && e.data.get("state").and_then(|v| v.as_str()) == Some("attack")
        }));
    }

    #[test]
    fn collect_animation_states_skips_entities_without_network_id() {
        let mut world = World::new();
        world.spawn(AnimationController {
            graph: "basic_actor".to_string(),
            state: "idle".to_string(),
            frame: 0,
            timer: 0.0,
            speed: 1.0,
            playing: true,
            facing_right: true,
            auto_from_velocity: false,
        });
        world.spawn((
            NetworkId(42),
            AnimationController {
                graph: "basic_actor".to_string(),
                state: "run".to_string(),
                frame: 3,
                timer: 0.0,
                speed: 1.0,
                playing: true,
                facing_right: false,
                auto_from_velocity: false,
            },
        ));

        let mut query = world.query::<(&AnimationController, &NetworkId)>();
        let states = collect_animation_states(&world, &mut query);
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].id, 42);
        assert_eq!(states[0].state, "run");
    }
}
