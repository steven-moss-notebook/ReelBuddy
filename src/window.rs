use bevy::prelude::*;
use bevy::window::{
    CompositeAlphaMode, Monitor, PrimaryMonitor, PrimaryWindow, WindowLevel, WindowResizeConstraints,
    WindowResolution,
};

use crate::editor::{EditorCommand, EditorLink};

pub const WIDTH: f32 = 280.0;
pub const HEIGHT: f32 = 360.0;
pub const BOARD_WIDTH: f32 = 780.0;
pub const BOARD_HEIGHT: f32 = 640.0;

pub fn main_window() -> Window {
    Window {
        title: "ReelBuddy".into(),
        resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32),
        decorations: false,
        transparent: true,
        resizable: false,
        window_level: WindowLevel::AlwaysOnTop,
        composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
        resize_constraints: WindowResizeConstraints {
            min_width: WIDTH,
            min_height: HEIGHT,
            max_width: WIDTH,
            max_height: HEIGHT,
        },
        has_shadow: false,
        titlebar_shown: false,
        ..default()
    }
}

pub fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

pub fn open_file(keys: Res<ButtonInput<KeyCode>>, link: Res<EditorLink>) {
    if !keys.just_pressed(KeyCode::KeyO) {
        return;
    }
    let Some(path) = rfd::FileDialog::new().set_title("ReelBuddy").pick_file() else {
        return;
    };
    if let Ok(api) = link.api.lock() {
        api.send(EditorCommand::Ingest(path));
    }
}

pub fn on_drop(mut drops: MessageReader<bevy::window::FileDragAndDrop>, link: Res<EditorLink>) {
    for event in drops.read() {
        if let bevy::window::FileDragAndDrop::DroppedFile { path_buf, .. } = event {
            if let Ok(api) = link.api.lock() {
                api.ingest(path_buf.clone());
            }
        }
    }
}

pub fn drag_window(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    crate::sorter::drag_picture_frame(mouse, windows);
}

pub fn place_bottom_right(
    ui: Res<crate::chores_ui::ChoreUi>,
    mut shown: Local<Option<bool>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
) {
    let board = ui.board;
    if *shown == Some(board) {
        return;
    }
    let Ok(monitor) = monitors.single() else {
        return;
    };
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let (width, height) = if board {
        (BOARD_WIDTH, BOARD_HEIGHT)
    } else {
        (WIDTH, HEIGHT)
    };
    window.resize_constraints = WindowResizeConstraints {
        min_width: width,
        min_height: height,
        max_width: width,
        max_height: height,
    };
    window.resolution.set(width, height);
    let scale = monitor.scale_factor;
    let (right, bottom) = usable_insets_pts();
    let margin_x = ((right + 24.0) * scale).round() as i32;
    let margin_y = ((bottom + 24.0) * scale).round() as i32;
    let phys_w = (f64::from(width) * scale).round() as i32;
    let phys_h = (f64::from(height) * scale).round() as i32;
    let x = monitor.physical_position.x + monitor.physical_width as i32 - phys_w - margin_x;
    let y = monitor.physical_position.y + monitor.physical_height as i32 - phys_h - margin_y;
    window.position = WindowPosition::At(IVec2::new(x, y));
    *shown = Some(board);
}

/// Right and bottom insets, in points, so the window clears the Dock.
fn usable_insets_pts() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    {
        mac_insets().unwrap_or((0.0, 0.0))
    }
    #[cfg(not(target_os = "macos"))]
    {
        (0.0, 0.0)
    }
}

#[cfg(target_os = "macos")]
fn mac_insets() -> Option<(f64, f64)> {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NsRect {
        origin: NsPoint,
        size: NsSize,
    }

    unsafe extern "C" {
        fn objc_getClass(name: *const i8) -> *mut c_void;
        fn sel_registerName(name: *const i8) -> *mut c_void;
        fn objc_msgSend();
    }

    unsafe {
        let cls = objc_getClass(c"NSScreen".as_ptr());
        if cls.is_null() {
            return None;
        }
        let send_obj: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let send_rect: unsafe extern "C" fn(*mut c_void, *mut c_void) -> NsRect =
            std::mem::transmute(objc_msgSend as *const ());
        let screen = send_obj(cls, sel_registerName(c"mainScreen".as_ptr()));
        if screen.is_null() {
            return None;
        }
        let frame = send_rect(screen, sel_registerName(c"frame".as_ptr()));
        let visible = send_rect(screen, sel_registerName(c"visibleFrame".as_ptr()));
        let right = (frame.origin.x + frame.size.width) - (visible.origin.x + visible.size.width);
        let bottom = visible.origin.y - frame.origin.y;
        Some((right.max(0.0), bottom.max(0.0)))
    }
}

pub fn esc_quit(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    let cmd_or_ctrl = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    let q_pressed = keys.just_pressed(KeyCode::KeyQ);

    if keys.just_pressed(KeyCode::Escape) || (cmd_or_ctrl && q_pressed) {
        exit.write(AppExit::Success);
    }
}
