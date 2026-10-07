//! Sediment physics, particle simulation, and floating feedback text.

use bevy::prelude::*;

#[derive(Component)]
pub struct DustParticle {
    pub vel: Vec2,
    pub life: f32,
    pub max_life: f32,
}

#[derive(Component)]
pub struct FloatingText {
    pub vel: Vec2,
    pub life: f32,
    pub max_life: f32,
}

pub fn spawn_dust_puff(commands: &mut Commands, origin: Vec2, count: usize, color: Color) {
    let mut rng_seed = (origin.x.to_bits() ^ origin.y.to_bits()) as u64;
    for i in 0..count {
        rng_seed = rng_seed.wrapping_mul(0x517cc1b727220a95).wrapping_add(i as u64);
        let angle = ((rng_seed % 360) as f32).to_radians();
        let speed = 20.0 + ((rng_seed >> 8) % 40) as f32;
        let vel = Vec2::new(angle.cos() * speed, angle.sin() * speed + 15.0);

        commands.spawn((
            DustParticle {
                vel,
                life: 0.0,
                max_life: 0.45 + (((rng_seed >> 16) % 20) as f32) * 0.01,
            },
            Sprite {
                color,
                custom_size: Some(Vec2::splat(3.0 + ((rng_seed % 4) as f32))),
                ..default()
            },
            Transform::from_translation(origin.extend(4.0)),
        ));
    }
}

pub fn spawn_floating_text(
    commands: &mut Commands,
    origin: Vec2,
    text: &str,
    color: Color,
) {
    commands.spawn((
        FloatingText {
            vel: Vec2::new(0.0, 32.0),
            life: 0.0,
            max_life: 1.1,
        },
        Text2d::new(text),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(color),
        TextShadow {
            offset: Vec2::new(1.0, 1.0),
            color: Color::BLACK,
        },
        Transform::from_translation(origin.extend(6.0)),
    ));
}

pub fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut DustParticle, &mut Transform, &mut Sprite), Without<FloatingText>>,
    mut texts: Query<(Entity, &mut FloatingText, &mut Transform, &mut TextColor), Without<DustParticle>>,
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
        p.vel.y -= 30.0 * dt; // slight gravity

        let alpha = 1.0 - (p.life / p.max_life);
        let current = sprite.color.to_srgba();
        sprite.color = Color::srgba(current.red, current.green, current.blue, alpha * 0.85);
    }

    for (entity, mut t, mut transform, mut text_color) in &mut texts {
        t.life += dt;
        if t.life >= t.max_life {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.y += t.vel.y * dt;
        let alpha = (1.0 - (t.life / t.max_life)).clamp(0.0, 1.0);
        let current = text_color.0.to_srgba();
        text_color.0 = Color::srgba(current.red, current.green, current.blue, alpha);
    }
}
