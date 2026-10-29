use super::engine::Engine;
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
    assert_eq!(engine.size(), (64, 48));
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
    assert!(Engine::open(&path).is_err());
    assert!(Engine::open(&path.with_file_name("snippinglite-player-missing.mp4")).is_err());
    let _ = std::fs::remove_file(path);
}
