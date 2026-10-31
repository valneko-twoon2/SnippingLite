//! 壊れた画像のファジング。正しい見本（GDI+ が書いた PNG、つまり履歴に置かれる形式と、参照の
//! 書き方で書いた stored の PNG）を、バイト単位と構造を知った変異で壊して `decode` に読ませる。
//! オラクル: どの入力でも panic せず、読めないものは `None`、読めたものは大きさと画素の配列の長さが
//! 合い、IHDR が正しければその大きさで、PNG として最後まで正しければ参照の読み取りと同じ画素になる。
//! 巨大な大きさの宣言に引きずられて、メモリーを取りすぎない。

use super::*;
use crate::test_support::{
    png::{self, Header, Picture, Sample, Verdict},
    rng::{self, Reach, Rng},
};

/// 1 つの入力を読む間に、コミットしたメモリーの最大値が増えてよい量。見本は数百バイトなので、
/// 読むべき画素はわずかで、増えるのは並んで走るほかのテストのぶんだけ。
const MEMORY_GROWTH: usize = 256 << 20;

/// GDI+ が書いた PNG（アプリが履歴に書く形式）と、その元の画素。
fn encoded(name: &str, width: i32, height: i32, pixels: Vec<u8>) -> Sample {
    let path = scratch(&format!("fuzz-sample-{name}.png"));
    encode(&path, &pixels, width, height).unwrap();
    Sample {
        name: format!("GDI+ {name}"),
        png: fs::read(&path).unwrap(),
        bgra: pixels,
    }
}

fn gdiplus_samples() -> Vec<Sample> {
    let gradient = (0..16 * 9u32)
        .flat_map(|i| [i as u8, (i * 7) as u8, 255 - i as u8, 255])
        .collect();
    vec![
        encoded("3x2", 3, 2, sample()),
        encoded("16x9", 16, 9, gradient),
        encoded("1x1", 1, 1, vec![9, 8, 7, 255]),
    ]
}

/// 参照の書き方で書いた、1 〜 6 画素四方の stored の PNG。
fn stored_sample(rng: &mut Rng) -> Sample {
    let picture = Picture::random(rng);
    let block = rng.range(1..=64usize);
    Sample {
        name: format!("stored {}x{}", picture.width, picture.height),
        png: picture.png(block),
        bgra: png::swap_red_blue(&picture.rgba),
    }
}

/// 巨大とみなす宣言の画素数（BGRA で 100 MB）。
const HUGE: u64 = 25_000_000;

/// 先頭のチャンクが IHDR として読めるなら、その宣言の画素数。
fn declared(bytes: &[u8]) -> u64 {
    png::parse(bytes)
        .and_then(|parsed| Header::read(&parsed.chunks.first()?.data))
        .map_or(0, |h| h.width as u64 * h.height as u64)
}

/// `decode` の結果をオラクルに照らす。失敗したら、入力の先頭を 16 進で見せる。
fn check(bytes: &[u8], verdict: &Verdict, read: &Option<(i32, i32, Vec<u8>)>) {
    let shown: Vec<String> = bytes.iter().take(80).map(|b| format!("{b:02x}")).collect();
    let input = format!("{} bytes: {}", bytes.len(), shown.join(" "));
    if let Some((width, height, pixels)) = read {
        assert!(
            fits(pixels, *width, *height),
            "{width}x{height} with {} bytes of pixels from {input}",
            pixels.len()
        );
        if *verdict != Verdict::Header {
            let header = Header::read(&png::parse(bytes).unwrap().chunks[0].data).unwrap();
            assert_eq!(
                (*width as u32, *height as u32),
                (header.width, header.height),
                "the size read is the size declared, from {input}"
            );
        }
    }
    if let Verdict::Pixels {
        width,
        height,
        bgra,
    } = verdict
    {
        let (w, h, pixels) = read
            .as_ref()
            .unwrap_or_else(|| panic!("a correct {width}x{height} PNG is read, from {input}"));
        assert_eq!((*w as u32, *h as u32), (*width, *height), "{input}");
        assert!(
            png::same_pixels(pixels, bgra),
            "the pixels are {pixels:?}, not {bgra:?}, from {input}"
        );
    }
}

#[test]
fn damaged_pictures_read_as_nothing_or_as_a_picture_of_the_declared_size() {
    let samples = gdiplus_samples();
    let path = scratch("fuzz-case.png");
    let mut reach = Reach::new(&[
        "header", "chunk", "inflate", "pixels", "decoded", "refused", "huge",
    ]);
    let cases = rng::check(
        "damaged_pictures_read_as_nothing_or_as_a_picture_of_the_declared_size",
        300,
        |rng| {
            let built;
            let sample = if rng.chance(0.5) {
                &samples[rng.below(samples.len())]
            } else {
                built = stored_sample(rng);
                &built
            };
            rng.note(format!("from {}", sample.name));
            let donor = &samples[rng.below(samples.len())].png;
            let bytes = png::damage(rng, sample, donor);
            let verdict = png::judge(&bytes, sample);
            fs::write(&path, &bytes).unwrap();
            let peak = peak_commit();
            let read = decode(&path);
            let grown = peak_commit().saturating_sub(peak);
            assert!(
                grown < MEMORY_GROWTH,
                "reading {} bytes that declare {} pixels raised the peak commit by {} MB",
                bytes.len(),
                declared(&bytes),
                grown >> 20
            );
            check(&bytes, &verdict, &read);
            reach.hit(match verdict {
                Verdict::Header => "header",
                Verdict::Chunk => "chunk",
                Verdict::Inflate => "inflate",
                Verdict::Pixels { .. } => "pixels",
            });
            reach.hit(if read.is_some() { "decoded" } else { "refused" });
            if declared(&bytes) >= HUGE {
                reach.hit("huge");
            }
        },
    );
    reach.finish("damaged_pictures_read", cases);
}

#[test]
fn the_reference_reader_agrees_with_gdiplus_on_correct_pictures() {
    // 変異なしの見本はすべて正しい PNG で、GDI+ と参照の読み取りが同じ画素を返す。参照の書き方と
    // 読み方の誤りが、ファジングの判定を狂わせていないことを確かめる。
    let path = scratch("fuzz-reference.png");
    let mut filters = Reach::new(&["none", "sub", "up", "average", "paeth"]);
    let cases = rng::check(
        "the_reference_reader_agrees_with_gdiplus_on_correct_pictures",
        100,
        |rng| {
            let sample = stored_sample(rng);
            let verdict = png::judge(&sample.png, &sample);
            assert!(matches!(verdict, Verdict::Pixels { .. }), "{}", sample.name);
            fs::write(&path, &sample.png).unwrap();
            check(&sample.png, &verdict, &decode(&path));
            let chunks = png::parse(&sample.png).unwrap().chunks;
            let raw = png::unstore(&png::idat(&chunks)).unwrap();
            let line = 1 + 4 * Header::read(&chunks[0].data).unwrap().width as usize;
            for row in raw.chunks(line) {
                filters.hit(["none", "sub", "up", "average", "paeth"][row[0] as usize]);
            }
        },
    );
    filters.finish("the_reference_reader_agrees", cases);
    for sample in gdiplus_samples() {
        let verdict = png::judge(&sample.png, &sample);
        assert!(matches!(verdict, Verdict::Pixels { .. }), "{}", sample.name);
        fs::write(&path, &sample.png).unwrap();
        check(&sample.png, &verdict, &decode(&path));
    }
}
