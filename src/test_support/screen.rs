use eframe::egui::{self, epaint::Shape, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2};

pub struct Screen {
    pub ctx: egui::Context,
    size: Vec2,
    time: f64,
    events: Vec<Event>,
    shapes: Vec<egui::epaint::ClippedShape>,
}

pub type Show<'a> = &'a mut dyn FnMut(&mut egui::Ui);

impl Screen {
    /// A tall screen shows a whole scrolling page without scrolling.
    pub fn new(width: f32, height: f32) -> Screen {
        let ctx = egui::Context::default();
        assert!(
            crate::settings_ui::install_fonts(&ctx),
            "a system font is needed"
        );
        crate::settings_ui::install_visuals(&ctx, false);
        Screen {
            ctx,
            size: Vec2::new(width, height),
            time: 0.0,
            events: Vec::new(),
            shapes: Vec::new(),
        }
    }

    pub fn frame(&mut self, show: Show) {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            ..Default::default()
        };
        input.events = std::mem::take(&mut self.events);
        self.time += 0.1;
        let mut output = self.ctx.run_ui(input, |ui| show(ui));
        // Nothing uploads the textures, so there is nothing to apply.
        output.textures_delta.clear();
        self.shapes = output.shapes;
    }

    /// Runs frames until the layout and animations settle.
    pub fn settle(&mut self, show: Show) {
        for _ in 0..4 {
            self.frame(show);
        }
    }

    /// Every piece of text painted in the last frame, with its rectangle.
    pub fn texts(&self) -> Vec<(String, Rect)> {
        fn collect(shape: &Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                Shape::Text(text) => out.push((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &self.shapes {
            collect(&clipped.shape, &mut out);
        }
        out
    }

    /// The rectangles of the pictures painted in the last frame (not text or shapes). 絵は自分の
    /// テクスチャを持つ。既定のテクスチャ（文字の画像の白い点）で塗った面は、スクロールの端の影の
    /// ような図形なので数えない。
    pub fn images(&self) -> Vec<Rect> {
        fn collect(shape: &Shape, out: &mut Vec<Rect>) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                    out.push(mesh.calc_bounds())
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &self.shapes {
            collect(&clipped.shape, &mut out);
        }
        out
    }

    /// The centers of the filled circles of this radius painted in the last frame.
    pub fn circles(&self, radius: f32) -> Vec<Pos2> {
        fn collect(shape: &Shape, radius: f32, out: &mut Vec<Pos2>) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, radius, out)),
                Shape::Circle(circle)
                    if circle.radius == radius && circle.fill != egui::Color32::TRANSPARENT =>
                {
                    out.push(circle.center)
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &self.shapes {
            collect(&clipped.shape, radius, &mut out);
        }
        out
    }
    pub fn find(&self, text: &str) -> Option<Rect> {
        self.texts()
            .into_iter()
            .find(|(t, _)| t == text)
            .map(|(_, r)| r)
    }

    pub fn has(&self, text: &str) -> bool {
        self.find(text).is_some()
    }

    pub fn click(&mut self, at: Pos2, show: Show) {
        self.click_with(at, PointerButton::Primary, show);
    }

    pub fn click_with(&mut self, at: Pos2, button: PointerButton, show: Show) {
        self.events.push(Event::PointerMoved(at));
        self.frame(show);
        self.events.push(Event::PointerButton {
            pos: at,
            button,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        self.frame(show);
        self.events.push(Event::PointerButton {
            pos: at,
            button,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        self.frame(show);
        self.settle(show);
    }

    pub fn click_text(&mut self, text: &str, show: Show) {
        let at = self
            .find(text)
            .unwrap_or_else(|| panic!("no text {text:?} on screen: {:?}", self.texts()))
            .center();
        self.click(at, show);
    }

    pub fn drag(&mut self, from: Pos2, to: Pos2, show: Show) {
        self.events.push(Event::PointerMoved(from));
        self.frame(show);
        self.events.push(Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        self.frame(show);
        for step in 1..=4 {
            let at = from + (to - from) * (step as f32 / 4.0);
            self.events.push(Event::PointerMoved(at));
            self.frame(show);
        }
        self.events.push(Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        self.frame(show);
        self.settle(show);
    }

    pub fn hover(&mut self, at: Pos2, show: Show) {
        self.events.push(Event::PointerMoved(at));
        self.frame(show);
        self.settle(show);
    }

    pub fn key(&mut self, key: Key, modifiers: Modifiers, show: Show) {
        for pressed in [true, false] {
            self.events.push(Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            });
            self.frame(show);
        }
        self.settle(show);
    }

    pub fn type_text(&mut self, text: &str, show: Show) {
        self.events.push(Event::Text(text.to_owned()));
        self.frame(show);
        self.settle(show);
    }
}
