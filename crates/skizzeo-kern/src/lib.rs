//! Geometriekern von Skizzeo.
//!
//! Der Kern ist eine eigenständige Einheit: Er besitzt das Gebäudemodell (Grundrisse),
//! seine Geometrie, die Maßketten und die fachlichen Regeln der Eingabe. Er kennt
//! weder Bildschirm, Kamera, Zeit noch Plattform und meldet Fehler als [`KernError`],
//! nicht als Text. Darstellung und Bedienung bauen auf ihm auf, nie umgekehrt.

pub mod error;
pub mod geometry;
pub mod measurements;
pub mod model;
pub mod units;

pub use error::KernError;

/// Weltpunkt in Metern. X/Y horizontal, Z nach oben.
pub type Point3 = [f64; 3];

/// Grenzwerte des Kerns (geometrischer Teil von SETTINGS im Prototyp).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub geometry_epsilon: f64,
    pub minimum_length: f64,
    /// Bevorzugung der bisherigen Richtung beim rechtwinkligen Entwurf.
    pub direction_hysteresis: f64,
}

pub const SETTINGS: Settings = Settings {
    geometry_epsilon: 1e-8,
    minimum_length: 1e-6,
    direction_hysteresis: 1.4,
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
