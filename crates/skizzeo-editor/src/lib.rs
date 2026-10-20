//! Bedienlogik von Skizzeo: Zeichenmodus, Zeiger und Gesten, Maßeditor, Einpassung
//! der Kamera und die Vorbereitung jedes Frames.
//!
//! Der Editor verändert das Modell ausschließlich über den Geometriekern und nutzt
//! die Darstellung für Kamera, Maßlinien und Zeichenaufrufe. Er kennt keine
//! Grafikbibliothek und keine Plattform; die App reicht Eingaben herein und zeichnet
//! die gelieferte [`skizzeo_darstellung::render::DrawList`].

pub mod state;
pub mod viewport;
pub mod workflow;

pub use viewport::{DimensionChoice, Focus, PointerEvent, Viewport};

/// Bedienparameter (Bildschirmteil von SETTINGS im Prototyp), in CSS-Pixeln bzw. ms.
#[derive(Clone, Copy, Debug)]
pub struct InputSettings {
    pub touch_deadband: f64,
    pub mouse_deadband: f64,
    pub touch_close_radius: f64,
    pub mouse_close_radius: f64,
    pub fit_duration_ms: f64,
    pub orbit_sensitivity: f64,
}

pub const INPUT: InputSettings = InputSettings {
    touch_deadband: 6.0,
    mouse_deadband: 3.0,
    touch_close_radius: 52.0,
    mouse_close_radius: 24.0,
    fit_duration_ms: 320.0,
    orbit_sensitivity: 0.006,
};
