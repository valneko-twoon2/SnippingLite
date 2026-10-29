use super::*;

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
