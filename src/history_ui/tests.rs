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

mod browse;
mod edit;
mod gallery;
mod window;
