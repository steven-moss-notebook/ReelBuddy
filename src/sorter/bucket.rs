//! Shoreline Buckets that accumulate top video candidates and displace lower cuts.
//!
//! When candidates for best short are found, they settle into ranked buckets.
//! If a better short is subsequently identified, the displaced bucket is emptied
//! with a dynamic splash effect, and the new superior treasure claims the slot.

use bevy::prelude::*;
use crate::sorter::treasure::Treasure;

pub const BUCKET_COUNT: usize = 3;

#[derive(Component)]
pub struct BucketSlot {
    pub index: usize,
    pub title: &'static str,
    pub pos: Vec2,
    pub splash_timer: f32,
}

#[derive(Resource)]
pub struct ShorelineBuckets {
    pub slots: [Option<Treasure>; BUCKET_COUNT],
    pub total_treasures_sorted: u64,
    pub displacements: u64,
}

impl Default for ShorelineBuckets {
    fn default() -> Self {
        Self {
            slots: [None, None, None],
            total_treasures_sorted: 0,
            displacements: 0,
        }
    }
}

impl ShorelineBuckets {
    /// Attempt to insert a newly unearthed candidate treasure into the buckets.
    /// Returns Some(displaced_treasure) if a lower-ranked treasure was displaced,
    /// or None if inserted into an empty slot, or Err if not high enough to beat any bucket.
    pub fn insert_candidate(&mut self, candidate: Treasure) -> Result<Option<Treasure>, ()> {
        let score = candidate.score;

        // 1. Check for an empty slot first
        for i in 0..BUCKET_COUNT {
            if self.slots[i].is_none() {
                self.slots[i] = Some(candidate);
                self.total_treasures_sorted += 1;
                return Ok(None);
            }
        }

        // 2. Find the lowest scoring slot currently in the buckets
        let mut min_idx = 0;
        let mut min_score = f64::INFINITY;
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(t) = slot {
                if t.score < min_score {
                    min_score = t.score;
                    min_idx = i;
                }
            }
        }

        // 3. If the new candidate beats the lowest bucket, displace it!
        if score > min_score {
            let old = self.slots[min_idx].take();
            self.slots[min_idx] = Some(candidate);
            self.total_treasures_sorted += 1;
            self.displacements += 1;
            Ok(old)
        } else {
            Err(())
        }
    }
}

#[derive(Component)]
pub struct SplashWaterParticle {
    pub vel: Vec2,
    pub life: f32,
    pub max_life: f32,
}

/// Spawns a dynamic splash effect when a bucket is emptied/displaced.
pub fn spawn_bucket_splash(commands: &mut Commands, pos: Vec2) {
    for i in 0..14 {
        let angle = ((i as f32) * 25.0 + 30.0).to_radians();
        let speed = 40.0 + ((i % 5) as f32) * 14.0;
        let vel = Vec2::new(angle.cos() * speed, angle.sin() * speed + 25.0);

        commands.spawn((
            SplashWaterParticle {
                vel,
                life: 0.0,
                max_life: 0.65,
            },
            Sprite {
                color: Color::srgba(0.55, 0.85, 1.0, 0.9),
                custom_size: Some(Vec2::splat(4.0)),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y, 4.5),
        ));
    }
}

pub fn update_bucket_splashes(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut SplashWaterParticle, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (entity, mut p, mut transform, mut sprite) in &mut particles {
        p.life += dt;
        if p.life >= p.max_life {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.x += p.vel.x * dt;
        transform.translation.y += p.vel.y * dt;
        p.vel.y -= 75.0 * dt; // gravity

        let alpha = 1.0 - (p.life / p.max_life);
        let cur = sprite.color.to_srgba();
        sprite.color = Color::srgba(cur.red, cur.green, cur.blue, alpha);
    }
}

pub fn bucket_position(index: usize) -> Vec2 {
    match index {
        0 => Vec2::new(-45.0, 36.0), // Rank 1 (Left-Center)
        1 => Vec2::new(0.0, 36.0),   // Rank 2 (Center)
        _ => Vec2::new(45.0, 36.0),  // Rank 3 (Right-Center)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_insert_and_displace_correctly() {
        let mut buckets = ShorelineBuckets::default();
        let t1 = Treasure::new(1, 10, 0.0, 10.0, 72.0);
        let t2 = Treasure::new(1, 11, 10.0, 20.0, 80.0);
        let t3 = Treasure::new(1, 12, 20.0, 30.0, 68.0);

        assert!(buckets.insert_candidate(t1).is_ok());
        assert!(buckets.insert_candidate(t2).is_ok());
        assert!(buckets.insert_candidate(t3).is_ok());

        // Now all 3 are filled with 72, 80, 68. Lowest is 68.
        let t_better = Treasure::new(1, 13, 30.0, 40.0, 88.0);
        let displaced = buckets.insert_candidate(t_better).unwrap();
        assert!(displaced.is_some());
        assert_eq!(displaced.unwrap().score, 68.0);
        assert_eq!(buckets.displacements, 1);
    }
}
