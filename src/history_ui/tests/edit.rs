use super::*;

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
    // 送り手を持ったままにする。落とすと、次の描画で認識が終わったことになり、押したときに新しい
    // 認識が始まって、それが終わる速さで結果が変わる。
    let (_sender, receiver) = std::sync::mpsc::channel();
    viewer.app.ocr = Some(receiver);
    viewer.app.notice = None;
    viewer.click_text(tr("Copy text"));
    assert!(viewer.app.ocr.is_some(), "the recognition goes on");
    assert!(viewer.app.notice.is_none(), "no new recognition starts");
}
