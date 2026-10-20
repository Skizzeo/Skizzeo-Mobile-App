//! 11-lifecycle: Größen- und Ausrichtungswechsel der Zeichenfläche.

use super::*;

impl Viewport {
    /// Neue Größe der Zeichenfläche (`syncViewport` / `resizeViewport`).
    pub fn set_size(&mut self, width: f64, height: f64) {
        if self.width == width && self.height == height {
            return;
        }
        self.restore_panel_view();
        let restored = self.restore_viewport_view(
            width,
            height,
            self.interaction
                .dimension_edit
                .as_ref()
                .is_some_and(|e| e.touch_panel),
        );
        self.width = width;
        self.height = height;
        let old = self.render_state.viewport_size;
        self.render_state.viewport_size = Some((width, height));
        if old != Some((width, height))
            && !self
                .interaction
                .dimension_edit
                .as_ref()
                .is_some_and(|e| e.touch_panel)
        {
            self.fit.requested = self.is_drawing() || !self.model.polygons.is_empty();
            self.fit.tight = true;
        }
        if restored {
            self.fit.requested = false;
        }
        self.invalidate();
    }

    pub(super) fn restore_viewport_view(&mut self, width: f64, height: f64, preserve_scale: bool) -> bool {
        let Some(old) = self.render_state.viewport_size else {
            return false;
        };
        if (width - old.0).abs() <= 2.0
            || (old.0 > old.1) == (width > height)
            || (self.model.polygons.is_empty() && !self.is_drawing())
        {
            return false;
        }
        let from = if old.0 > old.1 { "landscape" } else { "portrait" };
        let to = if width > height { "landscape" } else { "portrait" };
        self.render_state.viewport_views.insert(
            from,
            SavedView {
                camera: self.camera,
                height: old.1,
            },
        );
        self.fit.animation = None;
        if let Some(saved) = self.render_state.viewport_views.get(to).copied() {
            self.camera.azimuth = saved.camera.azimuth;
            self.camera.elevation = saved.camera.elevation;
            self.camera.distance = saved.camera.distance * (height / saved.height);
            self.camera.target = saved.camera.target;
        } else if preserve_scale {
            self.camera.distance *= height / old.1;
        } else {
            return false;
        }
        self.fit.requested = false;
        true
    }
}
