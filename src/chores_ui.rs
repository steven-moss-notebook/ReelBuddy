use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_flair::prelude::{ClassList, Styled};

use crate::editor::{ChoreKind, ChoreView, EditorCommand, EditorLink, HuntOption, PetUpdated};
use crate::taste::REASONS;

#[derive(Component)]
pub(crate) struct ChoreRoot;

#[derive(Component)]
pub(crate) struct ChoreUiNode;

#[derive(Component)]
pub(crate) struct ReelCardNode {
    pub(crate) window_id: i64,
}

#[derive(Component)]
pub(crate) struct ReelCardImage {
    pub(crate) card_entity: Entity,
    pub(crate) handles: Vec<Handle<Image>>,
    pub(crate) index: usize,
    pub(crate) timer: f32,
}

#[derive(Component)]
pub(crate) struct ChoreButton {
    command: Option<EditorCommand>,
    show_reasons: bool,
    open_board: bool,
    reject_for: Option<i64>,
}

#[derive(Resource, Default)]
pub(crate) struct ChoreUi {
    key: String,
    reasons: bool,
    pub(crate) board: bool,
    reject_for: Option<i64>,
    root: Option<Entity>,
}

pub fn sync_card(
    mut commands: Commands,
    mut ui: ResMut<ChoreUi>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    mut messages: MessageReader<PetUpdated>,
    mut last: Local<Option<ChoreView>>,
    chore_nodes: Query<Entity, With<ChoreUiNode>>,
) {
    for update in messages.read() {
        *last = update.0.chore.clone();
    }

    if last.is_none() {
        ui.board = false;
        ui.reject_for = None;
        ui.reasons = false;
        if ui.key != "none" || !chore_nodes.is_empty() {
            for entity in &chore_nodes {
                commands.entity(entity).despawn();
            }
            ui.root = None;
            ui.key = "none".into();
        }
        return;
    }

    let showing = ui.reasons;
    let board = ui.board;
    let reject = ui.reject_for;
    let view = last.as_ref().unwrap();
    let key = format!("{}:{showing}:{board}:{reject:?}", view.id);

    if key == ui.key {
        return;
    }

    // Cleanly despawn all chore nodes to avoid orphaned cards locking the frame
    for entity in &chore_nodes {
        commands.entity(entity).despawn();
    }
    ui.root = None;
    ui.key = key;

    if !showing {
        ui.reasons = false;
    }

    if let ChoreKind::Hunt { options } = &view.kind {
        if ui.board && ui.reject_for.is_none() {
            ui.root = Some(spawn_board(&mut commands, &mut images, &asset_server, view.id, options));
            return;
        }
    }

    let root = commands
        .spawn((
            ChoreRoot,
            ChoreUiNode,
            Node {
                position_type: PositionType::Absolute,
                left: px(14.0),
                right: px(14.0),
                bottom: px(48.0),
                max_height: px(210.0),
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                row_gap: px(5.0),
                padding: UiRect::all(px(8.0)),
                border: UiRect::all(px(1.0)),
                border_radius: BorderRadius::all(px(6.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.82, 0.70, 0.42, 0.45)),
            BackgroundColor(Color::srgba(0.07, 0.08, 0.11, 0.94)),
        ))
        .id();
    ui.root = Some(root);
    let buttons = buttons_for(view, ui.reasons, ui.reject_for);
    for (label, command, show_reasons, open_board, reject_for) in buttons {
        spawn_button(&mut commands, root, label, command, show_reasons, open_board, reject_for);
    }
}

pub fn press_buttons(
    mut commands: Commands,
    mut ui: ResMut<ChoreUi>,
    query: Query<(&Interaction, &ChoreButton), Changed<Interaction>>,
    chore_nodes: Query<Entity, With<ChoreUiNode>>,
    link: Res<EditorLink>,
) {
    for (interaction, button) in &query {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if button.open_board {
            ui.board = true;
            ui.key.clear();
            continue;
        }
        if let Some(window) = button.reject_for {
            ui.reject_for = Some(window);
            ui.key.clear();
            continue;
        }
        if button.show_reasons {
            ui.reasons = true;
            ui.key.clear();
            continue;
        }
        if let Some(command) = button.command.clone() {
            let is_pick = matches!(command, EditorCommand::Pick { .. });
            if is_pick {
                // Immediately remove all cards from screen upon selecting the reel
                for entity in &chore_nodes {
                    commands.entity(entity).despawn();
                }
                ui.root = None;
                ui.board = false;
                ui.reasons = false;
                ui.reject_for = None;
                ui.key = "none".into();
            } else {
                let keep_board = matches!(command, EditorCommand::RejectClip { .. });
                ui.reasons = false;
                ui.reject_for = None;
                if !keep_board {
                    ui.board = false;
                }
                ui.key.clear();
            }
            if let Ok(api) = link.api.lock() {
                api.send(command);
            }
        }
    }
}

pub fn hover_cards(
    time: Res<Time>,
    card_query: Query<(Entity, &Interaction, &ReelCardNode)>,
    button_query: Query<(&Interaction, &ChoreButton)>,
    mut card_styles: Query<(&mut BorderColor, &mut BackgroundColor), With<ReelCardNode>>,
    mut image_query: Query<(&mut ImageNode, &mut ReelCardImage)>,
) {
    let dt = time.delta_secs();

    for (card_entity, interaction, card_node) in &card_query {
        let btn_hovered = button_query.iter().any(|(btn_inter, btn)| {
            *btn_inter == Interaction::Hovered
                && (btn.reject_for == Some(card_node.window_id)
                    || matches!(btn.command, Some(EditorCommand::Pick { window_id, .. }) if window_id == card_node.window_id))
        });
        let is_hovered = *interaction == Interaction::Hovered || btn_hovered;

        if let Ok((mut border, mut bg)) = card_styles.get_mut(card_entity) {
            if is_hovered {
                *border = BorderColor::all(Color::srgba(1.0, 0.84, 0.35, 0.95));
                bg.0 = Color::srgba(0.14, 0.13, 0.12, 0.98);
            } else {
                *border = BorderColor::all(Color::srgba(0.82, 0.70, 0.42, 0.35));
                bg.0 = Color::srgba(0.10, 0.09, 0.08, 0.94);
            }
        }

        for (mut image_node, mut reel_image) in &mut image_query {
            if reel_image.card_entity != card_entity {
                continue;
            }
            if is_hovered && reel_image.handles.len() > 1 {
                reel_image.timer += dt;
                // Cycle every 0.35s through the sample images
                if reel_image.timer >= 0.35 {
                    reel_image.timer = 0.0;
                    reel_image.index = (reel_image.index + 1) % reel_image.handles.len();
                    image_node.image = reel_image.handles[reel_image.index].clone();
                }
            } else if !is_hovered && reel_image.index != 0 {
                reel_image.timer = 0.0;
                reel_image.index = 0;
                if let Some(first) = reel_image.handles.first() {
                    image_node.image = first.clone();
                }
            }
        }
    }
}

fn buttons_for(
    view: &ChoreView,
    reasons: bool,
    reject_for: Option<i64>,
) -> Vec<(String, Option<EditorCommand>, bool, bool, Option<i64>)> {
    let id = view.id;
    match &view.kind {
        ChoreKind::Hunt { .. } if reject_for.is_some() => reason_buttons(id, reject_for),
        ChoreKind::Hunt { .. } => vec![(
            "Inspect Finds (3 Reels)".into(),
            None,
            false,
            true,
            None,
        )],
        ChoreKind::Cut { .. } if reasons => reason_buttons(id, None),
        ChoreKind::Cut { .. } => vec![
            ("Pocket Find".into(), Some(EditorCommand::AcceptCut { chore_id: id }), false, false, None),
            (
                "Nudge -0.5s".into(),
                Some(EditorCommand::Nudge { chore_id: id, delta: -0.5 }),
                false,
                false,
                None,
            ),
            (
                "Nudge +0.5s".into(),
                Some(EditorCommand::Nudge { chore_id: id, delta: 0.5 }),
                false,
                false,
                None,
            ),
            ("Toss Back".into(), None, true, false, None),
        ],
        ChoreKind::Hook { a, b, .. } => vec![
            (
                a.clone(),
                Some(EditorCommand::PickHook { chore_id: id, hook: a.clone() }),
                false,
                false,
                None,
            ),
            (
                b.clone(),
                Some(EditorCommand::PickHook { chore_id: id, hook: b.clone() }),
                false,
                false,
                None,
            ),
        ],
        ChoreKind::Remix { .. } => vec![
            ("Recut Old Find".into(), Some(EditorCommand::RemixTry { chore_id: id }), false, false, None),
            ("Skip Tide".into(), Some(EditorCommand::RemixSkip { chore_id: id }), false, false, None),
        ],
        ChoreKind::Ship { .. } => vec![
            ("Ship to Shore".into(), Some(EditorCommand::Ship { chore_id: id }), false, false, None),
            ("Hold in Chest".into(), Some(EditorCommand::Hold { chore_id: id }), false, false, None),
        ],
    }
}

fn friendly_reason_label(reason: &str) -> &'static str {
    match reason {
        "slow_open" => "Slow Opening",
        "face_out" => "Off-Frame Subject",
        "no_payoff" => "No Payoff",
        "caption_wall" => "Too Wordy",
        "dead_air" => "Dead Air",
        "boring" => "Flat Energy",
        _ => "Pass",
    }
}

fn reason_buttons(
    id: i64,
    reject_for: Option<i64>,
) -> Vec<(String, Option<EditorCommand>, bool, bool, Option<i64>)> {
    REASONS
        .into_iter()
        .map(|reason| {
            let cmd = match reject_for {
                Some(window_id) => EditorCommand::RejectClip {
                    chore_id: id,
                    window_id,
                    reason: reason.to_string(),
                },
                None => EditorCommand::Reject {
                    chore_id: id,
                    reason: reason.to_string(),
                },
            };
            (friendly_reason_label(reason).to_string(), Some(cmd), false, false, None)
        })
        .collect()
}

fn spawn_styled_button(
    commands: &mut Commands,
    root: Entity,
    label: &str,
    button: ChoreButton,
    min_height_px: f32,
    bg_color: Color,
) {
    let btn_entity = commands
        .spawn((
            ChoreUiNode,
            button,
            Button,
            Node {
                width: percent(100.0),
                min_height: px(min_height_px),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(4.0)),
                ..default()
            },
            BackgroundColor(bg_color),
        ))
        .id();
    let text = commands
        .spawn((
            ChoreUiNode,
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::WHITE),
        ))
        .id();
    commands.entity(btn_entity).add_child(text);
    commands.entity(root).add_child(btn_entity);
}

fn spawn_button(
    commands: &mut Commands,
    root: Entity,
    label: String,
    command: Option<EditorCommand>,
    show_reasons: bool,
    open_board: bool,
    reject_for: Option<i64>,
) {
    spawn_styled_button(
        commands,
        root,
        &label,
        ChoreButton {
            command,
            show_reasons,
            open_board,
            reject_for,
        },
        26.0,
        Color::srgb(0.18, 0.16, 0.14),
    );
}

fn friendly_motion(motion: &str) -> &'static str {
    match motion {
        "push_in" => "Dynamic Push-In",
        "pull_out" => "Revealing Pull-Out",
        "pan_left" => "Smooth Pan Left",
        "pan_right" => "Smooth Pan Right",
        "hold" => "Locked Focus",
        _ => "Steady Framing",
    }
}

fn score_tier_badge(s: f64) -> (&'static str, Color) {
    if s >= 85.0 {
        ("[S] Kraken Relic", Color::srgb(1.0, 0.84, 0.3))
    } else if s >= 75.0 {
        ("[A] Sunken Gold", Color::srgb(0.4, 0.85, 1.0))
    } else if s >= 65.0 {
        ("[B] Silver Real", Color::srgb(0.4, 0.9, 0.5))
    } else {
        ("[C] Shore Shell", Color::srgb(0.85, 0.7, 0.4))
    }
}

fn flair_class_for_caption(caption: &str) -> &'static str {
    let lower = caption.to_lowercase();
    if lower.contains("shake") || lower.contains("pause") {
        "flair-shake"
    } else if lower.contains("bounce") || lower.contains("speed_up") || lower.contains("speedup") {
        "flair-bounce"
    } else if lower.contains("wave") || lower.contains("slow_down") || lower.contains("slowdown") {
        "flair-wave"
    } else if lower.contains("pulse") || lower.contains("emit_events") || lower.contains("emitevents") {
        "flair-pulse"
    } else if lower.contains("color_shift") || lower.contains("colorshift") || lower.contains("one_shot") || lower.contains("oneshot") {
        "flair-color-shift"
    } else if lower.contains("fire") || lower.contains("sunset") {
        "flair-gradient-fire"
    } else if lower.contains("ocean") || lower.contains("ice") {
        "flair-gradient-ocean"
    } else if lower.contains("cyberpunk") || lower.contains("neon") {
        "flair-gradient-cyberpunk"
    } else if lower.contains("gold") || lower.contains("luxury") {
        "flair-gradient-gold"
    } else if lower.contains("emerald") || lower.contains("toxic") {
        "flair-gradient-emerald"
    } else if lower.contains("galaxy") || lower.contains("cosmic") {
        "flair-gradient-galaxy"
    } else if lower.contains("candy") || lower.contains("pastel") {
        "flair-gradient-candy"
    } else if lower.contains("monochrome") || lower.contains("silver") {
        "flair-gradient-monochrome"
    } else if lower.contains("lava") || lower.contains("magma") {
        "flair-gradient-lava"
    } else if lower.contains("gradient") {
        "flair-gradient-rainbow"
    } else {
        ""
    }
}

fn spawn_board(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    asset_server: &AssetServer,
    chore_id: i64,
    options: &[HuntOption],
) -> Entity {
    let n = options.len();
    let (top, bottom) = match n {
        0..=3 => (n, 0),
        4 => (2, 2),
        _ => (3, n.saturating_sub(3)),
    };
    let root = commands
        .spawn((
            ChoreRoot,
            ChoreUiNode,
            Node {
                position_type: PositionType::Absolute,
                top: px(8.0),
                left: px(12.0),
                right: px(12.0),
                bottom: px(36.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                justify_content: JustifyContent::Center,
                row_gap: px(8.0),
                overflow: Overflow::clip_y(),
                ..default()
            },
        ))
        .id();
    spawn_row(commands, images, asset_server, root, chore_id, &options[..top.min(n)]);
    if bottom > 0 && n > top {
        spawn_row(commands, images, asset_server, root, chore_id, &options[top..]);
    }
    root
}

fn spawn_row(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    asset_server: &AssetServer,
    root: Entity,
    chore_id: i64,
    options: &[HuntOption],
) {
    let row = commands
        .spawn((
            ChoreUiNode,
            Node {
                width: percent(100.0),
                flex_grow: 1.0,
                min_height: px(0.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Stretch,
                column_gap: px(8.0),
                ..default()
            },
        ))
        .id();
    let cards: Vec<Entity> = options
        .iter()
        .map(|option| spawn_card(commands, images, asset_server, chore_id, option))
        .collect();
    for card in cards {
        commands.entity(row).add_child(card);
    }
    commands.entity(root).add_child(row);
}

fn spawn_card(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    asset_server: &AssetServer,
    chore_id: i64,
    option: &HuntOption,
) -> Entity {
    let (tier_title, tier_color) = score_tier_badge(option.s);
    let card = commands
        .spawn((
            ChoreRoot,
            ChoreUiNode,
            ReelCardNode { window_id: option.window_id },
            Interaction::None,
            Button,
            Styled::new(asset_server.load("flair_text_effects.css")),
            Node {
                width: px(236.0),
                height: percent(100.0),
                max_height: percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                padding: UiRect::all(px(8.0)),
                row_gap: px(6.0),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(8.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.82, 0.70, 0.42, 0.35)),
            BackgroundColor(Color::srgba(0.10, 0.09, 0.08, 0.94)),
        ))
        .id();

    let body = commands
        .spawn((
            ChoreUiNode,
            Node {
                width: percent(100.0),
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(body);

    // Tier badge header
    let tier_text = commands
        .spawn((
            ChoreUiNode,
            Text::new(format!("{} · {:.0}/100", tier_title, option.s)),
            TextFont {
                font_size: FontSize::Px(12.5),
                ..default()
            },
            TextColor(tier_color),
            Node {
                margin: UiRect::bottom(px(2.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(body).add_child(tier_text);

    let image_paths: Vec<&str> = if !option.thumbs.is_empty() {
        option.thumbs.iter().map(|s| s.as_str()).collect()
    } else if !option.thumb.is_empty() {
        vec![option.thumb.as_str()]
    } else {
        Vec::new()
    };

    let handles: Vec<Handle<Image>> = image_paths
        .into_iter()
        .filter_map(|p| thumb_image(images, p))
        .collect();

    if let Some(first_handle) = handles.first().cloned() {
        let image = commands
            .spawn((
                ChoreUiNode,
                ReelCardImage {
                    card_entity: card,
                    handles,
                    index: 0,
                    timer: 0.0,
                },
                ImageNode::new(first_handle),
                Node {
                    width: px(108.0),
                    height: px(192.0),
                    border_radius: BorderRadius::all(px(4.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(body).add_child(image);
    }

    // Duration & cut count
    let duration_line = if !option.segments.is_empty() {
        let total_dur: f64 = option.segments.iter().map(|(s, e)| (e - s).max(0.0)).sum();
        format!("{:.0}s duration ({} cuts)", total_dur, option.segments.len())
    } else {
        format!("{:.1}s-{:.1}s ({:.0}s)", option.start_s, option.end_s, (option.end_s - option.start_s).max(0.0))
    };

    let motion_line = friendly_motion(&option.motion).to_string();

    let hook_line = if !option.hook.is_empty() {
        format!("Hook: \"{}\"", option.hook)
    } else {
        String::new()
    };

    let caption_line = if !option.caption.is_empty() {
        format!("Caption: \"{}\"", option.caption)
    } else {
        String::new()
    };

    let scores_line_1 = format!(
        "Hook {:.0}% | Virality {:.0}% | Energy {:.0}%",
        option.h, option.v, option.e
    );
    let scores_line_2 = format!(
        "Speech {:.0}% | Payoff {:.0}%",
        option.a, option.p
    );

    let lesson_line = if !option.lesson.is_empty() {
        format!("Note: {}", option.lesson)
    } else {
        String::new()
    };

    for (line, color, font_size, is_caption) in [
        (duration_line, Color::srgb(0.75, 0.85, 0.95), 11.5, false),
        (motion_line, Color::srgb(0.95, 0.80, 0.50), 12.0, false),
        (hook_line, Color::srgb(1.0, 0.92, 0.45), 12.0, false),
        (caption_line, Color::srgb(0.95, 0.95, 0.95), 11.5, true),
        (scores_line_1, Color::srgb(0.85, 0.85, 0.85), 10.5, false),
        (scores_line_2, Color::srgb(0.85, 0.85, 0.85), 10.5, false),
        (lesson_line, Color::srgb(0.78, 0.78, 0.78), 11.0, false),
    ] {
        if line.is_empty() {
            continue;
        }
        let flair = if is_caption { flair_class_for_caption(&option.caption) } else { "" };
        let mut text_cmd = commands.spawn((
            ChoreUiNode,
            Text::new(line),
            TextFont {
                font_size: FontSize::Px(font_size),
                ..default()
            },
            TextColor(color),
            Node {
                width: percent(100.0),
                margin: UiRect::vertical(px(1.0)),
                ..default()
            },
        ));
        if !flair.is_empty() {
            text_cmd.insert(ClassList::new(flair));
        }
        let text = text_cmd.id();
        commands.entity(body).add_child(text);
    }

    spawn_styled_button(
        commands,
        card,
        "Pocket This Cut",
        ChoreButton {
            command: Some(EditorCommand::Pick {
                chore_id,
                window_id: option.window_id,
            }),
            show_reasons: false,
            open_board: false,
            reject_for: None,
        },
        26.0,
        Color::srgb(0.20, 0.36, 0.24),
    );
    spawn_styled_button(
        commands,
        card,
        "Toss Back",
        ChoreButton {
            command: None,
            show_reasons: false,
            open_board: false,
            reject_for: Some(option.window_id),
        },
        24.0,
        Color::srgb(0.32, 0.18, 0.16),
    );
    card
}

fn thumb_image(images: &mut Assets<Image>, path: &str) -> Option<Handle<Image>> {
    if path.is_empty() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(images.add(Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_tier_badge_mapping() {
        assert_eq!(score_tier_badge(92.0).0, "[S] Kraken Relic");
        assert_eq!(score_tier_badge(85.0).0, "[S] Kraken Relic");
        assert_eq!(score_tier_badge(80.0).0, "[A] Sunken Gold");
        assert_eq!(score_tier_badge(75.0).0, "[A] Sunken Gold");
        assert_eq!(score_tier_badge(68.0).0, "[B] Silver Real");
        assert_eq!(score_tier_badge(65.0).0, "[B] Silver Real");
        assert_eq!(score_tier_badge(55.0).0, "[C] Shore Shell");
    }

    #[test]
    fn test_friendly_motion_mapping() {
        assert_eq!(friendly_motion("push_in"), "Dynamic Push-In");
        assert_eq!(friendly_motion("pull_out"), "Revealing Pull-Out");
        assert_eq!(friendly_motion("pan_left"), "Smooth Pan Left");
        assert_eq!(friendly_motion("pan_right"), "Smooth Pan Right");
        assert_eq!(friendly_motion("hold"), "Locked Focus");
        assert_eq!(friendly_motion("other"), "Steady Framing");
    }

    #[test]
    fn test_hunt_option_thumbs_support() {
        let opt = HuntOption {
            window_id: 1,
            label: "Test".into(),
            start_s: 0.0,
            end_s: 5.0,
            segments: vec![],
            motion: "hold".into(),
            caption: "".into(),
            hook: "".into(),
            hooks: vec![],
            h: 0.0,
            v: 0.0,
            e: 0.0,
            a: 0.0,
            p: 0.0,
            s: 80.0,
            lesson: "".into(),
            thumb: "thumb.jpg".into(),
            thumbs: vec!["t1.jpg".into(), "t2.jpg".into()],
        };
        let json = serde_json::to_string(&opt).unwrap();
        let decoded: HuntOption = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.thumbs.len(), 2);
        assert_eq!(decoded.thumbs[0], "t1.jpg");
    }
}
