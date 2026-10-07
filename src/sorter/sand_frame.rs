//! The picture frame around the sand painting. Download, load, and analysis share one pour.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

use crate::editor::{ModelProgress, PetUpdated};
use crate::pet_state::{
    chamber_scan_line, chamber_shore_line, chamber_waking_line, CHAMBER_BROKEN, CHAMBER_CACHE,
    CHAMBER_CACHE_DETAIL, CHAMBER_CHOOSE, CHAMBER_SCAN, CHAMBER_SET, CHAMBER_SET_DETAIL, CHAMBER_SHORE,
    CHAMBER_STILL, CHAMBER_STILL_DETAIL, CHAMBER_WAKING,
};
use crate::field::FieldSig;
use crate::sorter::sand_canvas::{Cell, ChamberLayout, SandCanvas, RAIL_BOTTOM, RAIL_TOP, STILE, TEX_H, TEX_W};
use crate::sorter::sand_hanabi::{picture_grains, PourKind, SandConfig, SandParticle};

#[derive(Component)]
pub struct SandFrameRoot;

#[derive(Component)]
pub struct SandDuneCanvas;

#[derive(Component)]
pub struct SandStatusTitle;

#[derive(Component)]
pub struct SandStatusDetail;

#[derive(Component, Clone, Copy)]
pub enum FrameEdge {
    Top,
    Bottom,
    Left,
    Right,
    Glass,
    Canvas,
}

#[derive(Resource)]
pub struct SandProgressState {
    pub progress: ModelProgress,
    pub status_title: String,
    pub status_detail: String,
    pub dune_image: Handle<Image>,
    pub timer: f32,
    pub phase: String,
    pub busy: bool,
    pub theme_banner_timer: f32,
    pub theme_banner_name: Option<String>,
}

impl SandProgressState {
    pub fn announce_theme(&mut self, name: &str) {
        self.theme_banner_timer = 2.5;
        self.theme_banner_name = Some(name.to_string());
    }
}

impl Default for SandProgressState {
    fn default() -> Self {
        Self {
            progress: ModelProgress::Idle,
            status_title: CHAMBER_STILL.to_string(),
            status_detail: CHAMBER_STILL.to_string(),
            dune_image: Handle::default(),
            timer: 0.0,
            phase: "idle".to_string(),
            busy: false,
            theme_banner_timer: 0.0,
            theme_banner_name: None,
        }
    }
}

pub fn setup_sand_frame(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    state: &mut SandProgressState,
    canvas: &SandCanvas,
) {
    let pixels = canvas.to_rgba();
    let dune_handle = images.add(Image::new(
        Extent3d {
            width: TEX_W,
            height: TEX_H,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    state.dune_image = dune_handle.clone();

    let root = commands
        .spawn((
            SandFrameRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                right: px(0.0),
                top: px(0.0),
                bottom: px(0.0),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();

    spawn_edge(commands, FrameEdge::Top, Color::srgb(0.17, 0.12, 0.09), 8.0);
    spawn_edge(
        commands,
        FrameEdge::Bottom,
        Color::srgb(0.11, 0.08, 0.06),
        8.0,
    );
    spawn_edge(
        commands,
        FrameEdge::Left,
        Color::srgb(0.15, 0.10, 0.08),
        8.0,
    );
    spawn_edge(
        commands,
        FrameEdge::Right,
        Color::srgb(0.10, 0.07, 0.06),
        8.0,
    );
    spawn_edge(
        commands,
        FrameEdge::Glass,
        Color::srgb(0.05, 0.045, 0.06),
        0.1,
    );
    commands.spawn((
        FrameEdge::Canvas,
        SandDuneCanvas,
        Sprite {
            image: dune_handle,
            custom_size: Some(Vec2::new(248.0, 280.0)),
            ..default()
        },
        Transform::from_xyz(0.0, -6.0, 0.5),
    ));

    let header = commands
        .spawn((
            SandStatusTitle,
            Text::new(CHAMBER_STILL),
            TextFont {
                font_size: FontSize::Px(11.0),
                ..default()
            },
            TextColor(Color::srgb(0.96, 0.88, 0.65)),
            TextShadow {
                offset: Vec2::new(1.0, 1.0),
                color: Color::BLACK,
            },
            Node {
                position_type: PositionType::Absolute,
                top: px(6.0),
                height: px(RAIL_TOP - 10.0),
                left: px(STILE + 6.0),
                right: px(STILE + 6.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();
    commands.entity(root).add_child(header);

    let detail = commands
        .spawn((
            SandStatusDetail,
            Text::new(CHAMBER_STILL),
            TextFont {
                font_size: FontSize::Px(10.0),
                ..default()
            },
            TextColor(Color::srgb(0.92, 0.90, 0.85)),
            TextShadow {
                offset: Vec2::new(1.0, 1.0),
                color: Color::BLACK,
            },
            Node {
                position_type: PositionType::Absolute,
                bottom: px(3.0),
                height: px(RAIL_BOTTOM - 8.0),
                left: px(STILE + 6.0),
                right: px(STILE + 6.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();
    commands.entity(root).add_child(detail);
}

fn spawn_edge(commands: &mut Commands, edge: FrameEdge, color: Color, z: f32) {
    commands.spawn((
        edge,
        Sprite::from_color(color, Vec2::new(16.0, 16.0)),
        Transform::from_xyz(0.0, 0.0, z),
    ));
}

pub fn update_sand_frame(
    mut commands: Commands,
    time: Res<Time>,
    mut messages: MessageReader<PetUpdated>,
    mut state: ResMut<SandProgressState>,
    mut canvas: ResMut<SandCanvas>,
    mut config: ResMut<SandConfig>,
    mut layout: ResMut<ChamberLayout>,
    mut images: ResMut<Assets<Image>>,
    mut title_query: Query<(&mut Text, &mut Node), (With<SandStatusTitle>, Without<SandStatusDetail>)>,
    mut detail_query: Query<(&mut Text, &mut Node), (With<SandStatusDetail>, Without<SandStatusTitle>)>,
    mut pieces: Query<(&FrameEdge, &mut Sprite, &mut Transform), Without<SandStatusTitle>>,
    floaters: Query<(Entity, &SandParticle)>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let dt = time.delta_secs().min(0.05);
    state.timer += dt;
    if state.theme_banner_timer > 0.0 {
        state.theme_banner_timer -= dt;
    }

    if canvas.active_theme != config.active_theme {
        canvas.apply_theme(config.active_theme);
    }

    if let Ok(window) = windows.single() {
        *layout = ChamberLayout::from_window(window.width(), window.height());
        place_frame(window.width(), window.height(), *layout, &mut pieces);
    }

    let fc = config.active_theme.frame_colors();
    for (edge, mut sprite, _) in &mut pieces {
        match *edge {
            FrameEdge::Top => sprite.color = fc.top,
            FrameEdge::Bottom => sprite.color = fc.bottom,
            FrameEdge::Left | FrameEdge::Right => sprite.color = fc.sides,
            FrameEdge::Glass => sprite.color = fc.glass,
            FrameEdge::Canvas => {}
        }
    }

    for update in messages.read() {
        let prog = &update.0.progress;
        state.busy = update.0.pet.busy;
        let phase = phase_name(prog);
        if phase != state.phase {
            // A job that is already painting keeps its picture. A new video turns the frame.
            let continuing = matches!(
                state.phase.as_str(),
                "cache" | "download" | "load" | "scan" | "choose"
            ) && matches!(phase, "cache" | "download" | "load" | "scan" | "choose" | "ready");
            state.phase = phase.to_string();
            if !continuing {
                canvas.begin_turn();
            }
        }
        state.progress = prog.clone();
        config.is_streaming = false;
        match prog {
            ModelProgress::Idle => {
                config.flow_rate = 0.0;
                config.pour = PourKind::Idle;
                state.status_title = CHAMBER_STILL.to_string();
                state.status_detail = format!("{} • Theme: {}", CHAMBER_STILL_DETAIL, config.active_theme.display_name());
            }
            ModelProgress::CheckingCache => {
                config.flow_rate = 0.0;
                config.pour = PourKind::Picture;
                config.shore_pct = config.shore_pct.max(8.0);
                state.status_title = CHAMBER_CACHE.to_string();
                state.status_detail = CHAMBER_CACHE_DETAIL.to_string();
            }
            ModelProgress::Downloading { file, pct, .. } => {
                config.flow_rate = 0.0;
                config.pour = PourKind::Picture;
                config.shore_pct = *pct;
                state.status_title = CHAMBER_SHORE.to_string();
                state.status_detail = chamber_shore_line(file, *pct);
            }
            ModelProgress::Loading { pct, stage } => {
                config.flow_rate = 0.0;
                config.pour = PourKind::Picture;
                config.shore_pct = (*pct).clamp(0.0, 1.0) * 100.0;
                state.status_title = CHAMBER_WAKING.to_string();
                state.status_detail = chamber_waking_line(stage, *pct);
            }
            ModelProgress::Scanning { done, total } => {
                config.flow_rate = 0.0;
                config.pour = PourKind::Picture;
                let pct = if *total == 0 {
                    0.0
                } else {
                    (*done as f32 / *total as f32).clamp(0.0, 1.0) * 100.0
                };
                config.shore_pct = pct;
                state.status_title = CHAMBER_SCAN.to_string();
                state.status_detail = chamber_scan_line(*done, *total);
            }
            ModelProgress::Choosing => {
                config.flow_rate = 14.0;
                config.is_streaming = true;
                config.pour = PourKind::Picture;
                state.status_title = CHAMBER_CHOOSE.to_string();
                state.status_detail = choosing_detail_line(state.timer).to_string();
            }
            ModelProgress::Ready => {
                if update.0.pet.busy {
                    config.pour = PourKind::Picture;
                    config.flow_rate = 16.0;
                    config.is_streaming = true;
                    state.status_title = "Exporting Reel...".to_string();
                    state.status_detail = if update.0.pet.line.is_empty() {
                        "Encoding 1080x1920 & burning style".to_string()
                    } else {
                        update.0.pet.line.clone()
                    };
                } else if !config.hold_pour {
                    config.is_streaming = false;
                    config.flow_rate = 0.0;
                    config.pour = PourKind::Idle;
                    if canvas.occupied > 0 {
                        state.status_title = CHAMBER_SET.to_string();
                        state.status_detail = if update.0.pet.line.is_empty() {
                            CHAMBER_SET_DETAIL.to_string()
                        } else {
                            update.0.pet.line.clone()
                        };
                    } else {
                        state.status_title = CHAMBER_STILL.to_string();
                        state.status_detail = if update.0.pet.line.is_empty() {
                            CHAMBER_STILL_DETAIL.to_string()
                        } else {
                            update.0.pet.line.clone()
                        };
                    }
                }
            }
            ModelProgress::Error(err) => {
                config.flow_rate = 0.0;
                config.pour = PourKind::Idle;
                state.status_title = CHAMBER_BROKEN.to_string();
                state.status_detail = crate::pet_state::model_error_line(err);
            }
        }
    }

    if !config.is_streaming {
        for (entity, particle) in &floaters {
            if !particle.settle {
                commands.entity(entity).despawn();
            }
        }
    }

    if canvas.turning {
        canvas.drain_floor();
    }

    if !canvas.turning && picture_job(&state.progress, state.busy) && !config.is_streaming {
        release_settling_grains(&mut canvas, &mut config, dt, state.timer);
    } else if matches!(state.progress, ModelProgress::Ready) && !state.busy {
        canvas.clear_unsettled_above_limit();
    }

    canvas.step(48);

    if matches!(state.progress, ModelProgress::Choosing) {
        state.status_detail = choosing_detail_line(state.timer).to_string();
        config.is_streaming = true;
        config.pour = PourKind::Picture;
        if config.flow_rate < 3.5 {
            config.flow_rate = 6.0;
        }
        let prev_t = (state.timer - dt).max(0.0);
        if (state.timer * 6.0) as usize != (prev_t * 6.0) as usize {
            let col = canvas.advance_column();
            config.neck_column = col;
        }
    }

    let (display_title, display_detail) = if state.theme_banner_timer > 0.0 {
        let name = state
            .theme_banner_name
            .as_deref()
            .unwrap_or(config.active_theme.display_name());
        (
            format!("✦ {} ✦", name.to_uppercase()),
            format!("Visual equipped: {} sand & frame style", name),
        )
    } else {
        (state.status_title.clone(), state.status_detail.clone())
    };

    if let Ok((mut title, mut node)) = title_query.single_mut() {
        title.0 = display_title;
        node.top = px(6.0);
        node.height = px(RAIL_TOP - 10.0);
        node.left = px(STILE + 6.0);
        node.right = px(STILE + 6.0);
        node.justify_content = JustifyContent::Center;
        node.align_items = AlignItems::Center;
    }
    if let Ok((mut detail, mut node)) = detail_query.single_mut() {
        detail.0 = display_detail;
        node.bottom = px(3.0);
        node.height = px(RAIL_BOTTOM - 8.0);
        node.left = px(STILE + 6.0);
        node.right = px(STILE + 6.0);
        node.justify_content = JustifyContent::Center;
        node.align_items = AlignItems::Center;
    }

    if canvas.dirty {
        if let Some(mut img) = images.get_mut(&state.dune_image) {
            img.data = Some(canvas.to_rgba());
        }
        canvas.dirty = false;
    }
}

fn choosing_detail_line(timer: f32) -> &'static str {
    let step = ((timer / 3.0) as usize) % 5;
    match step {
        0 => "* Spotting high-tide hooks...",
        1 => "* Checking stop-scroll energy...",
        2 => "* Catching punchlines & speech...",
        3 => "* Framing 9:16 vertical focus...",
        _ => "* Polishing top 3 reel finds...",
    }
}

fn phase_name(progress: &ModelProgress) -> &'static str {
    match progress {
        ModelProgress::Idle => "idle",
        ModelProgress::CheckingCache => "cache",
        ModelProgress::Downloading { .. } => "download",
        ModelProgress::Loading { .. } => "load",
        ModelProgress::Scanning { .. } => "scan",
        ModelProgress::Choosing => "choose",
        ModelProgress::Ready => "ready",
        ModelProgress::Error(_) => "error",
    }
}

fn picture_job(progress: &ModelProgress, busy: bool) -> bool {
    match progress {
        ModelProgress::CheckingCache
        | ModelProgress::Downloading { .. }
        | ModelProgress::Loading { .. }
        | ModelProgress::Scanning { .. }
        | ModelProgress::Choosing => true,
        ModelProgress::Ready => busy,
        _ => false,
    }
}

/// A short stroke of the same grains a frame paints. They fall and stack.
fn release_settling_grains(canvas: &mut SandCanvas, config: &mut SandConfig, dt: f32, timer: f32) {
    if canvas.occupied >= ((crate::sorter::sand_canvas::GRID_W * crate::sorter::sand_canvas::GRID_H) as f32 * crate::sorter::sand_canvas::FILL_FRACTION * 0.95) as u32 {
        return;
    }
    config.spawn_accumulator += 10.0 * dt;
    let count = (config.spawn_accumulator as usize).min(4);
    if count == 0 {
        return;
    }
    config.spawn_accumulator -= count as f32;
    let sig = mineral_sig(timer);
    let col = canvas.advance_column();
    config.neck_column = col;
    let grains = picture_grains(config.active_theme, &sig, col);
    let stroke: Vec<Cell> = grains
        .iter()
        .take(count)
        .map(|rgb| Cell::with_base(rgb[0], rgb[1], rgb[2], sig.r, sig.g, sig.b))
        .collect();
    let oil = if sig.energy > 1.2 {
        let bead = crate::sorter::sand_hanabi::oil_from_sand(grains[0]);
        Some(Cell::oil(bead[0], bead[1], bead[2]))
    } else {
        None
    };
    canvas.pour_stream(col as i32, &stroke, oil);
}

fn mineral_sig(timer: f32) -> FieldSig {
    let (r, g, b) = organic_mineral_blend(timer);
    FieldSig {
        r,
        g,
        b,
        theta: 0.4,
        freq: 0.18,
        energy: 1.0,
    }
}

fn organic_mineral_blend(timer: f32) -> (u8, u8, u8) {
    // Smooth, continuous sinusoidal waves over 16-24s, harmoniously interpolating warm sand minerals
    let t = timer * 0.28;
    let w1 = (t * 0.6).sin() * 0.5 + 0.5;
    let w2 = (t * 1.3 + 0.8).cos() * 0.5 + 0.5;

    // Harmonious golden sandstone palette: amber, warm quartz, sunlit beige
    let r = 195.0 + w1 * 22.0 - w2 * 14.0;
    let g = 148.0 + w1 * 24.0 - w2 * 16.0;
    let b = 88.0 + w1 * 22.0 - w2 * 10.0;

    (
        r.round().clamp(0.0, 255.0) as u8,
        g.round().clamp(0.0, 255.0) as u8,
        b.round().clamp(0.0, 255.0) as u8,
    )
}

fn place_frame(
    window_w: f32,
    window_h: f32,
    layout: ChamberLayout,
    pieces: &mut Query<(&FrameEdge, &mut Sprite, &mut Transform), Without<SandStatusTitle>>,
) {
    for (edge, mut sprite, mut transform) in pieces.iter_mut() {
        match *edge {
            FrameEdge::Top => {
                sprite.custom_size = Some(Vec2::new(window_w, RAIL_TOP));
                transform.translation = Vec3::new(0.0, window_h * 0.5 - RAIL_TOP * 0.5, 8.0);
            }
            FrameEdge::Bottom => {
                sprite.custom_size = Some(Vec2::new(window_w, RAIL_BOTTOM));
                transform.translation = Vec3::new(0.0, -window_h * 0.5 + RAIL_BOTTOM * 0.5, 8.0);
            }
            FrameEdge::Left => {
                sprite.custom_size = Some(Vec2::new(STILE, window_h));
                transform.translation = Vec3::new(-window_w * 0.5 + STILE * 0.5, 0.0, 8.0);
            }
            FrameEdge::Right => {
                sprite.custom_size = Some(Vec2::new(STILE, window_h));
                transform.translation = Vec3::new(window_w * 0.5 - STILE * 0.5, 0.0, 8.0);
            }
            FrameEdge::Glass => {
                sprite.custom_size = Some(Vec2::new(layout.width, layout.height));
                transform.translation = Vec3::new(0.0, layout.offset_y, 0.1);
            }
            FrameEdge::Canvas => {
                sprite.custom_size = Some(Vec2::new(layout.width, layout.height));
                transform.translation = Vec3::new(0.0, layout.offset_y, 0.5);
            }
        }
    }
}

pub fn drag_picture_frame(
    mouse: Res<ButtonInput<MouseButton>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let w = window.width();
    let h = window.height();
    let is_on_border =
        cursor.x <= 22.0 || cursor.x >= w - 22.0 || cursor.y <= 42.0 || cursor.y >= h - 24.0;
    if is_on_border {
        window.start_drag_move();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organic_mineral_blend_maintains_smooth_warm_palette_without_jarring_jumps() {
        let mut prev = organic_mineral_blend(0.0);
        for i in 1..200 {
            let t = i as f32 * 0.1;
            let cur = organic_mineral_blend(t);
            // Verify channels are in warm sandstone range
            assert!(cur.0 >= 160, "red channel too low: {}", cur.0);
            assert!(cur.1 >= 110, "green channel too low: {}", cur.1);
            assert!(cur.2 >= 60, "blue channel too low: {}", cur.2);
            // Verify smooth delta: step of 0.1s should not jump by more than 5 in any channel
            let dr = (cur.0 as i16 - prev.0 as i16).abs();
            let dg = (cur.1 as i16 - prev.1 as i16).abs();
            let db = (cur.2 as i16 - prev.2 as i16).abs();
            assert!(dr <= 5 && dg <= 5 && db <= 5, "jarring jump at t={t}: dr={dr}, dg={dg}, db={db}");
            prev = cur;
        }
    }

    #[test]
    fn announce_theme_sets_timer_and_name() {
        let mut state = SandProgressState::default();
        assert_eq!(state.theme_banner_timer, 0.0);
        assert!(state.theme_banner_name.is_none());

        state.announce_theme("Coral Pink");
        assert!(state.theme_banner_timer > 2.0);
        assert_eq!(state.theme_banner_name.as_deref(), Some("Coral Pink"));
    }
}
