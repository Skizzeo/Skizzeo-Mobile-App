//! Zeichnet einen Frame des Viewports mit egui.

use crate::controller::Viewport;

pub fn paint_frame(vp: &mut Viewport, painter: &egui::Painter, origin: egui::Pos2) {
    vp.render_frame(painter, origin);
}
