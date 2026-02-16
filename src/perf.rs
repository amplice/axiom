use bevy::prelude::*;

use crate::spatial_hash::SpatialHash;

const PERF_HISTORY_CAPACITY: usize = 300;
const PERF_HISTORY_MIN_INTERVAL_SECONDS: f64 = 0.2;

#[derive(Resource, Clone, serde::Serialize, Default)]
pub struct PerfStats {
    pub fps: f32,
    pub frame_time_ms: f32,
    pub entity_count: usize,
    pub physics_time_ms: f32,
    pub script_time_ms: f32,
    pub render_time_ms: f32,
    pub collision_checks: u64,
    pub spatial_hash_cells: usize,
    pub history: PerfHistory,
}

#[derive(Clone, serde::Serialize)]
pub struct PerfSample {
    pub seq: u64,
    pub at_seconds: f64,
    pub fps: f32,
    pub frame_time_ms: f32,
    pub entity_count: usize,
    pub physics_time_ms: f32,
    pub script_time_ms: f32,
    pub render_time_ms: f32,
    pub collision_checks: u64,
    pub spatial_hash_cells: usize,
}

#[derive(Clone, serde::Serialize)]
pub struct PerfHistory {
    pub samples: Vec<PerfSample>,
    pub capacity: usize,
    pub dropped_samples: u64,
    #[serde(skip)]
    next_seq: u64,
    #[serde(skip)]
    last_recorded_at_seconds: f64,
}

impl Default for PerfHistory {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            capacity: PERF_HISTORY_CAPACITY,
            dropped_samples: 0,
            next_seq: 1,
            last_recorded_at_seconds: -1.0,
        }
    }
}

impl PerfHistory {
    fn push_sample(&mut self, sample: PerfSampleInput) {
        if self.last_recorded_at_seconds >= 0.0
            && (sample.at_seconds - self.last_recorded_at_seconds)
                < PERF_HISTORY_MIN_INTERVAL_SECONDS
        {
            return;
        }
        self.last_recorded_at_seconds = sample.at_seconds;
        self.samples.push(PerfSample {
            seq: self.next_seq,
            at_seconds: sample.at_seconds,
            fps: sample.fps,
            frame_time_ms: sample.frame_time_ms,
            entity_count: sample.entity_count,
            physics_time_ms: sample.physics_time_ms,
            script_time_ms: sample.script_time_ms,
            render_time_ms: sample.render_time_ms,
            collision_checks: sample.collision_checks,
            spatial_hash_cells: sample.spatial_hash_cells,
        });
        self.next_seq = self.next_seq.saturating_add(1);
        if self.samples.len() > self.capacity {
            let excess = self.samples.len() - self.capacity;
            self.samples.drain(0..excess);
            self.dropped_samples = self.dropped_samples.saturating_add(excess as u64);
        }
    }
}

#[derive(Clone, Copy)]
struct PerfSampleInput {
    at_seconds: f64,
    fps: f32,
    frame_time_ms: f32,
    entity_count: usize,
    physics_time_ms: f32,
    script_time_ms: f32,
    render_time_ms: f32,
    collision_checks: u64,
    spatial_hash_cells: usize,
}

#[derive(Resource, Default)]
pub struct PerfAccum {
    pub physics_time_ms: f32,
    pub script_time_ms: f32,
    pub render_time_ms: f32,
    pub collision_checks: u64,
}

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PerfStats::default())
            .insert_resource(PerfAccum::default())
            .add_systems(Update, update_perf_stats);
    }
}

fn update_perf_stats(
    time: Res<Time>,
    entities: Query<Entity>,
    hash: Option<Res<SpatialHash>>,
    mut accum: ResMut<PerfAccum>,
    mut perf: ResMut<PerfStats>,
) {
    let dt = time.delta_secs().max(0.000_001);
    perf.frame_time_ms = dt * 1000.0;
    perf.fps = 1.0 / dt;
    perf.entity_count = entities.iter().count();
    perf.spatial_hash_cells = hash.map_or(0, |h| h.cells.len());
    let has_new_sample = accum.physics_time_ms > 0.0
        || accum.script_time_ms > 0.0
        || accum.render_time_ms > 0.0
        || accum.collision_checks > 0;
    if has_new_sample {
        perf.physics_time_ms = accum.physics_time_ms;
        perf.script_time_ms = accum.script_time_ms;
        perf.render_time_ms = accum.render_time_ms;
        perf.collision_checks = accum.collision_checks;

        accum.physics_time_ms = 0.0;
        accum.script_time_ms = 0.0;
        accum.render_time_ms = 0.0;
        accum.collision_checks = 0;
    }
    let at_seconds = time.elapsed_secs_f64();
    let fps = perf.fps;
    let frame_time_ms = perf.frame_time_ms;
    let entity_count = perf.entity_count;
    let physics_time_ms = perf.physics_time_ms;
    let script_time_ms = perf.script_time_ms;
    let render_time_ms = perf.render_time_ms;
    let collision_checks = perf.collision_checks;
    let spatial_hash_cells = perf.spatial_hash_cells;
    perf.history.push_sample(PerfSampleInput {
        at_seconds,
        fps,
        frame_time_ms,
        entity_count,
        physics_time_ms,
        script_time_ms,
        render_time_ms,
        collision_checks,
        spatial_hash_cells,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_plugin_publishes_and_resets_accumulators() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(PerfPlugin);

        {
            let mut accum = app.world_mut().resource_mut::<PerfAccum>();
            accum.physics_time_ms = 2.5;
            accum.script_time_ms = 1.25;
            accum.render_time_ms = 0.75;
            accum.collision_checks = 42;
        }

        app.update();

        let perf = app.world().resource::<PerfStats>();
        assert!((perf.physics_time_ms - 2.5).abs() < 0.0001);
        assert!((perf.script_time_ms - 1.25).abs() < 0.0001);
        assert!((perf.render_time_ms - 0.75).abs() < 0.0001);
        assert_eq!(perf.collision_checks, 42);

        let accum = app.world().resource::<PerfAccum>();
        assert_eq!(accum.physics_time_ms, 0.0);
        assert_eq!(accum.script_time_ms, 0.0);
        assert_eq!(accum.render_time_ms, 0.0);
        assert_eq!(accum.collision_checks, 0);
    }

    #[test]
    fn perf_plugin_keeps_last_non_zero_sample_between_fixed_ticks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(PerfPlugin);

        {
            let mut accum = app.world_mut().resource_mut::<PerfAccum>();
            accum.physics_time_ms = 3.5;
            accum.script_time_ms = 2.25;
            accum.render_time_ms = 1.0;
            accum.collision_checks = 17;
        }
        app.update();

        {
            let perf = app.world().resource::<PerfStats>();
            assert!((perf.physics_time_ms - 3.5).abs() < 0.0001);
            assert!((perf.script_time_ms - 2.25).abs() < 0.0001);
            assert!((perf.render_time_ms - 1.0).abs() < 0.0001);
            assert_eq!(perf.collision_checks, 17);
        }

        // No new fixed-step sample yet: keep reporting the latest one.
        app.update();
        let perf = app.world().resource::<PerfStats>();
        assert!((perf.physics_time_ms - 3.5).abs() < 0.0001);
        assert!((perf.script_time_ms - 2.25).abs() < 0.0001);
        assert!((perf.render_time_ms - 1.0).abs() < 0.0001);
        assert_eq!(perf.collision_checks, 17);
    }

    #[test]
    fn perf_history_keeps_recent_downsampled_samples() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(PerfPlugin);

        for _ in 0..200 {
            {
                let mut accum = app.world_mut().resource_mut::<PerfAccum>();
                accum.physics_time_ms = 1.0;
                accum.script_time_ms = 2.0;
                accum.render_time_ms = 0.5;
                accum.collision_checks = 3;
            }
            app.update();
        }

        let perf = app.world().resource::<PerfStats>();
        assert!(!perf.history.samples.is_empty());
        assert!(perf.history.samples.len() <= perf.history.capacity);
    }
}
