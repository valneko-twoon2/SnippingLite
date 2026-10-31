//! End-to-end tests of the real app.
//!
//! Each test starts the built app with its own settings, history and output
//! folders, covers the primary monitor with a backdrop of known colors, drives
//! the app with synthetic mouse and keyboard input like a user and checks what
//! reaches the clipboard, the disk and the screen. They take over the mouse and
//! keyboard and need an unlocked desktop where nothing else uses Alt+Shift+S,
//! so they are ignored by default. Run them one at a time with
//! `cargo test --manifest-path src/Cargo.toml --config src/.cargo/config.toml --test e2e -- --ignored --test-threads=1`.
//!
//! キーやボタンを押す前に、前面の窓とポインターの下の窓がこのテストかテストするアプリのものかを
//! 確かめ、ほかのアプリには入力を送らない（`focus.rs`）。ほかのプロセスの窓が前面に出たり、
//! カーソルが動かされたりした回に失敗したテストは、テストの外のせいなので、上限まで初めから
//! やり直す（`on_desktop`）。

use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
        Mutex, Once,
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::*,
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard},
        LibraryLoader::GetModuleHandleW,
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    },
    UI::{
        HiDpi::{
            GetDpiForMonitor, SetProcessDpiAwarenessContext,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
        },
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::*,
    },
};

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn a_selection_reaches_the_clipboard_and_the_screenshot_folder() {
    on_desktop(|backdrop| {
        for freeze in [false, true] {
            let app = App::start(
                &format!("snip-{freeze}"),
                &format!(
                    "copy=1\r\nsave=1\r\nask=0\r\nhistory=0\r\nnotify=0\r\nfreeze={}",
                    u8::from(freeze)
                ),
            );
            clear_clipboard();
            shortcut();
            assert!(covered(backdrop.toolbar()), "The toolbar opens");
            assert_eq!(
                dimmed(backdrop.free()),
                freeze,
                "Dimming follows the setting"
            );
            select(backdrop.free(), 240, 160);
            let (width, height, pixels) =
                wait_for(10, clipboard_image).expect("A screenshot on the clipboard");
            assert_eq!((width, height), (240, 160), "freeze {freeze}");
            for y in 0..height {
                for x in 0..width {
                    let (color, alpha) = pixel(&pixels, width, x, y);
                    assert!(
                        near(color, BACKGROUND, 4) && alpha == 255,
                        "{x},{y} is {color:06x} with alpha {alpha}"
                    );
                }
            }
            let png = wait_for(10, || app.files("shots", "png").into_iter().next())
                .expect("A PNG in the screenshot folder");
            assert_eq!(png_size(&png), Some((240, 160)));
            assert!(!covered(backdrop.toolbar()), "The toolbar closes");
            assert!(!dimmed(backdrop.free()), "The overlay closes");
        }
    });
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn the_border_is_drawn_inside_rectangles_and_not_around_freeform_shapes() {
    on_desktop(|backdrop| {
        let _app = App::start(
            "border",
            "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=0\r\noutline=1\r\noutline_color=0\r\noutline_width=2",
        );
        clear_clipboard();
        shortcut();
        select(backdrop.free(), 200, 120);
        let (width, height, pixels) = wait_for(10, clipboard_image).expect("A screenshot");
        assert_eq!((width, height), (200, 120));
        // The thick border is 4 pixels wide, inside the edges.
        for y in 0..height {
            for x in 0..width {
                let edge = x < 4 || y < 4 || x >= width - 4 || y >= height - 4;
                let (color, _) = pixel(&pixels, width, x, y);
                let expected = if edge { RED } else { BACKGROUND };
                assert!(near(color, expected, 4), "{x},{y} is {color:06x}");
            }
        }
        // Choose Freeform in the area menu with the keyboard, then draw a triangle.
        clear_clipboard();
        shortcut();
        click(backdrop.button(backdrop.toolbar(), AREA));
        keys(&[VK_DOWN]);
        keys(&[VK_DOWN]);
        keys(&[VK_RETURN]);
        let start = backdrop.free();
        drag(&[
            POINT {
                x: start.x,
                y: start.y + 150,
            },
            POINT {
                x: start.x + 100,
                y: start.y,
            },
            POINT {
                x: start.x + 200,
                y: start.y + 150,
            },
            POINT {
                x: start.x + 2,
                y: start.y + 148,
            },
        ]);
        let (width, height, pixels) = wait_for(10, clipboard_image).expect("A freeform screenshot");
        assert_eq!((width, height), (200, 150));
        assert_eq!(
            pixel(&pixels, width, 0, 0).1,
            0,
            "Outside the shape is transparent"
        );
        assert_eq!(pixel(&pixels, width, width - 1, 0).1, 0);
        let (inside, alpha) = pixel(&pixels, width, 100, 120);
        assert!(near(inside, BACKGROUND, 4) && alpha == 255, "{inside:06x}");
        assert!(
            (0..height).all(|y| (0..width).all(|x| !near(pixel(&pixels, width, x, y).0, RED, 40))),
            "A freeform shape gets no border"
        );
    });
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn a_kept_toolbar_returns_moves_stays_out_of_screenshots_and_closes() {
    on_desktop(|backdrop| {
        let _app = App::start(
            "keep",
            "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=1\r\nkeep_toolbar=1",
        );
        let home = backdrop.toolbar();
        let probe = backdrop.free();
        shortcut();
        assert!(covered(home) && dimmed(probe), "A snip opens");
        keys(&[VK_ESCAPE]);
        assert!(covered(home), "Escape keeps the toolbar");
        assert!(!dimmed(probe), "Escape ends the snip");

        // Drag the space between the buttons.
        let moved = offset(home, -300, 250);
        let grip = backdrop.button(home, GAP);
        drag(&[
            grip,
            POINT {
                x: grip.x - 300,
                y: grip.y + 250,
            },
        ]);
        assert!(covered(moved), "The toolbar follows the drag");
        assert!(!covered(home), "The toolbar left its old place");

        // A new snip keeps the moved toolbar, and a selection over its place does not show it.
        clear_clipboard();
        shortcut();
        assert!(covered(moved) && !covered(home) && dimmed(probe));
        let from = POINT {
            x: moved.left - 30,
            y: moved.top - 30,
        };
        let (width, height) = (moved.right - moved.left + 60, moved.bottom - moved.top + 60);
        select(from, width, height);
        let (w, h, pixels) = wait_for(10, clipboard_image).expect("A screenshot");
        assert_eq!((w, h), (width, height));
        let toolbar = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| {
                let color = (u32::from(p[2]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[0]);
                !near(color, BACKGROUND, 4)
            })
            .count();
        assert_eq!(toolbar, 0, "The kept toolbar is not in the screenshot");
        assert!(covered(moved), "The toolbar returns after the snip");
        assert!(!dimmed(probe));

        // Its camera button starts the next snip.
        click(backdrop.button(moved, CAMERA));
        assert!(dimmed(probe), "The camera button starts a snip");
        keys(&[VK_ESCAPE]);
        assert!(!dimmed(probe) && covered(moved));

        // The close button closes it; the next snip opens it at the top center again.
        click(backdrop.button(moved, CLOSE));
        assert!(
            !covered(moved) && !covered(home),
            "The close button closes the toolbar"
        );
        shortcut();
        assert!(
            covered(home) && !covered(moved),
            "A reopened toolbar starts at the top center"
        );
        keys(&[VK_ESCAPE]);
    });
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn a_recording_ends_only_from_the_stop_button() {
    on_desktop(|backdrop| {
        let app = App::start(
            "record",
            "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=0\r\ncountdown=0\r\nmic=0\r\nsystem_audio=0\r\nrecord_cursor=0\r\nrecord_fps=30",
        );
        shortcut();
        click(backdrop.button(backdrop.toolbar(), VIDEO));
        select(backdrop.free(), 320, 200);
        settle(1000);
        let indicator = backdrop.indicator();
        click(backdrop.button(indicator, TIMER));
        settle(1500);
        // 録画中は一時名（`.partial`）に書き、仕上がるまで最後の名前の MP4 はない。
        assert_eq!(
            app.files("videos", "partial").len(),
            1,
            "One recording is running"
        );
        assert!(
            app.files("videos", "mp4").is_empty(),
            "A click on the elapsed time does not end the recording"
        );
        click(backdrop.button(indicator, STOP));
        let seconds =
            wait_for(15, || finished_video(&app)).expect("The Stop button finishes the recording");
        assert!(
            seconds >= 2.0,
            "The video covers the whole recording: {seconds} s"
        );
        assert_eq!(app.files("videos", "mp4").len(), 1);
        assert!(app.files("videos", "partial").is_empty());
    });
}

/// 仕上がった録画の長さ。まだ録画中（最後の名前の MP4 がない）なら `None`。
fn finished_video(app: &App) -> Option<f64> {
    app.files("videos", "mp4")
        .first()
        .and_then(|f| mp4_seconds(f))
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn recording_areas_below_the_encoder_minimum_are_ignored() {
    on_desktop(|backdrop| {
        let app = App::start(
            "small",
            "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=1\r\ncountdown=0\r\nmic=0\r\nsystem_audio=0\r\nrecord_cursor=0\r\nrecord_fps=30",
        );
        shortcut();
        click(backdrop.button(backdrop.toolbar(), VIDEO));
        let probe = POINT {
            x: backdrop.free().x + 200,
            y: backdrop.free().y + 200,
        };
        for size in [20, 33] {
            select(backdrop.free(), size, size);
            assert!(
                app.files("videos", "mp4").is_empty() && app.files("videos", "partial").is_empty(),
                "{size} px starts no recording"
            );
            assert!(dimmed(probe), "The snip stays open after {size} px");
        }
        select(backdrop.free(), 34, 34);
        settle(1200);
        // 録画中は一時名（`.partial`）に書く。
        assert_eq!(app.files("videos", "partial").len(), 1, "34 px records");
        shortcut();
        let seconds = wait_for(15, || finished_video(&app)).expect("The shortcut finishes it");
        assert!(seconds > 0.5, "{seconds} s");
    });
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn the_history_view_copies_text_and_colors() {
    on_desktop(|backdrop| {
        let app = App::start(
            "history",
            "copy=1\r\nsave=0\r\nhistory=1\r\nnotify=0\r\nfreeze=0",
        );
        backdrop.show_panel(true);
        clear_clipboard();
        shortcut();
        let panel = backdrop.rect(PANEL);
        select(
            POINT {
                x: panel.left,
                y: panel.top,
            },
            panel.right - panel.left,
            panel.bottom - panel.top,
        );
        let (width, _, pixels) = wait_for(10, clipboard_image).expect("A screenshot of the panel");
        let square = offset(SQUARE, -PANEL.left, -PANEL.top);
        assert_eq!(
            pixel(&pixels, width, square.left + 5, square.top + 5).0,
            CYAN
        );
        wait_for(10, || {
            (!app.files("local/Snipping Lite/history", "png").is_empty()).then_some(())
        })
        .expect("A history entry");

        // Only the history window shows the square from now on.
        backdrop.show_panel(false);
        backdrop.topmost(false);
        click(backdrop.at(backdrop.width() / 2, backdrop.height() / 2 + 200));
        // SAFETY: lets the new process take the foreground from this one.
        unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        let _history = app.history();
        let shown = wait_for(30, || find(CYAN)).expect("The history window shows the screenshot");
        // Clicking the image without a tool only activates the window.
        click(center(shown));

        // Tab order: Back, Delete, Show in folder, Save as..., Copy, Copy text.
        clear_clipboard();
        for _ in 0..6 {
            keys(&[VK_TAB]);
        }
        keys(&[VK_RETURN]);
        let text = wait_for(30, || clipboard_text().filter(|t| !t.is_empty()))
            .expect("Copy text puts the recognized text on the clipboard");
        assert!(text.contains("World"), "{text:?}");

        // Then the tools: Pen, Highlighter, Crop, Pick color.
        clear_clipboard();
        for _ in 0..4 {
            keys(&[VK_TAB]);
        }
        keys(&[VK_RETURN]);
        click(center(shown));
        let color = wait_for(10, clipboard_text).expect("Pick color copies the color");
        assert_eq!(color, "#00FFFF");
    });
}

mod constants;
use constants::*;
mod util;
use util::*;
mod backdrop;
use backdrop::*;
mod app;
use app::*;
mod input;
use input::*;
mod screen;
use screen::*;
mod focus;
use focus::*;

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn a_border_chosen_in_the_settings_applies_to_the_next_screenshot_without_a_restart() {
    on_desktop(|backdrop| {
        let app = App::start(
            "border-reload",
            "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=0\r\nkeep_toolbar=1\r\noutline=0\r\noutline_color=0\r\noutline_width=2",
        );
        let has_red_edge = |pixels: &[u8], width: i32| {
            let (color, _) = pixel(pixels, width, 0, 0);
            near(color, RED, 4)
        };
        clear_clipboard();
        shortcut();
        select(backdrop.free(), 200, 120);
        let (width, _, pixels) = wait_for(10, clipboard_image).expect("A screenshot");
        assert!(!has_red_edge(&pixels, width), "no border while it is off");
        // Switch the border on the way the settings window does: write the file, tell the tray.
        let file = app.root.join("roaming/Snipping Lite/settings.ini");
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, text.replace("outline=0", "outline=1")).unwrap();
        let tray = app.tray();
        assert!(!tray.is_null(), "the tray window is found by its class");
        unsafe { PostMessageW(tray, WM_APP + 2, 0, 0) };
        settle(500);
        clear_clipboard();
        shortcut();
        select(backdrop.free(), 200, 120);
        let (width, _, pixels) = wait_for(10, clipboard_image).expect("A second screenshot");
        assert!(
            has_red_edge(&pixels, width),
            "the border appears after the change"
        );
    });
}
