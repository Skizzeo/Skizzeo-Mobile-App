//! 12-events und Oberfläche: Ereignisverteilung wie im DOM des Prototyps und alle
//! Bedienelemente mit den CSS-Werten aus V039.

use std::cell::RefCell;
use std::sync::Arc;

use egui::epaint::{RectShape, Shadow};
use egui::text::{LayoutJob, TextFormat};
use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Galley, Id, Key, LayerId, Order, Painter,
    Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};
use serde_json::Value as Json;

use crate::controller::{Focus, PointerEvent, Viewport};
use crate::fonts::font;
use crate::model::{Drag, Rect as BoxRect};
use crate::platform::{Insets, Platform, RawInput, RawPointer};
use crate::style::{
    self, defaults_json, hex_color, number_text, parse_number_input, Appearance, Kind, Partial, Tab,
    FIELDS,
};

const VERSION: &str = "V039";
const DRAWING_HINT: &str = "Zurückfahren gesperrt. Richtung wechseln oder „Schritt zurück“ wählen.";
const DIRECTION_HINT: &str = "Polygon im Uhrzeigersinn eingeben. Das erste Segment ist mit 10 m vorgegeben; seine Länge änderst du durch Antippen des Maßes.";
const TABS: [(Tab, &str); 4] = [
    (Tab::Dimensions, "Bemaßung"),
    (Tab::View, "Ansicht"),
    (Tab::Input, "Eingabe"),
    (Tab::Program, "Programmeinstellung"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Canvas,
    Dimension(usize),
    Overlay,
}

/// Zustand der DOM-Bedienelemente.
struct Overlay {
    targets: Vec<(i64, Target)>,
    hover_canvas: bool,
    start_hover: bool,
    settings_open: bool,
    tab: Tab,
    inputs: Vec<String>,
    focus_values: Vec<Option<String>>,
    invalid: Vec<bool>,
    status: String,
    saved: Appearance,
    open_select: Option<(usize, Rect)>,
    open_color: Option<(usize, Color32)>,
    color_anchor: Rect,
    started: bool,
    focus_length: bool,
    focus_choice: bool,
    undo_rect: Option<Rect>,
    length_focused: bool,
}

pub struct App {
    vp: Viewport,
    platform: Platform,
    ov: Overlay,
    /// Trefferflächen der Bedienelemente (aktueller und letzter Frame).
    hits: RefCell<Vec<Rect>>,
    hits_last: Vec<Rect>,
}

fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

fn hex_alpha(hex: &str, a: u8) -> Color32 {
    alpha(hex_color(hex), a)
}

fn faded(c: Color32, opacity: f32) -> Color32 {
    c.gamma_multiply(opacity)
}

fn radius(r: f64) -> CornerRadius {
    CornerRadius::same(r.round().clamp(0.0, 255.0) as u8)
}

fn css_shadow(y: i8, blur: u8, spread: u8, color: Color32) -> Shadow {
    Shadow {
        offset: [0, y],
        blur,
        spread,
        color,
    }
}

/// Kasten mit Hintergrund, innenliegendem Rahmen und optionalem Schatten.
fn paint_box(
    painter: &Painter,
    rect: Rect,
    corner: CornerRadius,
    fill: Color32,
    border: Option<(f32, Color32)>,
    shadows: &[Shadow],
) {
    for shadow in shadows {
        painter.add(shadow.as_shape(rect, corner));
    }
    let stroke = border.map_or(Stroke::NONE, |(w, c)| Stroke::new(w, c));
    painter.add(RectShape::new(rect, corner, fill, stroke, StrokeKind::Inside));
}

/// Ring wie `box-shadow: 0 0 0 <spread>` um einen Kreis.
fn annulus(painter: &Painter, center: Pos2, inner: f32, outer: f32, color: Color32) {
    let mut mesh = egui::epaint::Mesh::default();
    let segments = 96u32;
    for i in 0..=segments {
        let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
        let dir = Vec2::angled(angle);
        mesh.colored_vertex(center + dir * inner, color);
        mesh.colored_vertex(center + dir * outer, color);
    }
    for i in 0..segments {
        let k = 2 * i;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Text mit CSS-Zeilenhöhe, optional umbrechend.
fn galley(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    wrap: Option<f32>,
    line_height: Option<f32>,
) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font,
            color,
            line_height,
            ..TextFormat::default()
        },
    );
    if let Some(width) = wrap {
        job.wrap.max_width = width;
    }
    painter.layout_job(job)
}

/// Normale Zeilenhöhe (`line-height: normal`) der Oberflächenschrift.
fn normal_line(size: f32) -> f32 {
    (size * 1.21).round()
}

fn animated_color(ctx: &egui::Context, id: Id, target: Color32, secs: f32) -> Color32 {
    let [r, g, b, a] = target.to_srgba_unmultiplied();
    let channel = |i: usize, v: u8| {
        ctx.animate_value_with_time(id.with(("farbe", i)), f32::from(v), secs)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_unmultiplied(channel(0, r), channel(1, g), channel(2, b), channel(3, a))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    /// `.tool` bzw. `aria-pressed="true"`
    Active,
    /// `.tool.secondary` bzw. `aria-pressed="false"`
    Plain,
    /// `#confirm-close`
    Confirm,
}

struct ToolSpec<'a> {
    label: &'a str,
    size: f32,
    padding: f32,
    tone: Tone,
    disabled: bool,
}

/// `.tool`-Schaltfläche (Farben, Hover-Übergang, Anheben, Schatten, Deckkraft).
fn tool(ui: &mut egui::Ui, a: &Appearance, hover_capable: bool, id: Id, rect: Rect, spec: ToolSpec) -> egui::Response {
    let ctx = ui.ctx().clone();
    let sense = if spec.disabled { Sense::hover() } else { Sense::click() };
    let response = ui.interact(rect, id, sense);
    let hovered = hover_capable && response.hovered();
    let secs = (a.num("hoverDuration") / 1000.0) as f32;
    let (fill, border, text) = match spec.tone {
        Tone::Active => {
            let hover = hovered && !spec.disabled;
            let fill = a.color(if hover { "buttonActiveHover" } else { "buttonActive" });
            (fill, fill, a.color("buttonActiveText"))
        }
        Tone::Plain => (
            a.color(if hovered { "buttonHover" } else { "buttonBackground" }),
            a.color(if hovered { "buttonHoverBorder" } else { "buttonBorder" }),
            a.color("buttonText"),
        ),
        Tone::Confirm => (
            a.color(if hovered { "editHover" } else { "editFill" }),
            a.color("editText"),
            a.color("editText"),
        ),
    };
    let fill = animated_color(&ctx, id.with("fill"), fill, secs);
    let border = animated_color(&ctx, id.with("border"), border, secs);
    let lift = ctx.animate_value_with_time(
        id.with("lift"),
        if hovered && !spec.disabled { a.num("hoverLift") as f32 } else { 0.0 },
        secs,
    );
    let opacity = if spec.disabled { 0.45 } else { 1.0 };
    let rect = rect.translate(Vec2::new(0.0, -lift));
    let painter = ui.painter();
    paint_box(
        painter,
        rect,
        radius(a.num("buttonRadius")),
        faded(fill, opacity),
        Some((1.0, faded(border, opacity))),
        &[css_shadow(2, 6, 0, faded(Color32::from_black_alpha(0x18), opacity))],
    );
    let mut job = LayoutJob::single_section(
        spec.label.to_owned(),
        TextFormat {
            font_id: font(a.text("uiFont"), 500, spec.size),
            color: faded(text, opacity),
            line_height: Some(spec.size),
            ..TextFormat::default()
        },
    );
    job.wrap.max_width = rect.width() - 2.0 * spec.padding - 2.0;
    job.halign = egui::Align::Center;
    let g = painter.layout_job(job);
    painter.galley(rect.center() - g.rect.center().to_vec2(), g, text);
    response
}

fn tool_width(painter: &Painter, a: &Appearance, label: &str, size: f32, padding: f32, min: f32) -> f32 {
    let width = painter
        .layout_no_wrap(label.to_owned(), font(a.text("uiFont"), 500, size), Color32::WHITE)
        .size()
        .x;
    (width + 2.0 * padding + 2.0).max(min).ceil()
}

/// Positioniertes Element; Aufrufreihenfolge = Stapelreihenfolge (alles in einer Ebene,
/// egui trifft das zuletzt hinzugefügte Element zuerst). Trefferflächen für die Zeichenfläche.
fn layer<R>(
    ui: &mut egui::Ui,
    hits: &RefCell<Vec<Rect>>,
    rect: Rect,
    interactable: bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if interactable {
        hits.borrow_mut().push(rect);
    }
    add(ui)
}

/// Browser-Standardschaltfläche (dunkles Farbschema).
fn plain_button(ui: &mut egui::Ui, a: &Appearance, id: Id, rect: Rect, label: &str) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let fill = if response.is_pointer_button_down_on() {
        Color32::from_rgb(0x52, 0x52, 0x52)
    } else if response.hovered() {
        Color32::from_rgb(0x7b, 0x7b, 0x7b)
    } else {
        Color32::from_rgb(0x6b, 0x6b, 0x6b)
    };
    let painter = ui.painter();
    paint_box(painter, rect, CornerRadius::same(3), fill, None, &[]);
    let g = galley(
        painter,
        label,
        font(a.text("uiFont"), 400, 13.333),
        Color32::WHITE,
        Some(rect.width() - 16.0),
        None,
    );
    painter.galley(Pos2::new(rect.left() + 8.0, rect.center().y - g.size().y / 2.0), g, Color32::WHITE);
    response
}

impl App {
    #[cfg(target_arch = "wasm32")]
    pub fn new(cc: &eframe::CreationContext<'_>, canvas: web_sys::HtmlCanvasElement) -> Self {
        let platform = Platform::new(&cc.egui_ctx, canvas);
        Self::with_platform(cc, platform)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_platform(cc, Platform::new())
    }

    fn with_platform(cc: &eframe::CreationContext<'_>, platform: Platform) -> Self {
        cc.egui_ctx.set_fonts(crate::fonts::definitions());
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let saved = style::load_standard(|key| platform.storage_get(key));
        let mut vp = Viewport::new(saved.clone());
        vp.reduced_motion = platform.reduced_motion();
        vp.coarse_touch_device = platform.coarse_touch_device();
        let ov = Overlay {
            targets: Vec::new(),
            hover_canvas: false,
            start_hover: false,
            settings_open: false,
            tab: Tab::Dimensions,
            inputs: vec![String::new(); FIELDS.len()],
            focus_values: vec![None; FIELDS.len()],
            invalid: vec![false; FIELDS.len()],
            status: String::new(),
            saved,
            open_select: None,
            open_color: None,
            color_anchor: Rect::NOTHING,
            started: false,
            focus_length: false,
            focus_choice: false,
            undo_rect: None,
            length_focused: false,
        };
        let mut app = Self {
            vp,
            platform,
            ov,
            hits: RefCell::default(),
            hits_last: Vec::new(),
        };
        app.sync_style_controls();
        app
    }

    // ------------------------------------------------------------ Ereignisverteilung

    fn overlay_at(&self, _ctx: &egui::Context, x: f64, y: f64) -> bool {
        let p = Pos2::new(x as f32, y as f32);
        self.hits_last.iter().any(|r| r.contains(p))
    }

    /// Oberste Maß-Trefferfläche (spätere Slots liegen im DOM darüber).
    fn dimension_at(&self, x: f64, y: f64) -> Option<usize> {
        self.vp
            .render_state
            .dimension_layouts
            .iter()
            .filter(|l| l.hit_box.contains(x, y))
            .map(|l| l.slot)
            .max()
    }

    fn target_at(&self, ctx: &egui::Context, x: f64, y: f64) -> Target {
        if self.overlay_at(ctx, x, y) {
            Target::Overlay
        } else if let Some(slot) = self.dimension_at(x, y) {
            Target::Dimension(slot)
        } else {
            Target::Canvas
        }
    }

    fn event(p: &RawPointer) -> PointerEvent {
        PointerEvent {
            id: p.id,
            kind: p.kind,
            x: p.x,
            y: p.y,
            button: p.button,
            buttons: p.buttons,
        }
    }

    fn set_hover(&mut self, target: Target) {
        let canvas = target == Target::Canvas;
        if self.ov.hover_canvas && !canvas {
            self.vp.on_pointer_leave();
        }
        self.ov.hover_canvas = canvas;
        let slot = match target {
            Target::Dimension(slot) => Some(slot),
            _ => None,
        };
        if self.vp.highlighted_slot != slot {
            self.vp.highlighted_slot = slot;
            self.vp.invalidate();
        }
    }

    fn take_target(&mut self, id: i64) -> Option<Target> {
        let index = self.ov.targets.iter().position(|(i, _)| *i == id)?;
        Some(self.ov.targets.remove(index).1)
    }

    fn route(&mut self, ctx: &egui::Context, input: RawInput) {
        match input {
            RawInput::Down(p) => {
                self.vp.note_input_type(p.kind);
                let target = self.target_at(ctx, p.x, p.y);
                self.ov.targets.retain(|(i, _)| *i != p.id);
                self.ov.targets.push((p.id, target));
                self.set_hover(target);
                if target == Target::Canvas {
                    self.vp.on_pointer_down(Self::event(&p));
                }
            }
            RawInput::Move(p) => {
                let pressed = self.ov.targets.iter().find(|(i, _)| *i == p.id).map(|(_, t)| *t);
                match pressed {
                    Some(Target::Canvas) => self.vp.on_pointer_move(Self::event(&p)),
                    Some(_) => {}
                    None => {
                        let target = self.target_at(ctx, p.x, p.y);
                        self.set_hover(target);
                        if target == Target::Canvas {
                            self.vp.on_pointer_move(Self::event(&p));
                        }
                    }
                }
            }
            RawInput::Up(p) => match self.take_target(p.id) {
                Some(Target::Canvas) => self.vp.release_pointer(Self::event(&p), false),
                Some(Target::Dimension(slot)) => {
                    if self.dimension_at(p.x, p.y) == Some(slot) && !self.overlay_at(ctx, p.x, p.y) {
                        self.vp.select_dimension_at(slot, p.x, p.y);
                        if self.vp.dimension_choice.is_some() {
                            self.ov.focus_choice = true;
                        }
                    }
                }
                _ => {}
            },
            RawInput::Cancel(p) => {
                let target = self.take_target(p.id);
                if target == Some(Target::Canvas)
                    || (target.is_none() && self.vp.interaction.has_pointer(p.id))
                {
                    self.vp.release_pointer(Self::event(&p), true);
                }
            }
            RawInput::Leave => {
                if self.ov.hover_canvas {
                    self.vp.on_pointer_leave();
                    self.ov.hover_canvas = false;
                }
                if self.ov.targets.is_empty() && self.vp.highlighted_slot.is_some() {
                    self.vp.highlighted_slot = None;
                    self.vp.invalidate();
                }
            }
            RawInput::Wheel { x, y, delta_y, mode } => {
                if self.target_at(ctx, x, y) == Target::Canvas {
                    self.vp.on_wheel(delta_y, mode);
                }
            }
            RawInput::Blur => {
                self.ov.targets.clear();
                self.vp.cancel_interactions(false);
            }
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if self.ov.open_select.is_some() || self.ov.open_color.is_some() {
                if key == Key::Escape {
                    self.close_popups();
                }
                continue;
            }
            if self.ov.settings_open {
                if key == Key::Escape {
                    self.set_settings_open(false);
                }
                continue;
            }
            if self.vp.dimension_choice.is_some() {
                if key == Key::Escape {
                    self.vp.dismiss_dimension_choice();
                }
                continue;
            }
            let touch_panel = self
                .vp
                .interaction
                .dimension_edit
                .as_ref()
                .is_some_and(|e| e.touch_panel);
            if touch_panel {
                if modifiers.ctrl || modifiers.command || modifiers.alt {
                    continue;
                }
                let digit = match key {
                    Key::Num0 => Some("0"),
                    Key::Num1 => Some("1"),
                    Key::Num2 => Some("2"),
                    Key::Num3 => Some("3"),
                    Key::Num4 => Some("4"),
                    Key::Num5 => Some("5"),
                    Key::Num6 => Some("6"),
                    Key::Num7 => Some("7"),
                    Key::Num8 => Some("8"),
                    Key::Num9 => Some("9"),
                    Key::Comma | Key::Period => Some(","),
                    Key::Backspace | Key::Delete => Some("delete"),
                    _ => None,
                };
                if let Some(value) = digit {
                    if key == Key::Delete {
                        if let Some(edit) = self.vp.interaction.dimension_edit.as_mut() {
                            edit.replace_input = true;
                        }
                    }
                    self.vp.enter_length_key(value);
                } else if key == Key::Escape {
                    self.vp.close_length_editor(true);
                    self.platform.focus_canvas();
                } else if key == Key::Enter {
                    self.vp.submit_length();
                }
                continue;
            }
            if key == Key::Escape {
                self.vp.on_escape();
            }
            if key == Key::Z && modifiers.command {
                self.vp.undo_point();
            }
        }
    }

    // ------------------------------------------------------------ Einstellungen

    fn sync_style_controls(&mut self) {
        for (i, field) in FIELDS.iter().enumerate() {
            self.ov.inputs[i] = match field.kind {
                Kind::Number { factor, .. } => number_text(self.vp.appearance.num(field.key), factor),
                _ => self.vp.appearance.text(field.key).to_owned(),
            };
        }
    }

    fn control_value(&self, index: usize) -> Json {
        let field = &FIELDS[index];
        let text = &self.ov.inputs[index];
        match field.kind {
            Kind::Number { factor, .. } => serde_json::Number::from_f64(parse_number_input(text) / factor)
                .map_or(Json::Null, Json::Number),
            _ => Json::String(text.clone()),
        }
    }

    fn apply_values(&mut self, values: &Partial) -> Result<(), String> {
        let next = style::validated(&style::merged(&self.vp.appearance, values))?;
        self.vp.apply_style(next);
        Ok(())
    }

    /// `changeStyleControl`
    fn change_control(&mut self, index: usize) {
        let mut values = Partial::new();
        values.insert(FIELDS[index].key.to_owned(), self.control_value(index));
        match self.apply_values(&values) {
            Ok(()) => {
                self.ov.invalid[index] = false;
                self.ov.status = "Vorschau aktualisiert.".into();
            }
            Err(message) => {
                self.ov.invalid[index] = true;
                self.ov.status = message;
            }
        }
    }

    /// `saveStyleStandard`
    fn save_standard(&mut self) {
        let mut values = Partial::new();
        for (i, field) in FIELDS.iter().enumerate() {
            values.insert(field.key.to_owned(), self.control_value(i));
        }
        match style::validated(&values) {
            Ok(validated) => {
                let stored = self
                    .platform
                    .storage_set(style::STORAGE_KEY, &validated.to_json().to_string());
                self.ov.saved = validated.clone();
                self.vp.apply_style(validated);
                self.ov.status = if stored {
                    "Standard gespeichert. Der Download enthält alle Einstellungen."
                } else {
                    "Standard für den Download übernommen. Dauerhafter Browserspeicher ist nicht verfügbar."
                }
                .into();
            }
            Err(message) => self.ov.status = message,
        }
    }

    fn download_standard(&mut self) {
        let text = self.ov.saved.to_json().to_string();
        self.ov.status = if self
            .platform
            .download(&format!("3D-Viewport-{VERSION}-Standard.json"), &text)
        {
            "Gespeicherter Standard als Datei heruntergeladen.".into()
        } else {
            "Download nicht möglich.".into()
        };
    }

    fn reset_standard(&mut self) {
        let _ = self.apply_values(&defaults_json());
        self.sync_style_controls();
        self.ov.invalid.iter_mut().for_each(|v| *v = false);
        self.ov.status = "Werkseinstellungen in der Vorschau. Gespeicherter Standard bleibt bis zum erneuten Speichern erhalten."
            .into();
    }

    fn set_settings_open(&mut self, open: bool) {
        if open && self.vp.interaction.dimension_edit.is_some() && !self.vp.commit_length() {
            return;
        }
        self.vp.cancel_interactions(true);
        self.close_popups();
        self.ov.settings_open = open;
        if open {
            self.sync_style_controls();
            self.ov.tab = Tab::Dimensions;
        }
    }

    fn close_popups(&mut self) {
        self.ov.open_select = None;
        if let Some((index, color)) = self.ov.open_color.take() {
            let hex = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
            if hex != self.ov.inputs[index] {
                self.ov.inputs[index] = hex;
                self.change_control(index);
            }
        }
    }

    // ------------------------------------------------------------ Oberfläche

    fn touch_entry(&self) -> bool {
        self.vp
            .interaction
            .dimension_edit
            .as_ref()
            .is_some_and(|e| e.touch_panel)
    }

    fn draw_axis_labels(&self, painter: &Painter) {
        let a = &self.vp.appearance;
        for (axis, item) in self.vp.axis_labels.iter().enumerate() {
            let Some((x, y)) = item else { continue };
            let color = a.color(["axisX", "axisY", "axisZ"][axis]);
            let g = galley(painter, ["X", "Y", "Z"][axis], font(a.text("uiFont"), 500, 11.0), color, None, Some(11.0));
            painter.galley(Pos2::new(*x as f32, *y as f32) - g.size() / 2.0, g, color);
        }
    }

    fn draw_start_target(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let Some([x, y]) = self.vp.start_target() else {
            self.ov.start_hover = false;
            return;
        };
        let a = self.vp.appearance.clone();
        let center = Pos2::new(x as f32, y as f32);
        let s_rect = (28.0 * (self.vp.appearance.num("startTargetScale") as f32)).max(22.0);
        let rect = Rect::from_center_size(center, Vec2::splat(2.0 * s_rect));
        let near = self.vp.interaction.start_near || self.vp.is_closing();
        // Größe einstellbar; Trefferfläche mindestens 44 px.
        let s = self.vp.appearance.num("startTargetScale") as f32;
        let hit = (28.0 * s).max(22.0);
        let (clicked, hovered) = layer(ui, &self.hits, rect, true, |ui| {
            let response = ui.interact(rect, Id::unique("start-target-button"), Sense::click());
            let inside = response
                .hover_pos()
                .is_some_and(|p| p.distance(center) <= hit);
            let painter = ui.painter();
            let (fill, border, text, ring, ring_width) = if near {
                (
                    a.color("editHover"),
                    a.color("editText"),
                    Color32::WHITE,
                    hex_alpha(a.text("editColor"), 0x55),
                    12.0,
                )
            } else {
                (
                    hex_alpha("#20262d", 0x99),
                    a.color("startColor"),
                    hex_color("#ffe5ad"),
                    alpha(a.color("startColor"), 38),
                    7.0,
                )
            };
            annulus(painter, center, 28.0 * s, (28.0 + ring_width) * s, ring);
            painter.circle(center, 28.0 * s - 1.0, fill, Stroke::new(2.0, border));
            let size = (13.0 * s).max(10.0);
            let g = galley(painter, "Start", font(a.text("uiFont"), 600, size), text, None, Some(size));
            painter.galley(center - g.size() / 2.0, g, text);
            (response.clicked() && inside, inside)
        });
        if hovered != self.ov.start_hover {
            self.ov.start_hover = hovered;
            if hovered {
                self.vp.on_start_target_enter();
            } else {
                self.vp.on_start_target_leave();
            }
        }
        if clicked {
            self.vp.request_close();
        }
    }

    fn draw_tools(&mut self, ui: &mut egui::Ui, safe: Insets, width: f32, height: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let a = self.vp.appearance.clone();
        let hover = self.platform.hover_capable();
        let column = self.touch_entry() && width <= 520.0;
        let size = if column { 14.0 } else { a.num("buttonFontSize") as f32 };
        let bottom = height - (safe.bottom as f32).max(20.0);
        let left = if column { 12.0 } else { (safe.left as f32).max(20.0) };
        let axes = self.vp.render_state.axes_visible;
        let drawing = self.vp.is_drawing();
        let probe = ctx.layer_painter(LayerId::background());
        let measure = |label: &str| {
            if column {
                100.0
            } else {
                let w = probe
                    .layout_no_wrap(label.to_owned(), font(a.text("uiFont"), 500, size), Color32::WHITE)
                    .size()
                    .x;
                (w + 42.0).max(96.0).ceil()
            }
        };
        let (wa, wp) = (measure("Achsen"), measure("Polygon"));
        let (ra, rp) = if column {
            (
                Rect::from_min_size(Pos2::new(left, bottom - 48.0 * 2.0 - 6.0), Vec2::new(100.0, 48.0)),
                Rect::from_min_size(Pos2::new(left, bottom - 48.0), Vec2::new(100.0, 48.0)),
            )
        } else {
            (
                Rect::from_min_size(Pos2::new(left, bottom - 48.0), Vec2::new(wa, 48.0)),
                Rect::from_min_size(Pos2::new(left + wa + 10.0, bottom - 48.0), Vec2::new(wp, 48.0)),
            )
        };
        let (axes_clicked, polygon_clicked) = layer(ui, &self.hits, ra.union(rp), true, |ui| {
            let tone = |on: bool| if on { Tone::Active } else { Tone::Plain };
            let axes_clicked = tool(ui, &a, hover, Id::unique("axes-toggle"), ra, ToolSpec {
                label: "Achsen",
                size,
                padding: if column { 6.0 } else { 20.0 },
                tone: tone(axes),
                disabled: false,
            })
            .clicked();
            let polygon_clicked = tool(ui, &a, hover, Id::unique("polygon-toggle"), rp, ToolSpec {
                label: "Polygon",
                size,
                padding: if column { 6.0 } else { 20.0 },
                tone: tone(drawing),
                disabled: false,
            })
            .clicked();
            (axes_clicked, polygon_clicked)
        });
        if axes_clicked {
            self.vp.on_axes_toggle();
        }
        if polygon_clicked {
            self.vp.on_polygon_toggle();
        }
    }

    fn draw_undo(&mut self, ui: &mut egui::Ui, safe: Insets, width: f32, height: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        self.ov.undo_rect = None;
        if !self.vp.undo_visible() {
            return;
        }
        let a = self.vp.appearance.clone();
        let column = self.touch_entry() && width <= 520.0;
        let label = "Schritt zurück";
        let (left, bottom, size) = if column {
            (12.0, (safe.bottom as f32 + 112.0).max(132.0), 14.0)
        } else {
            ((safe.left as f32).max(20.0), (safe.bottom as f32 + 58.0).max(78.0), 16.0)
        };
        let w = if column {
            100.0
        } else {
            tool_width(&ctx.layer_painter(LayerId::background()), &a, label, size, 20.0, 96.0)
        };
        let rect = Rect::from_min_size(Pos2::new(left, height - bottom - 48.0), Vec2::new(w, 48.0));
        self.ov.undo_rect = Some(rect);
        let disabled = self.vp.undo_disabled();
        let hover = self.platform.hover_capable();
        let clicked = layer(ui, &self.hits, rect, true, |ui| {
            tool(ui, &a, hover, Id::unique("undo-point-button"), rect, ToolSpec {
                label,
                size,
                padding: if column { 6.0 } else { 20.0 },
                tone: Tone::Plain,
                disabled,
            })
            .clicked()
        });
        if clicked {
            self.vp.undo_point();
        }
    }

    fn draw_hint(&self, ui: &mut egui::Ui, safe: Insets, width: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let text = if self.vp.hint_visible() {
            DRAWING_HINT
        } else if self.vp.direction_hint_visible() {
            DIRECTION_HINT
        } else {
            return;
        };
        let a = &self.vp.appearance;
        let painter = ctx.layer_painter(LayerId::background());
        let inner = width - 32.0 - 24.0;
        let g = galley(&painter, text, font(a.text("uiFont"), 400, 14.0), hex_color("#ffe5ad"), Some(inner), Some(14.0 * 1.4));
        // Unter der Schaltfläche „Einstellungen“, damit der Hinweis sie nicht verdeckt.
        let top = (safe.top as f32).max(12.0 + 48.0 + 8.0);
        let rect = Rect::from_min_size(Pos2::new(16.0, top), Vec2::new(width - 32.0, g.size().y + 20.0));
        layer(ui, &self.hits, rect, false, |ui| {
            let painter = ui.painter();
            paint_box(painter, rect, CornerRadius::same(8), hex_alpha("#20262d", 0xed), None, &[]);
            painter.galley(rect.min + Vec2::new(12.0, 10.0), g, Color32::WHITE);
        });
    }

    fn draw_close_prompt(&mut self, ui: &mut egui::Ui, safe: Insets, width: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        if !self.vp.is_closing() {
            return;
        }
        let a = self.vp.appearance.clone();
        let hover = self.platform.hover_capable();
        let w = 330f32.min(width - 32.0);
        let top = (safe.top as f32).max(16.0);
        let line = normal_line(16.0);
        let rect = Rect::from_min_size(Pos2::new((width - w) / 2.0, top), Vec2::new(w, 1.0 + 12.0 + line + 10.0 + 48.0 + 12.0 + 1.0));
        let (confirm, cancel) = layer(ui, &self.hits, rect, true, |ui| {
            let painter = ui.painter().clone();
            paint_box(
                &painter,
                rect,
                CornerRadius::same(10),
                a.color("panelBackground"),
                Some((1.0, a.color("editText"))),
                &[css_shadow(4, 20, 0, Color32::from_black_alpha(0x44))],
            );
            let text = a.color("panelText");
            let g = galley(&painter, "Polygon schließen?", font(a.text("uiFont"), 400, 16.0), text, Some(w - 26.0), Some(line));
            painter.galley(rect.min + Vec2::new(13.0, 13.0), g, text);
            let y = rect.top() + 13.0 + line + 10.0;
            let confirm_w = tool_width(ui.painter(), &a, "✓", 26.0, 14.0, 56.0);
            let confirm_rect = Rect::from_min_size(Pos2::new(rect.left() + 13.0, y), Vec2::new(confirm_w, 48.0));
            let cancel_w = tool_width(ui.painter(), &a, "Weiterzeichnen", 16.0, 14.0, 56.0);
            let cancel_rect = Rect::from_min_size(Pos2::new(confirm_rect.right() + 8.0, y), Vec2::new(cancel_w, 48.0));
            let confirm = tool(ui, &a, hover, Id::unique("confirm-close"), confirm_rect, ToolSpec {
                label: "✓",
                size: 26.0,
                padding: 14.0,
                tone: Tone::Confirm,
                disabled: false,
            })
            .clicked();
            let cancel = tool(ui, &a, hover, Id::unique("cancel-close"), cancel_rect, ToolSpec {
                label: "Weiterzeichnen",
                size: 16.0,
                padding: 14.0,
                tone: Tone::Plain,
                disabled: false,
            })
            .clicked();
            (confirm, cancel)
        });
        if confirm {
            self.vp.complete_polygon();
        } else if cancel {
            self.vp.dismiss_close();
        }
    }

    fn draw_settings_toggle(&mut self, ui: &mut egui::Ui, safe: Insets, width: f32, height: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let a = self.vp.appearance.clone();
        let hover = self.platform.hover_capable();
        let size = a.num("buttonFontSize") as f32;
        let label = "Einstellungen";
        let w = tool_width(&ctx.layer_painter(LayerId::background()), &a, label, size, 20.0, 96.0);
        let left = if self.touch_entry() && width >= 521.0 && height <= 430.0 {
            (safe.left as f32).max(12.0)
        } else {
            width - 12.0 - w
        };
        let rect = Rect::from_min_size(Pos2::new(left, 12.0), Vec2::new(w, 48.0));
        let clicked = layer(ui, &self.hits, rect, true, |ui| {
            tool(ui, &a, hover, Id::unique("settings-toggle-button"), rect, ToolSpec {
                label,
                size,
                padding: 20.0,
                tone: Tone::Plain,
                disabled: false,
            })
            .clicked()
        });
        if clicked {
            self.set_settings_open(true);
        }
    }

    fn draw_dimension_choice(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let Some(choice) = self.vp.dimension_choice.clone() else {
            return;
        };
        let a = self.vp.appearance.clone();
        let w = 320f32.min(width - 16.0);
        let inner = w - 26.0;
        let painter = ctx.layer_painter(LayerId::background());
        let title = galley(&painter, "Maß auswählen", font(a.text("uiFont"), 700, 16.0), hex_color("#eef0f2"), Some(inner), Some(normal_line(16.0)));
        let heights: Vec<f32> = choice
            .options
            .iter()
            .map(|(label, _)| {
                let g = galley(&painter, label, font(a.text("uiFont"), 400, 13.333), Color32::WHITE, Some(inner - 16.0), None);
                (g.size().y + 16.0).max(48.0)
            })
            .collect();
        let cancel_h = 48.0;
        let content = title.size().y + heights.iter().map(|h| h + 8.0).sum::<f32>() + 8.0 + cancel_h;
        let h = (content + 26.0).min(height - 16.0);
        let rect = Rect::from_min_size(Pos2::new(choice.left as f32, choice.top as f32), Vec2::new(w, h));
        let picked = layer(ui, &self.hits, rect, true, |ui| {
            let painter = ui.painter().clone();
            paint_box(&painter, rect, CornerRadius::same(10), hex_color("#20262d"), Some((1.0, hex_color("#a981cc"))), &[]);
            painter.galley(rect.min + Vec2::new(13.0, 13.0), title.clone(), Color32::WHITE);
            let mut y = rect.top() + 13.0 + title.size().y;
            let mut picked = None;
            for ((label, slot), h) in choice.options.iter().zip(&heights) {
                y += 8.0;
                let r = Rect::from_min_size(Pos2::new(rect.left() + 13.0, y), Vec2::new(inner, *h));
                if plain_button(ui, &a, Id::unique(("dimension-option", *slot)), r, label).clicked() {
                    picked = Some(Some(*slot));
                }
                y += h;
            }
            y += 8.0;
            let r = Rect::from_min_size(Pos2::new(rect.left() + 13.0, y), Vec2::new(inner, cancel_h));
            if plain_button(ui, &a, Id::unique("dimension-choice-cancel"), r, "Abbrechen").clicked() {
                picked = Some(None);
            }
            picked
        });
        self.ov.focus_choice = false;
        match picked {
            Some(Some(slot)) => self.open_slot(slot),
            Some(None) => {
                self.vp.dismiss_dimension_choice();
                self.platform.focus_canvas();
            }
            None => {}
        }
    }

    fn open_slot(&mut self, slot: usize) {
        self.vp.open_dimension_slot(slot);
    }

    fn draw_length_editor(&mut self, ui: &mut egui::Ui, safe: Insets, width: f32, height: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let Some(edit) = self.vp.interaction.dimension_edit.clone() else {
            self.vp.panel_rect = None;
            self.ov.length_focused = false;
            return;
        };
        if edit.touch_panel {
            self.draw_touch_panel(ui, safe, width, height, edit.replace_input);
        } else {
            self.vp.panel_rect = None;
            self.draw_native_editor(ui);
        }
    }

    fn draw_native_editor(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let Some((left, top)) = self.vp.length_editor_position() else {
            return;
        };
        let a = self.vp.appearance.clone();
        let (left, top) = (left as f32, top as f32);
        let painter = ctx.layer_painter(LayerId::background());
        let ui_font = a.text("uiFont").to_owned();
        let note = galley(&painter, "Meter · 5-mm-Schritte · Beispiel: 2,01⁵ = 2,015 m", font(&ui_font, 400, 12.0), a.color("panelText"), Some(184.0 - 18.0), Some(12.0 * 1.4));
        let error = self
            .vp
            .length_error
            .as_ref()
            .map(|e| galley(&painter, e, font(&ui_font, 400, 12.0), a.color("panelText"), Some(184.0 - 18.0), Some(12.0 * 1.4)));
        let field = Rect::from_min_size(Pos2::new(left, top), Vec2::new(184.0, 48.0));
        let note_rect = Rect::from_min_size(Pos2::new(left, field.bottom() + 5.0), Vec2::new(184.0, note.size().y + 14.0));
        let error_rect = error
            .as_ref()
            .map(|g| Rect::from_min_size(Pos2::new(left, note_rect.bottom() + 5.0), Vec2::new(184.0, g.size().y + 14.0)));
        let total = error_rect.map_or(note_rect, |r| field.union(r)).union(field);
        let input_id = Id::unique("length-input");
        let focus_now = std::mem::take(&mut self.ov.focus_length) || self.vp.focus_request == Some(Focus::LengthInput);
        if self.vp.focus_request == Some(Focus::LengthInput) {
            self.vp.focus_request = None;
        }
        let undo_rect = self.ov.undo_rect;
        let (response, submitted, escaped) = layer(ui, &self.hits, total, true, |ui| {
            let painter = ui.painter().clone();
            paint_box(
                &painter,
                field,
                CornerRadius::same(7),
                a.color("panelBackground"),
                Some((1.0, a.color("editText"))),
                &[
                    css_shadow(0, 0, 2, hex_alpha(a.text("editColor"), 0x55)),
                    css_shadow(4, 18, 0, Color32::from_black_alpha(0x44)),
                ],
            );
            let m = galley(&painter, "m", font(&ui_font, 400, 14.0), hex_color("#a8b1b9"), None, None);
            painter.galley(Pos2::new(field.left() + 1.0 + 145.0, field.center().y - m.size().y / 2.0), m, Color32::WHITE);
            let input_rect = Rect::from_min_size(field.min + Vec2::new(1.0, 1.0), Vec2::new(145.0, 46.0));
            let (enter, escape) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
            let edit = egui::TextEdit::singleline(&mut self.vp.length_input)
                .id(input_id)
                .frame(egui::Frame::NONE)
                .font(font(&ui_font, 500, 16.0))
                .text_color(a.color("editText"))
                .margin(egui::Margin::ZERO)
                .align(egui::Align2::LEFT_CENTER)
                .desired_width(145.0 - 24.0);
            let response = ui.put(input_rect.shrink2(Vec2::new(12.0, 0.0)), edit);
            paint_box(&painter, note_rect, CornerRadius::same(5), a.color("panelBackground"), None, &[]);
            painter.galley(note_rect.min + Vec2::new(9.0, 7.0), note.clone(), Color32::WHITE);
            if let (Some(g), Some(r)) = (error.clone(), error_rect) {
                paint_box(&painter, r, CornerRadius::same(5), a.color("panelBackground"), None, &[]);
                painter.galley(r.min + Vec2::new(9.0, 7.0), g, Color32::WHITE);
            }
            (response, enter, escape)
        });
        if focus_now {
            response.request_focus();
            if let Some(mut state) = egui::TextEdit::load_state(ctx, input_id) {
                state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(self.vp.length_input.chars().count()),
                )));
                state.store(ctx, input_id);
            } else {
                self.ov.focus_length = true;
            }
        }
        if response.changed() {
            self.vp.length_error = None;
        }
        if response.has_focus() {
            self.ov.length_focused = true;
        }
        if response.lost_focus() {
            self.ov.length_focused = false;
            if submitted {
                if self.vp.commit_length() {
                    self.platform.focus_canvas();
                } else {
                    self.ov.focus_length = true;
                }
            } else if escaped {
                self.vp.close_length_editor(true);
                self.platform.focus_canvas();
            } else {
                // Druck auf „Schritt zurück“ behält den Fokus (preventDefault im Original).
                let press = ctx.input(|i| i.pointer.press_origin());
                if press.is_some_and(|p| undo_rect.is_some_and(|r| r.contains(p))) {
                    self.ov.focus_length = true;
                } else if self.vp.interaction.dimension_edit.is_some() {
                    self.vp.commit_length();
                }
            }
        }
    }

    fn draw_touch_panel(&mut self, ui: &mut egui::Ui, safe: Insets, width: f32, height: f32, replace: bool) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let a = self.vp.appearance.clone();
        let scale = a.num("numberPadScale") as f32;
        let ui_font = a.text("uiFont").to_owned();
        let mut w = 196f32.min(width - 24.0);
        if width <= 520.0 {
            w = w.min((width - 136.0) / scale);
        }
        let inner = w - 22.0;
        let painter = ctx.layer_painter(LayerId::background());
        let display_text = self.vp.length_display();
        let m = galley(&painter, "m", font(&ui_font, 400, 14.0), hex_color("#a8b1b9"), None, None);
        let display_line = normal_line(22.0);
        let field_h = display_line + 18.0 + 2.0;
        let note = galley(&painter, "Meter · 5-mm-Schritte", font(&ui_font, 400, 11.0), hex_color("#a8b1b9"), Some(inner), Some(11.0 * 1.4));
        let error = self
            .vp
            .length_error
            .as_ref()
            .map(|e| galley(&painter, e, font(&ui_font, 400, 12.0), a.color("panelText"), Some(inner - 18.0), Some(12.0 * 1.4)));
        let error_h = error.as_ref().map_or(0.0, |g| 5.0 + g.size().y + 14.0);
        let grid_h = 4.0 * 44.0 + 3.0 * 5.0;
        let h = 1.0 + 10.0 + field_h + 5.0 + note.size().y + error_h + 8.0 + grid_h + 8.0 + 44.0 + 10.0 + 1.0;
        // Unskalierte Lage, Ursprung unten rechts (transform-origin: bottom right).
        let right = width - (safe.right as f32).max(12.0);
        let bottom = height - (safe.bottom as f32).max(12.0);
        let origin = Pos2::new(right, bottom);
        let map = |r: Rect| Rect::from_min_max(origin + (r.min - origin) * scale, origin + (r.max - origin) * scale);
        let base = Rect::from_min_max(Pos2::new(right - w, bottom - h), origin);
        let shown = map(base);
        self.vp.panel_rect = Some(BoxRect {
            left: f64::from(shown.left()),
            top: f64::from(shown.top()),
            right: f64::from(shown.right()),
            bottom: f64::from(shown.bottom()),
        });
        let s = scale;
        let edit_light = a.color("editText");
        let keys: [(&str, &str); 12] = [
            ("7", "7"), ("8", "8"), ("9", "9"),
            ("4", "4"), ("5", "5"), ("6", "6"),
            ("1", "1"), ("2", "2"), ("3", "3"),
            (",", ","), ("0", "0"), ("⌫", "delete"),
        ];
        let action = layer(ui, &self.hits, shown, true, |ui| {
            let painter = ui.painter().clone();
            let corner = |r: f32| CornerRadius::same((r * s).round() as u8);
            paint_box(
                &painter,
                shown,
                corner(14.0),
                hex_alpha("#20262d", 0xf5),
                Some((s, edit_light)),
                &[css_shadow((8.0 * s) as i8, (28.0 * s) as u8, 0, Color32::from_black_alpha(0x66))],
            );
            let x0 = base.left() + 11.0;
            let mut y = base.top() + 11.0;
            let field = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, field_h));
            paint_box(
                &painter,
                map(field),
                corner(7.0),
                a.color("panelBackground"),
                Some((s, edit_light)),
                &[
                    css_shadow(0, 0, (2.0 * s).round() as u8, hex_alpha(a.text("editColor"), 0x55)),
                    css_shadow((4.0 * s) as i8, (18.0 * s) as u8, 0, Color32::from_black_alpha(0x44)),
                ],
            );
            let m_w = m.size().x + 6.0;
            let display = Rect::from_min_size(field.min + Vec2::new(1.0, 1.0), Vec2::new(inner - 2.0 - m_w, field_h - 2.0));
            let mut action: Option<&str> = None;
            let display_response = ui.interact(map(display), Id::unique("length-display"), Sense::click());
            if display_response.clicked() {
                action = Some("replace");
            }
            if replace {
                paint_box(&painter, map(display), corner(6.0), hex_alpha(a.text("editColor"), 0x55), None, &[]);
            }
            let text = galley(&painter, &display_text, font(&ui_font, 400, 22.0 * s), edit_light, Some((display.width() - 16.0) * s), Some(display_line * s));
            let text_pos = map(display).right_top() + Vec2::new(-8.0 * s - text.size().x, 9.0 * s);
            painter.galley(text_pos, text, edit_light);
            let m_scaled = galley(&painter, "m", font(&ui_font, 400, 14.0 * s), hex_color("#a8b1b9"), None, None);
            let m_pos = map(Rect::from_min_size(Pos2::new(display.right(), field.center().y), Vec2::ZERO)).min;
            painter.galley(m_pos - Vec2::new(0.0, m_scaled.size().y / 2.0), m_scaled, Color32::WHITE);
            y = field.bottom() + 5.0;
            let note_s = galley(&painter, "Meter · 5-mm-Schritte", font(&ui_font, 400, 11.0 * s), hex_color("#a8b1b9"), Some(inner * s), Some(11.0 * 1.4 * s));
            painter.galley(map(Rect::from_min_size(Pos2::new(x0, y), Vec2::ZERO)).min, note_s, Color32::WHITE);
            y += note.size().y;
            if let Some(g) = &error {
                y += 5.0;
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, g.size().y + 14.0));
                paint_box(&painter, map(r), corner(5.0), a.color("panelBackground"), None, &[]);
                let gs = galley(&painter, &self.vp.length_error.clone().unwrap_or_default(), font(&ui_font, 400, 12.0 * s), a.color("panelText"), Some((inner - 18.0) * s), Some(12.0 * 1.4 * s));
                painter.galley(map(r).min + Vec2::new(9.0, 7.0) * s, gs, Color32::WHITE);
                y = r.bottom();
            }
            y += 8.0;
            let cell = (inner - 10.0) / 3.0;
            let key_button = |ui: &mut egui::Ui, id: Id, r: Rect, label: &str, size: f32, accept: bool| {
                let shown = map(r);
                let response = ui.interact(shown, id, Sense::click());
                let fill = if response.is_pointer_button_down_on() {
                    a.color("editHover")
                } else if accept {
                    a.color("editFill")
                } else {
                    hex_color("#343d47")
                };
                let border = if accept { edit_light } else { hex_color("#65717c") };
                paint_box(ui.painter(), shown, corner(7.0), fill, Some((s, border)), &[]);
                let g = galley(ui.painter(), label, font(&ui_font, 400, size * s), hex_color("#eef0f2"), None, None);
                ui.painter().galley(shown.center() - g.size() / 2.0, g, Color32::WHITE);
                response.clicked()
            };
            for (i, (label, key)) in keys.iter().enumerate() {
                let (row, col) = (i / 3, i % 3);
                let r = Rect::from_min_size(
                    Pos2::new(x0 + col as f32 * (cell + 5.0), y + row as f32 * 49.0),
                    Vec2::new(cell, 44.0),
                );
                if key_button(ui, Id::unique(("length-key", i)), r, label, 20.0, false) {
                    action = Some(key);
                }
            }
            y += grid_h + 8.0;
            let half = (inner - 5.0) / 2.0;
            let cancel = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(half, 44.0));
            let accept = Rect::from_min_size(Pos2::new(x0 + half + 5.0, y), Vec2::new(half, 44.0));
            if key_button(ui, Id::unique("length-cancel"), cancel, "Abbrechen", 13.0, false) {
                action = Some("cancel");
            }
            if key_button(ui, Id::unique("length-accept"), accept, "Übernehmen", 13.0, true) {
                action = Some("accept");
            }
            action
        });
        match action {
            Some("replace") => {
                if let Some(edit) = self.vp.interaction.dimension_edit.as_mut() {
                    edit.replace_input = true;
                }
            }
            Some("cancel") => {
                self.vp.close_length_editor(true);
                self.platform.focus_canvas();
            }
            Some("accept") => self.vp.submit_length(),
            Some(key) => self.vp.enter_length_key(key),
            None => {}
        }
    }

    fn draw_settings(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        if !self.ov.settings_open {
            return;
        }
        let a = self.vp.appearance.clone();
        let hover = self.platform.hover_capable();
        let w = 430f32.min(width - 16.0);
        let rect = Rect::from_min_max(Pos2::new(width - 8.0 - w, 8.0), Pos2::new(width - 8.0, height - 8.0));
        let ui_font = a.text("uiFont").to_owned();
        let text = a.color("panelText");
        let note_color = hex_color("#bec6cc");
        let mut actions: Vec<SettingsAction> = Vec::new();
        layer(ui, &self.hits, rect, true, |ui| {
            let painter = ui.painter().clone();
            paint_box(
                &painter,
                rect,
                CornerRadius::same(12),
                a.color("panelBackground"),
                Some((1.0, hex_color("#526170"))),
                &[css_shadow(8, 32, 0, Color32::from_black_alpha(0x66))],
            );
            let inner = rect.shrink(1.0);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(inner);
            child.spacing_mut().item_spacing = Vec2::ZERO;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(inner.height())
                .show(&mut child, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let content_w = inner.width() - 32.0;
                    let (row, _) = ui.allocate_exact_size(Vec2::new(inner.width(), 16.0), Sense::hover());
                    let x0 = row.left() + 16.0;
                    let paragraph = |ui: &mut egui::Ui, body: &str, top: f32, bottom: f32| {
                        let g = galley(ui.painter(), body, font(&ui_font, 400, 13.0), note_color, Some(content_w), Some(13.0 * 1.45));
                        let (r, _) = ui.allocate_exact_size(Vec2::new(inner.width(), top + g.size().y + bottom), Sense::hover());
                        ui.painter().galley(Pos2::new(x0, r.top() + top), g, note_color);
                    };
                    // Kopfzeile
                    let close_w = tool_width(ui.painter(), &a, "Schließen", a.num("buttonFontSize") as f32, 20.0, 96.0);
                    let (header, _) = ui.allocate_exact_size(Vec2::new(inner.width(), 48.0), Sense::hover());
                    let title = galley(ui.painter(), "Einstellungen", font(&ui_font, 700, 20.0), text, None, None);
                    ui.painter().galley(Pos2::new(x0, header.center().y - title.size().y / 2.0), title, text);
                    let close_rect = Rect::from_min_size(Pos2::new(x0 + content_w - close_w, header.top()), Vec2::new(close_w, 48.0));
                    if tool(ui, &a, hover, Id::unique("settings-close"), close_rect, ToolSpec {
                        label: "Schließen",
                        size: a.num("buttonFontSize") as f32,
                        padding: 20.0,
                        tone: Tone::Plain,
                        disabled: false,
                    })
                    .clicked()
                    {
                        actions.push(SettingsAction::Close);
                    }
                    ui.allocate_exact_size(Vec2::new(1.0, 14.0), Sense::hover());
                    // Karteireiter
                    let (tabs, _) = ui.allocate_exact_size(Vec2::new(inner.width(), 44.0 * 2.0 + 5.0 + 1.0), Sense::hover());
                    let tab_w = (content_w - 5.0) / 2.0;
                    for (i, (tab, label)) in TABS.iter().enumerate() {
                        let r = Rect::from_min_size(
                            Pos2::new(x0 + (i % 2) as f32 * (tab_w + 5.0), tabs.top() + (i / 2) as f32 * 49.0),
                            Vec2::new(tab_w, 44.0),
                        );
                        let selected = self.ov.tab == *tab;
                        let response = ui.interact(r, Id::unique(("settings-tab", i)), Sense::click());
                        let (fill, color) = if selected {
                            (a.color("editFill"), a.color("editText"))
                        } else {
                            (a.color("buttonBackground"), a.color("buttonText"))
                        };
                        paint_box(ui.painter(), r, CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, fill, None, &[]);
                        let g = galley(ui.painter(), label, font(&ui_font, 400, 15.0), color, Some(r.width() - 12.0), None);
                        ui.painter().galley(r.center() - g.size() / 2.0, g, color);
                        if response.clicked() {
                            actions.push(SettingsAction::Tab(*tab));
                        }
                    }
                    ui.painter().hline(
                        x0..=x0 + content_w,
                        tabs.bottom() - 0.5,
                        Stroke::new(1.0, hex_color("#526170")),
                    );
                    ui.allocate_exact_size(Vec2::new(1.0, 14.0), Sense::hover());
                    // Felder
                    let mut first = true;
                    for (index, field) in FIELDS.iter().enumerate() {
                        if field.tab != self.ov.tab {
                            continue;
                        }
                        if !first {
                            ui.allocate_exact_size(Vec2::new(1.0, 13.0), Sense::hover());
                        }
                        first = false;
                        let caption_w = content_w - 130.0 - 12.0;
                        let caption = galley(ui.painter(), field.label, font(&ui_font, 400, 14.0), text, Some(caption_w), None);
                        let row_h = caption.size().y.max(44.0);
                        let (row, _) = ui.allocate_exact_size(Vec2::new(inner.width(), row_h), Sense::hover());
                        ui.painter().galley(Pos2::new(x0, row.center().y - caption.size().y / 2.0), caption, text);
                        let control = Rect::from_min_size(Pos2::new(x0 + content_w - 130.0, row.center().y - 22.0), Vec2::new(130.0, 44.0));
                        let border = if self.ov.invalid[index] { hex_color("#ff937c") } else { hex_color("#526170") };
                        paint_box(ui.painter(), control, CornerRadius::same(6), a.color("inputBackground"), Some((1.0, border)), &[]);
                        match &field.kind {
                            Kind::Number { .. } => {
                                let id = Id::unique(("setting", field.key));
                                let edit = egui::TextEdit::singleline(&mut self.ov.inputs[index])
                                    .id(id)
                                    .frame(egui::Frame::NONE)
                                    .font(font(&ui_font, 400, 16.0))
                                    .text_color(text)
                                    .margin(egui::Margin::ZERO)
                                    .align(egui::Align2::LEFT_CENTER)
                                    .desired_width(130.0 - 18.0);
                                let response = ui.put(control.shrink2(Vec2::new(9.0, 1.0)), edit);
                                if response.gained_focus() {
                                    self.ov.focus_values[index] = Some(self.ov.inputs[index].clone());
                                }
                                if response.lost_focus() {
                                    let before = self.ov.focus_values[index].take();
                                    if before.as_deref() != Some(self.ov.inputs[index].as_str()) {
                                        actions.push(SettingsAction::Change(index));
                                    }
                                }
                            }
                            Kind::Choice(_, options) => {
                                let response = ui.interact(control, Id::unique(("setting", field.key)), Sense::click());
                                let current = &self.ov.inputs[index];
                                let label = options
                                    .iter()
                                    .find(|(v, _)| v == current)
                                    .map_or(current.as_str(), |(_, l)| *l);
                                let g = galley(ui.painter(), label, font(&ui_font, 400, 16.0), text, None, None);
                                let clip = ui.painter().with_clip_rect(control.shrink2(Vec2::new(8.0, 1.0)).with_max_x(control.right() - 24.0));
                                clip.galley(Pos2::new(control.left() + 9.0, control.center().y - g.size().y / 2.0), g, text);
                                let c = Pos2::new(control.right() - 15.0, control.center().y);
                                ui.painter().line_segment([c + Vec2::new(-4.0, -2.0), c + Vec2::new(0.0, 2.0)], Stroke::new(1.5, text));
                                ui.painter().line_segment([c + Vec2::new(0.0, 2.0), c + Vec2::new(4.0, -2.0)], Stroke::new(1.5, text));
                                if response.clicked() {
                                    actions.push(SettingsAction::OpenSelect(index, control));
                                }
                            }
                            Kind::Color(_) => {
                                let response = ui.interact(control, Id::unique(("setting", field.key)), Sense::click());
                                let swatch = control.shrink(9.0).shrink2(Vec2::new(2.0, 4.0));
                                let value = self
                                    .ov
                                    .open_color
                                    .filter(|(i, _)| *i == index)
                                    .map_or_else(|| hex_color(&self.ov.inputs[index]), |(_, c)| c);
                                ui.painter().rect(swatch, CornerRadius::ZERO, value, Stroke::new(1.0, hex_color("#777777")), StrokeKind::Inside);
                                if response.clicked() {
                                    actions.push(SettingsAction::OpenColor(index, control));
                                }
                            }
                        }
                    }
                    if self.ov.tab == Tab::Dimensions {
                        paragraph(ui, "Die Schrifthöhe gilt einheitlich für alle Maßzahlen auf der Grundebene. Es gibt keine automatische Größenanpassung. Bei Platzmangel bitte näher heranzoomen.", 13.0, 13.0);
                    } else if self.ov.tab == Tab::Program {
                        paragraph(ui, "Der Rand gilt pro Viewportseite für Geometrie einschließlich Maßlinien. Mindestabstände für Bemaßung und Bedienung bleiben erhalten. Größere Werte zeigen mehr freien Raum.", 13.0, 13.0);
                    }
                    // Aktionen (flex-wrap, gap 8)
                    ui.allocate_exact_size(Vec2::new(1.0, 20.0), Sense::hover());
                    let buttons = [
                        ("Als Standard speichern", Tone::Active, SettingsAction::Save),
                        ("Standard als Datei herunterladen", Tone::Active, SettingsAction::Download),
                        ("Werkseinstellungen", Tone::Plain, SettingsAction::Reset),
                    ];
                    let mut rects = Vec::new();
                    let (mut x, mut line) = (0.0f32, 0usize);
                    for (label, _, _) in &buttons {
                        let bw = tool_width(ui.painter(), &a, label, 15.0, 12.0, 96.0).min(content_w);
                        if x > 0.0 && x + bw > content_w {
                            x = 0.0;
                            line += 1;
                        }
                        rects.push((x, line, bw));
                        x += bw + 8.0;
                    }
                    let lines = rects.last().map_or(0, |r| r.1) + 1;
                    let (area, _) = ui.allocate_exact_size(Vec2::new(inner.width(), lines as f32 * 48.0 + (lines - 1) as f32 * 8.0), Sense::hover());
                    for ((label, tone, action), (bx, bl, bw)) in buttons.into_iter().zip(rects) {
                        let r = Rect::from_min_size(Pos2::new(x0 + bx, area.top() + bl as f32 * 56.0), Vec2::new(bw, 48.0));
                        if tool(ui, &a, hover, Id::unique(("settings-action", label)), r, ToolSpec {
                            label,
                            size: 15.0,
                            padding: 12.0,
                            tone,
                            disabled: false,
                        })
                        .clicked()
                        {
                            actions.push(action);
                        }
                    }
                    paragraph(ui, "Änderungen wirken sofort. Speichern und Download umfassen alle Karteikarten.", 13.0, 13.0);
                    if self.ov.status.is_empty() {
                        ui.allocate_exact_size(Vec2::new(1.0, 16.0), Sense::hover());
                    } else {
                        let status = self.ov.status.clone();
                        paragraph(ui, &status, 0.0, 29.0);
                    }
                });
        });
        for action in actions {
            match action {
                SettingsAction::Close => self.set_settings_open(false),
                SettingsAction::Tab(tab) => {
                    self.close_popups();
                    self.ov.tab = tab;
                }
                SettingsAction::Change(index) => self.change_control(index),
                SettingsAction::OpenSelect(index, rect) => {
                    let reopen = self.ov.open_select.map(|(i, _)| i) != Some(index);
                    self.close_popups();
                    if reopen {
                        self.ov.open_select = Some((index, rect));
                    }
                }
                SettingsAction::OpenColor(index, rect) => {
                    let reopen = self.ov.open_color.map(|(i, _)| i) != Some(index);
                    self.close_popups();
                    if reopen {
                        self.ov.open_color = Some((index, hex_color(&self.ov.inputs[index])));
                        self.ov.color_anchor = rect;
                    }
                }
                SettingsAction::Save => self.save_standard(),
                SettingsAction::Download => self.download_standard(),
                SettingsAction::Reset => self.reset_standard(),
            }
        }
    }
}

enum SettingsAction {
    Close,
    Tab(Tab),
    Change(usize),
    OpenSelect(usize, Rect),
    OpenColor(usize, Rect),
    Save,
    Download,
    Reset,
}

impl App {
    /// Auswahllisten und Farbwähler der Einstellungen (Ersatz für native Steuerelemente).
    fn draw_popups(&mut self, ui: &mut egui::Ui, height: f32) {
        let ctx = &ui.ctx().clone();
        let _ = ctx;
        let a = self.vp.appearance.clone();
        let ui_font = a.text("uiFont").to_owned();
        let text = a.color("panelText");
        let press = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.press_origin() } else { None });
        if let Some((index, anchor)) = self.ov.open_select {
            let Kind::Choice(_, options) = &FIELDS[index].kind else {
                self.ov.open_select = None;
                return;
            };
            let painter = ctx.layer_painter(LayerId::background());
            let w = options
                .iter()
                .map(|(_, l)| painter.layout_no_wrap((*l).to_owned(), font(&ui_font, 400, 16.0), text).size().x + 24.0)
                .fold(anchor.width(), f32::max);
            let h = options.len() as f32 * 40.0 + 2.0;
            let top = if anchor.bottom() + h <= height - 8.0 { anchor.bottom() + 2.0 } else { (anchor.top() - h - 2.0).max(8.0) };
            let rect = Rect::from_min_size(Pos2::new(anchor.right() - w, top), Vec2::new(w, h));
            let current = self.ov.inputs[index].clone();
            let picked = layer(ui, &self.hits, rect, true, |ui| {
                paint_box(ui.painter(), rect, CornerRadius::same(6), a.color("inputBackground"), Some((1.0, hex_color("#526170"))), &[css_shadow(4, 16, 0, Color32::from_black_alpha(0x66))]);
                let mut picked = None;
                for (i, (value, label)) in options.iter().enumerate() {
                    let r = Rect::from_min_size(Pos2::new(rect.left() + 1.0, rect.top() + 1.0 + i as f32 * 40.0), Vec2::new(w - 2.0, 40.0));
                    let response = ui.interact(r, Id::unique(("select-option", index, i)), Sense::click());
                    if *value == current || response.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(4), if *value == current { a.color("editFill") } else { a.color("buttonHover") });
                    }
                    let g = galley(ui.painter(), label, font(&ui_font, 400, 16.0), text, None, None);
                    ui.painter().galley(Pos2::new(r.left() + 11.0, r.center().y - g.size().y / 2.0), g, text);
                    if response.clicked() {
                        picked = Some(*value);
                    }
                }
                picked
            });
            if let Some(value) = picked {
                self.ov.open_select = None;
                if value != current {
                    self.ov.inputs[index] = value.to_owned();
                    self.change_control(index);
                }
            } else if press.is_some_and(|p| !rect.contains(p) && !anchor.contains(p)) {
                self.ov.open_select = None;
            }
        }
        if let Some((index, mut color)) = self.ov.open_color {
            let anchor = self.ov.color_anchor;
            let size = Vec2::new(260.0, 230.0);
            let top = if anchor.bottom() + size.y <= height - 8.0 { anchor.bottom() + 2.0 } else { (anchor.top() - size.y - 2.0).max(8.0) };
            let rect = Rect::from_min_size(Pos2::new((anchor.right() - size.x).max(8.0), top), size);
            let shown = layer(ui, &self.hits, rect, true, |ui| {
                paint_box(ui.painter(), rect, CornerRadius::same(8), a.color("panelBackground"), Some((1.0, hex_color("#526170"))), &[css_shadow(4, 16, 0, Color32::from_black_alpha(0x66))]);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(10.0)));
                child.spacing_mut().slider_width = 220.0;
                egui::color_picker::color_picker_color32(&mut child, &mut color, egui::color_picker::Alpha::Opaque);
                child.min_rect().union(rect)
            });
            self.ov.open_color = Some((index, color));
            if press.is_some_and(|p| !shown.contains(p) && !anchor.contains(p)) {
                self.close_popups();
            }
        }
    }

    fn cursor(&self) -> CursorIcon {
        let i = &self.vp.interaction;
        if i.gesture.is_some() || matches!(i.drag, Some(Drag::Orbit { .. }) | Some(Drag::Edit { .. })) {
            CursorIcon::Grabbing
        } else if i.hover_edge.is_some() {
            CursorIcon::Grab
        } else if self.vp.is_drawing() {
            CursorIcon::Crosshair
        } else {
            CursorIcon::Default
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let c = self.vp.appearance.color("groundColor");
        [
            f32::from(c.r()) / 255.0,
            f32::from(c.g()) / 255.0,
            f32::from(c.b()) / 255.0,
            1.0,
        ]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let (width, height) = (screen.width(), screen.height());
        self.vp.now = self.platform.now();
        self.vp.set_size(f64::from(width), f64::from(height));
        let safe = self.platform.safe_area();

        for input in self.platform.drain(&ctx) {
            self.route(&ctx, input);
        }
        self.keyboard(&ctx);

        // Zeichenfläche (WebGL-Durchgänge) und Achsbeschriftungen
        let background = ctx.layer_painter(LayerId::background());
        self.vp.render_frame(&background, screen.min);
        self.draw_axis_labels(&background);
        if !self.ov.started {
            self.ov.started = true;
            self.platform.hide_startup_message();
        }

        // Bedienelemente in DOM-/z-index-Reihenfolge, alle in einer Ebene
        self.hits.borrow_mut().clear();
        egui::Area::new(Id::unique("bedienelemente"))
            .fixed_pos(screen.min)
            .order(Order::Foreground)
            .constrain(false)
            .show(&ctx, |ui| {
                ui.set_clip_rect(screen);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.expand_to_include_rect(screen);
                self.draw_start_target(ui);
                self.draw_tools(ui, safe, width, height);
                self.draw_undo(ui, safe, width, height);
                self.draw_hint(ui, safe, width);
                self.draw_close_prompt(ui, safe, width);
                self.draw_length_editor(ui, safe, width, height);
                self.draw_dimension_choice(ui, width, height);
                self.draw_settings_toggle(ui, safe, width, height);
                self.draw_settings(ui, width, height);
                self.draw_popups(ui, height);
            });
        self.hits_last = self.hits.borrow().clone();

        match self.vp.focus_request.take() {
            Some(Focus::Canvas) => {
                ctx.memory_mut(|m| {
                    if let Some(id) = m.focused() {
                        m.surrender_focus(id);
                    }
                });
                self.platform.focus_canvas();
            }
            Some(Focus::LengthInput) => self.ov.focus_length = true,
            _ => {}
        }

        if !ctx.is_pointer_over_egui() || self.ov.hover_canvas {
            ctx.set_cursor_icon(self.cursor());
        }
        if self.vp.needs_repaint {
            ctx.request_repaint();
        }
    }
}
