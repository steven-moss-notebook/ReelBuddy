//! Museum conservation table and procedural sifting engine.
//!
//! Blends the tactile sifting of Needle In A Haystack with the modular assembly
//! and curation of DinoBones: Tidy Up the Museum, driven by LLM video insights.

pub mod bucket;
pub mod cosmetics;
pub mod goals;
pub mod museum;
pub mod needle;
pub mod sand_canvas;
pub mod sand_frame;
pub mod sand_hanabi;
pub mod sifter;
pub mod specimen;
pub mod treasure;

pub use bucket::update_bucket_splashes;
pub use sand_frame::{drag_picture_frame, setup_sand_frame, update_sand_frame, SandProgressState};
pub use sand_hanabi::{update_continuous_sand_stream, update_sand_physics};
pub use sifter::update_particles;
pub use treasure::update_treasure_gleams;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

use crate::editor::{ChoreKind, CrateBatch, PetUpdated, SandFinds};
use crate::field::{self, Recipe};
use bucket::ShorelineBuckets;
use cosmetics::BeachcomberBoutique;
use goals::BeachcomberGoals;
use museum::Museum;

#[derive(Component)]
pub(crate) struct Worker;
use sand_canvas::{Cell, ChamberLayout, SandCanvas};
use sand_hanabi::{PourKind, SandConfig};
use sifter::{spawn_dust_puff, spawn_floating_text};
use specimen::socket_for_recipe;
use treasure::{Treasure, spawn_treasure_sparkle, Unearthing};

const SPRITE: u32 = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Job {
    Fall,
    Wait,
    Peel,
    Assemble,
    Chute,
    Drawer,
    Case,
}

#[derive(Component)]
pub(crate) struct Bit {
    recipe: Recipe,
    t: f64,
    footage: bool,
    job: Job,
    goal: Vec2,
    peel: f32,
    strokes: u8,
    sediment: f32,
    socket: usize,
}

#[derive(Component, Clone, Copy)]
pub(crate) enum Prop {
    Table,
    Chute,
    Drawer,
    Pedestal,
    SocketBracket(usize),
}

#[derive(Component)]
pub(crate) struct MuseumHudText;

#[derive(Component)]
pub(crate) struct BlueprintTitleText;

#[derive(Component)]
pub(crate) struct GalleryRoot;

#[derive(Component, Clone, Copy)]
pub struct ThemeButton(pub sand_hanabi::SandTheme);

#[derive(Resource)]
pub(crate) struct Table {
    clock: f64,
    next_drop: f64,
    serial: u64,
    hunger: u8,
    busy: bool,
    windows: Vec<(f64, f64)>,
    worker: Vec2,
    pace: f32,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            clock: 0.0,
            next_drop: 0.4,
            serial: 1,
            hunger: 40,
            busy: false,
            windows: Vec::new(),
            worker: Vec2::new(0.0, -80.0),
            pace: 0.0,
        }
    }
}

struct Stage {
    top: f32,
    table_y: f32,
    worker_y: f32,
    chute: Vec2,
    drawer: Vec2,
    case_y: f32,
    pedestal: Vec2,
}

pub fn boot(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let canvas = SandCanvas::new();
    let mut sand_state = SandProgressState::default();
    setup_sand_frame(&mut commands, &mut images, &mut sand_state, &canvas);
    commands.insert_resource(sand_state);
    commands.insert_resource(canvas);
    commands.insert_resource(ChamberLayout::from_window(crate::window::WIDTH, crate::window::HEIGHT));

    commands.insert_resource(Table::default());
    commands.insert_resource(Museum::default());
    commands.insert_resource(sand_hanabi::SandConfig::default());
    commands.insert_resource(bucket::ShorelineBuckets::default());
    commands.insert_resource(goals::BeachcomberGoals::default());
    commands.insert_resource(cosmetics::BeachcomberBoutique::default());
}

pub fn tick_museum(
    time: Res<Time>,
    mut museum: ResMut<Museum>,
    mut goals: ResMut<BeachcomberGoals>,
    mut boutique: ResMut<BeachcomberBoutique>,
    buckets: Res<ShorelineBuckets>,
    mut sand_config: ResMut<SandConfig>,
    mut canvas: ResMut<SandCanvas>,
    mut sand_state: ResMut<SandProgressState>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut hud_query: Query<&mut Text, (With<MuseumHudText>, Without<BlueprintTitleText>)>,
    mut banner_query: Query<&mut Text, (With<BlueprintTitleText>, Without<MuseumHudText>)>,
    gallery_query: Query<Entity, With<GalleryRoot>>,
    theme_buttons: Query<(&Interaction, &ThemeButton), Changed<Interaction>>,
) {
    let dt = time.delta_secs();
    // Passive museum economy
    museum.tick_economy(dt as f64);

    // Toggle Menu (Quests & Shoreline Log) with 'M' key
    if keys.just_pressed(KeyCode::KeyM) {
        museum.show_menu = !museum.show_menu;
        if museum.show_menu {
            museum.show_store = false;
        }
        museum.show_gallery = museum.show_menu || museum.show_store;
        for entity in &gallery_query {
            commands.entity(entity).despawn();
        }
    }

    // Toggle Store (Themes & Upgrades) with 'S' key
    if keys.just_pressed(KeyCode::KeyS) {
        museum.show_store = !museum.show_store;
        if museum.show_store {
            museum.show_menu = false;
        }
        museum.show_gallery = museum.show_menu || museum.show_store;
        for entity in &gallery_query {
            commands.entity(entity).despawn();
        }
    }

    // Keyboard shortcuts for upgrades & boutique:
    // 1 (Detector), 2 (Sieve), 3 (Cycle Hat), 4 (Cycle Sand Theme)
    let mut store_needs_refresh = false;
    if keys.just_pressed(KeyCode::Digit1) {
        if boutique.buy_detector(&mut museum.amber) {
            spawn_floating_text(&mut commands, Vec2::new(0.0, 0.0), &format!("Scanner Mk {}!", boutique.detector_level), Color::srgb(0.4, 0.9, 1.0));
            store_needs_refresh = true;
        } else if museum.buy_bellows() {
            spawn_floating_text(&mut commands, Vec2::new(0.0, 0.0), "Bellows Upgraded!", Color::srgb(0.5, 0.9, 1.0));
            store_needs_refresh = true;
        }
    }
    if keys.just_pressed(KeyCode::Digit2) {
        if boutique.buy_sieve(&mut museum.amber) {
            spawn_floating_text(&mut commands, Vec2::new(0.0, 0.0), &format!("Sifter Lv {}!", boutique.sieve_level), Color::srgb(1.0, 0.85, 0.3));
            store_needs_refresh = true;
        } else if museum.buy_magnet() {
            spawn_floating_text(&mut commands, Vec2::new(0.0, 0.0), "Magnet Upgraded!", Color::srgb(1.0, 0.8, 0.3));
            store_needs_refresh = true;
        }
    }
    if keys.just_pressed(KeyCode::Digit4) {
        let old_theme = sand_config.active_theme;
        let next_theme = boutique.cycle_theme(old_theme);
        if next_theme != old_theme {
            sand_config.active_theme = next_theme;
            canvas.apply_theme(next_theme);
            sand_state.announce_theme(next_theme.display_name());
            spawn_dust_puff(&mut commands, Vec2::new(0.0, 0.0), 16, next_theme.tint_color(Color::WHITE));
            spawn_floating_text(
                &mut commands,
                Vec2::new(0.0, 0.0),
                &format!("✦ {} ✦", next_theme.display_name()),
                Color::srgb(0.6, 1.0, 0.8),
            );
            store_needs_refresh = true;
        }
    }
    if store_needs_refresh && museum.show_store {
        for entity in &gallery_query {
            commands.entity(entity).despawn();
        }
    }
    sand_config.comb_radius = (boutique.detector_ping_radius() / 12.0).clamp(3.0, 8.0) as i32;

    for (interaction, ThemeButton(theme)) in &theme_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let worn = if boutique.owns_theme(*theme) {
            true
        } else {
            boutique.buy_theme(*theme, &mut museum.amber)
        };
        if worn {
            let changed = sand_config.active_theme != *theme;
            sand_config.active_theme = *theme;
            canvas.apply_theme(*theme);
            if changed {
                sand_state.announce_theme(theme.display_name());
                spawn_dust_puff(&mut commands, Vec2::new(0.0, 0.0), 16, theme.tint_color(Color::WHITE));
            }
            spawn_floating_text(
                &mut commands,
                Vec2::new(0.0, 0.0),
                &format!("✦ {} ✦", theme.display_name()),
                Color::srgb(0.6, 1.0, 0.8),
            );
            for entity in &gallery_query {
                commands.entity(entity).despawn();
            }
        }
    }

    // Update HUD text with Pearls
    if let Ok(mut hud) = hud_query.single_mut() {
        hud.0 = format!(
            "Museum: {} | Renown: {} | Amber: {:.0} | Pearls: {}",
            museum.completed_exhibits.len(),
            museum.renown,
            museum.amber,
            goals.total_pearls,
        );
    }

    // Update Blueprint / Bucket Banner
    if let Ok(mut banner) = banner_query.single_mut() {
        let best_score = buckets.slots[0].as_ref().map(|t| format!("{:.1}", t.score)).unwrap_or_else(|| "--".to_string());
        let filled_buckets = buckets.slots.iter().filter(|s| s.is_some()).count();
        banner.0 = format!(
            "Top Find: {} | Bucket: {}/3 | [M] Quests  [S] Store",
            best_score,
            filled_buckets,
        );
    }

    // Render Quests Menu or Store modal if toggled open
    if gallery_query.is_empty() {
        if museum.show_menu {
            render_menu(
                &mut commands,
                &goals,
                &buckets,
            );
        } else if museum.show_store {
            render_store(
                &mut commands,
                &museum,
                &boutique,
                &goals,
                sand_config.active_theme,
            );
        }
    }
}

pub fn ingest_crates(
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut table: ResMut<Table>,
    mut museum: ResMut<Museum>,
    mut sand_config: ResMut<SandConfig>,
    mut canvas: ResMut<SandCanvas>,
    layout: Res<ChamberLayout>,
    mut buckets: ResMut<ShorelineBuckets>,
    mut goals: ResMut<BeachcomberGoals>,
    mut batches: MessageReader<CrateBatch>,
    mut finds: MessageReader<SandFinds>,
    mut updates: MessageReader<PetUpdated>,
    bits: Query<Entity, With<Bit>>,
    mut buried: Query<&mut Treasure>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let stage = stage_of(window.width(), window.height());

    // Update LLM state and pet updates
    let mut latest = None;
    for update in updates.read() {
        latest = Some(update.0.clone());
    }
    if let Some(update) = latest {
        let became_busy = update.pet.busy && !table.busy;
        table.busy = update.pet.busy;
        table.hunger = update.pet.hunger;
        if let Some(chore) = &update.chore {
            museum.ingest_chore_insight(chore);
            table.windows = match &chore.kind {
                ChoreKind::Hunt { options } => options
                    .iter()
                    .filter(|option| option.end_s > option.start_s)
                    .map(|option| (option.start_s, option.end_s))
                    .collect(),
                _ => Vec::new(),
            };

            if let ChoreKind::Hunt { options } = &chore.kind {
                for opt in options {
                    let score = crate::score::combine(opt.h, opt.v, opt.e, opt.a, opt.p);
                    if score >= 60.0 {
                        canvas.attach_window(opt.start_s, opt.end_s, opt.window_id);
                        for mut tr in &mut buried {
                            if tr.start_s + 0.05 >= opt.start_s && tr.start_s <= opt.end_s + 0.05 {
                                tr.window_id = opt.window_id;
                                tr.chore_id = chore.id;
                            }
                        }
                        let candidate = Treasure::new(chore.id, opt.window_id, opt.start_s, opt.end_s, score);
                        match buckets.insert_candidate(candidate.clone()) {
                            Ok(Some(displaced)) => {
                                let b_pos = bucket::bucket_position(0);
                                bucket::spawn_bucket_splash(&mut commands, b_pos);
                                spawn_floating_text(&mut commands, b_pos, &format!("Splashed Out ({:.1})", displaced.score), Color::srgb(0.4, 0.9, 1.0));
                                goals.on_bucket_displaced(&mut museum.amber);
                                spawn_floating_text(&mut commands, b_pos + Vec2::new(0.0, 16.0), &format!("New Best Find ({:.1})!", score), Color::srgb(1.0, 0.85, 0.2));
                                goals.on_bucket_placed(score, &mut museum.amber);
                            }
                            Ok(None) => {
                                let b_pos = bucket::bucket_position(0);
                                spawn_treasure_sparkle(&mut commands, b_pos, candidate.tier.color());
                                spawn_floating_text(&mut commands, b_pos, &format!("Bucket Stored ({:.1})!", score), candidate.tier.color());
                                goals.on_bucket_placed(score, &mut museum.amber);
                            }
                            Err(_) => {}
                        }
                    }
                }
            }
        } else {
            table.windows = Vec::new();
        }
        if became_busy {
            for entity in &bits {
                commands.entity(entity).despawn();
            }
        }
    }

    let mut poured = 0usize;
    for batch in batches.read() {
        if let Some(first) = batch.0.first()
            && canvas.last_sample_t.is_some_and(|last| first.t + 0.5 < last)
        {
            canvas.begin_turn();
        }
        for spawn in &batch.0 {
            let sig = &spawn.sig;
            sand_config.last_signature = Some(*sig);
            if sand_config.hold_pour {
                canvas.note_sample(spawn.t, sand_config.neck_column);
            } else {
                let col = canvas.advance_column();
                canvas.note_sample(spawn.t, col);
                sand_config.neck_column = col;
                sand_config.is_streaming = false;
                sand_config.pour = PourKind::Picture;
                sand_config.flow_rate = 0.0;
                let grains = sand_hanabi::picture_grains(sand_config.active_theme, sig, col);
                let stroke: Vec<Cell> = grains
                    .iter()
                    .map(|rgb| Cell::with_base(rgb[0], rgb[1], rgb[2], sig.r, sig.g, sig.b))
                    .collect();
                let oil = if sig.energy > 1.2 {
                    let bead = sand_hanabi::oil_from_sand(grains[0]);
                    Some(Cell::oil(bead[0], bead[1], bead[2]))
                } else {
                    None
                };
                canvas.pour_stream(col as i32, &stroke, oil);
                let bubble_cap = 16u32;
                let mut bubbles = canvas.count_kind(sand_canvas::GrainKind::Bubble);
                let extra = if sig.energy > 0.8 {
                    2
                } else if sig.energy > 0.2 || sand_canvas::frame_bubble(sig.energy, col) {
                    1
                } else {
                    0
                };
                for b in 0..extra {
                    if bubbles >= bubble_cap {
                        break;
                    }
                    let bx = (col as i32 + 11 + b * 23 - (sig.theta * 6.0) as i32).rem_euclid(sand_canvas::GRID_W as i32);
                    canvas.pour(bx, Cell::bubble());
                    bubbles += 1;
                }
                let n = sand_hanabi::spawn_ribbon(
                    &mut commands,
                    sig,
                    sand_config.active_theme,
                    *layout,
                    col,
                    6,
                    PourKind::Picture,
                    poured,
                    time.elapsed_secs(),
                );
                poured += n;
                goals.on_sand_sifted(stroke.len() as u64 + n as u64, &mut museum.amber);
            }
        }
    }

    for batch in finds.read() {
        for find in &batch.0 {
            plant_find(&mut commands, &mut canvas, &layout, &sand_config, find.start_s, find.end_s, find.score);
        }
    }
}

fn plant_find(
    commands: &mut Commands,
    canvas: &mut SandCanvas,
    layout: &ChamberLayout,
    config: &SandConfig,
    start_s: f64,
    end_s: f64,
    score: f64,
) {
    if canvas.has_span(start_s) {
        return;
    }
    let mut candidate = Treasure::new(0, 0, start_s, end_s, score);
    let col = canvas
        .column_for_time(start_s)
        .unwrap_or(canvas.column_cursor)
        .min(sand_canvas::GRID_W - 1);
    let rgb = jewel_rgb(&candidate);
    let cover = config.active_theme.stamp_color(190, 150, 80);
    if canvas.pile_top(col) < 4 {
        let left_col = col.saturating_sub(1);
        let right_col = (col + 1).min(sand_canvas::GRID_W - 1);
        canvas.deposit(left_col as i32, Cell::with_base(cover[0], cover[1], cover[2], 190, 150, 80));
        canvas.deposit(col as i32, Cell::with_base(cover[0], cover[1], cover[2], 190, 150, 80));
        canvas.deposit(col as i32, Cell::with_base(cover[0], cover[1], cover[2], 190, 150, 80));
        canvas.deposit(right_col as i32, Cell::with_base(cover[0], cover[1], cover[2], 190, 150, 80));
    }
    let id = canvas.bury_section(col, start_s, score, rgb);
    candidate.glint_id = id;
    candidate.buried_depth = canvas.glint(id).map(|glint| glint.buried_depth).unwrap_or(1.0);
    let anchor = canvas
        .jewel_center(layout, id)
        .unwrap_or_else(|| Vec2::new(layout.world_x(col), layout.surface_y(canvas.pile_top(col))));

    // Celebratory sparkle bloom when reel candidate is discovered
    treasure::spawn_treasure_sparkle(commands, anchor, candidate.tier.color());
    sifter::spawn_floating_text(
        commands,
        anchor + Vec2::new(0.0, 12.0),
        &format!("{} {:.0}", candidate.tier.icon(), candidate.score),
        candidate.tier.color(),
    );

    commands.spawn((
        candidate.clone(),
        Unearthing {
            progress: 0.0,
            duration: 0.65,
            start_y: anchor.y - 7.0,
            target_y: anchor.y,
        },
        Sprite {
            color: candidate.tier.color(),
            custom_size: Some(Vec2::splat(1.0)),
            ..default()
        },
        Transform::from_xyz(anchor.x, anchor.y - 7.0, 4.5),
    ));
}

fn jewel_rgb(candidate: &Treasure) -> [u8; 3] {
    let tint = candidate.tier.color().to_srgba();
    [
        (tint.red * 255.0) as u8,
        (tint.green * 255.0) as u8,
        (tint.blue * 255.0) as u8,
    ]
}

pub(crate) type WorkerQuery<'w, 's> =
    Query<'w, 's, (&'static mut Transform, &'static mut Sprite), (With<Worker>, Without<Bit>, Without<Prop>)>;

pub(crate) type BitQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static mut Bit, &'static mut Transform, &'static mut Sprite), (Without<Worker>, Without<Prop>, Without<Treasure>)>;

pub(crate) type TreasureQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static mut Treasure, &'static mut Transform, &'static mut Sprite), (Without<Worker>, Without<Bit>, Without<Prop>)>;

pub(crate) type PropQuery<'w, 's> =
    Query<'w, 's, (&'static Prop, &'static mut Transform, &'static mut Sprite), (Without<Worker>, Without<Bit>, Without<Treasure>)>;

pub fn play(
    time: Res<Time>,
    mut table: ResMut<Table>,
    mut museum: ResMut<Museum>,
    boutique: Res<BeachcomberBoutique>,
    mut goals: ResMut<BeachcomberGoals>,
    mut canvas: ResMut<SandCanvas>,
    layout: Res<ChamberLayout>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    buttons: Query<&Interaction, With<Button>>,
    mut worker: WorkerQuery,
    mut bits: BitQuery,
    mut treasures: TreasureQuery,
    mut props: PropQuery,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let stage = stage_of(window.width(), window.height());
    let dt = time.delta_secs().min(0.05);
    table.clock += dt as f64;

    apply_bellows_cleaning(museum.bellows_level, dt, &mut bits);
    if canvas.occupied == 0 && !canvas.turning {
        try_spawn_fidget_bits(&mut table, &bits, &stage, &mut commands, &mut images);
    }

    let cursor_world = window.cursor_position().map(|cursor| {
        Vec2::new(cursor.x - window.width() * 0.5, window.height() * 0.5 - cursor.y)
    });
    let on_worker = cursor_world.is_some_and(|pos| (pos - table.worker).length() <= 24.0);
    let ui_pressed = buttons.iter().any(|interaction| *interaction != Interaction::None);

    if mouse.just_pressed(MouseButton::Left)
        && !on_worker
        && !ui_pressed
        && crate::taste::fidget_taste_reason().is_none()
        && let Some(cursor) = cursor_world
    {
        handle_mouse_sifting(
            cursor,
            table.serial,
            &mut museum,
            &boutique,
            &mut goals,
            &mut canvas,
            &layout,
            &mut bits,
            &mut treasures,
            &mut commands,
        );
    }

    update_bits_state(&mut bits, &table.windows, &mut museum, &stage, dt, &mut commands);
    apply_apprentice_cleaning(museum.apprentice_level, dt, &mut bits);
    update_worker_movement(&mut worker, &mut table, &bits, &stage, museum.apprentice_level, dt);
    update_props_positions(&mut props, &museum, &stage);
}

fn apply_bellows_cleaning(bellows_level: u32, dt: f32, bits: &mut BitQuery) {
    if bellows_level > 0 {
        let clean_rate = (bellows_level as f32) * 0.18 * dt;
        for (_, mut bit, _, _) in bits {
            if bit.sediment > 0.0 {
                bit.sediment = (bit.sediment - clean_rate).max(0.0);
            }
        }
    }
}

fn try_spawn_fidget_bits(
    table: &mut Table,
    bits: &BitQuery,
    stage: &Stage,
    commands: &mut Commands,
    images: &mut Assets<Image>,
) {
    let waiting = bits.iter().filter(|(_, bit, ..)| bit.job == Job::Wait || bit.job == Job::Fall).count();
    if !table.busy && table.windows.is_empty() && waiting < 7 && table.clock >= table.next_drop {
        table.serial = table.serial.wrapping_add(1);
        let sig = field::fidget_signature(table.serial);
        let recipe = field::recipe_from_signature(&sig);
        let x = ((table.serial % 5) as f32 - 2.0) * 28.0;
        drop_bit(
            commands,
            images,
            recipe,
            -1.0,
            false,
            Vec2::new(x, stage.top),
            Vec2::new(x, stage.table_y),
        );
        table.next_drop = table.clock + drop_every(table.hunger);
    }
}

fn handle_mouse_sifting(
    cursor: Vec2,
    serial: u64,
    museum: &mut Museum,
    boutique: &BeachcomberBoutique,
    goals: &mut BeachcomberGoals,
    canvas: &mut SandCanvas,
    layout: &ChamberLayout,
    bits: &mut BitQuery,
    treasures: &mut TreasureQuery,
    commands: &mut Commands,
) {
    let mut nearest = None;
    let mut min_dist = 36.0f32;
    for (entity, bit, transform, _) in bits.iter() {
        if bit.job == Job::Wait || bit.job == Job::Fall {
            let dist = (transform.translation.truncate() - cursor).length();
            if dist < min_dist {
                min_dist = dist;
                nearest = Some(entity);
            }
        }
    }

    if let Some(entity) = nearest
        && let Ok((_, mut bit, transform, _)) = bits.get_mut(entity)
    {
        let pos = transform.translation.truncate();
        if bit.sediment > 0.0 {
            bit.sediment = (bit.sediment - 0.45).max(0.0);
            spawn_dust_puff(commands, pos, 6, Color::srgb(0.65, 0.58, 0.45));

            if let Some(needle) = needle::roll_needle(serial.wrapping_add(entity.to_bits()), museum.magnet_level) {
                let yield_amber = needle.amber_yield();
                let yield_renown = needle.renown_yield();
                museum.amber += yield_amber;
                museum.renown += yield_renown;
                spawn_floating_text(
                    commands,
                    pos,
                    &format!("Found: {} (+{:.0} Amber)!", needle.name(), yield_amber),
                    needle.color(),
                );
                spawn_dust_puff(commands, pos, 10, needle.color());
            }
        }

        if bit.sediment <= 0.0 {
            bit.strokes = bit.recipe.strokes;
            bit.job = Job::Peel;
            bit.peel = 10.0;
        }
    }

    let _ = (boutique, goals);
    for (tr_entity, tr, transform, _) in treasures.iter() {
        let pos = canvas
            .jewel_center(layout, tr.glint_id)
            .unwrap_or_else(|| transform.translation.truncate());
        if pos.distance(cursor) <= 22.0 {
            let pay = tr.sell_amber();
            museum.amber += pay;
            canvas.remove_glint(tr.glint_id);
            spawn_floating_text(commands, pos, &format!("+{:.0} Amber", pay), tr.tier.color());
            commands.entity(tr_entity).despawn();
            break;
        }
    }
}

fn update_bits_state(
    bits: &mut BitQuery,
    windows: &[(f64, f64)],
    museum: &mut Museum,
    stage: &Stage,
    dt: f32,
    commands: &mut Commands,
) {
    let mut case_i = 0u8;
    let mut drawer_i = 0u8;
    for (entity, mut bit, mut transform, mut sprite) in bits {
        if bit.footage && in_case(bit.t, windows) && bit.job != Job::Case && bit.job != Job::Chute && bit.job != Job::Assemble {
            bit.job = Job::Case;
        }

        match bit.job {
            Job::Fall | Job::Wait => {
                step_toward(&mut transform, bit.goal, 140.0 * dt);
                if transform.translation.truncate().distance(bit.goal) < 4.0 && bit.job == Job::Fall {
                    bit.job = Job::Wait;
                }
                if bit.sediment > 0.0 {
                    let dim = 1.0 - bit.sediment * 0.45;
                    sprite.color = Color::srgb(dim * 0.8, dim * 0.75, dim * 0.65);
                } else {
                    sprite.color = Color::WHITE;
                }
            }
            Job::Peel => {
                bit.peel += dt;
                let pulse = 1.0 + 0.14 * (bit.peel * 18.0).sin();
                sprite.custom_size = Some(Vec2::splat(28.0 * pulse));
                if bit.peel > 0.32 {
                    bit.peel = 0.0;
                    bit.strokes = bit.strokes.saturating_add(1);
                    sprite.custom_size = Some(Vec2::splat(28.0));
                    if bit.strokes >= bit.recipe.strokes {
                        let pos = transform.translation.truncate();
                        route_finished_peel(&mut bit, pos, windows, museum, stage, &mut drawer_i, commands);
                    }
                }
            }
            Job::Assemble => {
                step_toward(&mut transform, bit.goal, 190.0 * dt);
                if transform.translation.truncate().distance(bit.goal) < 4.0 {
                    commands.entity(entity).despawn();
                }
            }
            Job::Chute => {
                step_toward(&mut transform, stage.chute, 160.0 * dt);
                if transform.translation.truncate().distance(stage.chute) < 6.0 {
                    commands.entity(entity).despawn();
                }
            }
            Job::Drawer => {
                let goal = stage.drawer + Vec2::new((drawer_i % 3) as f32 * 8.0 - 8.0, (drawer_i / 3) as f32 * 8.0);
                drawer_i = drawer_i.saturating_add(1);
                bit.goal = goal;
                step_toward(&mut transform, goal, 150.0 * dt);
            }
            Job::Case => {
                let goal = Vec2::new(-36.0 + case_i as f32 * 18.0, stage.case_y);
                case_i = case_i.saturating_add(1);
                bit.goal = goal;
                step_toward(&mut transform, goal, 170.0 * dt);
            }
        }
    }
}

fn route_finished_peel(
    bit: &mut Bit,
    pos: Vec2,
    windows: &[(f64, f64)],
    museum: &mut Museum,
    stage: &Stage,
    drawer_i: &mut u8,
    commands: &mut Commands,
) {
    if bit.recipe.is_flat {
        bit.job = Job::Chute;
        bit.goal = stage.chute;
    } else if bit.footage && in_case(bit.t, windows) {
        bit.job = Job::Case;
    } else if museum.active_blueprint.needs_slot(bit.socket) {
        museum.active_blueprint.insert_part(bit.socket, bit.recipe);
        bit.job = Job::Assemble;
        bit.goal = Vec2::new(socket_x(bit.socket), 56.0);
        spawn_floating_text(commands, pos, "Part Fitted!", Color::srgb(0.3, 0.95, 0.45));
        spawn_dust_puff(commands, pos, 8, Color::srgb(0.4, 0.8, 1.0));

        if museum.active_blueprint.is_complete() {
            let exhibit = museum.complete_current_blueprint();
            let msg = format!("Restored: {} (+{} Renown)!", exhibit.name, exhibit.renown_reward);
            spawn_floating_text(commands, Vec2::new(0.0, 50.0), &msg, Color::srgb(1.0, 0.85, 0.25));
            spawn_dust_puff(commands, Vec2::new(0.0, 50.0), 22, Color::srgb(1.0, 0.85, 0.25));
        }
    } else {
        museum.amber += 5.0;
        spawn_floating_text(commands, pos, "+5 Amber (Archived)", Color::srgb(0.85, 0.85, 0.85));
        bit.job = Job::Drawer;
        bit.goal = stage.drawer + Vec2::new(0.0, *drawer_i as f32 * 6.0);
        *drawer_i = drawer_i.saturating_add(1);
    }
}

fn apply_apprentice_cleaning(apprentice_level: u32, dt: f32, bits: &mut BitQuery) {
    if apprentice_level > 0
        && let Some((_, mut bit, ..)) = bits.iter_mut().find(|(_, bit, ..)| bit.job == Job::Wait && bit.sediment > 0.0)
    {
        let apprentice_clean_rate = (apprentice_level as f32) * 0.4 * dt;
        bit.sediment = (bit.sediment - apprentice_clean_rate).max(0.0);
        if bit.sediment <= 0.0 {
            bit.strokes = bit.recipe.strokes;
            bit.job = Job::Peel;
            bit.peel = 10.0;
        }
    }
}

fn update_worker_movement(
    worker: &mut WorkerQuery,
    table: &mut Table,
    bits: &BitQuery,
    stage: &Stage,
    apprentice_level: u32,
    dt: f32,
) {
    let mut home = Vec2::new(0.0, stage.worker_y);
    let mut peeling = false;
    if let Some((_, _bit, transform, _)) = bits.iter().find(|(_, bit, ..)| bit.job == Job::Peel) {
        home = transform.translation.truncate();
        peeling = true;
    } else if let Some((_, _, transform, _)) = bits.iter().find(|(_, bit, ..)| bit.job == Job::Wait) {
        home = transform.translation.truncate() + Vec2::new(0.0, 22.0);
    } else {
        table.pace += dt;
        home.x = (table.pace * 0.8).sin() * 36.0;
    }
    let speed = if peeling {
        220.0 + (apprentice_level as f32) * 35.0
    } else {
        90.0 + (apprentice_level as f32) * 20.0
    };
    step_toward_vec(&mut table.worker, home, speed * dt);
    if let Ok((mut transform, mut sprite)) = worker.single_mut() {
        transform.translation = table.worker.extend(3.0);
        sprite.custom_size = Some(Vec2::splat(48.0));
    }
}

fn update_props_positions(props: &mut PropQuery, museum: &Museum, stage: &Stage) {
    for (prop, mut transform, mut sprite) in props {
        match prop {
            Prop::Table => {
                transform.translation = Vec2::new(0.0, stage.table_y - 16.0).extend(0.0);
            }
            Prop::Chute => {
                transform.translation = stage.chute.extend(0.0);
            }
            Prop::Drawer => {
                transform.translation = stage.drawer.extend(0.0);
            }
            Prop::Pedestal => {
                transform.translation = stage.pedestal.extend(0.5);
            }
            Prop::SocketBracket(slot) => {
                let x = socket_x(*slot);
                transform.translation = Vec2::new(x, stage.pedestal.y + 18.0).extend(1.0);
                if museum.active_blueprint.slots[*slot].is_some() {
                    sprite.color = Color::srgba(0.3, 0.95, 0.45, 0.85);
                } else {
                    sprite.color = Color::srgba(0.35, 0.33, 0.40, 0.35);
                }
            }
        }
    }
}

fn render_menu(
    commands: &mut Commands,
    goals: &BeachcomberGoals,
    buckets: &ShorelineBuckets,
) {
    let root = commands
        .spawn((
            GalleryRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(12.0),
                right: px(12.0),
                top: px(70.0),
                bottom: px(68.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(10.0)),
                row_gap: px(6.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.96)),
        ))
        .id();

    spawn_gallery_text(
        commands,
        root,
        "BEACHCOMBER LOG & QUESTS",
        12.0,
        Color::srgb(1.0, 0.86, 0.35),
    );

    // Daily Goals
    let mut d_text = String::from("Daily Quests:\n");
    for g in &goals.daily_goals {
        let status = if g.completed { "✓ [Done]" } else { "" };
        d_text.push_str(&format!(
            "  • {}: {}/{} ({} Amber, {} Pearls) {}\n",
            g.title, g.current, g.target, g.reward_amber as u64, g.reward_pearls, status
        ));
    }
    spawn_gallery_text(commands, root, &d_text, 9.5, Color::srgb(0.85, 0.95, 0.75));

    // Weekly Bounties
    let mut w_text = String::from("Weekly Bounties:\n");
    for g in &goals.weekly_goals {
        let status = if g.completed { "✓ [Done]" } else { "" };
        w_text.push_str(&format!(
            "  ★ {}: {}/{} ({} Amber, {} Pearls) {}\n",
            g.title, g.current, g.target, g.reward_amber as u64, g.reward_pearls, status
        ));
    }
    spawn_gallery_text(commands, root, &w_text, 9.5, Color::srgb(1.0, 0.8, 0.5));

    // Shoreline Buckets Podium
    let mut b_text = format!(
        "Shoreline Buckets (Sorted: {}, Splashed: {}):\n",
        buckets.total_treasures_sorted, buckets.displacements
    );
    for (i, slot) in buckets.slots.iter().enumerate() {
        let rank_title = match i {
            0 => "Rank 1",
            1 => "Rank 2",
            _ => "Rank 3",
        };
        if let Some(t) = slot {
            b_text.push_str(&format!("  {}: {} (Score: {:.1})\n", rank_title, t.name, t.score));
        } else {
            b_text.push_str(&format!("  {}: [Empty Slot]\n", rank_title));
        }
    }
    spawn_gallery_text(commands, root, &b_text, 9.5, Color::srgb(0.7, 0.9, 1.0));

    spawn_gallery_text(commands, root, "Press [M] to close", 9.0, Color::srgb(0.5, 0.5, 0.5));
}

fn render_store(
    commands: &mut Commands,
    museum: &Museum,
    boutique: &BeachcomberBoutique,
    goals: &BeachcomberGoals,
    active_theme: sand_hanabi::SandTheme,
) {
    let root = commands
        .spawn((
            GalleryRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(12.0),
                right: px(12.0),
                top: px(70.0),
                bottom: px(68.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(10.0)),
                row_gap: px(5.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.96)),
        ))
        .id();

    spawn_gallery_text(
        commands,
        root,
        "BEACHCOMBER BOUTIQUE & THEME STORE",
        12.0,
        Color::srgb(1.0, 0.86, 0.35),
    );

    let wallet_text = format!(
        "Wallet: Amber {:.0} | Pearls {}",
        museum.amber, goals.total_pearls
    );
    spawn_gallery_text(commands, root, &wallet_text, 9.5, Color::srgb(1.0, 0.95, 0.7));

    let up_text = format!(
        "Tool Upgrades:\n  [1] Scanner Mk{} ({:.0} Amber)\n  [2] Sifter Lv{} ({:.0} Amber)",
        boutique.detector_level,
        boutique.detector_cost(),
        boutique.sieve_level,
        boutique.sieve_cost(),
    );
    spawn_gallery_text(commands, root, &up_text, 9.5, Color::srgb(0.9, 0.85, 1.0));

    let theme_header = format!(
        "Active Visual: {}  [Click to Equip / Buy, or press [4]]",
        active_theme.display_name()
    );
    spawn_gallery_text(
        commands,
        root,
        &theme_header,
        10.0,
        Color::srgb(0.4, 1.0, 0.8),
    );

    for theme in sand_hanabi::SandTheme::catalog() {
        let is_active = *theme == active_theme;
        let is_owned = boutique.owns_theme(*theme);
        let label = if is_active {
            format!("★ {} (Equipped)", theme.display_name())
        } else if is_owned {
            format!("● {} (Equip)", theme.display_name())
        } else {
            format!("○ {} - {:.0} Amber", theme.display_name(), theme.amber_cost())
        };
        let bg = if is_active {
            Color::srgb(0.18, 0.44, 0.28)
        } else if is_owned {
            Color::srgb(0.24, 0.22, 0.30)
        } else {
            Color::srgb(0.16, 0.15, 0.18)
        };
        spawn_theme_button(commands, root, &label, *theme, bg, is_active);
    }

    spawn_gallery_text(commands, root, "Press [S] to close", 9.0, Color::srgb(0.5, 0.5, 0.5));
}

fn spawn_theme_button(
    commands: &mut Commands,
    root: Entity,
    label: &str,
    theme: sand_hanabi::SandTheme,
    bg: Color,
    is_active: bool,
) {
    let text_color = if is_active {
        Color::srgb(1.0, 0.95, 0.4)
    } else {
        Color::WHITE
    };
    let button = commands
        .spawn((
            ThemeButton(theme),
            Button,
            Node {
                width: percent(100.0),
                min_height: px(22.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: if is_active { UiRect::all(px(1.0)) } else { UiRect::ZERO },
                ..default()
            },
            BorderColor::all(if is_active { Color::srgb(0.4, 0.95, 0.6) } else { Color::NONE }),
            BackgroundColor(bg),
        ))
        .id();
    let text = commands
        .spawn((
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(11.0),
                ..default()
            },
            TextColor(text_color),
        ))
        .id();
    commands.entity(button).add_child(text);
    commands.entity(root).add_child(button);
}

fn spawn_gallery_text(commands: &mut Commands, root: Entity, text: &str, font_size: f32, color: Color) {
    let node = commands
        .spawn((
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(font_size),
                ..default()
            },
            TextColor(color),
            Node { ..default() },
        ))
        .id();
    commands.entity(root).add_child(node);
}

fn socket_x(slot: usize) -> f32 {
    match slot {
        0 => -42.0,
        1 => -14.0,
        2 => 14.0,
        _ => 42.0,
    }
}

fn drop_bit(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    recipe: Recipe,
    t: f64,
    footage: bool,
    from: Vec2,
    goal: Vec2,
) {
    let image = spec_image(images, &recipe);
    let socket = socket_for_recipe(&recipe);
    let sediment = if recipe.is_flat { 0.3 } else { 1.0 };
    commands.spawn((
        Bit {
            strokes: 0,
            recipe,
            t,
            footage,
            job: Job::Fall,
            goal,
            peel: 0.0,
            sediment,
            socket,
        },
        Sprite {
            image,
            custom_size: Some(Vec2::splat(28.0)),
            ..default()
        },
        Transform::from_translation(from.extend(2.0)),
    ));
}

fn spec_image(images: &mut Assets<Image>, recipe: &Recipe) -> Handle<Image> {
    let data = field::sprite_rgba(recipe);
    images.add(Image::new(
        Extent3d {
            width: SPRITE,
            height: SPRITE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ))
}

fn stage_of(width: f32, height: f32) -> Stage {
    Stage {
        top: height * 0.5 - 18.0,
        table_y: -height * 0.5 + 168.0,
        worker_y: -height * 0.5 + 92.0,
        chute: Vec2::new(-width * 0.32, -height * 0.5 + 52.0),
        drawer: Vec2::new(width * 0.28, -height * 0.5 + 58.0),
        case_y: -height * 0.5 + 132.0,
        pedestal: Vec2::new(0.0, -height * 0.5 + 218.0),
    }
}

fn drop_every(hunger: u8) -> f64 {
    1.7 - (hunger as f64 / 100.0) * 1.15
}

fn in_case(t: f64, windows: &[(f64, f64)]) -> bool {
    t >= 0.0 && windows.iter().any(|(start, end)| t + 0.05 >= *start && t <= *end + 0.05)
}

fn step_toward(transform: &mut Transform, goal: Vec2, step: f32) {
    let pos = transform.translation.truncate();
    let next = move_toward(pos, goal, step);
    transform.translation = next.extend(transform.translation.z);
}

fn step_toward_vec(pos: &mut Vec2, goal: Vec2, step: f32) {
    *pos = move_toward(*pos, goal, step);
}

fn move_toward(pos: Vec2, goal: Vec2, step: f32) -> Vec2 {
    let delta = goal - pos;
    let len = delta.length();
    if len <= step || len < 0.001 {
        goal
    } else {
        pos + delta / len * step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_boots_and_updates_without_ecs_query_conflict() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<crate::editor::PetUpdated>();
        app.add_message::<crate::editor::CrateBatch>();
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.init_resource::<Assets<Image>>();
        app.add_systems(Startup, boot);
        app.add_systems(
            Update,
            (
                play,
                tick_museum,
                update_particles,
                update_sand_physics,
                update_treasure_gleams,
                update_bucket_splashes,
            )
                .chain(),
        );
        app.update();
    }

    #[test]
    fn fidget_sort_does_not_insert_a_taste_row() {
        let dir = std::env::temp_dir().join(format!(
            "reelbuddy-fidget-{}-{}",
            std::process::id(),
            crate::db::now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::Db::open(&dir.join("reelbuddy.db")).unwrap();
        if let Some(reason) = crate::taste::fidget_taste_reason() {
            db.insert_taste(&reason, None).unwrap();
        }
        assert!(db.taste_counts().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn s_key_toggles_store_modal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<crate::editor::PetUpdated>();
        app.add_message::<crate::editor::CrateBatch>();
        app.insert_resource(ButtonInput::<MouseButton>::default());
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyS);
        app.insert_resource(keys);
        app.init_resource::<Assets<Image>>();
        app.add_systems(Startup, boot);
        app.add_systems(Update, tick_museum);
        app.update();

        let museum = app.world().resource::<museum::Museum>();
        assert!(museum.show_store, "Pressing S key should toggle the store modal open");
        assert!(!museum.show_menu, "Store modal should not open the quests menu");
    }

    #[test]
    fn m_key_toggles_quests_menu() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<crate::editor::PetUpdated>();
        app.add_message::<crate::editor::CrateBatch>();
        app.insert_resource(ButtonInput::<MouseButton>::default());
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyM);
        app.insert_resource(keys);
        app.init_resource::<Assets<Image>>();
        app.add_systems(Startup, boot);
        app.add_systems(Update, tick_museum);
        app.update();

        let museum = app.world().resource::<museum::Museum>();
        assert!(museum.show_menu, "Pressing M key should toggle the quests menu open");
        assert!(!museum.show_store, "Quests menu should not open the store modal");
    }

    #[test]
    fn digit4_key_cycles_theme_and_immediately_applies_to_canvas() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<crate::editor::PetUpdated>();
        app.add_message::<crate::editor::CrateBatch>();
        app.insert_resource(ButtonInput::<MouseButton>::default());
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Digit4);
        app.insert_resource(keys);
        app.init_resource::<Assets<Image>>();
        app.add_systems(Startup, boot);
        app.update();

        // Unlock Snow so cycle has something to change to
        {
            let mut boutique = app.world_mut().resource_mut::<cosmetics::BeachcomberBoutique>();
            boutique.unlocked_themes.push(sand_hanabi::SandTheme::Snow);
        }

        app.add_systems(Update, tick_museum);
        app.update();

        let config = app.world().resource::<sand_hanabi::SandConfig>();
        let canvas = app.world().resource::<sand_canvas::SandCanvas>();
        let state = app.world().resource::<sand_frame::SandProgressState>();

        assert_eq!(config.active_theme, sand_hanabi::SandTheme::Snow);
        assert_eq!(canvas.active_theme, sand_hanabi::SandTheme::Snow, "canvas active_theme must update immediately");
        assert!(canvas.dirty, "canvas must be marked dirty on theme change");
        assert!(state.theme_banner_timer > 0.0, "theme banner must be announced");
        assert_eq!(state.theme_banner_name.as_deref(), Some("Snow"));
    }
}
