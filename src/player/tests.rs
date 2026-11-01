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

/// 見本の動画の `frames` 枚（30 fps）を、テストの `sandbox` の中に書く。
fn sample_in(sandbox: &crate::test_support::Sandbox, frames: u32) -> PathBuf {
    let path = sandbox.path().join("sample.mp4");
    crate::record::write_sample(&path, 64, 48, frames).expect("write the sample video");
    path
}

/// 色の最初の 3 つが、圧縮の誤差と隣の 1 枚の違いの範囲で `expected` に近く、不透明か。
fn near(color: &[u8], expected: [u8; 3]) -> bool {
    color[..3]
        .iter()
        .zip(expected)
        .all(|(got, want)| got.abs_diff(want) <= 24)
        && color[3] == 255
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
    // 送る前に届いていた知らせで、送る前の絵がもう一度渡ることがある。止まっている間に絵が変わる
    // のは送ったときだけなので、変わった絵を待ち、それが 900 ms の絵かを色で確かめる。900 ms は
    // 30 fps の 27 枚目で、濃さは 27 * 255 / 30 = 229。BGRA で (229, 26, 128) に近い。
    let last = wait_for(|| {
        engine
            .frame()
            .unwrap()
            .filter(|frame| frame.pixels[..4] != first.pixels[..4])
    })
    .expect("a frame after seeking");
    assert!(
        near(&last.pixels[..4], [229, 26, 128]),
        "{:?}",
        &last.pixels[..4]
    );
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

/// プレイヤーの窓（動画とバー）を、窓なしの egui の画面で動かすテスト。
mod window;
