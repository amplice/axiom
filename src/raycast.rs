use bevy::prelude::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaycastAabb {
    pub id: u64,
    pub min: Vec2,
    pub max: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaycastHit {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub distance: f32,
}

pub fn ray_aabb_distance(
    origin: Vec2,
    dir_normalized: Vec2,
    max_distance: f32,
    min: Vec2,
    max: Vec2,
) -> Option<f32> {
    let mut tmin = 0.0f32;
    let mut tmax = max_distance.max(0.0);

    for axis in 0..2 {
        let (o, d, mn, mx) = if axis == 0 {
            (origin.x, dir_normalized.x, min.x, max.x)
        } else {
            (origin.y, dir_normalized.y, min.y, max.y)
        };
        if d.abs() < 1e-6 {
            if o < mn || o > mx {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d;
        let mut t1 = (mn - o) * inv;
        let mut t2 = (mx - o) * inv;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        tmin = tmin.max(t1);
        tmax = tmax.min(t2);
        if tmin > tmax {
            return None;
        }
    }

    if tmax < 0.0 {
        return None;
    }
    let hit = if tmin >= 0.0 { tmin } else { tmax };
    if hit <= max_distance {
        Some(hit)
    } else {
        None
    }
}

pub fn raycast_aabbs(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    targets: impl IntoIterator<Item = RaycastAabb>,
) -> Vec<RaycastHit> {
    let len = direction.length();
    if len <= 0.0001 {
        return Vec::new();
    }
    let dir = direction / len;
    let mut hits = Vec::new();
    for target in targets {
        if let Some(distance) = ray_aabb_distance(origin, dir, max_distance, target.min, target.max)
        {
            hits.push(RaycastHit {
                id: target.id,
                x: origin.x + dir.x * distance,
                y: origin.y + dir.y * distance,
                distance,
            });
        }
    }
    hits.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_raycast_hits_sorted() {
        let hits = raycast_aabbs(
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            100.0,
            [
                RaycastAabb {
                    id: 2,
                    min: Vec2::new(20.0, -5.0),
                    max: Vec2::new(30.0, 5.0),
                },
                RaycastAabb {
                    id: 1,
                    min: Vec2::new(10.0, -5.0),
                    max: Vec2::new(15.0, 5.0),
                },
            ],
        );
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, 1);
        assert_eq!(hits[1].id, 2);
        assert!(hits[0].distance < hits[1].distance);
    }
}
