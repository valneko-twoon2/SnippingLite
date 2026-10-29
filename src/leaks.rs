//! Leak tests: a tray app that runs for weeks must give back every GDI and USER object that a
//! capture takes. Each flow runs once to warm up its caches, then many times while the object
//! counts of the process are watched. The counts belong to the whole process, so these tests are\n//! ignored by default and run alone (coverage.ps1 does that).

use crate::{
    history, imaging,
    test_support::{desk::Desk, gui_objects, Sandbox},
    toast, ui,
};
use std::time::Duration;

const ROUNDS: usize = 40;
/// Objects the system may keep for itself (cached fonts, theme handles).
const SLACK: u32 = 3;

fn assert_steady(name: &str, before: (u32, u32), after: (u32, u32)) {
    assert!(
        after.0 <= before.0 + SLACK && after.1 <= before.1 + SLACK,
        "{name}: GDI {} -> {}, USER {} -> {} after {ROUNDS} rounds",
        before.0,
        after.0,
        before.1,
        after.1
    );
}

#[test]
#[ignore = "Counts the objects of the whole process; run it alone"]
fn drawing_surfaces_give_back_their_gdi_objects() {
    let round = || {
        // SAFETY: the canvas is private to this call and dropped at its end.
        let mut canvas = unsafe { ui::Canvas::new(96, 64) }.unwrap();
        canvas.pixels()[0] = 0xff00ff00;
    };
    round();
    let before = gui_objects();
    for _ in 0..ROUNDS * 10 {
        round();
    }
    assert_eq!(gui_objects(), before);
}

#[test]
#[ignore = "Counts the objects of the whole process; run it alone"]
fn saving_reading_and_copying_pictures_gives_back_their_objects() {
    let _sandbox = Sandbox::new();
    let path = std::env::temp_dir().join(format!("snipping-lite-leak-{}.png", std::process::id()));
    let pixels = vec![200u8; 64 * 48 * 4];
    let round = || {
        imaging::encode(&path, &pixels, 64, 48).unwrap();
        assert!(imaging::decode(&path).is_some());
        assert!(imaging::copy_to_clipboard(
            std::ptr::null_mut(),
            &pixels,
            64,
            48
        ));
    };
    round();
    let before = gui_objects();
    for _ in 0..ROUNDS {
        round();
    }
    assert_steady("pictures", before, gui_objects());
    let _ = std::fs::remove_file(path);
}

#[test]
#[ignore = "Covers the screen"]
fn repeated_snips_give_back_their_objects() {
    let desk = Desk::start_with(|settings| {
        settings.freeze = true;
        settings.copy = true;
        settings.save = false;
        settings.history = true;
        settings.notify = true;
    });
    let snip = || {
        // SAFETY: the windows belong to this test.
        unsafe { crate::begin_capture() };
        desk.pump(Duration::from_millis(30));
        // SAFETY: the overlay was just created by the test desk.
        let overlay = unsafe { crate::state().overlay };
        desk.drag(overlay, (200, 200), &[(250, 230)], (360, 280));
        assert!(!unsafe { crate::state().active }, "the snip ended");
    };
    snip();
    snip();
    let before = gui_objects();
    for _ in 0..ROUNDS / 2 {
        snip();
    }
    // SAFETY: plain query on this thread's toast.
    let _ = unsafe { toast::visible() };
    assert_steady("snips", before, gui_objects());
    assert!(!history::list().is_empty());
}
