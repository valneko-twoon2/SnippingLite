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

fn noon() -> u128 {
    june_15(12, 0)
}

fn five_past_midnight() -> u128 {
    june_15(0, 5)
}

/// 時計が `clock` を返すときに、その時刻、15 分前、3 日前に撮った見本を並べたギャラリー。
fn three_screenshots_at(clock: fn() -> u128) -> Viewer {
    let t = clock();
    for (offset, size) in [(0, 40), (HOUR / 4, 50), (3 * DAY, 60)] {
        seed(t - offset, size, 30);
    }
    let mut app = HistoryApp::new(false, false);
    app.clock = clock;
    Viewer::with(app)
}

/// 日付の段が上から `days` の順に並び、各段の下（次の段より上）にその時刻の札があり、ほかに日付の段がない。
fn assert_days(viewer: &Viewer, days: &[(&str, &[&str])]) {
    let texts = viewer.screen.texts();
    let is_date = |text: &str| {
        text.len() == 10
            && text.char_indices().all(|(i, c)| {
                if i == 4 || i == 7 {
                    c == '/'
                } else {
                    c.is_ascii_digit()
                }
            })
    };
    let mut headings: Vec<(&str, f32)> = texts
        .iter()
        .filter(|(text, _)| is_date(text))
        .map(|(text, rect)| (text.as_str(), rect.top()))
        .collect();
    headings.sort_by(|a, b| a.1.total_cmp(&b.1));
    let dates: Vec<&str> = headings.iter().map(|h| h.0).collect();
    let expected: Vec<&str> = days.iter().map(|d| d.0).collect();
    assert_eq!(dates, expected, "the day headings, newest first");
    for (index, (date, times)) in days.iter().enumerate() {
        let below = headings[index].1;
        let above = headings.get(index + 1).map_or(f32::INFINITY, |h| h.1);
        for time in *times {
            assert!(
                texts
                    .iter()
                    .any(|(text, rect)| text == time && below < rect.top() && rect.top() < above),
                "a screenshot at {time} under {date}: {texts:?}"
            );
        }
    }
}

/// 3 枚の見本の件数の表示、1 枚に 1 つのサムネイル、絞り込みと消去のボタン。
fn assert_three_counted(viewer: &Viewer) {
    let count = tr("{total} screenshots, newest first (the oldest beyond {limit} are removed)")
        .replace("{total}", "3")
        .replace("{limit}", "200");
    assert!(viewer.screen.has(&count), "{:?}", viewer.screen.texts());
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
fn the_gallery_groups_screenshots_by_day_and_counts_them() {
    let _sandbox = Sandbox::new();
    let viewer = three_screenshots_at(noon);
    // 正午: 15 分前（11:45）は同じ日、3 日前は 6 月 12 日の正午。
    assert_days(
        &viewer,
        &[
            ("2026/06/15", &["12:00", "11:45"]),
            ("2026/06/12", &["12:00"]),
        ],
    );
    assert_three_counted(&viewer);
}

#[test]
fn a_screenshot_from_just_before_midnight_is_grouped_under_the_day_before() {
    let _sandbox = Sandbox::new();
    let viewer = three_screenshots_at(five_past_midnight);
    // 0:05: 15 分前は前の日の 23:50 なので、日付の段は 3 つになり、ギャラリーは画面に収まらない。
    assert_days(
        &viewer,
        &[
            ("2026/06/15", &["00:05"]),
            ("2026/06/14", &["23:50"]),
            ("2026/06/12", &["00:05"]),
        ],
    );
    assert_three_counted(&viewer);
}

#[test]
fn the_range_filters_count_back_from_the_clock_of_the_window() {
    let _sandbox = Sandbox::new();
    // 時計の 23 時間前と 25 時間前。最近 24 時間に入るのは前者だけで、実行した時刻にはよらない。
    let t = noon();
    seed(t - 23 * HOUR, 40, 30);
    seed(t - 25 * HOUR, 40, 30);
    let mut app = HistoryApp::new(false, false);
    app.clock = noon;
    app.range = Range::Day;
    assert_eq!(app.visible().len(), 1);
    assert_eq!(app.visible()[0].id, t - 23 * HOUR);
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
