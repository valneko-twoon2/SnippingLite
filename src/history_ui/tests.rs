use super::*;
use crate::test_support::{
    answer_dialog, clipboard_text,
    screen::{Screen, Show},
    Sandbox, CAPTIONS,
};
use eframe::App;
use std::{fs, path::PathBuf};

const HOUR: u128 = 60 * 60 * 1000;
const DAY: u128 = 24 * HOUR;

/// The history window running on a screen of its default size.
struct Viewer {
    app: HistoryApp,
    screen: Screen,
    frame: eframe::Frame,
}

impl Viewer {
    fn open() -> Viewer {
        Viewer::with(HistoryApp::new(false, false))
    }

    fn with(app: HistoryApp) -> Viewer {
        let mut viewer = Viewer {
            app,
            screen: Screen::new(960.0, 640.0),
            frame: eframe::Frame::_new_kittest(),
        };
        viewer.act(|screen, show| screen.settle(show));
        viewer
    }

    fn act<R>(&mut self, action: impl FnOnce(&mut Screen, Show) -> R) -> R {
        let app = &mut self.app;
        let frame = &mut self.frame;
        let mut show = |ui: &mut egui::Ui| app.ui(ui, frame);
        action(&mut self.screen, &mut show)
    }

    fn settle(&mut self) {
        self.act(|screen, show| screen.settle(show));
    }

    fn click_text(&mut self, text: &str) {
        self.act(|screen, show| screen.click_text(text, show));
    }

    fn click(&mut self, at: egui::Pos2) {
        self.act(|screen, show| screen.click(at, show));
    }

    fn key(&mut self, key: Key) {
        self.act(|screen, show| screen.key(key, egui::Modifiers::NONE, show));
    }

    /// The picture of the opened screenshot, as painted.
    fn canvas(&self) -> Rect {
        *self.screen.images().last().expect("a picture is shown")
    }

    fn opened_id(&self) -> Option<u128> {
        self.app.opened.as_ref().map(|o| o.entry.id)
    }
}

/// A stored screenshot: red on the left half and cyan on the right half.
fn seed(id: u128, width: i32, height: i32) -> Entry {
    let folder = history::folder().unwrap();
    fs::create_dir_all(&folder).unwrap();
    let entry = Entry {
        id,
        image: folder.join(format!("{id}.png")),
        thumbnail: folder.join(format!("{id}.thumb.png")),
    };
    let mut pixels = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            pixels.extend(if x < width / 2 {
                [0, 0, 255, 255] // red, as BGRA
            } else {
                [255, 255, 0, 255] // cyan
            });
        }
    }
    imaging::encode(&entry.image, &pixels, width, height).unwrap();
    imaging::encode(&entry.thumbnail, &pixels, width, height).unwrap();
    entry
}

fn now() -> u128 {
    imaging::timestamp().unwrap()
}

#[test]
fn dates_and_times_are_written_with_fixed_widths() {
    let id = now();
    let date = date_label(id);
    let time = time_label(id);
    let digits = |text: &str, parts: &[usize], separator: char| {
        let pieces: Vec<&str> = text.split(separator).collect();
        pieces.len() == parts.len()
            && pieces
                .iter()
                .zip(parts)
                .all(|(p, &n)| p.len() == n && p.chars().all(|c| c.is_ascii_digit()))
    };
    assert!(digits(&date, &[4, 2, 2], '/'), "{date}");
    assert!(digits(&time, &[2, 2], ':'), "{time}");
}

#[test]
fn ranges_cover_the_last_day_week_and_month() {
    assert_eq!(Range::All.milliseconds(), None);
    assert_eq!(Range::Day.milliseconds(), Some(DAY));
    assert_eq!(Range::Week.milliseconds(), Some(7 * DAY));
    assert_eq!(Range::Month.milliseconds(), Some(30 * DAY));
    let labels: Vec<_> = Range::ALL.iter().map(|r| r.label()).collect();
    for (i, a) in labels.iter().enumerate() {
        assert!(!a.is_empty());
        assert!(labels.iter().skip(i + 1).all(|b| a != b));
    }
}

#[test]
fn the_gallery_filters_by_range_and_by_date_text() {
    let _sandbox = Sandbox::new();
    let mut app = HistoryApp::new(false, false);
    let t = now();
    // Equivalence classes by age; each sits well inside its class so the clock cannot tip it over.
    let ages = [HOUR, 3 * DAY, 10 * DAY, 60 * DAY];
    app.entries = ages
        .iter()
        .map(|&age| Entry {
            id: t - age,
            image: PathBuf::new(),
            thumbnail: PathBuf::new(),
        })
        .collect();
    for (range, expected) in [
        (Range::All, 4),
        (Range::Day, 1),
        (Range::Week, 2),
        (Range::Month, 3),
    ] {
        app.range = range;
        assert_eq!(app.visible().len(), expected, "{}", range.label());
    }
    // Boundary: an entry just inside and just outside the day.
    app.range = Range::Day;
    app.entries[0].id = t - DAY + 60_000;
    assert_eq!(app.visible().len(), 1);
    app.entries[0].id = t - DAY - 60_000;
    assert_eq!(app.visible().len(), 0);
    // The search accepts '-' and '.' for '/' and ignores surrounding spaces.
    app.range = Range::All;
    let date = date_label(app.entries[1].id);
    for query in [
        date.clone(),
        date.replace('/', "-"),
        date.replace('/', "."),
        format!("  {date} "),
    ] {
        app.query = query.clone();
        assert!(
            app.visible().iter().any(|e| e.id == app.entries[1].id),
            "{query:?}"
        );
    }
    app.query = "1999/01/01".into();
    assert!(app.visible().is_empty());
    app.query = format!("{date} {}", time_label(app.entries[1].id));
    assert!(!app.visible().is_empty());
}

#[test]
fn an_empty_history_says_so_and_offers_nothing_to_clear() {
    let _sandbox = Sandbox::new();
    let viewer = Viewer::open();
    assert!(viewer.screen.has(tr("No screenshots yet")));
    assert!(viewer
        .screen
        .has(tr("Captures appear here after you take them.")));
    assert!(!viewer.screen.has(tr("Clear")));
    for range in Range::ALL {
        assert!(
            !viewer.screen.has(range.label()),
            "no filters without screenshots"
        );
    }
    assert!(viewer.screen.images().is_empty());
}

#[test]
fn the_gallery_groups_screenshots_by_day_and_counts_them() {
    let _sandbox = Sandbox::new();
    let t = now();
    for (offset, size) in [(0, 40), (HOUR / 4, 50), (3 * DAY, 60)] {
        seed(t - offset, size, 30);
    }
    let viewer = Viewer::open();
    let count = tr("{total} screenshots, newest first (the oldest beyond {limit} are removed)")
        .replace("{total}", "3")
        .replace("{limit}", "200");
    assert!(viewer.screen.has(&count), "{:?}", viewer.screen.texts());
    assert!(viewer.screen.has(&date_label(t - 3 * DAY)));
    assert_eq!(
        viewer.screen.images().len(),
        3,
        "one thumbnail per screenshot"
    );
    for range in Range::ALL {
        assert!(viewer.screen.has(range.label()));
    }
    assert!(viewer.screen.has(tr("Clear")));
    assert!(!viewer
        .screen
        .has(tr("History recording is turned off in Settings.")));
}

#[test]
fn a_range_chip_narrows_the_gallery_and_the_count_says_how_many_match() {
    let _sandbox = Sandbox::new();
    let t = now();
    seed(t - HOUR, 40, 30);
    seed(t - 3 * DAY, 40, 30);
    seed(t - 60 * DAY, 40, 30);
    let mut viewer = Viewer::open();
    viewer.click_text(Range::Week.label());
    assert!(viewer.app.range == Range::Week);
    assert_eq!(viewer.screen.images().len(), 2);
    let matches = tr("{shown} of {total} screenshots match")
        .replace("{shown}", "2")
        .replace("{total}", "3");
    assert!(viewer.screen.has(&matches));
    // Typing a date that nothing has leaves the "no match" message.
    viewer.app.query = "1999/01/01".into();
    viewer.settle();
    assert!(viewer.screen.has(tr("No screenshots match.")));
    assert!(viewer.screen.images().is_empty());
    viewer.click_text(Range::All.label());
    assert!(viewer.app.range == Range::All);
}

#[test]
fn the_search_box_filters_while_typing() {
    let _sandbox = Sandbox::new();
    let t = now();
    seed(t - HOUR, 40, 30);
    seed(t - 40 * DAY, 40, 30);
    let mut viewer = Viewer::open();
    viewer.click_text(tr("Search by date, e.g. 2026/10/28"));
    let wanted = date_label(t - 40 * DAY);
    viewer.act(|screen, show| screen.type_text(&wanted, show));
    assert_eq!(viewer.app.query, wanted);
    assert_eq!(viewer.screen.images().len(), 1);
}

#[test]
fn clearing_the_history_asks_first() {
    let _sandbox = Sandbox::new();
    let t = now();
    seed(t - HOUR, 40, 30);
    seed(t - 2 * HOUR, 40, 30);
    let mut viewer = Viewer::open();
    viewer.click_text(tr("Clear"));
    assert!(viewer.app.confirm_clear);
    assert!(viewer
        .screen
        .has(tr("Delete every screenshot in the history?")));
    // Backing out keeps everything.
    viewer.click_text(tr("Cancel"));
    assert!(!viewer.app.confirm_clear);
    assert_eq!(history::list().len(), 2);
    // Confirming deletes it all.
    viewer.click_text(tr("Clear"));
    viewer.click_text(tr("Delete all"));
    assert!(history::list().is_empty());
    assert!(viewer.screen.has(tr("No screenshots yet")));
}

#[test]
fn history_recording_that_is_off_is_reported() {
    let _sandbox = Sandbox::new();
    config::save_settings(&config::Settings {
        history: false,
        ..config::parse_settings("")
    })
    .unwrap();
    let viewer = Viewer::open();
    assert!(viewer
        .screen
        .has(tr("History recording is turned off in Settings.")));
}

#[test]
fn new_screenshots_appear_without_reopening_the_window() {
    let _sandbox = Sandbox::new();
    let mut viewer = Viewer::open();
    assert!(viewer.app.entries.is_empty());
    seed(now(), 40, 30);
    // The list is read again every couple of seconds, and settings changes come with it.
    viewer.app.refreshed = Instant::now() - Duration::from_secs(3);
    config::save_settings(&config::Settings {
        history_limit: 50,
        ..config::parse_settings("")
    })
    .unwrap();
    viewer.settle();
    assert_eq!(viewer.app.entries.len(), 1);
    assert_eq!(viewer.app.limit, 50);
    // Thumbnails of removed screenshots are forgotten.
    viewer
        .app
        .thumbnail(&viewer.screen.ctx.clone(), &viewer.app.entries[0].clone());
    assert_eq!(viewer.app.thumbnails.len(), 1);
    history::clear();
    viewer.app.refresh();
    assert!(viewer.app.thumbnails.is_empty());
}

#[test]
fn a_missing_thumbnail_leaves_the_cell_empty() {
    let _sandbox = Sandbox::new();
    let entry = seed(now(), 40, 30);
    fs::remove_file(&entry.thumbnail).unwrap();
    let viewer = Viewer::open();
    assert!(viewer.screen.images().is_empty());
    assert!(load_texture(&viewer.screen.ctx, &entry.thumbnail).is_none());
}

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

#[test]
fn a_chosen_area_is_cropped_and_can_be_stored() {
    let _sandbox = Sandbox::new();
    let entry = seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    viewer.click_text(tr("Crop"));
    assert!(!viewer.screen.has(tr("Apply crop")));
    let canvas = viewer.canvas();
    let from = canvas.min + egui::vec2(50.0, 40.0);
    viewer.act(|s, show| s.drag(from, from + egui::vec2(100.0, 60.0), show));
    assert!(viewer.app.opened.as_ref().unwrap().crop.is_some());
    viewer.click_text(tr("Apply crop"));
    {
        let opened = viewer.app.opened.as_ref().unwrap();
        assert!(opened.cropped && opened.crop.is_none());
        assert!(opened.size.0 < 300 && opened.size.1 < 200);
        // The area starts where the drag was first noticed, a little after the press.
        let (width, height) = opened.size;
        assert!(
            (50..=100).contains(&width) && (30..=60).contains(&height),
            "{:?}",
            opened.size
        );
    }
    assert!(viewer.app.tool == Tool::None);
    // The change is only in memory until it is stored.
    assert_eq!(imaging::decode(&entry.image).unwrap().0, 300);
    viewer.click_text(tr("Save changes"));
    let (width, height, _) = imaging::decode(&entry.image).unwrap();
    assert!(width < 300 && height < 200);
    assert!(viewer.screen.has(tr("Saved the changes.")));
    assert!(!viewer.app.opened.as_ref().unwrap().cropped);
    assert!(!viewer.screen.has(tr("Save changes")));
}

#[test]
fn an_empty_crop_area_changes_nothing() {
    let _sandbox = Sandbox::new();
    seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    viewer.click_text(tr("Crop"));
    let canvas = viewer.canvas();
    let from = canvas.min + egui::vec2(50.0, 40.0);
    // A drag of a single pixel is below the smallest area that can be cut out.
    viewer.act(|s, show| s.drag(from, from + egui::vec2(0.2, 0.2), show));
    if viewer.app.opened.as_ref().unwrap().crop.is_some() {
        viewer.click_text(tr("Apply crop"));
    }
    let opened = viewer.app.opened.as_ref().unwrap();
    assert_eq!(opened.size, (300, 200));
    assert!(!opened.cropped);
}

#[test]
fn marks_are_baked_into_the_stored_picture() {
    let _sandbox = Sandbox::new();
    let entry = seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    viewer.click_text(tr("Pen"));
    let canvas = viewer.canvas();
    let from = canvas.min + egui::vec2(20.0, 100.0);
    viewer.act(|s, show| s.drag(from, from + egui::vec2(100.0, 0.0), show));
    let before = fs::read(&entry.image).unwrap();
    viewer.click_text(tr("Save changes"));
    assert_ne!(fs::read(&entry.image).unwrap(), before);
    let opened = viewer.app.opened.as_ref().unwrap();
    assert!(
        opened.marks.is_empty(),
        "the marks are part of the picture now"
    );
    // The picture is rewritten, so the cached thumbnail is dropped and loaded again.
    assert!(!viewer.app.thumbnails.contains_key(&entry.id));
}

#[test]
fn the_color_under_the_pointer_is_shown_as_it_is_in_the_picture() {
    let _sandbox = Sandbox::new();
    seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    viewer.click_text(tr("Pick color"));
    let canvas = viewer.canvas();
    // Equivalence: the left half is red, the right half cyan; outside the picture shows nothing.
    viewer.act(|s, show| s.hover(canvas.min + egui::vec2(30.0, 50.0), show));
    assert!(viewer.screen.has("#FF0000"), "{:?}", viewer.screen.texts());
    viewer.act(|s, show| s.hover(canvas.min + egui::vec2(250.0, 50.0), show));
    assert!(viewer.screen.has("#00FFFF"));
    assert!(!viewer.screen.has("#FF0000"));
    // Boundary: the last column and the first one are still inside; the readout flips to stay on screen.
    viewer.act(|s, show| s.hover(egui::pos2(canvas.max.x - 0.5, canvas.min.y + 1.0), show));
    assert!(viewer.screen.has("#00FFFF"));
    viewer.act(|s, show| s.hover(egui::pos2(canvas.min.x + 0.5, canvas.min.y + 1.0), show));
    assert!(viewer.screen.has("#FF0000"));
    viewer.act(|s, show| s.hover(canvas.min - egui::vec2(5.0, 5.0), show));
    assert!(!viewer.screen.has("#FF0000") && !viewer.screen.has("#00FFFF"));
    // Marks drawn on the picture change what is picked.
    viewer.app.tool = Tool::Pen;
    viewer.app.opened.as_mut().unwrap().marks.push(Mark {
        highlight: false,
        color: [0, 255, 0],
        radius: 30.0,
        points: vec![(30.0, 50.0)],
    });
    viewer.app.tool = Tool::Picker;
    viewer.act(|s, show| s.hover(canvas.min + egui::vec2(30.0, 50.0), show));
    assert!(viewer.screen.has("#00FF00"));
}

#[test]
#[ignore = "Replaces the clipboard content"]
fn clicking_the_picker_copies_the_color() {
    let _sandbox = Sandbox::new();
    seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    viewer.click_text(tr("Pick color"));
    let canvas = viewer.canvas();
    viewer.click(canvas.min + egui::vec2(250.0, 50.0));
    assert_eq!(clipboard_text().as_deref(), Some("#00FFFF"));
    assert!(viewer.screen.has(tr("Copied the color.")));
}

#[test]
#[ignore = "Replaces the clipboard content"]
fn the_copy_button_puts_the_picture_on_the_clipboard() {
    let _sandbox = Sandbox::new();
    seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    viewer.click_text(tr("Copy"));
    assert!(viewer.screen.has(tr("Copied to the clipboard.")));
}

#[test]
#[ignore = "Replaces the clipboard content"]
fn a_finished_recognition_copies_its_text_or_says_why_not() {
    let _sandbox = Sandbox::new();
    seed(now(), 60, 40);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    let ctx = viewer.screen.ctx.clone();
    let feed = |app: &mut HistoryApp, result: Option<Result<String, crate::ocr::Error>>| {
        let (sender, receiver) = std::sync::mpsc::channel();
        if let Some(result) = result {
            sender.send(result).unwrap();
        }
        app.ocr = Some(receiver);
        app.finish_recognition(&ctx);
        drop(sender);
    };
    // Decision table over the engine's answer.
    feed(&mut viewer.app, Some(Ok("Hello 日本語".into())));
    assert_eq!(clipboard_text().as_deref(), Some("Hello 日本語"));
    assert_eq!(viewer.app.notice.map(|n| n.1), Some(tr("Copied the text.")));
    assert!(viewer.app.ocr.is_none());
    feed(&mut viewer.app, Some(Ok("  \r\n ".into())));
    assert_eq!(
        viewer.app.notice.map(|n| n.1),
        Some(tr("No text was found in the image."))
    );
    feed(&mut viewer.app, Some(Err(crate::ocr::Error::NoLanguage)));
    assert_eq!(
        viewer.app.notice.map(|n| n.1),
        Some(crate::ocr::Error::NoLanguage.message())
    );
    feed(&mut viewer.app, Some(Err(crate::ocr::Error::TooLarge)));
    assert_eq!(
        viewer.app.notice.map(|n| n.1),
        Some(crate::ocr::Error::TooLarge.message())
    );
    // The recognizer ended without an answer.
    let (sender, receiver) = std::sync::mpsc::channel::<Result<String, crate::ocr::Error>>();
    drop(sender);
    viewer.app.ocr = Some(receiver);
    viewer.app.finish_recognition(&ctx);
    assert_eq!(
        viewer.app.notice.map(|n| n.1),
        Some(crate::ocr::Error::Failed.message())
    );
    // Still working: keep waiting and report nothing new.
    let (sender, receiver) = std::sync::mpsc::channel::<Result<String, crate::ocr::Error>>();
    viewer.app.ocr = Some(receiver);
    viewer.app.notice = None;
    viewer.app.finish_recognition(&ctx);
    assert!(viewer.app.ocr.is_some() && viewer.app.notice.is_none());
    drop(sender);
}

#[test]
#[ignore = "Runs the Windows text recognition engine"]
fn the_copy_text_button_recognizes_the_picture() {
    let _sandbox = Sandbox::new();
    // A plain two-color picture holds no text, which the notice must say.
    seed(now(), 120, 80);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    viewer.click_text(tr("Copy text"));
    assert!(viewer.app.ocr.is_some() || viewer.app.notice.is_some());
    let deadline = Instant::now() + Duration::from_secs(20);
    while viewer.app.ocr.is_some() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        viewer.settle();
    }
    assert!(viewer.app.ocr.is_none(), "the recognition finished");
    let message = viewer.app.notice.map(|n| n.1);
    assert!(
        message == Some(tr("No text was found in the image."))
            || message == Some(crate::ocr::Error::NoLanguage.message()),
        "{message:?}"
    );
    // A second press while one is running starts nothing new.
    viewer.app.ocr = Some(std::sync::mpsc::channel().1);
    viewer.click_text(tr("Copy text"));
    assert!(viewer.app.ocr.is_some());
}

#[test]
#[ignore = "Shows a dialog"]
fn save_as_writes_the_picture_where_the_dialog_says() {
    let sandbox = Sandbox::new();
    let shots = sandbox.path().join("saved");
    fs::create_dir_all(&shots).unwrap();
    config::save_settings(&config::Settings {
        screenshot_folder: shots.to_string_lossy().into_owned(),
        ..config::parse_settings("")
    })
    .unwrap();
    let entry = seed(now(), 120, 80);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    let answer = answer_dialog(2);
    viewer.click_text(tr("Save as..."));
    assert!(answer.join().unwrap());
    assert!(viewer.app.notice.is_none(), "cancelling says nothing");
    assert!(!shots
        .join(format!("Snipping Lite {}.png", entry.id))
        .exists());
    let answer = answer_dialog(1);
    viewer.click_text(tr("Save as..."));
    assert!(answer.join().unwrap());
    assert!(shots
        .join(format!("Snipping Lite {}.png", entry.id))
        .is_file());
    assert_eq!(viewer.app.notice.map(|n| n.1), Some(tr("Saved the image.")));
}

#[test]
fn show_in_folder_selects_the_file_in_explorer() {
    let _sandbox = Sandbox::new();
    let entry = seed(now(), 120, 80);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    crate::test_support::take_shell_opened();
    viewer.click_text(tr("Show in folder"));
    assert_eq!(
        crate::test_support::take_shell_opened(),
        vec![(
            "explorer.exe".to_owned(),
            Some(format!("/select,\"{}\"", entry.image.display()))
        )]
    );
}

#[test]
fn hovering_and_keyboard_focus_highlight_the_controls() {
    let _sandbox = Sandbox::new();
    seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    // Hover a range chip, then walk the focus over the chips and the cell with Tab.
    let chip = viewer.screen.find(Range::Day.label()).unwrap().center();
    viewer.act(|s, show| s.hover(chip, show));
    for _ in 0..8 {
        viewer.act(|s, show| s.key(Key::Tab, egui::Modifiers::NONE, show));
    }
    let cell = viewer.screen.images()[0].center();
    viewer.act(|s, show| s.hover(cell, show));
    viewer.click(cell);
    viewer.click_text(tr("Pen"));
    let swatch = viewer.screen.find(tr("Thin")).unwrap().center() - egui::vec2(120.0, 0.0);
    viewer.act(|s, show| s.hover(swatch, show));
    for _ in 0..12 {
        viewer.act(|s, show| s.key(Key::Tab, egui::Modifiers::NONE, show));
    }
    let back = viewer.screen.find(tr("Back")).unwrap().center();
    viewer.act(|s, show| s.hover(back, show));
    viewer.click(back);
    assert!(viewer.app.opened.is_none());
}

#[test]
fn the_window_frame_follows_the_theme_and_the_backdrop() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetPropW, WS_POPUP,
    };
    let _captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let _sandbox = Sandbox::new();
    for mica in [false, true] {
        // SAFETY: a private, hidden window that stands in for the history window.
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                w("STATIC").as_ptr(),
                w(HISTORY_TITLE).as_ptr(),
                WS_POPUP,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            let viewer = Viewer::with(HistoryApp::new(mica, false));
            assert!(!GetPropW(hwnd, w(HISTORY_WINDOW).as_ptr()).is_null());
            let clear = viewer.app.clear_color(&egui::Visuals::dark());
            assert_eq!(clear == [0.0; 4], mica);
            DestroyWindow(hwnd);
        }
    }
}

#[test]
fn the_history_window_is_not_found_when_it_is_not_open() {
    let _captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    assert!(window().is_null());
}
