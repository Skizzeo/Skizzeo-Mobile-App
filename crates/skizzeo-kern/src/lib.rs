//! Deterministischer Rechenkern des Skizzeo-Viewports.
//!
//! Der Kern ist frei von Oberfläche, Zeit und Plattform. Er entspricht den Kapiteln
//! 03-workflow, 04-geometry, 04-dimensions und den reinen Teilen von 05-camera des
//! Prototyps V039.

pub mod camera;
pub mod dimension_geometry;
pub mod format;
pub mod geometry;
pub mod measurements;
pub mod workflow;

/// Weltpunkt in Metern. X/Y horizontal, Z nach oben.
pub type Point3 = [f64; 3];

/// Gemeinsame Parameter (SETTINGS im Prototyp).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub geometry_epsilon: f64,
    pub minimum_length: f64,
    pub direction_hysteresis: f64,
    pub touch_deadband: f64,
    pub mouse_deadband: f64,
    pub touch_close_radius: f64,
    pub mouse_close_radius: f64,
    pub fit_duration_ms: f64,
    pub orbit_sensitivity: f64,
}

pub const SETTINGS: Settings = Settings {
    geometry_epsilon: 1e-8,
    minimum_length: 1e-6,
    direction_hysteresis: 1.4,
    touch_deadband: 6.0,
    mouse_deadband: 3.0,
    touch_close_radius: 52.0,
    mouse_close_radius: 24.0,
    fit_duration_ms: 320.0,
    orbit_sensitivity: 0.006,
};

/// `Math.sign` aus JavaScript: liefert 0 für 0 (anders als `f64::signum`).
pub fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        value
    }
}

/// `Math.round` aus JavaScript: rundet .5 Richtung +∞.
pub fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}
