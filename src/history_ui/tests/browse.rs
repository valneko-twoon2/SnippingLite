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

/// 開いている画像が、開いている間に履歴から消えた（トレイが上限を超えた古いものを消した、など）。
/// 一覧に場所がないので隣を決められず、矢印キーは何もしない。別の画像へ飛ばず、開いた画像はそのまま。
#[test]
fn the_arrow_keys_stay_on_a_screenshot_removed_while_it_is_open() {
    let _sandbox = Sandbox::new();
    let t = now();
    let entries: Vec<Entry> = [t - HOUR, t - 2 * HOUR, t - 3 * HOUR]
        .into_iter()
        .map(|id| seed(id, 60, 40))
        .collect();
    let mut viewer = Viewer::open();
    let ctx = viewer.screen.ctx.clone();
    viewer.app.open(&ctx, entries[1].clone());
    history::remove_entry(&entries[1]);
    viewer.app.refreshed = Instant::now() - Duration::from_secs(3);
    viewer.settle();
    assert_eq!(
        viewer.app.entries,
        vec![entries[0].clone(), entries[2].clone()]
    );
    for key in [Key::ArrowRight, Key::ArrowLeft] {
        viewer.key(key);
        assert_eq!(viewer.opened_id(), Some(entries[1].id), "{key:?}");
    }
    assert!(
        viewer.screen.has("   60 x 40"),
        "the picture is still shown"
    );
    viewer.key(Key::Escape);
    assert!(viewer.app.opened.is_none());
    assert_eq!(
        viewer.screen.images().len(),
        2,
        "the gallery shows the rest"
    );
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

/// 開いている画像の表示用のテクスチャの大きさ（幅、高さ）。
fn shown_texture(viewer: &Viewer) -> [usize; 2] {
    let opened = viewer.app.opened.as_ref().expect("an image is opened");
    opened.texture.as_ref().expect("a texture is made").size()
}

#[test]
fn a_picture_wider_than_a_texture_can_be_opens_shrunk_for_the_screen_only() {
    // テクスチャの辺の上限（ウィンドウなしの画面では egui の既定の 2048、実際のウィンドウでは GPU の
    // 上限）を 1 画素超える絵。そのままテクスチャにすると egui が panic するので、表示だけを上限に
    // 収まるよう縮める。手計算: 2049 x 1 は長い辺を 2048 に合わせ、高さは 1 x 2048 / 2049 = 0.9995
    // を丸めて 1。開いた画像そのもの（コピーや保存に使う画素）は 2049 x 1 のまま。
    let _sandbox = Sandbox::new();
    let t = now();
    let keep = seed(t - HOUR, 60, 40);
    let wide = seed(t, 2049, 1);
    let mut viewer = Viewer::open();
    assert_eq!(viewer.screen.ctx.input(|i| i.max_texture_side), 2048);
    assert_eq!(viewer.app.entries.len(), 2);
    assert_eq!(
        viewer.screen.images().len(),
        2,
        "the wide thumbnail is drawn too"
    );
    let ctx = viewer.screen.ctx.clone();
    viewer.app.open(&ctx, wide.clone());
    viewer.settle();
    assert_eq!(viewer.opened_id(), Some(wide.id));
    assert_eq!(viewer.app.notice, None);
    assert_eq!(shown_texture(&viewer), [2048, 1]);
    let opened = viewer.app.opened.as_ref().unwrap();
    assert_eq!(opened.size, (2049, 1));
    assert_eq!(opened.flattened().len(), 2049 * 4);
    assert!(viewer.screen.has("   2049 x 1"));
    // 境界: ちょうど上限の幅なら縮めない。ほかの項目もそのまま開ける。
    let edge = seed(t + 1, 2048, 1);
    viewer.app.open(&ctx, edge.clone());
    viewer.settle();
    assert_eq!(viewer.opened_id(), Some(edge.id));
    assert_eq!(shown_texture(&viewer), [2048, 1]);
    viewer.app.open(&ctx, keep.clone());
    assert_eq!(viewer.opened_id(), Some(keep.id));
    assert_eq!(shown_texture(&viewer), [60, 40]);
}

#[test]
fn a_shrunk_picture_keeps_its_shape_and_the_pen_draws_in_its_own_pixels() {
    // 手計算: 3000 x 1500 は長い辺を 2048 に合わせ、高さは 1500 x 2048 / 3000 = 1024。縦横比 2:1 の
    // まま 2048 x 1024 のテクスチャになる。画面上の絵は元の画像の大きさから配置するので、絵の右下の
    // 角は元の画像の (3000, 1500) に当たり、縮めたテクスチャの (2048, 1024) ではない。
    let _sandbox = Sandbox::new();
    let big = seed(now(), 3000, 1500);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    assert_eq!(viewer.opened_id(), Some(big.id));
    assert_eq!(shown_texture(&viewer), [2048, 1024]);
    assert_eq!(viewer.app.opened.as_ref().unwrap().size, (3000, 1500));
    let canvas = viewer.canvas();
    assert!(
        (canvas.width() / canvas.height() - 2.0).abs() < 0.01,
        "{canvas:?}"
    );
    viewer.click_text(tr("Pen"));
    let (from, to) = (
        canvas.min + egui::vec2(1.0, 1.0),
        canvas.max - egui::vec2(1.0, 1.0),
    );
    viewer.act(|s, show| s.drag(from, to, show));
    let opened = viewer.app.opened.as_ref().unwrap();
    let &(x, y) = opened.marks[0].points.last().unwrap();
    // 1 画面画素は 3000 / 絵の幅 画像画素。端から 1 画面画素の内側なので、数画素の内側に来る。
    let per = 3000.0 / canvas.width();
    assert!((x - (3000.0 - per)).abs() < 2.0 * per, "{x}");
    assert!((y - (1500.0 - per)).abs() < 2.0 * per, "{y}");
    // コピーや保存に使う画素は元の解像度のまま。
    assert_eq!(opened.flattened().len(), 3000 * 1500 * 4);
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
