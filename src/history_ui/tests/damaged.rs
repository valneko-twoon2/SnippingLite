//! 壊れた 1 件のファジング。正しい項目の間に、画像かサムネイル（または両方）を壊した 1 件を置いて
//! ギャラリーを開き、すべての項目を開く。オラクル: どのフレームも panic せず、すべての項目が一覧と
//! 件数に入り、正しい項目のサムネイルはすべて描かれ、正しい項目は元の大きさで開ける。壊れた項目は、
//! 読めれば（テクスチャの辺の上限を超えるなら表示を縮めて）元の大きさで開き、そうでなければ
//! 「開けません」と知らせて、ギャラリーに留まる。

use super::*;
use crate::test_support::{
    png::{self, Sample},
    rng::{self, Reach},
};

fn noon() -> u128 {
    june_15(12, 0)
}

/// 項目の画像を、元の画像を壊したものに置き換える。読めるなら、その大きさを返す。テクスチャの辺の
/// 上限を超える大きさでも、表示を縮めて開けるので、読めるかどうかだけで決まる。
fn damage(rng: &mut rng::Rng, path: &Path, donor: &[u8]) -> Option<(i32, i32)> {
    let sample = Sample {
        name: path.display().to_string(),
        png: fs::read(path).unwrap(),
        bgra: Vec::new(),
    };
    fs::write(path, png::damage(rng, &sample, donor)).unwrap();
    imaging::decode(path).map(|(width, height, _)| (width, height))
}

#[test]
fn one_damaged_screenshot_leaves_the_others_listed_shown_and_openable() {
    let _sandbox = Sandbox::new();
    let mut viewer = Viewer::open();
    let mut reach = Reach::new(&[
        "thumbnail drawn",
        "thumbnail left empty",
        "opened",
        "reported",
    ]);
    let cases = rng::check(
        "one_damaged_screenshot_leaves_the_others_listed_shown_and_openable",
        40,
        |rng| {
            history::clear();
            let t = noon();
            let good: Vec<Entry> = (0..rng.range(1..=3u64) as u128)
                .map(|i| seed(t - (2 * i + 1) * HOUR, 60, 40))
                .collect();
            let broken = seed(t - 2 * rng.range(0..=3u64) as u128 * HOUR, 60, 40);
            let donor = fs::read(&good[0].image).unwrap();
            let part = rng.below(3);
            rng.note(format!("damage part {part} of {}", broken.id));
            let image = if part != 1 {
                damage(rng, &broken.image, &donor)
            } else {
                Some((60, 40))
            };
            let thumbnail = if part != 0 {
                damage(rng, &broken.thumbnail, &donor)
            } else {
                Some((60, 40))
            };
            let mut app = HistoryApp::new(false, false);
            app.clock = noon;
            viewer.app = app;
            viewer.settle();
            // すべての項目が一覧と件数に入り、正しい項目のサムネイルはすべて描かれる。
            let total = good.len() + 1;
            let count =
                tr("{total} screenshots, newest first (the oldest beyond {limit} are removed)")
                    .replace("{total}", &total.to_string())
                    .replace("{limit}", "200");
            assert!(viewer.screen.has(&count), "{:?}", viewer.screen.texts());
            assert_eq!(viewer.app.entries.len(), total);
            let drawn = viewer.screen.images().len();
            assert_eq!(drawn, good.len() + thumbnail.is_some() as usize);
            reach.hit(if thumbnail.is_some() {
                "thumbnail drawn"
            } else {
                "thumbnail left empty"
            });
            // 正しい項目は元の大きさで開け、壊れた項目は開けるか、開けないと知らせる。
            for entry in good.iter().chain([&broken]) {
                let ctx = viewer.screen.ctx.clone();
                viewer.app.notice = None;
                viewer.app.open(&ctx, entry.clone());
                viewer.settle();
                let opened = viewer.app.opened.as_ref().map(|o| (o.entry.id, o.size));
                if entry.id != broken.id || image.is_some() {
                    let size = if entry.id == broken.id {
                        image.unwrap()
                    } else {
                        (60, 40)
                    };
                    assert_eq!(opened, Some((entry.id, size)), "{}", entry.id);
                } else {
                    assert_eq!(opened, None);
                    assert_eq!(
                        viewer.app.notice.map(|n| n.1),
                        Some(tr("Could not open the image."))
                    );
                }
                if entry.id == broken.id {
                    reach.hit(if opened.is_some() {
                        "opened"
                    } else {
                        "reported"
                    });
                }
                viewer.key(Key::Escape);
                assert!(viewer.app.opened.is_none(), "back in the gallery");
            }
        },
    );
    reach.finish("one_damaged_screenshot", cases);
}
