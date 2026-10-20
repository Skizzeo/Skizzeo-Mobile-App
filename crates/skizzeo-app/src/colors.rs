//! Farben der Darstellung als egui-Farben.

use egui::Color32;
use skizzeo_darstellung::style::{hex_rgb, rgb_color, Appearance, Rgb8};

pub fn color32([r, g, b]: Rgb8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn hex_color(hex: &str) -> Color32 {
    color32(rgb_color(hex_rgb(hex)))
}

pub trait Colors {
    /// Farbeinstellung `key` als egui-Farbe.
    fn color(&self, key: &str) -> Color32;
}

impl Colors for Appearance {
    fn color(&self, key: &str) -> Color32 {
        hex_color(self.text(key))
    }
}
