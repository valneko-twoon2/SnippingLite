use super::*;
use crate::test_support::{
    answer_dialog, clipboard_text,
    screen::{Screen, Show},
    Sandbox, CAPTIONS,
};
use eframe::App;
use std::{fs, path::PathBuf};

const HOUR: u128 = 60 * 60 * 1000;
const DAY: u128 = 24 * HOUR;

/// The history window running on a screen of its default size.
struct Viewer {
    app: HistoryApp,
    screen: Screen,
    frame: eframe::Frame,
}

impl Viewer {
    fn open() -> Viewer {
        Viewer::with(HistoryApp::new(false, false))
    }

    fn with(app: HistoryApp) -> Viewer {
        let mut viewer = Viewer {
            app,
            screen: Screen::new(960.0, 640.0),
            frame: eframe::Frame::_new_kittest(),
        };
        viewer.act(|screen, show| screen.settle(show));
        viewer
    }

    fn act<R>(&mut self, action: impl FnOnce(&mut Screen, Show) -> R) -> R {
        let app = &mut self.app;
        let frame = &mut self.frame;
        let mut show = |ui: &mut egui::Ui| app.ui(ui, frame);
        action(&mut self.screen, &mut show)
    }

    fn settle(&mut self) {
        self.act(|screen, show| screen.settle(show));
    }

    fn click_text(&mut self, text: &str) {
        self.act(|screen, show| screen.click_text(text, show));
    }

    fn click(&mut self, at: egui::Pos2) {
        self.act(|screen, show| screen.click(at, show));
    }

    fn key(&mut self, key: Key) {
        self.act(|screen, show| screen.key(key, egui::Modifiers::NONE, show));
    }

    /// The picture of the opened screenshot, as painted.
    fn canvas(&self) -> Rect {
        *self.screen.images().last().expect("a picture is shown")
    }

    fn opened_id(&self) -> Option<u128> {
        self.app.opened.as_ref().map(|o| o.entry.id)
    }
}

/// A stored screenshot: red on the left half and cyan on the right half.
fn seed(id: u128, width: i32, height: i32) -> Entry {
    let folder = history::folder().unwrap();
    fs::create_dir_all(&folder).unwrap();
    let entry = Entry {
        id,
        image: folder.join(format!("{id}.png")),
        thumbnail: folder.join(format!("{id}.thumb.png")),
    };
    let mut pixels = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            pixels.extend(if x < width / 2 {
                [0, 0, 255, 255] // red, as BGRA
            } else {
                [255, 255, 0, 255] // cyan
            });
        }
    }
    imaging::encode(&entry.image, &pixels, width, height).unwrap();
    imaging::encode(&entry.thumbnail, &pixels, width, height).unwrap();
    entry
}

fn now() -> u128 {
    imaging::timestamp().unwrap()
}

/// この機械の時間帯で、2026 年 6 月 15 日の `hour`:`minute`（Unix 紀元からのミリ秒）。6 月の半ばは
/// どの時間帯でも夏時間の切り替えから離れているので、前の日の同じ時刻はちょうど 24 時間前にある。
fn june_15(hour: u16, minute: u16) -> u128 {
    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::Time::{SystemTimeToFileTime, TzSpecificLocalTimeToSystemTime},
    };
    let local = SYSTEMTIME {
        wYear: 2026,
        wMonth: 6,
        wDayOfWeek: 0,
        wDay: 15,
        wHour: hour,
        wMinute: minute,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: all structures are valid, initialized locals.
    unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        assert_ne!(
            TzSpecificLocalTimeToSystemTime(std::ptr::null(), &local, &mut utc),
            0
        );
        let mut file: FILETIME = std::mem::zeroed();
        assert_ne!(SystemTimeToFileTime(&utc, &mut file), 0);
        let ticks = ((file.dwHighDateTime as u64) << 32) | file.dwLowDateTime as u64;
        // FILETIME は 1601 年からの 100 ns 単位で、Unix 紀元はその 11644473600 秒後。
        (ticks / 10_000 - 11_644_473_600_000) as u128
    }
}

mod browse;
mod damaged;
mod edit;
mod gallery;
mod window;
