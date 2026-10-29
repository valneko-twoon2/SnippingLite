use super::engine::{Engine, Status};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn sample(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("snippinglite-player-{name}.mp4"));
    crate::record::write_sample(&path, 64, 48, 30).expect("write the sample video");
    path
}

fn wait_for<T>(mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(found) = probe() {
            return Some(found);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

#[test]
#[ignore = "plays a video through the system media player"]
fn a_recorded_video_opens_seeks_and_serves_frames() {
    let path = sample("open");
    let mut engine = Engine::open(&path).expect("open the video");
    assert!(wait_for(|| (engine.status() == Status::Ready).then_some(())).is_some());
    let length = engine.duration();
    assert!(
        length > Duration::from_millis(800) && length < Duration::from_millis(1300),
        "{length:?}"
    );
    let first = wait_for(|| engine.frame().unwrap()).expect("a first frame");
    assert_eq!((first.width, first.height), (64, 48));
    assert_eq!(first.pixels.len(), 64 * 48 * 4);
    engine.seek(Duration::from_millis(900));
    let last = wait_for(|| engine.frame().unwrap()).expect("a frame after seeking");
    assert_ne!(first.pixels[..4], last.pixels[..4]);
    engine.set_volume(0.0);
    assert_eq!(engine.volume(), 0.0);
    engine.play();
    assert!(wait_for(|| engine.playing().then_some(())).is_some());
    assert!(wait_for(|| engine.ended().then_some(())).is_some());
    assert!(engine.position() > Duration::from_millis(800));
    engine.pause();
    assert!(!engine.playing());
    let _ = std::fs::remove_file(path);
}

#[test]
#[ignore = "plays a video through the system media player"]
fn a_file_that_is_not_a_video_does_not_open() {
    let path = std::env::temp_dir().join("snippinglite-player-bad.mp4");
    std::fs::write(&path, b"not a video").unwrap();
    for file in [
        path.clone(),
        path.with_file_name("snippinglite-player-missing.mp4"),
    ] {
        let Ok(mut engine) = Engine::open(&file) else {
            continue;
        };
        assert!(wait_for(|| (engine.status() == Status::Failed).then_some(())).is_some());
    }
    let _ = std::fs::remove_file(path);
}

mod window {
    use super::*;
    use crate::{
        i18n::tr,
        player::{controls::clock, window::PlayerApp},
        test_support::screen::{Screen, Show},
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
    }

    use eframe::App;

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
}
