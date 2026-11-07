//! 設定のスイッチとボタンの、キーボードとポインターへの応え方。docs/behavior.md によると、設定ウィンドウは
//! Snipping Tool の設定（トグル、カード）をまね、操作を支援技術にも公開する。キーボードだけでも
//! 使えることと、押せるボタンがポインターに応えることを、利用者に見える形（保存された設定、開いた
//! フォルダー、フォーカスの枠、ボタンの塗りの明るさ）で確かめる。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | Tab | 状態 | フォーカスがない → 最初の Tab でページの最初の操作（スクリーンショットのコピーのスイッチ）に枠が付く |
//! | Tab の順 | 性質 | 枠はページを上から下へ進み、下のスイッチにも届く |
//! | Space | 同値分割 | フォーカスのあるスイッチを切り替えて保存する（オンからオフ、オフからオン） |
//! | Enter | 同値分割 | フォーカスのあるボタン（フォルダーを開く）を押す |
//! | ボタンの塗り | 決定表（テーマ × 状態） | ライト: 休み > ホバー > 押下 の明るさ。ダーク: ホバー > 休み > 押下（Windows 11 と同じく、ダークではホバーで明るく、押すと暗く） |

use super::*;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};

impl Window {
    fn press(&mut self, key: Key) {
        self.act(|screen, show| screen.key(key, Modifiers::NONE, show));
    }

    /// フォーカスの枠: 塗りのない、外側へ引いた幅 2 の線の四角形（スイッチとボタンが描く）。
    fn ring(&self) -> Option<Rect> {
        self.screen
            .rects()
            .into_iter()
            .find(|r| {
                r.fill == Color32::TRANSPARENT
                    && r.stroke.width == 2.0
                    && r.stroke_kind == StrokeKind::Outside
            })
            .map(|r| r.rect)
    }

    /// 枠が `target` を囲むまで Tab を押す（長くても `limit` 回）。囲んだら true。
    fn tab_to(&mut self, target: Pos2, limit: usize) -> bool {
        (0..limit).any(|_| {
            self.press(Key::Tab);
            self.ring().is_some_and(|r| r.contains(target))
        })
    }

    /// `label` の上にある、高さ 32 のボタンの塗り。
    fn button_fill(&self, label: Pos2) -> Color32 {
        self.screen
            .rects()
            .into_iter()
            .find(|r| {
                r.rect.height() == 32.0 && r.rect.contains(label) && r.fill != Color32::TRANSPARENT
            })
            .map(|r| r.fill)
            .expect("the button is painted")
    }

    /// `at` で左ボタンを押し、押したままのフレームを 1 つ描く。
    fn hold(&mut self, at: Pos2) {
        let press = Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        };
        self.act(|screen, show| screen.send([press], show));
    }

    /// 押したままポインターを `away` へ動かしてから離す。ボタンの外で離すので、クリックにならない。
    fn let_go_at(&mut self, away: Pos2) {
        let release = Event::PointerButton {
            pos: away,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        };
        self.act(|screen, show| screen.send([Event::PointerMoved(away), release], show));
        self.act(|screen, show| screen.settle(show));
    }
}

/// 人の目に見える明るさ（sRGB の値に、輝度の重みを掛けた和）。
fn brightness(color: Color32) -> f32 {
    0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32
}

#[test]
fn the_switches_and_buttons_can_be_used_from_the_keyboard() {
    let sandbox = Sandbox::new();
    let folder = folder_in(&sandbox, "keyboard");
    config::save_settings(&config::Settings {
        save: true,
        screenshot_folder: folder.clone(),
        ..config::parse_settings("")
    })
    .unwrap();
    let mut window = Window::open(true, false);
    assert_eq!(window.ring(), None, "nothing has the focus yet");
    // 最初の Tab は、ページの最初の操作（スクリーンショットのコピーのスイッチ）へ。
    window.press(Key::Tab);
    let copy = window.switch_point(tr("Automatically copy screenshots"), 0);
    let first = window.ring().expect("the focused switch is ringed");
    assert!(first.contains(copy), "{first:?} around {copy:?}");
    // Space はフォーカスのあるスイッチを切り替える。保存がオンなので、コピーはオフにできる。
    window.press(Key::Space);
    assert!(!saved().copy && saved().save, "Space switches copying off");
    window.press(Key::Space);
    assert!(saved().copy, "and on again");
    // 枠はページを上から下へ進み、通知のスイッチにも届く。
    let notify = window.switch_point(tr("Show notifications"), 0);
    let mut top = first.top();
    let reached = (0..20).any(|_| {
        window.press(Key::Tab);
        window.ring().is_some_and(|ring| {
            assert!(ring.top() >= top, "the focus went back up to {ring:?}");
            top = ring.top();
            ring.contains(notify)
        })
    });
    assert!(reached, "Tab reaches the notification switch");
    window.press(Key::Space);
    assert!(!saved().notify, "Space switches notifications off");
    assert!(saved().copy && saved().save, "and nothing else");
    // ボタンも Enter で押せる。保存先のカードを開き、「フォルダーを開く」まで進む。
    window.click_text(tr("Automatically save screenshots"));
    let open = window
        .screen
        .find(tr("Open folder"))
        .expect("the card is open")
        .center();
    assert!(
        window.tab_to(open, 40),
        "Tab reaches the Open folder button"
    );
    crate::test_support::take_shell_opened();
    window.press(Key::Enter);
    assert_eq!(
        crate::test_support::take_shell_opened(),
        vec![(folder, None)]
    );
}

#[test]
fn a_button_answers_the_pointer_in_both_themes() {
    let _sandbox = Sandbox::new();
    let mut window = Window::open(true, false);
    // ボタンのない、ページの左上の隅。
    let away = pos2(1.0, 1.0);
    for theme in [Theme::Light, Theme::Dark] {
        window.screen.ctx.set_theme(theme);
        let at = window
            .screen
            .find(tr("Send feedback"))
            .expect("the feedback button")
            .center();
        window.act(|screen, show| screen.hover(away, show));
        let rest = window.button_fill(at);
        window.act(|screen, show| screen.hover(at, show));
        let hovered = window.button_fill(at);
        window.hold(at);
        let pressed = window.button_fill(at);
        // 外で離すとクリックにならず、ボタンは休みの塗りに戻る。
        window.let_go_at(away);
        assert_eq!(window.button_fill(at), rest, "{theme:?}: back at rest");
        let (rest, hovered, pressed) = (brightness(rest), brightness(hovered), brightness(pressed));
        let what = format!("{theme:?}: rest {rest}, hovered {hovered}, pressed {pressed}");
        match theme {
            Theme::Light => assert!(rest > hovered && hovered > pressed, "{what}"),
            Theme::Dark => assert!(hovered > rest && rest > pressed, "{what}"),
        }
    }
}
