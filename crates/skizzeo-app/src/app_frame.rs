//! Überträgt die Zeichenaufrufe eines Frames in egui-Dreiecksnetze.

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Painter, Pos2, Shape};
use skizzeo_darstellung::render::DrawList;
use skizzeo_editor::Viewport;

use crate::colors::color32;

/// Einen Frame des Viewports vorbereiten und an der Stelle `origin` zeichnen.
pub fn paint_frame(vp: &mut Viewport, painter: &Painter, origin: Pos2) {
    let list = vp.render_frame(f64::from(painter.ctx().pixels_per_point()));
    paint(&list, painter, origin);
}

fn paint(list: &DrawList, painter: &Painter, origin: Pos2) {
    for batch in &list.batches {
        let mut mesh = Mesh::default();
        for (index, (p, color)) in batch.vertices.iter().enumerate() {
            mesh.vertices.push(Vertex {
                pos: Pos2::new(origin.x + p[0] as f32, origin.y + p[1] as f32),
                uv: WHITE_UV,
                color: color32(*color),
            });
            mesh.indices.push(index as u32);
        }
        painter.add(Shape::mesh(mesh));
    }
}
