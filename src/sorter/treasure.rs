//! Shiny Beachcombing Relics representing discovered Video Short candidates.
//!
//! As video moments are analyzed, they manifest as buried treasures beneath the sand,
//! with tier, rarity, and luster directly determined by their LLM editorial score.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreasureTier {
    Shell,        // 60 - 69
    SilverCoin,   // 70 - 79
    GoldDoubloon, // 80 - 89
    KrakenRelic,  // 90 - 100
}

impl TreasureTier {
    pub fn from_score(score: f64) -> Self {
        if score >= 90.0 {
            Self::KrakenRelic
        } else if score >= 80.0 {
            Self::GoldDoubloon
        } else if score >= 70.0 {
            Self::SilverCoin
        } else {
            Self::Shell
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Shell => "[Shell]",
            Self::SilverCoin => "[Silver]",
            Self::GoldDoubloon => "[Gold]",
            Self::KrakenRelic => "[Relic]",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Shell => Color::srgb(0.95, 0.85, 0.75),
            Self::SilverCoin => Color::srgb(0.75, 0.85, 0.95),
            Self::GoldDoubloon => Color::srgb(1.0, 0.85, 0.25),
            Self::KrakenRelic => Color::srgb(0.4, 0.9, 1.0),
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct Treasure {
    pub chore_id: i64,
    pub window_id: i64,
    pub start_s: f64,
    pub end_s: f64,
    pub score: f64,
    pub tier: TreasureTier,
    pub name: String,
    pub buried_depth: f32, // 1.0 = completely covered, 0.0 = fully unearthed
    pub in_bucket: Option<usize>, // Which bucket index (0, 1, 2) it occupies
    pub gleam_timer: f32,
    pub glint_id: u64,
}

impl Treasure {
    pub fn new(chore_id: i64, window_id: i64, start_s: f64, end_s: f64, score: f64) -> Self {
        let tier = TreasureTier::from_score(score);
        let name = match tier {
            TreasureTier::Shell => format!("Spiral Conch [{:.1}s-{:.1}s]", start_s, end_s),
            TreasureTier::SilverCoin => {
                format!("Spanish Silver Real [{:.1}s-{:.1}s]", start_s, end_s)
            }
            TreasureTier::GoldDoubloon => {
                format!("Sunken Gold Doubloon [{:.1}s-{:.1}s]", start_s, end_s)
            }
            TreasureTier::KrakenRelic => {
                format!("Starlight Kraken Crown [{:.1}s-{:.1}s]", start_s, end_s)
            }
        };

        Self {
            chore_id,
            window_id,
            start_s,
            end_s,
            score,
            tier,
            name,
            buried_depth: 0.85,
            in_bucket: None,
            gleam_timer: 0.0,
            glint_id: 0,
        }
    }

    pub fn sell_amber(&self) -> f64 {
        self.tier.sell_amber()
    }
}

impl TreasureTier {
    pub fn sell_amber(&self) -> f64 {
        match self {
            Self::Shell => 15.0,
            Self::SilverCoin => 30.0,
            Self::GoldDoubloon => 60.0,
            Self::KrakenRelic => 120.0,
        }
    }
}

#[derive(Component)]
pub struct Unearthing {
    pub progress: f32,
    pub duration: f32,
    pub start_y: f32,
    pub target_y: f32,
}

#[derive(Component)]
pub struct SparkleParticle {
    pub vel: Vec2,
    pub life: f32,
    pub max_life: f32,
}

pub fn spawn_treasure_sparkle(commands: &mut Commands, origin: Vec2, color: Color) {
    for i in 0..6 {
        let angle = ((i as f32) * 60.0).to_radians();
        let speed = 25.0 + ((i % 3) as f32) * 10.0;
        let vel = Vec2::new(angle.cos() * speed, angle.sin() * speed);

        commands.spawn((
            SparkleParticle {
                vel,
                life: 0.0,
                max_life: 0.5,
            },
            Sprite {
                color,
                custom_size: Some(Vec2::splat(4.0)),
                ..default()
            },
            Transform::from_xyz(origin.x, origin.y, 4.0),
        ));
    }
}

pub fn update_treasure_gleams(
    mut commands: Commands,
    time: Res<Time>,
    mut canvas: ResMut<crate::sorter::sand_canvas::SandCanvas>,
    layout: Res<crate::sorter::sand_canvas::ChamberLayout>,
    mut goals: ResMut<crate::sorter::goals::BeachcomberGoals>,
    mut museum: ResMut<crate::sorter::museum::Museum>,
    mut treasures: Query<(Entity, &mut Treasure, &mut Transform, &mut Sprite, Option<&mut Unearthing>)>,
    mut sparkles: Query<
        (Entity, &mut SparkleParticle, &mut Transform, &mut Sprite),
        Without<Treasure>,
    >,
) {
    let dt = time.delta_secs();
    let fresh = canvas.newly_uncovered();

    for (entity, mut tr, mut transform, mut sprite, unearth_opt) in &mut treasures {
        if let Some(mut unearth) = unearth_opt {
            unearth.progress = (unearth.progress + dt / unearth.duration).min(1.0);
            let t = 1.0 - (1.0 - unearth.progress).powi(2);
            transform.translation.y = unearth.start_y + (unearth.target_y - unearth.start_y) * t;
            let scale_t = (t * 7.0).max(1.0);
            sprite.custom_size = Some(Vec2::splat(scale_t));
            if unearth.progress >= 1.0 {
                let pos = transform.translation.truncate();
                spawn_treasure_sparkle(&mut commands, pos, tr.tier.color());
                commands.entity(entity).remove::<Unearthing>();
            }
            continue;
        }

        tr.gleam_timer += dt;
        if tr.glint_id != 0 && canvas.glint(tr.glint_id).is_none() {
            commands.entity(entity).despawn();
            continue;
        }
        let placed = canvas
            .glint(tr.glint_id)
            .map(|glint| (glint.buried_depth, glint.column));
        if let Some((depth, _)) = placed {
            tr.buried_depth = depth;
            if let Some(center) = canvas.jewel_center(&layout, tr.glint_id) {
                transform.translation.x = center.x;
                transform.translation.y = center.y;
                transform.translation.z = 6.0;
            }
        }
        if fresh.contains(&tr.glint_id) {
            let pos = transform.translation.truncate();
            goals.on_treasure_uncovered(tr.score, &mut museum.amber);
            spawn_treasure_sparkle(&mut commands, pos, tr.tier.color());
            crate::sorter::sifter::spawn_floating_text(
                &mut commands,
                pos,
                &format!("{} {:.0}", tr.tier.icon(), tr.score),
                tr.tier.color(),
            );
        }
        if tr.buried_depth <= 0.0 {
            let pulse = 1.0 + 0.12 * (tr.gleam_timer * 6.0).sin();
            sprite.custom_size = Some(Vec2::splat(18.0 * pulse));
            sprite.color = tr.tier.color();
            if (tr.gleam_timer * 10.0) as u32 % 8 == 0 {
                spawn_treasure_sparkle(
                    &mut commands,
                    transform.translation.truncate(),
                    tr.tier.color(),
                );
            }
        } else {
            let spark = 4.5 + 1.6 * (tr.gleam_timer * 5.0).sin().abs();
            sprite.custom_size = Some(Vec2::splat(spark));
            sprite.color = Color::srgb(1.0, 0.97, 0.9);
        }
    }

    // Update sparkle particles
    for (entity, mut p, mut transform, mut sprite) in &mut sparkles {
        p.life += dt;
        if p.life >= p.max_life {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.x += p.vel.x * dt;
        transform.translation.y += p.vel.y * dt;
        let alpha = 1.0 - (p.life / p.max_life);
        let cur = sprite.color.to_srgba();
        sprite.color = Color::srgba(cur.red, cur.green, cur.blue, alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treasure_tier_mapping_correctly_categorizes_scores() {
        assert_eq!(TreasureTier::from_score(65.0), TreasureTier::Shell);
        assert_eq!(TreasureTier::from_score(75.0), TreasureTier::SilverCoin);
        assert_eq!(TreasureTier::from_score(85.0), TreasureTier::GoldDoubloon);
        assert_eq!(TreasureTier::from_score(95.0), TreasureTier::KrakenRelic);
    }
}
