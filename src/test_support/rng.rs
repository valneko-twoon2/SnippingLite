//! シード固定の乱数と、ランダムテスト（性質ベース、モデルベース、モンキーテスト）のケースを回す仕組み。
//!
//! 既定のシードとケース数は固定なので、通常の `cargo test` は毎回同じ入力を試す。長く探すときは
//! 環境変数で変える。`SNIPPING_LITE_SEED` は実行全体のシード（10 進、または `0x` で始まる 16 進）、
//! `SNIPPING_LITE_CASES` はテストごとのケース数（`MAX_CASES` まで）。ケースが失敗すると、
//! そのケースだけを再現するシードと、テストが `Rng::note` で残した手順を表示する。
//!
//! 生成器は SplitMix64。依存を足さずに、近いシード（1 と 2 など）からも互いに関係のない列を作る。

use std::{
    collections::VecDeque,
    ops::{Bound, RangeBounds},
    time::{Duration, Instant},
};

/// 環境変数がないときのシード。値に意味はなく、変えると試す入力が変わる。
pub const DEFAULT_SEED: u64 = 0x5EED_2026_1101;
/// `SNIPPING_LITE_CASES` の上限。桁を打ち間違えても、実行が終わらなくならないようにする。
pub const MAX_CASES: usize = 1_000_000;
/// 1 ケースにかけてよい時間。小さな入力でこれより遅いのは、それ自体が発見なので失敗にする。
pub const CASE_LIMIT: Duration = Duration::from_secs(10);
/// 失敗したときに表示する手順の数。長い列では最後のものだけを残す。
const NOTES: usize = 200;
/// SplitMix64 の増分（2^64 を黄金比で割った値）。
const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// シード付きの乱数。同じシードからは、いつも同じ列が出る。
pub struct Rng {
    state: u64,
    notes: VecDeque<String>,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng {
            state: seed,
            notes: VecDeque::new(),
        }
    }

    /// 次の 64 ビット（SplitMix64）。
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `0..n` の整数。`n` が 0 なら panic する（選ぶものがない）。
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0) has nothing to choose from");
        self.below_u64(n as u64) as usize
    }

    /// 掛けて上位を取る方法なので、棄却のループがない（偏りは n / 2^64 以下）。
    fn below_u64(&mut self, n: u64) -> u64 {
        ((self.next_u64() as u128 * n as u128) >> 64) as u64
    }

    /// 範囲の中の整数。`a..b`、`a..=b`、`..b` などを受け取る。空の範囲なら panic する。
    pub fn range<T: Int>(&mut self, range: impl RangeBounds<T>) -> T {
        let low = match range.start_bound() {
            Bound::Included(&v) => v.wide(),
            Bound::Excluded(&v) => v.wide() + 1,
            Bound::Unbounded => T::MIN.wide(),
        };
        let high = match range.end_bound() {
            Bound::Included(&v) => v.wide(),
            Bound::Excluded(&v) => v.wide() - 1,
            Bound::Unbounded => T::MAX.wide(),
        };
        assert!(low <= high, "the range {low}..={high} is empty");
        let span = (high - low) as u128 + 1;
        let offset = if span > u64::MAX as u128 {
            self.next_u64() as u128
        } else {
            self.below_u64(span as u64) as u128
        };
        T::narrow(low + offset as i128)
    }

    /// `[0, 1)` の実数。
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// `[low, high)` の実数。
    pub fn float(&mut self, low: f32, high: f32) -> f32 {
        (low as f64 + self.unit() * (high as f64 - low as f64)) as f32
    }

    /// 確率 `probability` で true。
    pub fn chance(&mut self, probability: f64) -> bool {
        self.unit() < probability
    }

    /// 1 つを等しい確率で選ぶ。空なら panic する。
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// 重みに比例した確率で、添字を選ぶ。重みの合計が 0 なら panic する。
    pub fn weighted(&mut self, weights: &[u32]) -> usize {
        let total: u64 = weights.iter().map(|&w| w as u64).sum();
        assert!(total > 0, "the weights add up to nothing");
        let mut left = self.below_u64(total);
        for (index, &weight) in weights.iter().enumerate() {
            if left < weight as u64 {
                return index;
            }
            left -= weight as u64;
        }
        unreachable!("the draw is below the total of the weights")
    }

    pub fn bytes(&mut self, length: usize) -> Vec<u8> {
        (0..length).map(|_| self.next_u64() as u8).collect()
    }

    /// 並びを無作為に入れ替える（Fisher–Yates）。
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for last in (1..items.len()).rev() {
            items.swap(last, self.below(last + 1));
        }
    }

    /// ケースが失敗したときに表示する手順を残す。最後の `NOTES` 件だけを持つ。
    pub fn note(&mut self, step: impl Into<String>) {
        if self.notes.len() == NOTES {
            self.notes.pop_front();
        }
        self.notes.push_back(step.into());
    }
}

/// `Rng::range` が受け取る整数。
pub trait Int: Copy {
    const MIN: Self;
    const MAX: Self;
    fn wide(self) -> i128;
    fn narrow(value: i128) -> Self;
}

macro_rules! int {
    ($($t:ty),*) => {$(
        impl Int for $t {
            const MIN: Self = <$t>::MIN;
            const MAX: Self = <$t>::MAX;
            fn wide(self) -> i128 {
                self as i128
            }
            fn narrow(value: i128) -> Self {
                value as $t
            }
        }
    )*};
}
int!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

/// 実行全体のシード: `SNIPPING_LITE_SEED`、なければ `DEFAULT_SEED`。
pub fn seed() -> u64 {
    match variable("SNIPPING_LITE_SEED") {
        None => DEFAULT_SEED,
        Some(text) => parse_seed(&text).unwrap_or_else(|| {
            panic!("SNIPPING_LITE_SEED={text} is not a decimal or 0x-prefixed hexadecimal number")
        }),
    }
}

/// テストのケース数: `SNIPPING_LITE_CASES`（`MAX_CASES` まで）、なければ `default`。
pub fn cases(default: usize) -> usize {
    match variable("SNIPPING_LITE_CASES") {
        None => default,
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|&count| count > 0)
            .unwrap_or_else(|| panic!("SNIPPING_LITE_CASES={text} is not a positive number"))
            .min(MAX_CASES),
    }
}

fn variable(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_seed(text: &str) -> Option<u64> {
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(&hex.replace('_', ""), 16).ok(),
        None => text.replace('_', "").parse().ok(),
    }
}

/// `name` の性質を、既定では `default` ケース確かめる。ケースごとに新しい乱数を渡し、
/// 回したケースの数を返す。ケース `k` のシードは実行全体のシード + `k` なので、表示された
/// シードとケース数 1 で、そのケースだけを再現できる。
pub fn check(name: &str, default: usize, case: impl FnMut(&mut Rng)) -> usize {
    run(name, seed(), cases(default), case)
}

fn run(name: &str, base: u64, count: usize, mut case: impl FnMut(&mut Rng)) -> usize {
    for index in 0..count {
        let seed = base.wrapping_add(index as u64);
        let mut failure = Failure {
            name,
            index,
            seed,
            rng: Rng::new(seed),
        };
        let started = Instant::now();
        case(&mut failure.rng);
        let spent = started.elapsed();
        assert!(spent <= CASE_LIMIT, "one case took {spent:?}");
    }
    count
}

/// ケースの途中で panic したら、再現の方法と残した手順を表示する。
struct Failure<'a> {
    name: &'a str,
    index: usize,
    seed: u64,
    rng: Rng,
}

impl Drop for Failure<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        let Failure {
            name, index, seed, ..
        } = *self;
        eprintln!("{name}: case {index} failed with seed {seed:#x}. To run only this case:");
        eprintln!(
            "  $env:SNIPPING_LITE_SEED='{seed:#x}'; $env:SNIPPING_LITE_CASES='1'; \
             cargo test --config src/.cargo/config.toml --manifest-path src/Cargo.toml {name}"
        );
        if !self.rng.notes.is_empty() {
            eprintln!("  the last {} steps:", self.rng.notes.len());
            for step in &self.rng.notes {
                eprintln!("    {step}");
            }
        }
    }
}

/// 入力がどの分岐まで届いたかの集計。生成器が偏って、ある分岐をほとんど試していないことに
/// 気づくためのもの。
pub struct Reach {
    counts: Vec<(&'static str, usize)>,
}

impl Reach {
    pub fn new(labels: &[&'static str]) -> Reach {
        Reach {
            counts: labels.iter().map(|&label| (label, 0)).collect(),
        }
    }

    /// 知らない名前は打ち間違いなので panic する。
    pub fn hit(&mut self, label: &str) {
        match self.counts.iter_mut().find(|(known, _)| *known == label) {
            Some((_, count)) => *count += 1,
            None => panic!("unknown branch {label}"),
        }
    }

    /// 集計を表示する（`--nocapture` で見える）。50 ケース以上なら、どの分岐にもケース数の
    /// 2% 以上が届いていることを確かめる。少ないケースで 1 つを再現しているときは確かめない。
    pub fn finish(&self, name: &str, cases: usize) {
        let summary: Vec<String> = self
            .counts
            .iter()
            .map(|(label, count)| format!("{label}={count}"))
            .collect();
        eprintln!("{name}: {cases} cases reached {}", summary.join(", "));
        if cases >= 50 {
            for &(label, count) in &self.counts {
                assert!(
                    count >= cases / 50,
                    "{name}: only {count} of {cases} cases reached {label}; the generator is skewed"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generator_is_splitmix64_so_printed_seeds_stay_valid() {
        // SplitMix64 の公開された参照値（シード 1234567）。生成器が変わると、報告に残ったシードで
        // 同じケースを再現できなくなる。
        let mut rng = Rng::new(1234567);
        let first: Vec<u64> = (0..5).map(|_| rng.next_u64()).collect();
        assert_eq!(
            first,
            [
                6457827717110365317,
                3203168211198807973,
                9817491932198370423,
                4593380528125082431,
                16408922859458223821
            ]
        );
        // 隣り合うシードは別の列になる（`seed | 1` のような作り方では 2 と 3 が同じになる）。
        assert_ne!(Rng::new(2).next_u64(), Rng::new(3).next_u64());
    }

    #[test]
    fn each_case_gets_the_seed_it_would_print() {
        let mut firsts = Vec::new();
        let count = run("each_case", 40, 5, |rng| firsts.push(rng.next_u64()));
        assert_eq!(count, 5);
        let expected: Vec<u64> = (40..45).map(|seed| Rng::new(seed).next_u64()).collect();
        assert_eq!(firsts, expected);
        // 環境変数のシードは 10 進と 16 進で書け、区切りの _ を許す。
        assert_eq!(parse_seed("31"), Some(31));
        assert_eq!(parse_seed("0x1f"), Some(31));
        assert_eq!(parse_seed("0X5EED_2026"), Some(0x5EED_2026));
        assert_eq!(parse_seed("seed"), None);
        assert_eq!(parse_seed("-1"), None);
    }

    #[test]
    fn draws_stay_inside_their_range_and_reach_both_ends() {
        let mut rng = Rng::new(7);
        let (mut low, mut high) = (false, false);
        for _ in 0..2000 {
            let value = rng.range(-3i32..=3);
            assert!((-3..=3).contains(&value), "{value}");
            low |= value == -3;
            high |= value == 3;
            assert!(rng.range(5u8..6) == 5, "a range of one value");
            assert!(rng.below(4) < 4);
            let x = rng.float(-1.5, 2.5);
            assert!((-1.5..2.5).contains(&x), "{x}");
            // 重み 0 の添字は選ばれない。
            assert_ne!(rng.weighted(&[1, 0, 3]), 1);
        }
        assert!(low && high, "both ends are drawn");
        // 型の全体の範囲でも、端の計算があふれない。
        let _ = rng.range(i64::MIN..=i64::MAX);
        let _ = rng.range(..=u64::MAX);
        let mut items = [1, 2, 3, 4, 5];
        rng.shuffle(&mut items);
        items.sort();
        assert_eq!(items, [1, 2, 3, 4, 5], "shuffling keeps every item");
    }
}
