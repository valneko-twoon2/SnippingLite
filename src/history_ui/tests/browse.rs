use super::*;

#[test]
fn a_click_opens_the_screenshot_and_back_returns() {
    let _sandbox = Sandbox::new();
    let t = now();
    let entry = seed(t - HOUR, 200, 100);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    assert_eq!(viewer.opened_id(), Some(entry.id));
    assert!(viewer.screen.has(tr("Back")));
    assert!(viewer.screen.has("   200 x 100"));
    assert!(viewer
        .screen
        .has(tr("Left and right arrow keys browse, Esc goes back.")));
    viewer.click_text(tr("Back"));
    assert!(viewer.app.opened.is_none());
    viewer.click(cell);
    viewer.key(Key::Escape);
    assert!(viewer.app.opened.is_none());
}

#[test]
fn the_arrow_keys_browse_and_stop_at_both_ends() {
    let _sandbox = Sandbox::new();
    let t = now();
    let ids = [t - HOUR, t - 2 * HOUR, t - 3 * HOUR];
    for id in ids {
        seed(id, 60, 40);
    }
    let mut viewer = Viewer::open();
    let first = viewer.screen.images()[0].center();
    viewer.click(first);
    assert_eq!(viewer.opened_id(), Some(ids[0]));
    // Boundary: newer than the newest does nothing.
    viewer.key(Key::ArrowLeft);
    assert_eq!(viewer.opened_id(), Some(ids[0]));
    viewer.key(Key::ArrowRight);
    assert_eq!(viewer.opened_id(), Some(ids[1]));
    viewer.key(Key::ArrowRight);
    assert_eq!(viewer.opened_id(), Some(ids[2]));
    viewer.key(Key::ArrowRight);
    assert_eq!(
        viewer.opened_id(),
        Some(ids[2]),
        "older than the oldest does nothing"
    );
    viewer.key(Key::ArrowLeft);
    assert_eq!(viewer.opened_id(), Some(ids[1]));
}

#[test]
fn the_newest_screenshot_opens_on_request() {
    let _sandbox = Sandbox::new();
    let viewer = Viewer::with(HistoryApp::new(false, true));
    assert!(viewer.app.opened.is_none(), "nothing to open yet");
    assert!(!viewer.app.open_latest, "the request is used up");
    drop(viewer);
    let t = now();
    seed(t - DAY, 60, 40);
    let newest = seed(t - HOUR, 60, 40);
    let viewer = Viewer::with(HistoryApp::new(false, true));
    assert_eq!(viewer.opened_id(), Some(newest.id));
}

#[test]
fn an_unreadable_picture_is_reported_instead_of_opened() {
    let _sandbox = Sandbox::new();
    let entry = seed(now(), 60, 40);
    fs::write(&entry.image, b"not a picture").unwrap();
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    assert!(viewer.app.opened.is_none());
    assert_eq!(
        viewer.app.notice.map(|n| n.1),
        Some(tr("Could not open the image."))
    );
}

#[test]
fn a_notice_fades_back_to_the_hint_after_four_seconds() {
    let _sandbox = Sandbox::new();
    seed(now(), 60, 40);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    viewer.app.notice = Some((Instant::now(), tr("Saved the changes.")));
    viewer.settle();
    assert!(viewer.screen.has(tr("Saved the changes.")));
    assert!(!viewer
        .screen
        .has(tr("Left and right arrow keys browse, Esc goes back.")));
    viewer.app.notice = Some((
        Instant::now() - Duration::from_secs(5),
        tr("Saved the changes."),
    ));
    viewer.settle();
    assert!(!viewer.screen.has(tr("Saved the changes.")));
    assert!(viewer
        .screen
        .has(tr("Left and right arrow keys browse, Esc goes back.")));
}

#[test]
fn deleting_an_opened_screenshot_removes_it_and_returns_to_the_gallery() {
    let _sandbox = Sandbox::new();
    let t = now();
    seed(t - HOUR, 60, 40);
    let keep = seed(t - 2 * HOUR, 60, 40);
    let mut viewer = Viewer::open();
    let first = viewer.screen.images()[0].center();
    viewer.click(first);
    viewer.click_text(tr("Delete"));
    assert!(viewer.app.opened.is_none());
    assert_eq!(history::list(), vec![keep]);
    assert_eq!(viewer.app.entries.len(), 1);
}

#[test]
fn pen_and_highlighter_draw_strokes_that_can_be_undone() {
    let _sandbox = Sandbox::new();
    seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    let canvas = viewer.canvas();
    assert!(!viewer.screen.has(tr("Undo")));
    // Without a tool a drag over the picture draws nothing.
    let from = canvas.min + egui::vec2(20.0, 20.0);
    viewer.act(|s, show| s.drag(from, from + egui::vec2(60.0, 30.0), show));
    assert!(viewer.app.opened.as_ref().unwrap().marks.is_empty());

    viewer.click_text(tr("Pen"));
    assert!(viewer.app.tool == Tool::Pen);
    viewer.act(|s, show| s.drag(from, from + egui::vec2(60.0, 30.0), show));
    {
        let marks = &viewer.app.opened.as_ref().unwrap().marks;
        assert_eq!(marks.len(), 1);
        assert!(marks[0].points.len() >= 2);
        assert!(!marks[0].highlight);
        assert_eq!(marks[0].radius, annotate::PEN_RADII[viewer.app.thickness]);
    }
    viewer.click_text(tr("Highlighter"));
    viewer.act(|s, show| {
        s.drag(
            from + egui::vec2(0.0, 40.0),
            from + egui::vec2(80.0, 40.0),
            show,
        )
    });
    {
        let marks = &viewer.app.opened.as_ref().unwrap().marks;
        assert_eq!(marks.len(), 2);
        assert!(marks[1].highlight);
        assert_eq!(
            marks[1].radius,
            annotate::HIGHLIGHTER_RADII[viewer.app.thickness]
        );
    }
    viewer.click_text(tr("Undo"));
    assert_eq!(viewer.app.opened.as_ref().unwrap().marks.len(), 1);
    viewer.click_text(tr("Undo"));
    assert!(viewer.app.opened.as_ref().unwrap().marks.is_empty());
    assert!(!viewer.screen.has(tr("Undo")));
    // Choosing the same tool again puts it away.
    viewer.click_text(tr("Highlighter"));
    assert!(viewer.app.tool == Tool::None);
}

#[test]
fn the_pen_color_and_thickness_are_remembered() {
    let _sandbox = Sandbox::new();
    seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    assert!(
        !viewer.screen.has(tr("Thick")),
        "the choices belong to the pen tools"
    );
    viewer.click_text(tr("Pen"));
    viewer.click_text(tr("Thick"));
    assert_eq!(viewer.app.thickness, 2);
    assert_eq!(config::load_pen().1, 2);
    // The five colors are round swatches next to the chips.
    let swatches = viewer.screen.circles(9.0);
    assert_eq!(swatches.len(), annotate::COLORS.len());
    viewer.click(swatches[2]);
    assert_eq!(viewer.app.color, 2);
    assert_eq!(config::load_pen().0, 2);
    // A new window starts with what was chosen last.
    let again = HistoryApp::new(false, false);
    assert_eq!((again.color, again.thickness), (2, 2));
}
