//! Ribbons of sand. Grains are sprites only while they fall. Landing writes the glass picture.

use bevy::prelude::*;

use crate::field::FieldSig;
use crate::sorter::sand_canvas::{Cell, ChamberLayout, SandCanvas, GRID_W};

pub const GRAIN_CAP: usize = 180;

#[derive(Component)]
pub struct SandParticle {
    pub vel: Vec2,
    pub crystal: bool,
    pub rgb: [u8; 3],
    /// Picture grains join the glass when they land.
    pub settle: bool,
    /// Falls through the water instead of dropping like dry sand.
    pub liquid: bool,
}

pub const STROKE: usize = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandTheme {
    GoldenSunrise,
    Bioluminescent,
    VolcanicObsidian,
    CoralPink,
    Snow,
    Halloween,
    Space,
}

impl SandTheme {
    pub fn catalog() -> &'static [SandTheme] {
        &[
            Self::GoldenSunrise,
            Self::Bioluminescent,
            Self::VolcanicObsidian,
            Self::CoralPink,
            Self::Snow,
            Self::Halloween,
            Self::Space,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::GoldenSunrise => "Golden Sunrise",
            Self::Bioluminescent => "Bioluminescent Tide",
            Self::VolcanicObsidian => "Volcanic Obsidian",
            Self::CoralPink => "Coral Pink",
            Self::Snow => "Snow",
            Self::Halloween => "Halloween",
            Self::Space => "Space",
        }
    }

    pub fn amber_cost(&self) -> f64 {
        match self {
            Self::GoldenSunrise => 0.0,
            Self::Bioluminescent => 80.0,
            Self::VolcanicObsidian => 120.0,
            Self::CoralPink => 120.0,
            Self::Snow => 200.0,
            Self::Halloween => 200.0,
            Self::Space => 200.0,
        }
    }

    /// Mix the frame color into the theme so the picture still shows the footage.
    pub fn stamp_color(&self, r: u8, g: u8, b: u8) -> [u8; 3] {
        let (tr, tg, tb, keep) = match self {
            Self::GoldenSunrise => (210, 160, 70, 0.62),
            Self::Bioluminescent => (30, 190, 220, 0.58),
            Self::VolcanicObsidian => (70, 36, 32, 0.58),
            Self::CoralPink => (220, 110, 140, 0.58),
            Self::Snow => (245, 248, 252, 0.34),
            Self::Halloween => (96, 42, 130, 0.55),
            Self::Space => (16, 22, 64, 0.55),
        };
        mix_rgb(r, g, b, tr, tg, tb, keep)
    }

    pub fn tint_color(&self, sig_color: Color) -> Color {
        let s = sig_color.to_srgba();
        let rgb = self.stamp_color(
            (s.red.clamp(0.0, 1.0) * 255.0) as u8,
            (s.green.clamp(0.0, 1.0) * 255.0) as u8,
            (s.blue.clamp(0.0, 1.0) * 255.0) as u8,
        );
        Color::srgb(rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0)
    }

    pub fn pour_style(self) -> PourStyle {
        match self {
            Self::GoldenSunrise => PourStyle {
                gravity: 110.0,
                drift: 10.0,
                grain: 3.6,
                lift: 0.0,
                wander: 10.0,
            },
            Self::Bioluminescent => PourStyle {
                gravity: 62.0,
                drift: 16.0,
                grain: 3.2,
                lift: 8.0,
                wander: 12.0,
            },
            Self::VolcanicObsidian => PourStyle {
                gravity: 170.0,
                drift: 6.0,
                grain: 4.2,
                lift: 0.0,
                wander: 8.0,
            },
            Self::CoralPink => PourStyle {
                gravity: 70.0,
                drift: 28.0,
                grain: 3.4,
                lift: 0.0,
                wander: 16.0,
            },
            Self::Snow => PourStyle {
                gravity: 26.0,
                drift: 36.0,
                grain: 5.6,
                lift: 0.0,
                wander: 20.0,
            },
            Self::Halloween => PourStyle {
                gravity: 80.0,
                drift: 12.0,
                grain: 3.8,
                lift: 55.0,
                wander: 12.0,
            },
            Self::Space => PourStyle {
                gravity: 18.0,
                drift: 22.0,
                grain: 3.2,
                lift: 4.0,
                wander: 28.0,
            },
        }
    }

    pub fn frame_colors(&self) -> FrameThemeColors {
        match self {
            Self::GoldenSunrise => FrameThemeColors {
                top: Color::srgb(0.22, 0.15, 0.10),
                bottom: Color::srgb(0.14, 0.09, 0.06),
                sides: Color::srgb(0.18, 0.12, 0.08),
                glass: Color::srgba(0.08, 0.06, 0.04, 0.12),
            },
            Self::Bioluminescent => FrameThemeColors {
                top: Color::srgb(0.06, 0.16, 0.22),
                bottom: Color::srgb(0.04, 0.10, 0.15),
                sides: Color::srgb(0.05, 0.13, 0.18),
                glass: Color::srgba(0.02, 0.12, 0.18, 0.15),
            },
            Self::VolcanicObsidian => FrameThemeColors {
                top: Color::srgb(0.16, 0.08, 0.08),
                bottom: Color::srgb(0.10, 0.05, 0.05),
                sides: Color::srgb(0.13, 0.06, 0.06),
                glass: Color::srgba(0.10, 0.04, 0.04, 0.15),
            },
            Self::CoralPink => FrameThemeColors {
                top: Color::srgb(0.24, 0.12, 0.15),
                bottom: Color::srgb(0.16, 0.07, 0.09),
                sides: Color::srgb(0.20, 0.09, 0.12),
                glass: Color::srgba(0.14, 0.06, 0.10, 0.12),
            },
            Self::Snow => FrameThemeColors {
                top: Color::srgb(0.20, 0.22, 0.26),
                bottom: Color::srgb(0.12, 0.14, 0.18),
                sides: Color::srgb(0.16, 0.18, 0.22),
                glass: Color::srgba(0.12, 0.16, 0.22, 0.15),
            },
            Self::Halloween => FrameThemeColors {
                top: Color::srgb(0.18, 0.08, 0.22),
                bottom: Color::srgb(0.11, 0.04, 0.14),
                sides: Color::srgb(0.14, 0.06, 0.18),
                glass: Color::srgba(0.10, 0.04, 0.15, 0.15),
            },
            Self::Space => FrameThemeColors {
                top: Color::srgb(0.07, 0.08, 0.20),
                bottom: Color::srgb(0.04, 0.05, 0.13),
                sides: Color::srgb(0.05, 0.06, 0.16),
                glass: Color::srgba(0.05, 0.06, 0.18, 0.15),
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrameThemeColors {
    pub top: Color,
    pub bottom: Color,
    pub sides: Color,
    pub glass: Color,
}

#[derive(Clone, Copy, Debug)]
pub struct PourStyle {
    pub gravity: f32,
    pub drift: f32,
    pub grain: f32,
    pub lift: f32,
    pub wander: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PourKind {
    Idle,
    Shore,
    Crystal,
    Picture,
}

#[derive(Resource)]
pub struct SandConfig {
    pub active_theme: SandTheme,
    pub flow_rate: f32,
    pub last_signature: Option<FieldSig>,
    pub is_streaming: bool,
    pub spawn_accumulator: f32,
    pub pour: PourKind,
    pub neck_column: usize,
    pub hold_pour: bool,
    pub shore_pct: f32,
    pub comb_radius: i32,
}

impl Default for SandConfig {
    fn default() -> Self {
        Self {
            active_theme: SandTheme::GoldenSunrise,
            flow_rate: 0.0,
            last_signature: None,
            is_streaming: false,
            spawn_accumulator: 0.0,
            pour: PourKind::Idle,
            neck_column: 0,
            hold_pour: false,
            shore_pct: 0.0,
            comb_radius: 4,
        }
    }
}

pub fn spawn_ribbon(
    commands: &mut Commands,
    sig: &FieldSig,
    theme: SandTheme,
    layout: ChamberLayout,
    neck_column: usize,
    count: usize,
    kind: PourKind,
    alive: usize,
    time_s: f32,
) -> usize {
    let room = GRAIN_CAP.saturating_sub(alive);
    let count = count.min(room).min(16);
    if count == 0 {
        return 0;
    }
    let style = theme.pour_style();
    let picture = kind == PourKind::Picture;
    let neck = layout.world_x(neck_column.min(GRID_W - 1));
    let wander = if picture {
        (time_s * 0.85).sin() * layout.cell_w() * 10.0
    } else {
        (time_s * 1.7).sin() * style.wander
    };
    let span = if picture {
        layout.cell_w() * 28.0
    } else {
        layout.cell_w() * 14.0
    };
    let mut seed = (sig.r as u64)
        .wrapping_mul(41)
        .wrapping_add(sig.g as u64 * 23)
        .wrapping_add((sig.energy.to_bits() as u64) << 8)
        .wrapping_add(neck_column as u64 * 17);

    for i in 0..count {
        seed = seed.wrapping_mul(0x517cc1b727220a95).wrapping_add(i as u64 * 13 + 7);
        let along = ((seed >> 16) % 1000) as f32 / 1000.0 - 0.5;
        let x = neck + wander + along * span + sig.theta.cos() * layout.cell_w();
        let y = layout.top() - 4.0 + (((seed >> 8) % 10) as f32);
        let rgb = grain_rgb(theme, sig, kind, i, seed);
        let clump = if i % 6 == 0 { 1.7 } else { 1.0 };
        let size = if picture {
            (2.1 + ((seed % 5) as f32) * 0.45).clamp(2.0, 4.2)
        } else {
            (style.grain * clump).clamp(3.0, 8.0)
        };
        let lift = if picture {
            -22.0 - ((seed % 7) as f32)
        } else if style.lift > 0.0 && i % 3 == 0 {
            style.lift
        } else {
            -(78.0 + sig.freq * 36.0)
        };
        let drift = if picture {
            ((seed >> 4) % 5) as f32 - 2.0
        } else {
            sig.theta.cos() * style.drift + (((seed >> 4) % 17) as f32) - 8.0
        };
        commands.spawn((
            SandParticle {
                vel: Vec2::new(drift, lift),
                crystal: kind == PourKind::Crystal,
                rgb,
                settle: picture,
                liquid: picture,
            },
            Sprite {
                color: Color::srgb(rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0),
                custom_size: Some(Vec2::splat(size)),
                ..default()
            },
            Transform::from_xyz(x, y, 3.2),
        ));
    }
    count
}

fn grain_rgb(theme: SandTheme, sig: &FieldSig, kind: PourKind, i: usize, seed: u64) -> [u8; 3] {
    let (r, g, b) = match kind {
        PourKind::Crystal => {
            if i % 2 == 0 {
                (150, 220, 255)
            } else {
                (255, 228, 160)
            }
        }
        PourKind::Shore | PourKind::Idle => {
            let t = ((seed % 100) as f32) / 100.0;
            (
                (42.0 + t * 170.0) as u8,
                (36.0 + t * 130.0) as u8,
                (40.0 + t * 40.0) as u8,
            )
        }
        PourKind::Picture => {
            let stamped = theme.stamp_color(sig.r, sig.g, sig.b);
            return near_sand(stamped, seed);
        }
    };
    let mut rgb = theme.stamp_color(r, g, b);
    if theme == SandTheme::Halloween && i % 11 == 0 {
        rgb = mix_rgb(rgb[0], rgb[1], rgb[2], 236, 230, 214, 0.25);
    }
    if theme == SandTheme::Bioluminescent {
        rgb[1] = rgb[1].saturating_add(12);
        rgb[2] = rgb[2].saturating_add(16);
    }
    if theme == SandTheme::Space && i % 7 == 0 {
        rgb = mix_rgb(rgb[0], rgb[1], rgb[2], 236, 242, 255, 0.2);
    }
    rgb
}

/// One frame, spread into mixed minerals so a flat video still reads as sand.
pub fn picture_grains(theme: SandTheme, sig: &FieldSig, column: usize) -> [[u8; 3]; STROKE] {
    let mut out = [[0u8; 3]; STROKE];
    let mut seed = (sig.r as u64)
        .wrapping_mul(41)
        .wrapping_add(sig.g as u64 * 23)
        .wrapping_add((sig.energy.to_bits() as u64) << 8)
        .wrapping_add((column as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    for (i, slot) in out.iter_mut().enumerate() {
        seed = seed.wrapping_mul(0x517cc1b727220a95).wrapping_add(i as u64 * 13 + 7);
        *slot = grain_rgb(theme, sig, PourKind::Picture, i, seed);
    }
    out
}

/// A few levels of grain noise. The frame color stays recognizable.
pub fn near_sand(rgb: [u8; 3], seed: u64) -> [u8; 3] {
    let n = ((seed % 11) as i16) - 5;
    [
        (rgb[0] as i16 + n).clamp(0, 255) as u8,
        (rgb[1] as i16 + n / 2).clamp(0, 255) as u8,
        (rgb[2] as i16 - n / 2).clamp(0, 255) as u8,
    ]
}

/// Same hue as the sand, warmer and darker, so the bead reads as oil.
pub fn oil_from_sand(rgb: [u8; 3]) -> [u8; 3] {
    [
        (rgb[0] as u16 * 3 / 4 + 28).min(255) as u8,
        (rgb[1] as u16 * 2 / 5 + 10).min(255) as u8,
        (rgb[2] as u16 / 5).min(255) as u8,
    ]
}

pub fn update_continuous_sand_stream(
    mut commands: Commands,
    time: Res<Time>,
    mut sand_config: ResMut<SandConfig>,
    layout: Res<ChamberLayout>,
    alive: Query<(), With<SandParticle>>,
) {
    let dt = time.delta_secs().min(0.05);
    if sand_config.pour == PourKind::Idle
        || sand_config.hold_pour
        || !sand_config.is_streaming
        || sand_config.flow_rate <= 0.01
    {
        return;
    }
    let grains_per_sec = sand_config.flow_rate * 72.0;
    sand_config.spawn_accumulator += grains_per_sec * dt;
    let to_spawn = sand_config.spawn_accumulator as usize;
    if to_spawn == 0 {
        return;
    }
    sand_config.spawn_accumulator -= to_spawn as f32;

    let default_sig = FieldSig {
        r: 210,
        g: 170,
        b: 90,
        theta: 0.4,
        freq: 0.18,
        energy: 2.0,
    };
    let sig = sand_config.last_signature.unwrap_or(default_sig);
    let spawned = spawn_ribbon(
        &mut commands,
        &sig,
        sand_config.active_theme,
        *layout,
        sand_config.neck_column,
        to_spawn,
        sand_config.pour,
        alive.iter().count(),
        time.elapsed_secs(),
    );
    let _ = spawned;
    if sand_config.flow_rate > 3.5 && sand_config.pour == PourKind::Picture {
        sand_config.flow_rate -= 0.35 * dt;
    }
}

pub fn update_sand_physics(
    mut commands: Commands,
    time: Res<Time>,
    mut canvas: ResMut<SandCanvas>,
    layout: Res<ChamberLayout>,
    config: Res<SandConfig>,
    mut particles: Query<(Entity, &mut SandParticle, &mut Transform)>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
) {
    let dt = time.delta_secs().min(0.05);
    let style = config.active_theme.pour_style();
    let window = windows.iter().next();
    let cursor_world = window.and_then(|w| {
        w.cursor_position().map(|cursor| {
            Vec2::new(cursor.x - w.width() * 0.5, w.height() * 0.5 - cursor.y)
        })
    });
    let sweeping = mouse_buttons.pressed(MouseButton::Left);
    if sweeping && let Some(cursor) = cursor_world {
        let col = layout.column_at(cursor.x) as i32;
        let row = layout.row_at(cursor.y) as i32;
        canvas.brush(col, row, config.comb_radius.max(1));
    }

    for (entity, mut p, mut transform) in &mut particles {
        let x = transform.translation.x;
        let y = transform.translation.y;
        if sweeping && let Some(cursor) = cursor_world {
            let pos = Vec2::new(x, y);
            if pos.distance(cursor) < 28.0 {
                let repel = (pos - cursor).normalize_or_zero();
                p.vel = repel * 90.0 + Vec2::new(style.drift, 16.0);
            }
        }

        if p.liquid {
            p.vel.y -= 36.0 * dt;
            p.vel.x += (time.elapsed_secs() * 2.4 + x * 0.06).sin() * 14.0 * dt;
            p.vel *= 0.98;
        } else if config.active_theme == SandTheme::Snow || config.active_theme == SandTheme::Space {
            let sway = if config.active_theme == SandTheme::Space { 18.0 } else { 40.0 };
            p.vel.x += (time.elapsed_secs() * 2.2 + x * 0.05).sin() * sway * dt;
            p.vel.y -= style.gravity * dt;
        } else if config.active_theme == SandTheme::Halloween && p.vel.y > 0.0 {
            p.vel.y -= style.gravity * 0.35 * dt;
        } else {
            p.vel.y -= style.gravity * dt;
        }
        if config.active_theme == SandTheme::CoralPink {
            p.vel.x += (time.elapsed_secs() + y * 0.02).cos() * style.drift * dt;
        }

        transform.translation.x += p.vel.x * dt;
        transform.translation.y += p.vel.y * dt;

        let col = layout.column_at(transform.translation.x);
        let surface = layout.surface_y(canvas.pile_top(col));
        let below_glass = transform.translation.y < layout.bottom() - 8.0;
        if transform.translation.y <= surface || below_glass {
            if !below_glass && p.settle {
                let cell = if p.liquid {
                    Cell::rgb(p.rgb[0], p.rgb[1], p.rgb[2])
                } else if p.crystal {
                    Cell::oil(p.rgb[0], p.rgb[1], p.rgb[2])
                } else {
                    Cell::rgb(p.rgb[0], p.rgb[1], p.rgb[2])
                };
                canvas.deposit(col as i32, cell);
            }
            commands.entity(entity).despawn();
        }
    }
}

fn mix_rgb(r: u8, g: u8, b: u8, tr: u8, tg: u8, tb: u8, keep: f32) -> [u8; 3] {
    let k = keep.clamp(0.0, 1.0);
    let ch = |s: u8, t: u8| -> u8 {
        (s as f32 * k + t as f32 * (1.0 - k)).round().clamp(0.0, 255.0) as u8
    };
    [ch(r, tr), ch(g, tg), ch(b, tb)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldSig;
    use crate::sorter::sand_canvas::SandCanvas;

    #[test]
    fn theme_color_tinting_produces_valid_colors() {
        let base = Color::srgb(0.5, 0.4, 0.3);
        for theme in SandTheme::catalog() {
            let tinted = theme.tint_color(base);
            let s = tinted.to_srgba();
            assert!(s.red >= 0.0 && s.red <= 1.0);
            assert!(s.green >= 0.0 && s.green <= 1.0);
            assert!(s.blue >= 0.0 && s.blue <= 1.0);
        }
    }

    #[test]
    fn sand_config_starts_quiet() {
        let config = SandConfig::default();
        assert!(!config.is_streaming);
        assert_eq!(config.flow_rate, 0.0);
    }

    #[test]
    fn picture_grains_shift_color_inside_one_signature() {
        let sig = FieldSig {
            r: 190,
            g: 160,
            b: 90,
            theta: 0.4,
            freq: 0.2,
            energy: 3.0,
        };
        let grains = picture_grains(SandTheme::GoldenSunrise, &sig, 12);
        let stamped = SandTheme::GoldenSunrise.stamp_color(sig.r, sig.g, sig.b);
        let distinct = grains
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert!(distinct >= 4, "distinct {distinct}");
        for grain in grains {
            for (got, src) in grain.iter().zip(stamped) {
                assert!((*got as i16 - src as i16).abs() <= 8, "{grain:?} vs {stamped:?}");
            }
        }
        let oil = oil_from_sand(stamped);
        let sand_sum = stamped.iter().map(|c| *c as u16).sum::<u16>();
        let oil_sum = oil.iter().map(|c| *c as u16).sum::<u16>();
        assert!(oil_sum < sand_sum);
        assert!(oil[0] > oil[2]);
    }

    #[test]
    fn signature_color_survives_the_theme_mix_in_the_cell() {
        for theme in SandTheme::catalog() {
            let mixed = theme.stamp_color(220, 40, 30);
            let mut canvas = SandCanvas::new();
            assert!(canvas.deposit(1, crate::sorter::sand_canvas::Cell::rgb(mixed[0], mixed[1], mixed[2])));
            let cell = canvas.cell(1, 0).unwrap();
            assert!(cell.r > cell.b, "{theme:?} red {} blue {}", cell.r, cell.b);
            assert!(cell.r >= 80, "{theme:?} red {}", cell.r);
        }
    }

    #[test]
    fn frame_colors_are_valid_and_distinct_for_all_themes() {
        let mut top_colors = Vec::new();
        for theme in SandTheme::catalog() {
            let fc = theme.frame_colors();
            let top_rgba = fc.top.to_srgba();
            assert!(top_rgba.alpha > 0.0);
            assert!(fc.glass.to_srgba().alpha > 0.0);
            top_colors.push((top_rgba.red, top_rgba.green, top_rgba.blue));
        }
        // Ensure all themes have their own distinct frame top color
        let unique_count = top_colors.iter().map(|&(r, g, b)| ((r * 100.0) as i32, (g * 100.0) as i32, (b * 100.0) as i32)).collect::<std::collections::HashSet<_>>().len();
        assert_eq!(unique_count, SandTheme::catalog().len(), "every theme must have distinct frame styling");
    }

    #[test]
    fn continuous_sand_stream_spawns_grains_for_picture_pour() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(ChamberLayout::from_window(280.0, 360.0));
        let mut config = SandConfig::default();
        config.is_streaming = true;
        config.pour = PourKind::Picture;
        config.flow_rate = 14.0;
        app.insert_resource(config);
        app.add_systems(Update, update_continuous_sand_stream);

        app.update();
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.update();

        let mut query = app.world_mut().query::<&SandParticle>();
        let particle_count = query.iter(app.world()).count();
        assert!(particle_count > 0, "expected sand particles to spawn during picture pour, found none");
    }
}
