//! 壊れた画像を作るための PNG の部品。チャンクの分解と組み立て、検査値（CRC-32 と Adler-32）、
//! 無圧縮（stored）の deflate で包んだ 8 ビット RGBA の PNG の書き出し。

/// PNG の署名。
pub const SIGNATURE: [u8; 8] = *b"\x89PNG\r\n\x1a\n";
/// 1 つのファイルから取り出すチャンクの上限。長さの欄がすべて 0 のファイルでも止まるように。
const MAX_CHUNKS: usize = 4096;

/// CRC-32（ISO 3309。PNG はチャンクの種類とデータにかける）。
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Adler-32（RFC 1950。zlib ストリームの末尾に、展開した後のデータについて書く）。
pub fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in bytes {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// チャンク 1 つ。`crc` はファイルに書かれていた値で、中身に合っているとは限らない。
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub kind: [u8; 4],
    pub data: Vec<u8>,
    pub crc: u32,
}

impl Chunk {
    pub fn new(kind: &[u8; 4], data: Vec<u8>) -> Chunk {
        let mut chunk = Chunk {
            kind: *kind,
            data,
            crc: 0,
        };
        chunk.seal();
        chunk
    }

    /// 検査値を中身に合わせて書き直す。構造を知った変異が、チャンクの検査を越えて先の段へ届くように。
    pub fn seal(&mut self) {
        self.crc = crc32(&[&self.kind[..], &self.data].concat());
    }
}

/// 署名の後ろを分けたチャンク。
pub struct Parsed {
    pub chunks: Vec<Chunk>,
}

/// 署名を確かめ、長さの欄に従ってチャンクを順に取り出す。長さがファイルの外を指したら、そこで止める。
pub fn parse(bytes: &[u8]) -> Option<Parsed> {
    let mut rest = bytes.strip_prefix(&SIGNATURE[..])?;
    let mut chunks = Vec::new();
    while rest.len() >= 12 && chunks.len() < MAX_CHUNKS {
        let length = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
        if length > rest.len() - 12 {
            break;
        }
        chunks.push(Chunk {
            kind: rest[4..8].try_into().unwrap(),
            data: rest[8..8 + length].to_vec(),
            crc: u32::from_be_bytes(rest[8 + length..12 + length].try_into().unwrap()),
        });
        rest = &rest[12 + length..];
    }
    Some(Parsed { chunks })
}

/// 署名とチャンクを並べてファイルにする（検査値は各チャンクが持つ値のまま）。
pub fn join(chunks: &[Chunk]) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    for chunk in chunks {
        out.extend((chunk.data.len() as u32).to_be_bytes());
        out.extend(chunk.kind);
        out.extend(&chunk.data);
        out.extend(chunk.crc.to_be_bytes());
    }
    out
}

/// IHDR の中身（仕様 11.2.2）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub width: u32,
    pub height: u32,
    pub depth: u8,
    pub color: u8,
    pub compression: u8,
    pub filter: u8,
    pub interlace: u8,
}

impl Header {
    pub fn rgba(width: u32, height: u32) -> Header {
        Header {
            width,
            height,
            depth: 8,
            color: 6,
            compression: 0,
            filter: 0,
            interlace: 0,
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut out = self.width.to_be_bytes().to_vec();
        out.extend(self.height.to_be_bytes());
        out.extend([
            self.depth,
            self.color,
            self.compression,
            self.filter,
            self.interlace,
        ]);
        out
    }
}

/// 8 ビット RGBA、インターレースなしの絵。行ごとに選んだフィルターをかけて書く。
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// 上から下、左から右の RGBA。
    pub rgba: Vec<u8>,
    /// 行ごとのフィルターの種類（0 〜 4）。
    pub filters: Vec<u8>,
}

impl Picture {
    /// フィルターをかけた走査線（各行の先頭にフィルターの種類のバイト）。
    pub fn raw(&self) -> Vec<u8> {
        let row = self.width as usize * 4;
        let mut out = Vec::new();
        for (y, &kind) in self.filters.iter().enumerate() {
            let line = &self.rgba[y * row..][..row];
            let above = (y > 0).then(|| &self.rgba[(y - 1) * row..][..row]);
            out.push(kind);
            for x in 0..row {
                let a = if x >= 4 { line[x - 4] } else { 0 };
                let b = above.map_or(0, |above| above[x]);
                let c = if x >= 4 {
                    above.map_or(0, |above| above[x - 4])
                } else {
                    0
                };
                out.push(line[x].wrapping_sub(predict(kind, a, b, c)));
            }
        }
        out
    }

    /// `block` バイトずつの stored ブロックで包んだ PNG。
    pub fn png(&self, block: usize) -> Vec<u8> {
        join(&[
            Chunk::new(b"IHDR", Header::rgba(self.width, self.height).bytes()),
            Chunk::new(b"IDAT", stored(&self.raw(), block)),
            Chunk::new(b"IEND", Vec::new()),
        ])
    }
}

/// フィルターの種類ごとの予測（仕様 9.2）。`a` は左、`b` は上、`c` は左上の、同じ色の値。
fn predict(kind: u8, a: u8, b: u8, c: u8) -> u8 {
    match kind {
        1 => a,
        2 => b,
        3 => ((a as u16 + b as u16) / 2) as u8,
        4 => {
            let p = a as i16 + b as i16 - c as i16;
            let (pa, pb, pc) = (
                (p - a as i16).abs(),
                (p - b as i16).abs(),
                (p - c as i16).abs(),
            );
            if pa <= pb && pa <= pc {
                a
            } else if pb <= pc {
                b
            } else {
                c
            }
        }
        _ => 0,
    }
}

/// 無圧縮の deflate ブロック（RFC 1951 の 3.2.4）を `block` バイトずつ並べた zlib ストリーム。
pub fn stored(raw: &[u8], block: usize) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let pieces: Vec<&[u8]> = if raw.is_empty() {
        vec![&[]]
    } else {
        raw.chunks(block.clamp(1, 65535)).collect()
    };
    for (index, piece) in pieces.iter().enumerate() {
        out.push((index + 1 == pieces.len()) as u8);
        out.extend((piece.len() as u16).to_le_bytes());
        out.extend((!(piece.len() as u16)).to_le_bytes());
        out.extend(*piece);
    }
    out.extend(adler32(raw).to_be_bytes());
    out
}
