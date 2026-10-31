//! 表示用のテクスチャを、GPU のテクスチャの辺の上限に収まるよう縮める。縮めるのは画面に出す絵だけで、
//! 開いた画像の画素（書き込み、切り抜き、コピー、保存に使う）は元の解像度のまま持つ。
//!
//! 縮め方は面積平均（ボックスフィルター）。縮小先の 1 画素に入る元の画素をすべて平均する。
//! - 品質: 上限を超える画像は 2 倍から数十倍に縮むことがあり、最近傍や双線形の補間では元の画素の
//!   大半を読み飛ばすため、細い線や文字が消えたりちらついたりする。面積平均はすべての画素を使う。
//! - 速さ: 元の画素を 1 回ずつ足すだけで、計算量は元の画素数に比例する。依存も増やさない。
//! - 透明: 色は不透明度で重み付けして平均する。フリーフォームの外側のような透明な画素の色
//!   （中身は決まっていない）が、縁の色に混ざらないようにするため。
//!
//! 元の画素 x は縮小先の floor(x * 縮小後 / 元) に入る。縮小後は元以下なので、どの縮小先の画素にも
//! 1 つ以上の元の画素が入る。

/// `width` x `height` を、縦横比を保ってどちらの辺も `side` 以下にした大きさ。収まるならそのまま。
/// 縮めた辺は四捨五入し、1 画素より小さくしない。
pub(super) fn fit(width: i32, height: i32, side: usize) -> (i32, i32) {
    let longest = width.max(height) as u64;
    let side = side.max(1) as u64;
    if longest <= side {
        return (width, height);
    }
    let scale = |length: i32| (((length as u64) * side + longest / 2) / longest).max(1) as i32;
    (scale(width), scale(height))
}

/// BGRA の画素を `to_width` x `to_height` に面積平均で縮める（どちらも元の辺以下であること）。
pub(super) fn shrink(
    bgra: &[u8],
    width: i32,
    height: i32,
    to_width: i32,
    to_height: i32,
) -> Vec<u8> {
    let (width, height) = (width as usize, height as usize);
    let (to_width, to_height) = (to_width as usize, to_height as usize);
    // 元の列が入る縮小先の列と、縮小先の列ごとの元の列の数。
    let column: Vec<usize> = (0..width).map(|x| x * to_width / width).collect();
    let mut columns = vec![0u64; to_width];
    for &c in &column {
        columns[c] += 1;
    }
    let mut out = Vec::with_capacity(to_width * to_height * 4);
    // 縮小先の 1 行分の合計（不透明度で重み付けした B、G、R と、不透明度）。
    let mut sums = vec![[0u64; 4]; to_width];
    let mut rows = 0u64;
    for y in 0..height {
        let row = &bgra[y * width * 4..(y + 1) * width * 4];
        for (pixel, &c) in row.as_chunks::<4>().0.iter().zip(&column) {
            let alpha = pixel[3] as u64;
            let sum = &mut sums[c];
            sum[0] += pixel[0] as u64 * alpha;
            sum[1] += pixel[1] as u64 * alpha;
            sum[2] += pixel[2] as u64 * alpha;
            sum[3] += alpha;
        }
        rows += 1;
        // 次の行が別の縮小先の行に入るなら（または最後の行なら）、ためた行の平均を書き出す。
        let target = y * to_height / height;
        if y + 1 == height || (y + 1) * to_height / height != target {
            for (sum, &count) in sums.iter_mut().zip(&columns) {
                let count = count * rows;
                let weight = sum[3];
                // すべて透明なら（重みが 0 なら）色は 0 にする。
                let color =
                    |total: u64| (total + weight / 2).checked_div(weight).unwrap_or(0) as u8;
                out.extend([
                    color(sum[0]),
                    color(sum[1]),
                    color(sum[2]),
                    ((weight + count / 2) / count) as u8,
                ]);
                *sum = [0; 4];
            }
            rows = 0;
        }
    }
    out
}

#[cfg(test)]
mod tests;
