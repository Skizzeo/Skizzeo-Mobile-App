//! Verhaltensprotokoll eines vollständigen Bedienablaufs.
//!
//! Der Ablauf zeichnet, bemaßt, schließt und bearbeitet ein Polygon über die
//! öffentlichen Bedienmethoden und protokolliert nach jedem Schritt Modell,
//! Kamera, Bedienzustand sowie eine Prüfsumme der gezeichneten Dreiecke.
//! Das Protokoll muss bei Umbauten unverändert bleiben
//! (`SKIZZEO_GOLDEN=update cargo test -p skizzeo-app` schreibt es neu).

use std::fmt::Write as _;

use skizzeo_darstellung::render::Rect;
use skizzeo_darstellung::style::Appearance;
use skizzeo_editor::state::PointerType;
use skizzeo_editor::{PointerEvent, Viewport};

use crate::app_frame::paint_frame;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/scenario.txt");

struct Run {
    vp: Viewport,
    ctx: egui::Context,
    log: String,
    next_id: i64,
    now: f64,
}

fn fnv(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash ^= u64::from(*b);
        *hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
}

impl Run {
    fn new() -> Self {
        let mut vp = Viewport::new(Appearance::default());
        vp.set_size(390.0, 844.0);
        Self {
            vp,
            ctx: egui::Context::default(),
            log: String::new(),
            next_id: 1,
            now: 0.0,
        }
    }

    fn frame(&mut self, label: &str, now: f64) {
        self.vp.now = now;
        let vp = &mut self.vp;
        let mut output = self.ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::background());
            paint_frame(vp, &painter, egui::Pos2::ZERO);
        });
        output.textures_delta.clear();
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut vertices = 0usize;
        for clipped in &output.shapes {
            match &clipped.shape {
                egui::Shape::Mesh(mesh) => {
                    vertices += mesh.vertices.len();
                    for v in &mesh.vertices {
                        fnv(&mut hash, &v.pos.x.to_bits().to_le_bytes());
                        fnv(&mut hash, &v.pos.y.to_bits().to_le_bytes());
                        fnv(&mut hash, &v.color.to_array());
                    }
                    for i in &mesh.indices {
                        fnv(&mut hash, &i.to_le_bytes());
                    }
                }
                other => fnv(&mut hash, format!("{other:?}").as_bytes()),
            }
        }
        self.dump(label, output.shapes.len(), vertices, hash);
    }

    fn dump(&mut self, label: &str, shapes: usize, vertices: usize, hash: u64) {
        let vp = &self.vp;
        let log = &mut self.log;
        writeln!(log, "## {label}").unwrap();
        writeln!(log, "phase {} drawing {} closing {}", vp.interaction_phase(), vp.is_drawing(), vp.is_closing()).unwrap();
        writeln!(log, "camera {:?}", vp.camera).unwrap();
        for polygon in &vp.model.polygons {
            writeln!(log, "polygon {} {:?}", polygon.id, polygon.points).unwrap();
        }
        writeln!(log, "active {:?}", vp.model.active).unwrap();
        writeln!(log, "live {:?}", vp.interaction.live_point).unwrap();
        writeln!(log, "input {:?} error {:?}", vp.length_input, vp.length_error).unwrap();
        if let Some(edit) = &vp.interaction.dimension_edit {
            writeln!(log, "edit {:?} touch {} at ({:?}, {:?})", edit.record.name, edit.touch_panel, edit.x, edit.y).unwrap();
        }
        if let Some(choice) = &vp.dimension_choice {
            writeln!(log, "choice {:?}", choice.options).unwrap();
        }
        for (slot, record) in vp.frame_records.iter().enumerate() {
            writeln!(log, "record {slot} {:?} {:?}", record.name, vp.measurement_length(record)).unwrap();
        }
        for layout in &vp.render_state.dimension_layouts {
            writeln!(log, "layout {} {:?} {:?}", layout.slot, layout.box_, layout.hit_box).unwrap();
        }
        writeln!(log, "axes {:?}", vp.axis_labels).unwrap();
        writeln!(
            log,
            "ui start {:?} undo {} {} hints {} {} editor {:?}",
            vp.start_target(),
            vp.undo_visible(),
            vp.undo_disabled(),
            vp.direction_hint_visible(),
            vp.hint_visible(),
            vp.length_editor_position()
        )
        .unwrap();
        writeln!(log, "paint shapes {shapes} vertices {vertices} hash {hash:016x}").unwrap();
    }

    fn screen(&self, p: [f64; 3]) -> (f64, f64) {
        let s = self.vp.project(&p, &self.vp.matrix()).expect("sichtbar");
        (s[0], s[1])
    }

    fn event(&self, id: i64, kind: PointerType, (x, y): (f64, f64), button: i16, buttons: u16) -> PointerEvent {
        PointerEvent { id, kind, x, y, button, buttons }
    }

    fn tap(&mut self, kind: PointerType, at: (f64, f64)) {
        let id = self.next_id;
        self.next_id += 1;
        self.vp.note_input_type(kind);
        self.vp.on_pointer_down(self.event(id, kind, at, 0, 1));
        self.vp.release_pointer(self.event(id, kind, at, 0, 0), false);
    }

    fn drag(&mut self, kind: PointerType, from: (f64, f64), to: (f64, f64), button: i16) {
        let id = self.next_id;
        self.next_id += 1;
        let buttons = if button == 1 { 4 } else { 1 };
        self.vp.note_input_type(kind);
        self.vp.on_pointer_down(self.event(id, kind, from, button, buttons));
        for step in 1..=4 {
            let t = f64::from(step) / 4.0;
            let at = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            self.vp.on_pointer_move(self.event(id, kind, at, button, buttons));
        }
        self.vp.release_pointer(self.event(id, kind, to, button, 0), false);
    }

    fn tick(&mut self, label: &str) {
        self.now += 1000.0;
        self.frame(label, self.now);
    }

    fn type_length(&mut self, text: &str) -> bool {
        self.vp.length_input = text.to_string();
        self.vp.commit_length()
    }
}

fn scenario() -> String {
    let mut run = Run::new();
    run.tick("Start");
    run.vp.on_polygon_toggle();
    run.frame("Zeichnen begonnen", run.now + 100.0);
    run.tick("Zeichnen eingepasst");

    // Hover ohne Taste, dann Klick: Vorschausegment mit Längeneditor
    let target = run.screen([10.0, -6.0, 0.0]);
    run.vp.on_pointer_move(run.event(99, PointerType::Mouse, target, 0, 0));
    run.tick("Hover");
    run.tap(PointerType::Mouse, target);
    run.tick("Editor offen");
    assert!(run.type_length("6,5"));
    run.tick("Segment 2 übernommen");

    let target = run.screen([0.0, -6.5, 0.0]);
    run.tap(PointerType::Mouse, target);
    run.tick("Editor Segment 3");
    assert!(!run.type_length("abc"));
    run.tick("Ungültige Zahl");
    assert!(!run.type_length("0,001"));
    run.tick("Zu kurz");
    assert!(run.type_length("10"));
    run.tick("Segment 3 übernommen");

    // Startpunkt antippen: Schließen anfragen und bestätigen
    let start = run.screen([0.0, 0.0, 0.0]);
    run.tap(PointerType::Mouse, start);
    run.tick("Schließen angefragt");
    run.vp.complete_polygon();
    run.tick("Polygon geschlossen");
    run.frame("Einpassung halb", run.now + 160.0);
    run.tick("Einpassung fertig");

    // Kante per Touch verschieben, danach Touch-Zahlenfeld
    let a = run.screen([10.0, -1.0, 0.0]);
    let b = run.screen([11.0, -1.0, 0.0]);
    run.drag(PointerType::Touch, a, b, 0);
    run.vp.panel_rect = Some(Rect { left: 0.0, top: 560.0, right: 390.0, bottom: 844.0 });
    run.tick("Kante verschoben");
    for key in ["1", "2", "delete", "3", ",", "2", "5"] {
        run.vp.enter_length_key(key);
    }
    run.tick("Touch-Eingabe");
    run.vp.submit_length();
    run.vp.panel_rect = None;
    run.tick("Touch-Eingabe übernommen");

    // Maßzahl antippen (Maus) und ändern
    let layout = run.vp.render_state.dimension_layouts.first().cloned().expect("Maßzahl");
    run.vp.note_input_type(PointerType::Mouse);
    let (x, y) = (
        (layout.box_.left + layout.box_.right) / 2.0,
        (layout.box_.top + layout.box_.bottom) / 2.0,
    );
    run.vp.select_dimension_at(layout.slot, x, y);
    run.tick("Maßzahl gewählt");
    if run.vp.dimension_choice.is_some() {
        run.vp.open_dimension_slot(layout.slot);
        run.tick("Maßzahl aus Auswahl");
    }
    let accepted = run.type_length("8,01⁵");
    writeln!(run.log, "accepted {accepted}").unwrap();
    run.tick("Maß geändert");
    run.vp.close_length_editor(false);

    // Navigation: Rad, mittlere Taste, Zwei-Finger-Geste
    run.vp.on_wheel(240.0, 0);
    run.tick("Rad");
    run.drag(PointerType::Mouse, (200.0, 400.0), (260.0, 360.0), 1);
    run.tick("Orbit");
    run.vp.on_pointer_down(run.event(50, PointerType::Touch, (150.0, 400.0), 0, 1));
    run.vp.on_pointer_down(run.event(51, PointerType::Touch, (250.0, 400.0), 0, 1));
    run.vp.on_pointer_move(run.event(51, PointerType::Touch, (300.0, 420.0), 0, 1));
    run.vp.on_pointer_move(run.event(50, PointerType::Touch, (120.0, 380.0), 0, 1));
    run.vp.release_pointer(run.event(50, PointerType::Touch, (120.0, 380.0), 0, 0), false);
    run.vp.release_pointer(run.event(51, PointerType::Touch, (300.0, 420.0), 0, 0), false);
    run.tick("Geste");

    // Hover über eine Kante im Ansichtsmodus
    let edge = run.screen([5.0, 0.0, 0.0]);
    run.vp.on_pointer_move(run.event(98, PointerType::Mouse, edge, 0, 0));
    run.tick("Kante gehovert");
    run.vp.on_pointer_leave();

    // Zweiter Zug, Rücknahme, Abbruch
    run.vp.on_polygon_toggle();
    run.tick("Zweiter Zug");
    let target = run.screen([10.0, 4.0, 0.0]);
    run.tap(PointerType::Touch, target);
    run.tick("Vorschau per Touch");
    run.vp.enter_length_key("4");
    run.vp.submit_length();
    run.tick("Zweites Segment");
    run.vp.undo_point();
    run.tick("Rückgängig");
    run.vp.undo_point();
    run.tick("Rückgängig leer");
    run.vp.on_escape();
    run.tick("Abgebrochen");

    // Schließen und Wiederöffnen per Rückgängig
    run.vp.on_polygon_toggle();
    for (point, length) in [([10.0, 5.0, 0.0], "5"), ([0.0, 5.0, 0.0], "10")] {
        let target = run.screen(point);
        run.tap(PointerType::Mouse, target);
        assert!(run.type_length(length));
        run.tick("Zug");
    }
    run.vp.request_close();
    run.vp.complete_polygon();
    run.tick("Zweites Polygon");
    run.vp.undo_point();
    run.tick("Wiedergeöffnet");
    run.vp.on_escape();

    // Achsen, Stil, Ausrichtung
    run.vp.on_axes_toggle();
    run.tick("Achsen umgeschaltet");
    run.vp.apply_style(Appearance::default());
    run.tick("Stil");
    run.vp.set_size(844.0, 390.0);
    run.tick("Querformat");
    run.vp.set_size(390.0, 844.0);
    run.tick("Hochformat");
    run.log
}

#[test]
fn bedienablauf_bleibt_gleich() {
    let log = scenario();
    if std::env::var("SKIZZEO_GOLDEN").as_deref() == Ok("update") {
        std::fs::create_dir_all(std::path::Path::new(GOLDEN).parent().unwrap()).unwrap();
        std::fs::write(GOLDEN, &log).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(GOLDEN).expect("Protokoll fehlt");
    if log != expected {
        let line = log
            .lines()
            .zip(expected.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(log.lines().count().min(expected.lines().count()));
        panic!(
            "Ablauf weicht ab ab Zeile {}:\n  neu:  {:?}\n  alt:  {:?}",
            line + 1,
            log.lines().nth(line),
            expected.lines().nth(line)
        );
    }
}
