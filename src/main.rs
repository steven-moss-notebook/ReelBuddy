mod chores_ui;
mod db;
mod editor;
mod field;
mod media;
mod pet_state;
mod score;
mod sorter;
mod stdio;
mod taste;
mod window;

use bevy::prelude::*;
use bevy::window::WindowPlugin;

fn main() {
    if std::env::args().any(|arg| arg == "--stdio") {
        stdio::run();
        return;
    }
    let debug = std::env::args().any(|arg| arg == "--debug");
    App::new()
        .insert_resource(ClearColor(Color::NONE))
        .insert_resource(editor::spawn(debug))
        .insert_resource(chores_ui::ChoreUi::default())
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(window::main_window()),
                ..default()
            }),
            bevy_flair::FlairPlugin,
        ))
        .add_message::<editor::PetUpdated>()
        .add_message::<editor::CrateBatch>()
        .add_message::<editor::SandFinds>()
        .add_systems(Startup, (window::setup_camera, sorter::boot))
        .add_systems(
            Update,
            (
                window::place_bottom_right,
                editor::pump,
                sorter::update_sand_frame,
                sorter::ingest_crates,
                sorter::play,
                sorter::tick_museum,
                sorter::update_particles,
                sorter::update_sand_physics,
                sorter::update_continuous_sand_stream,
                sorter::update_treasure_gleams,
                sorter::update_bucket_splashes,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                chores_ui::sync_card,
                chores_ui::hover_cards,
                chores_ui::press_buttons,
                window::open_file,
                window::on_drop,
                window::drag_window,
                window::esc_quit,
            )
                .chain(),
        )
        .add_systems(PostUpdate, editor::shutdown_on_exit)
        .run();
}
