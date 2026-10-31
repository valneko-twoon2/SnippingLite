//! 参照の読み取り。変異した PNG を仕様に照らし、どの段で止まるか、最後まで正しければどんな画素に
//! なるかを決める。

use super::*;

/// 変異の元にする正しい PNG と、その画素（BGRA）。GDI+ が書いた見本は圧縮されていて、参照では
/// 展開しない。そのため、元と同じ IHDR と IDAT のデータを持つ入力は元と同じ画素になる、とだけ判定する。
pub struct Sample {
    pub name: String,
    pub png: Vec<u8>,
    pub bgra: Vec<u8>,
}

/// 参照の読み取りが止まった段。
#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// 署名か IHDR（先頭のチャンク）が壊れているか、IHDR の値が仕様の外。
    Header,
    /// IHDR の後ろのチャンクの長さ、検査値、並びが壊れている。
    Chunk,
    /// チャンクは正しいが、IDAT の zlib、deflate、フィルター、データの長さが壊れているか、
    /// 参照では確かめられない（圧縮されたデータ、RGBA 以外の形式）。
    Inflate,
    /// 正しい PNG で、画素はこのとおり。
    Pixels {
        width: u32,
        height: u32,
        bgra: Vec<u8>,
    },
}

/// 画素に関わらない補助チャンク（変異が足すもの）。tEXt は文字の注記、snIp は私用のチャンク
/// （1 文字目が小文字で補助、2 文字目が小文字で私用、3 文字目は大文字で予約どおり）。
pub const HARMLESS: [[u8; 4]; 2] = [*b"tEXt", *b"snIp"];

/// `bytes` を `sample` から作った入力として判定する。
pub fn judge(bytes: &[u8], sample: &Sample) -> Verdict {
    let Some(parsed) = parse(bytes) else {
        return Verdict::Header;
    };
    let chunks = &parsed.chunks;
    let header = chunks
        .first()
        .filter(|c| &c.kind == b"IHDR" && c.sealed())
        .and_then(|c| Header::read(&c.data))
        .filter(Header::valid);
    let Some(header) = header else {
        return Verdict::Header;
    };
    let original = parse(&sample.png).expect("the sample is a PNG").chunks;
    if !well_formed(&parsed) || skeleton(chunks) != skeleton(&original) {
        return Verdict::Chunk;
    }
    let data = idat(chunks);
    if chunks[0] == original[0] && data == idat(&original) {
        return Verdict::Pixels {
            width: header.width,
            height: header.height,
            bgra: sample.bgra.clone(),
        };
    }
    if header != Header::rgba(header.width, header.height) {
        return Verdict::Inflate;
    }
    match unstore(&data).and_then(|raw| unfilter(&raw, header.width, header.height)) {
        Some(rgba) => Verdict::Pixels {
            width: header.width,
            height: header.height,
            bgra: swap_red_blue(&rgba),
        },
        None => Verdict::Inflate,
    }
}

/// チャンクの枠が仕様どおりか。検査値がすべて合い、余りのバイトがなく、IHDR は 1 つだけ、IEND は
/// 最後に 1 つだけで空、IDAT は 1 つ以上が途切れずに並び、知らない必須チャンクがない。
fn well_formed(parsed: &Parsed) -> bool {
    let chunks = &parsed.chunks;
    let count = |kind: &[u8; 4]| chunks.iter().filter(|c| &c.kind == kind).count();
    let idats: Vec<usize> = (0..chunks.len())
        .filter(|&i| &chunks[i].kind == b"IDAT")
        .collect();
    parsed.leftover == 0
        && chunks.iter().all(Chunk::sealed)
        && count(b"IHDR") == 1
        && count(b"IEND") == 1
        && chunks
            .last()
            .is_some_and(|c| &c.kind == b"IEND" && c.data.is_empty())
        && idats
            .first()
            .zip(idats.last())
            .is_some_and(|(first, last)| last - first + 1 == idats.len())
        && chunks
            .iter()
            .all(|c| !c.critical() || [b"IHDR", b"IDAT", b"IEND"].contains(&&c.kind))
}

/// IHDR と、画素に関わらない補助チャンクを除いた並び。IDAT の連なりは 1 つの `None` にまとめる。
/// 元の見本と同じなら、補助チャンク（gAMA など、画素を変えうるもの）は元のままと分かる。
fn skeleton(chunks: &[Chunk]) -> Vec<Option<&Chunk>> {
    let mut out = Vec::new();
    for chunk in chunks.iter().skip(1) {
        if HARMLESS.contains(&chunk.kind) {
            continue;
        }
        if &chunk.kind == b"IDAT" {
            if out.last() != Some(&None) {
                out.push(None);
            }
            continue;
        }
        out.push(Some(chunk));
    }
    out
}

/// IDAT のデータをつなげたもの（zlib ストリーム）。
pub fn idat(chunks: &[Chunk]) -> Vec<u8> {
    chunks
        .iter()
        .filter(|c| &c.kind == b"IDAT")
        .flat_map(|c| c.data.iter().copied())
        .collect()
}

/// 読めた画素が期待どおりか。透明な画素（アルファ 0）は色が意味を持たないので、アルファだけを比べる。
pub fn same_pixels(read: &[u8], expected: &[u8]) -> bool {
    read.len() == expected.len()
        && read
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .all(|(r, e)| if e[3] == 0 { r[3] == 0 } else { r == e })
}
