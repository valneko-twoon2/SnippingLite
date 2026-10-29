use super::*;

/// The backdrop color as the screen reports it (0xRRGGBB).
pub const BACKGROUND: u32 = 0x336699;
/// The backdrop under the dimmed snipping overlay, which halves every channel.
pub const DIMMED: u32 = (BACKGROUND >> 1) & 0x7f7f7f;
pub const CYAN: u32 = 0x00ffff;
/// The first color of the border setting.
pub const RED: u32 = 0xe81123;
/// A white panel with text and a cyan square, in pixels from the monitor's top-left corner.
pub const PANEL: RECT = RECT {
    left: 60,
    top: 200,
    right: 700,
    bottom: 420,
};
pub const SQUARE: RECT = RECT {
    left: 600,
    top: 260,
    right: 680,
    bottom: 340,
};
pub const TEXT: &str = "Hello World 123";

/// Toolbar positions in DIPs from its left edge, as in `toolbar.rs`.
pub const CAMERA: f32 = 26.0;
pub const VIDEO: f32 = 62.0;
pub const AREA: f32 = 116.0;
pub const CLOSE: f32 = 306.0;
/// The space between the capture types and the area button, which moves the toolbar.
pub const GAP: f32 = 86.0;
/// Recording indicator positions in DIPs, as in `video.rs`.
pub const TIMER: f32 = 60.0;
pub const STOP: f32 = 130.0;

/// The tests share the screen, the mouse and the shortcut.
pub static DESKTOP: Mutex<()> = Mutex::new(());
pub static PANEL_SHOWN: AtomicBool = AtomicBool::new(false);
pub static SETUP: Once = Once::new();
