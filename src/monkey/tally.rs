//! 到達の集計。状態は全体の手数に対して数える。操作の種類は、その操作を受ける面（トレイ、
//! オーバーレイ、ツールバー、書き込みのバー、通知、カウントダウン）が出ていた手数に対して数える。
//! 面ごとに分けるのは、バーや通知のように短い間しか出ない面の操作を、出ていた間に偏りなく
//! 選んだかを見るため（全体の手数に対して数えると、面が出ていた時間の揺れで決まってしまう）。

use super::*;

/// 面の名前と、その面の操作の名前の頭。
const SURFACES: [(&str, &[&str]); 6] = [
    ("anytime", &["tray:", "system", "wait"]),
    ("overlay", &["overlay:", "key:"]),
    ("toolbar", &["toolbar:"]),
    ("markup bar", &["bar:"]),
    ("notification", &["toast:"]),
    ("countdown", &["countdown:"]),
];

pub(super) struct Tally {
    states: Reach,
    /// 面ごとの集計と、その面が出ていた手数。
    surfaces: Vec<(&'static str, Reach, usize)>,
}

fn surface(label: &str) -> usize {
    SURFACES
        .iter()
        .position(|(_, heads)| heads.iter().any(|head| label.starts_with(head)))
        .unwrap_or_else(|| panic!("{label} belongs to no surface"))
}

impl Tally {
    pub fn new() -> Tally {
        let surfaces = SURFACES
            .iter()
            .enumerate()
            .map(|(index, &(name, _))| {
                let labels: Vec<&'static str> = actions::KINDS
                    .iter()
                    .copied()
                    .filter(|label| surface(label) == index)
                    .collect();
                (name, Reach::new(&labels), 0)
            })
            .collect();
        Tally {
            states: Reach::new(&actions::STATES),
            surfaces,
        }
    }

    /// 手を選ぶ前の様子と、選んだ操作を数える。
    pub fn step(&mut self, now: &Snapshot, action: &Action) {
        now.reach(&mut self.states);
        let free = !now.held;
        let shown = [
            true,
            now.overlay,
            now.toolbar.is_some() && free,
            now.bar && free,
            now.toast.is_some(),
            now.count.is_some(),
        ];
        for ((_, _, count), shown) in self.surfaces.iter_mut().zip(shown) {
            *count += shown as usize;
        }
        let label = action.label();
        self.surfaces[surface(label)].1.hit(label);
    }

    /// 状態はどれも手数の 2% 以上、操作はどれも、その面が出ていた手数の 2% 以上（少なくとも 1 回）。
    pub fn finish(&self, steps: usize) {
        self.states.finish(NAME, steps);
        for (name, reach, count) in &self.surfaces {
            reach.finish(
                &format!("{NAME} ({name}, {count} steps shown)"),
                (*count).max(50),
            );
        }
    }
}
