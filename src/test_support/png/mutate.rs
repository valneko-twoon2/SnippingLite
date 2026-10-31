//! 正しい見本から壊れた入力を作る変異。どの PNG にも使えるバイト単位の変異と、PNG の構造を知った
//! 変異（狙った段の検査を越えるために、必要なら検査値を直す）。入力がどの段まで正しいかを決めるのは
//! `judge` で、ここで狙った段ではない。

use super::*;

/// 2 バイトと 4 バイトの欄に書く端の値。
const EDGES: [u32; 8] = [0, 1, 0x7f, 0x80, 0x7fff, 0xffff, 0x7fff_ffff, 0xffff_ffff];

/// バイト単位の変異を 1 〜 4 個かける。ビット反転、端の値の 1 バイト、切り詰め、ランダムなバイトの
/// 挿入、範囲の削除、範囲の複製、2/4 バイトの欄を端の値に、別の見本からのつなぎ込み。
pub fn mutate_bytes(rng: &mut Rng, bytes: &mut Vec<u8>, donor: &[u8]) {
    for _ in 0..rng.range(1..=4usize) {
        let len = bytes.len();
        let at = rng.below(len + 1);
        match rng.below(8) {
            0 if len > 0 => {
                let (index, bit) = (at.min(len - 1), rng.below(8));
                bytes[index] ^= 1 << bit;
                rng.note(format!("flip bit {bit} of byte {index}"));
            }
            1 if len > 0 => {
                let (index, value) = (at.min(len - 1), *rng.pick(&[0u8, 0xff, 0x7f, 0x80]));
                bytes[index] = value;
                rng.note(format!("set byte {index} to {value:#x}"));
            }
            2 => {
                bytes.truncate(at);
                rng.note(format!("cut at {at}"));
            }
            3 => {
                let count = rng.range(1..=16usize);
                let new = rng.bytes(count);
                bytes.splice(at..at, new);
                rng.note(format!("insert {count} random bytes at {at}"));
            }
            4 => {
                let end = (at + rng.range(1..=32usize)).min(len);
                bytes.drain(at..end);
                rng.note(format!("delete {at}..{end}"));
            }
            5 if len > 0 => {
                let from = rng.below(len);
                let end = (from + rng.range(1..=32usize)).min(len);
                let piece = bytes[from..end].to_vec();
                bytes.splice(at..at, piece);
                rng.note(format!("copy {from}..{end} to {at}"));
            }
            6 => {
                let size = *rng.pick(&[2usize, 4]);
                if at + size <= len {
                    let value = *rng.pick(&EDGES);
                    let field = &value.to_be_bytes()[4 - size..];
                    bytes[at..at + size].copy_from_slice(field);
                    rng.note(format!("write {value:#x} in {size} bytes at {at}"));
                }
            }
            _ if !donor.is_empty() => {
                let from = rng.below(donor.len());
                let end = (from + rng.range(1..=64usize)).min(donor.len());
                let piece = donor[from..end].to_vec();
                let over = (at + piece.len()).min(len);
                bytes.splice(at..over, piece);
                rng.note(format!(
                    "splice {from}..{end} of the donor over {at}..{over}"
                ));
            }
            _ => {}
        }
    }
}

/// 見本を壊す（または、正しいまま組み替える）。狙う段をおよそ同じ割合で選ぶ。ヘッダー、チャンクの枠、
/// IDAT のデータ、正しいままの組み替え、ファイル全体へのバイト単位の変異。
pub fn damage(rng: &mut Rng, sample: &Sample, donor: &[u8]) -> Vec<u8> {
    let mut chunks = parse(&sample.png).expect("the sample is a PNG").chunks;
    let mut bytes = match rng.below(5) {
        0 => header(rng, &mut chunks),
        1 => frame(rng, &mut chunks),
        2 => {
            data(rng, &mut chunks);
            join(&chunks)
        }
        3 => {
            regroup(rng, &mut chunks);
            join(&chunks)
        }
        _ => {
            let mut bytes = sample.png.clone();
            mutate_bytes(rng, &mut bytes, donor);
            return bytes;
        }
    };
    // ときどき、構造を知った変異にバイト単位の変異を重ねる。
    if rng.chance(0.1) {
        mutate_bytes(rng, &mut bytes, donor);
    }
    bytes
}

/// IHDR を壊す。値を変えて検査値を直す（巨大な幅と高さ、仕様の外の値）か、署名と IHDR のバイトを壊す。
fn header(rng: &mut Rng, chunks: &mut [Chunk]) -> Vec<u8> {
    let mut header = Header::read(&chunks[0].data).expect("the sample has a header");
    match rng.below(8) {
        0..=3 => {
            let old = header.width.max(header.height);
            let size = *rng.pick(&[
                0,
                1,
                old + 1,
                old * 2,
                20_000,
                65_535,
                0x7fff_ffff,
                0x8000_0000,
                0xffff_ffff,
            ]);
            match rng.below(3) {
                0 => header.width = size,
                1 => header.height = size,
                _ => {
                    // 両方を大きく。GDI+ が自分で断る 2 GB 未満の、本当にメモリーを取る宣言。
                    let side = *rng.pick(&[5_000, 12_000, 20_000, 23_000]);
                    (header.width, header.height) = (side, side);
                }
            }
            rng.note(format!("IHDR {}x{}", header.width, header.height));
        }
        4 | 5 => {
            let value = rng.next_u64() as u8;
            match rng.below(5) {
                0 => header.depth = *rng.pick(&[value, 1, 2, 4, 16]),
                1 => header.color = *rng.pick(&[value, 0, 2, 3, 4, 5]),
                2 => header.compression = value,
                3 => header.filter = value,
                _ => header.interlace = *rng.pick(&[value, 1, 2]),
            }
            rng.note(format!("IHDR {header:?}"));
        }
        6 => {
            let mut bytes = join(chunks);
            let index = rng.below(33);
            bytes[index] ^= 1 << rng.below(8);
            rng.note(format!("flip a bit of header byte {index}"));
            return bytes;
        }
        _ => {
            chunks.swap(0, 1);
            rng.note("IHDR is not first");
            return join(chunks);
        }
    }
    chunks[0].data = header.bytes();
    chunks[0].seal();
    join(chunks)
}

/// IHDR の後ろのチャンクの枠を壊す。長さの欄、検査値、チャンクの欠落と重複と入れ替え、途中での
/// 切り詰め、IEND の後ろのごみ、知らない必須チャンク。
fn frame(rng: &mut Rng, chunks: &mut Vec<Chunk>) -> Vec<u8> {
    let target = rng.range(1..chunks.len());
    match rng.below(8) {
        0 => {
            // 長さの欄はチャンクの先頭 4 バイト。
            let mut bytes = join(&chunks[..target]);
            let start = bytes.len();
            bytes.extend(join(&chunks[target..]).split_off(SIGNATURE.len()));
            let length = chunks[target].data.len() as u32;
            let value = *rng.pick(&[
                0,
                length + 1,
                length.wrapping_sub(1),
                0x7fff_ffff,
                0x8000_0000,
            ]);
            bytes[start..start + 4].copy_from_slice(&value.to_be_bytes());
            rng.note(format!("length of chunk {target} is {value:#x}"));
            bytes
        }
        1 => {
            chunks[target].crc ^= 1 << rng.below(32);
            rng.note(format!("bad CRC on chunk {target}"));
            join(chunks)
        }
        2 => {
            chunks.remove(target);
            rng.note(format!("drop chunk {target}"));
            join(chunks)
        }
        3 => {
            let copy = chunks[rng.below(chunks.len())].clone();
            chunks.insert(target, copy);
            rng.note(format!("duplicate a chunk at {target}"));
            join(chunks)
        }
        4 => {
            let other = rng.below(chunks.len());
            chunks.swap(target, other);
            rng.note(format!("swap chunks {target} and {other}"));
            join(chunks)
        }
        5 => {
            let mut bytes = join(chunks);
            let at = rng.range(33..bytes.len());
            bytes.truncate(at);
            rng.note(format!("cut at {at}"));
            bytes
        }
        6 => {
            let mut bytes = join(chunks);
            let count = rng.range(1..=16usize);
            bytes.extend(rng.bytes(count));
            rng.note(format!("{count} bytes after IEND"));
            bytes
        }
        _ => {
            chunks.insert(target, Chunk::new(b"ZZZZ", rng.bytes(4)));
            rng.note(format!("unknown critical chunk at {target}"));
            join(chunks)
        }
    }
}

/// IDAT のデータを壊し、検査値を直して展開まで届かせる。stored の見本は、展開した走査線を壊して
/// Adler-32 を直すこともある（フィルターの種類、走査線の長さ）。
fn data(rng: &mut Rng, chunks: &mut Vec<Chunk>) {
    let mut stream = idat(chunks);
    let line = scanline(chunks);
    match (unstore(&stream), rng.below(4)) {
        (Some(mut raw), 0) => {
            let row = rng.below(raw.len() / line) * line;
            raw[row] = rng.range(5u8..=255);
            rng.note(format!("filter byte {row} out of range"));
            stream = stored(&raw, 65535);
        }
        (Some(mut raw), 1) => {
            let at = rng.below(raw.len() + 1);
            if rng.chance(0.5) {
                raw.truncate(at);
            } else {
                let count = rng.range(1..=8usize);
                raw.splice(at..at, rng.bytes(count));
            }
            rng.note(format!("scanlines of {} bytes", raw.len()));
            stream = stored(&raw, 65535);
        }
        (_, 2) => {
            let at = rng.below(stream.len() + 1);
            stream.truncate(at);
            rng.note(format!("zlib stream cut at {at}"));
        }
        _ => {
            let index = rng.below(stream.len());
            stream[index] ^= 1 << rng.below(8);
            rng.note(format!("flip a bit of zlib byte {index}"));
        }
    }
    replace_data(chunks, &[stream]);
}

/// 正しいまま組み替える。IDAT を分ける、画素に関わらないチャンクを足す、stored の見本なら走査線の
/// 値を変えて Adler-32 を直す、ブロックの大きさを変える。どれも仕様上は正しい PNG のまま。
fn regroup(rng: &mut Rng, chunks: &mut Vec<Chunk>) {
    let stream = idat(chunks);
    let line = scanline(chunks);
    match (unstore(&stream), rng.below(4)) {
        (Some(mut raw), 0) => {
            let index = rng.below(raw.len());
            raw[index] = rng.next_u64() as u8;
            // フィルターの種類のバイト（各走査線の先頭）に当たったら、仕様の範囲（0 〜 4）に戻す。
            if index.is_multiple_of(line) {
                raw[index] %= 5;
            }
            rng.note(format!("scanline byte {index} changed"));
            replace_data(chunks, &[stored(&raw, rng.range(1..=64usize))]);
        }
        (_, 1) => {
            let cut = rng.below(stream.len() + 1);
            rng.note(format!("IDAT split at {cut}"));
            replace_data(chunks, &[stream[..cut].to_vec(), stream[cut..].to_vec()]);
        }
        _ => {
            let chunk = if rng.chance(0.5) {
                Chunk::new(b"tEXt", b"Comment\0Snipping Lite".to_vec())
            } else {
                let count = rng.range(0..=12usize);
                Chunk::new(b"snIp", rng.bytes(count))
            };
            let at = if rng.chance(0.5) { 1 } else { chunks.len() - 1 };
            rng.note(format!("{} at {at}", String::from_utf8_lossy(&chunk.kind)));
            chunks.insert(at, chunk);
        }
    }
}

/// 見本の 1 本の走査線のバイト数（フィルターの種類の 1 バイトと、RGBA の画素）。
fn scanline(chunks: &[Chunk]) -> usize {
    let header = Header::read(&chunks[0].data).expect("the sample has a header");
    1 + 4 * header.width as usize
}

/// IDAT を、与えたデータの IDAT（検査値は正しい）に置き換える。最初の IDAT の位置に並べる。
fn replace_data(chunks: &mut Vec<Chunk>, pieces: &[Vec<u8>]) {
    let first = chunks
        .iter()
        .position(|c| &c.kind == b"IDAT")
        .expect("the sample has image data");
    chunks.retain(|c| &c.kind != b"IDAT");
    for (offset, piece) in pieces.iter().enumerate() {
        chunks.insert(first + offset, Chunk::new(b"IDAT", piece.clone()));
    }
}
