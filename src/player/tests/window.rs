use super::*;
use crate::{
    config::AppTheme,
    i18n::tr,
    player::{controls::clock, window::PlayerApp},
    settings_ui::apply_theme,
    test_support::{
        screen::{Screen, Show},
        stub_window, Sandbox, CAPTIONS, THEME,
    },
};
use eframe::egui::{self, Key, Modifiers};

struct Viewer {
    app: PlayerApp,
    screen: Screen,
    frame: eframe::Frame,
}

impl Viewer {
    fn open(path: &std::path::Path) -> Viewer {
        Viewer {
            app: PlayerApp::new(false, path),
            screen: Screen::new(960.0, 640.0),
            frame: eframe::Frame::_new_kittest(),
        }
    }

    fn act<R>(&mut self, action: impl FnOnce(&mut Screen, Show) -> R) -> R {
        let (app, frame) = (&mut self.app, &mut self.frame);
        let mut show = |ui: &mut egui::Ui| app.ui(ui, frame);
        action(&mut self.screen, &mut show)
    }

    /// Draws frames until `text` shows up; the player thread needs real time.
    fn wait_for_text(&mut self, text: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            self.act(|screen, show| screen.settle(show));
            if self.screen.has(text) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        false
    }

    fn key(&mut self, key: Key) {
        self.act(|screen, show| screen.key(key, Modifiers::NONE, show));
    }

    fn click(&mut self, x: f32, y: f32) {
        self.act(|screen, show| screen.click(egui::pos2(x, y), show));
    }

    /// 1 フレームを描き、そのフレームで動画の絵が送られたなら、送られた先のテクスチャと、絵の
    /// 真ん中の色（RGBA）。文字のテクスチャ（既定の ID）は数えない。
    fn picture(&mut self) -> Option<(egui::TextureId, [u8; 4])> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(960.0, 640.0),
            )),
            ..Default::default()
        };
        let (app, frame) = (&mut self.app, &mut self.frame);
        let mut output = self.screen.ctx.run_ui(input, |ui| app.ui(ui, frame));
        let picture = output
            .textures_delta
            .set
            .iter()
            .filter(|(id, _)| **id != egui::TextureId::default())
            .find_map(|(id, deltas)| {
                let egui::ImageData::Color(image) = &deltas.last()?.image;
                let [width, height] = image.size;
                Some((*id, image.pixels[height / 2 * width + width / 2].to_array()))
            });
        // 送る先の GPU はないので、送られた絵は読んだら捨てる（`Screen::frame` と同じ）。
        output.textures_delta.clear();
        picture
    }

    /// 色が `expected` に近い絵が描かれるまで、フレームを描き続ける。
    fn wait_for_picture(&mut self, expected: [u8; 3]) -> Option<egui::TextureId> {
        wait_for(|| {
            self.picture()
                .filter(|(_, color)| near(color, expected))
                .map(|(id, _)| id)
        })
    }
}

use eframe::App;

/// 動画の部分（バーより上）の真ん中。窓は 640、バーは 72 の高さ。
const PICTURE: (f32, f32) = (480.0, (640.0 - 72.0) / 2.0);
/// バーの真ん中の高さ。
const BAR_Y: f32 = 640.0 - 36.0;

#[test]
fn times_read_as_minutes_and_seconds_and_hours_when_long() {
    for (seconds, text) in [
        (0, "0:00"),
        (59, "0:59"),
        (60, "1:00"),
        (3599, "59:59"),
        (3600, "1:00:00"),
        (3725, "1:02:05"),
    ] {
        assert_eq!(clock(Duration::from_secs(seconds)), text);
    }
    assert_eq!(clock(Duration::from_millis(1999)), "0:01");
}

#[test]
#[ignore = "plays a video through the system media player"]
fn a_video_that_cannot_be_played_says_so() {
    let path = std::env::temp_dir().join("snippinglite-player-broken.mp4");
    std::fs::write(&path, b"nothing").unwrap();
    let mut viewer = Viewer::open(&path);
    assert!(viewer.wait_for_text(tr("This video cannot be played.")));
    // The controls do nothing, and do not panic, without a player.
    viewer.key(Key::Space);
    viewer.key(Key::ArrowRight);
    viewer.app.mute();
    assert!(!viewer.app.playing());
    let _ = std::fs::remove_file(path);
}

#[test]
#[ignore = "plays a video through the system media player"]
fn the_keyboard_plays_pauses_and_skips() {
    let path = sample("window");
    let mut viewer = Viewer::open(&path);
    assert!(viewer.wait_for_text("0:00 / 0:01"));
    let shown = wait_for(|| {
        viewer.act(|screen, show| screen.settle(show));
        viewer.app.has_frame().then_some(())
    });
    assert!(shown.is_some(), "the first frame is shown");
    viewer.app.set_volume(0.0);
    viewer.key(Key::Space);
    assert!(wait_for(|| viewer.app.playing().then_some(())).is_some());
    viewer.key(Key::Space);
    assert!(wait_for(|| (!viewer.app.playing()).then_some(())).is_some());
    // Skipping past the end stops at the end, and playing again starts over.
    viewer.key(Key::ArrowRight);
    assert!(wait_for(|| {
        viewer.act(|screen, show| screen.settle(show));
        (viewer.app.position() > Duration::from_millis(800)).then_some(())
    })
    .is_some());
    viewer.key(Key::ArrowLeft);
    assert!(wait_for(|| {
        viewer.act(|screen, show| screen.settle(show));
        (viewer.app.position() < Duration::from_millis(200)).then_some(())
    })
    .is_some());
    // Muting remembers the volume.
    viewer.app.set_volume(0.4);
    viewer.app.mute();
    assert_eq!(viewer.app.volume(), 0.0);
    viewer.app.mute();
    assert!((viewer.app.volume() - 0.4).abs() < 0.01);
    let _ = std::fs::remove_file(path);
}

#[test]
#[ignore = "plays a video through the system media player"]
fn the_bar_plays_seeks_and_sets_the_volume_with_the_mouse() {
    let path = sample("bar");
    let mut viewer = Viewer::open(&path);
    assert!(viewer.wait_for_text("0:00 / 0:01"));
    let y = 640.0 - 36.0;
    viewer.app.set_volume(1.0);
    let click = |viewer: &mut Viewer, x: f32| {
        viewer.act(|screen, show| screen.click(eframe::egui::pos2(x, y), show));
    };
    // The play button, twice: play, then pause.
    click(&mut viewer, 36.0);
    assert!(wait_for(|| viewer.app.playing().then_some(())).is_some());
    click(&mut viewer, 36.0);
    assert!(wait_for(|| (!viewer.app.playing()).then_some(())).is_some());
    // The middle of the seek bar is about half way through the video.
    click(&mut viewer, 470.0);
    assert!(wait_for(|| {
        let at = viewer.app.position();
        (at > Duration::from_millis(250) && at < Duration::from_millis(750)).then_some(())
    })
    .is_some());
    // The loudspeaker mutes and unmutes; the volume bar sets a level.
    click(&mut viewer, 810.0);
    assert_eq!(viewer.app.volume(), 0.0);
    click(&mut viewer, 810.0);
    assert!(viewer.app.volume() > 0.9);
    click(&mut viewer, 889.0);
    assert!(viewer.app.volume() > 0.3 && viewer.app.volume() < 0.7);
    let _ = std::fs::remove_file(path);
}

#[test]
#[ignore = "plays a video through the system media player"]
fn a_missing_recording_cannot_be_played_and_its_controls_do_nothing() {
    // 通知から開いた録画が、もう消されていることがある。開けないので、プレイヤーはない。
    let sandbox = Sandbox::new();
    let mut viewer = Viewer::open(&sandbox.path().join("deleted.mp4"));
    viewer.act(|screen, show| screen.settle(show));
    assert!(viewer.screen.has(tr("This video cannot be played.")));
    assert!(viewer.screen.has("0:00 / 0:00"));
    // キーも、絵も、再生・シーク・スピーカー・音量のどこを押しても何も起きず、落ちない。
    for key in [Key::Space, Key::ArrowRight, Key::ArrowLeft] {
        viewer.key(key);
    }
    viewer.click(PICTURE.0, PICTURE.1);
    for x in [36.0, 470.0, 810.0, 889.0] {
        viewer.click(x, BAR_Y);
    }
    viewer.app.mute();
    assert!(!viewer.app.playing());
    assert_eq!(viewer.app.position(), Duration::ZERO);
    assert_eq!(viewer.app.volume(), 0.0);
    assert!(viewer.screen.has(tr("This video cannot be played.")));
}

#[test]
#[ignore = "plays a video through the system media player"]
fn the_picture_has_the_colors_of_the_video_and_follows_a_seek() {
    let sandbox = Sandbox::new();
    let mut viewer = Viewer::open(&sample_in(&sandbox, 30));
    // 見本の 1 枚目は BGRA で (0, 255, 128)。絵は RGBA なので (128, 255, 0) になる。
    let texture = viewer
        .wait_for_picture([128, 255, 0])
        .expect("the first frame in the colors of the video");
    // 900 ms は 30 fps の 27 枚目。濃さは 27 * 255 / 30 = 229 で、BGRA (229, 26, 128) は
    // RGBA (128, 26, 229)。止まったままでも、送った先の絵に変わる。
    viewer.app.seek_to(Duration::from_millis(900));
    let replaced = viewer
        .wait_for_picture([128, 26, 229])
        .expect("the frame at the new position");
    assert_eq!(replaced, texture, "the new frame replaces the old one");
}

#[test]
#[ignore = "plays a video through the system media player"]
fn clicking_the_picture_plays_and_pauses() {
    let sandbox = Sandbox::new();
    // 5 秒の動画。2 度目のクリックが、終わりではなく途中で止めたのだとわかる。
    let mut viewer = Viewer::open(&sample_in(&sandbox, 150));
    assert!(viewer.wait_for_text("0:00 / 0:05"));
    let shown = wait_for(|| {
        viewer.act(|screen, show| screen.settle(show));
        viewer.app.has_frame().then_some(())
    });
    assert!(shown.is_some(), "the first frame is shown");
    viewer.app.set_volume(0.0);
    viewer.click(PICTURE.0, PICTURE.1);
    assert!(wait_for(|| viewer.app.playing().then_some(())).is_some());
    viewer.click(PICTURE.0, PICTURE.1);
    assert!(wait_for(|| (!viewer.app.playing()).then_some(())).is_some());
    let paused = viewer.app.position();
    assert!(paused < Duration::from_secs(4), "{paused:?}");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(viewer.app.position(), paused, "a paused video stays put");
}

#[test]
#[ignore = "plays a video through the system media player"]
fn playing_from_the_end_starts_over() {
    let sandbox = Sandbox::new();
    let mut viewer = Viewer::open(&sample_in(&sandbox, 30));
    assert!(viewer.wait_for_text("0:00 / 0:01"));
    viewer.app.set_volume(0.0);
    // 長さより先へは送れない。終わり（1 秒）で止まる。
    viewer.app.seek_to(Duration::from_secs(60));
    let at_end = wait_for(|| (viewer.app.position() > Duration::from_millis(950)).then_some(()));
    assert!(at_end.is_some(), "{:?}", viewer.app.position());
    assert!(!viewer.app.playing());
    viewer.key(Key::Space);
    // 1 秒の動画を頭から再生すると、最初の半分の間は 0.5 秒より前にいる。
    let again = wait_for(|| (viewer.app.position() < Duration::from_millis(500)).then_some(()));
    assert!(again.is_some(), "playing again starts at the beginning");
}

#[test]
#[ignore = "creates a window"]
fn the_window_is_known_as_the_player_and_its_title_bar_follows_the_theme() {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND},
        Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE},
        UI::WindowsAndMessaging::DestroyWindow,
    };
    // The same order everywhere: theme, captions, then the environment.
    let _theme = THEME.lock().unwrap_or_else(|e| e.into_inner());
    let _captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new();
    // eframe が開く窓の代わり。題名は製品の窓と同じ「Snipping Lite」で、まだ役の印はない。
    let window = stub_window("SnippingLite.Test.Unnamed");
    let dark_title_bar = |window: HWND| {
        let mut dark: BOOL = -1;
        // SAFETY: the value and its size describe `dark`.
        let read = unsafe {
            DwmGetWindowAttribute(
                window,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                (&mut dark as *mut BOOL).cast(),
                4,
            )
        } == 0;
        read.then_some(dark != 0)
    };
    let mut viewer = Viewer::open(&sandbox.path().join("none.mp4"));
    // テーマを変えるたびに、題名の帯がそれに合う。
    for (choice, dark) in [
        (AppTheme::Dark, true),
        (AppTheme::Light, false),
        (AppTheme::Dark, true),
    ] {
        apply_theme(&viewer.screen.ctx, choice);
        viewer.act(|screen, show| screen.frame(show));
        assert_eq!(dark_title_bar(window), Some(dark), "{choice:?}");
    }
    // 撮影の前にトレイが隠せるよう、窓はプレイヤーの印を持つ。
    // SAFETY: plain window queries.
    assert_eq!(unsafe { crate::find_window(crate::PLAYER_WINDOW) }, window);
    crate::ui::set_theme_choice(AppTheme::System);
    // SAFETY: the window belongs to this test.
    unsafe { DestroyWindow(window) };
}

#[test]
fn with_mica_the_window_is_cleared_so_the_backdrop_shows_through() {
    let sandbox = Sandbox::new();
    let missing = sandbox.path().join("none.mp4");
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = egui::Color32::from_rgb(51, 102, 204);
    // Mica は窓の後ろに描かれるので、透明で塗る。
    assert_eq!(
        PlayerApp::new(true, &missing).clear_color(&visuals),
        [0.0; 4]
    );
    // Mica がなければ、窓の地の色で塗る。51, 102, 204 は 255 の 0.2, 0.4, 0.8。
    let plain = PlayerApp::new(false, &missing).clear_color(&visuals);
    for (got, want) in plain.into_iter().zip([0.2, 0.4, 0.8, 1.0]) {
        assert!((got - want).abs() < 1e-6, "{plain:?}");
    }
}
