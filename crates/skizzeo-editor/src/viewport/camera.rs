//! 05-camera: Projektion, Bodentreffer und Einpassung der Kamera.

use skizzeo_darstellung::camera::{
    camera_basis, dot, ground_point, project, view_projection, view_slope, Matrix,
    ViewSnapshot,
};
use skizzeo_darstellung::format::format_building_length;
use skizzeo_darstellung::render::{
    self, Rect,
};
use skizzeo_kern::model::PathKey;
use skizzeo_kern::Point3;

use crate::INPUT;

use super::*;

impl Viewport {
    pub fn matrix(&self) -> Matrix {
        view_projection(&self.camera, self.width, self.height)
    }

    pub fn project(&self, p: &Point3, matrix: &Matrix) -> Option<[f64; 2]> {
        project(p, matrix, self.width, self.height)
    }

    pub(super) fn snapshot(&self) -> ViewSnapshot {
        ViewSnapshot {
            camera: self.camera,
            width: self.width,
            height: self.height,
        }
    }

    pub(super) fn freeze_input_view(&mut self) {
        self.fit.frozen_input = Some(self.snapshot());
    }

    pub(super) fn ground_point(&self, x: f64, y: f64) -> Option<Point3> {
        ground_point(x, y, &self.snapshot())
    }

    pub(super) fn drawing_point(&mut self, x: f64, y: f64) -> Option<Point3> {
        if self.fit.animation.is_some() && self.fit.frozen_input.is_none() {
            self.freeze_input_view();
        }
        let view = self.fit.frozen_input.unwrap_or_else(|| self.snapshot());
        ground_point(x, y, &view)
    }

    pub(super) fn advance_fit(&mut self) {
        let Some(motion) = self.fit.animation.clone() else {
            return;
        };
        let progress = ((self.now - motion.started) / INPUT.fit_duration_ms).clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        for i in 0..3 {
            self.camera.target[i] =
                motion.from_target[i] + (motion.target[i] - motion.from_target[i]) * eased;
        }
        self.camera.distance =
            motion.from_distance + (motion.distance - motion.from_distance) * eased;
        if progress >= 1.0 {
            self.fit.animation = None;
        }
    }

    pub fn request_fit(&mut self, path: Option<PathKey>, tight: bool) {
        self.fit.path = path;
        self.fit.tight = self.fit.tight || tight;
        self.advance_fit();
        self.fit.animation = None;
        self.fit.requested = true;
        self.invalidate();
    }

    /// Aktiver Zug samt Schließergänzung oder Vorschau.
    pub(super) fn active_geometry(&self) -> Vec<Point3> {
        if self.is_closing() || self.interaction.start_near {
            if let Some(closed) = self.geometry.close_path(&self.model.active) {
                return closed;
            }
        }
        let mut points = self.model.active.clone();
        if let Some(live) = self.interaction.live_point {
            points.push(live);
        }
        points
    }

    pub(super) fn fit_drawing(&mut self) {
        let key = self.fit.path.or_else(|| {
            if self.is_drawing() {
                Some(self.model.active_key())
            } else {
                self.model.polygons.last().map(|p| PathKey::Polygon(p.id))
            }
        });
        let Some(key) = key else { return };
        let Some(path) = self.model.path(key).cloned() else {
            return;
        };
        if path.is_empty()
            || self.interaction.navigation
            || self.interaction.drag.as_ref().is_some_and(|d| !d.is_draw())
        {
            return;
        }
        if self.fit.animation.is_some() {
            self.fit.requested = true;
            return;
        }
        let (width, height) = (self.width, self.height);
        if width == 0.0 || height == 0.0 {
            return;
        }
        let is_active = key == self.model.active_key();
        let geometry_points = if is_active {
            self.active_geometry()
        } else {
            path.clone()
        };
        let mut points = geometry_points.clone();
        points.extend(self.dimension_bounds(&geometry_points, !is_active));
        let matrix = self.matrix();
        let edge = 28f64.min(width * 0.08).min(height * 0.08);
        let landscape = width > height;
        let margin = if landscape {
            self.appearance.num("fitMarginLandscape")
        } else {
            self.appearance.num("fitMargin")
        };
        let margin_x = (edge + 8.0).max(width * margin);
        let margin_y = if landscape {
            48f64.min(height * 0.12)
        } else {
            120f64.min(height * 0.25)
        }
        .max(height * margin);
        let fit_exact = self.fit.tight;
        self.fit.tight = false;
        if !fit_exact
            && points.iter().all(|p| {
                self.project(p, &matrix).is_some_and(|s| {
                    s[0] >= margin_x
                        && s[0] <= width - margin_x
                        && s[1] >= margin_y
                        && s[1] <= height - margin_y
                })
            })
        {
            return;
        }
        let basis = camera_basis(self.camera.azimuth, self.camera.elevation);
        let mut target = [0.0; 3];
        for (axis, value) in target.iter_mut().enumerate() {
            let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in &points {
                min = min.min(p[axis]);
                max = max.max(p[axis]);
            }
            *value = (min + max) / 2.0;
        }
        let slope_y = view_slope() * (1.0 - 2.0 * margin_y / height);
        let slope_x = view_slope() * (width / height) * (1.0 - 2.0 * margin_x / width);
        let mut distance = if fit_exact { 0.1 } else { self.camera.distance };
        for p in &points {
            let q = [p[0] - target[0], p[1] - target[1], p[2] - target[2]];
            let depth = dot(&q, &basis.backward);
            distance = distance
                .max(depth + dot(&q, &basis.right).abs() / slope_x)
                .max(depth + dot(&q, &basis.up).abs() / slope_y)
                .max(depth + 0.1);
        }
        if self.interaction.pointer.is_some()
            && self.interaction.live_point.is_some()
            && self.interaction.dimension_edit.is_none()
            && self.fit.frozen_input.is_none()
        {
            self.freeze_input_view();
        }
        if self.reduced_motion {
            self.camera.target = target;
            self.camera.distance = distance * 1.02;
        } else {
            self.fit.animation = Some(FitAnimation {
                from_target: self.camera.target,
                from_distance: self.camera.distance,
                target,
                distance: distance * 1.02,
                started: self.now,
            });
        }
        self.interaction.preview_bounds = None;
    }

    pub fn restore_panel_view(&mut self) {
        if let Some(panel) = self.render_state.panel_view.take() {
            self.camera = panel.camera;
        }
    }

    pub(super) fn fit_panel_view(&mut self) {
        if !self
            .interaction
            .dimension_edit
            .as_ref()
            .is_some_and(|e| e.touch_panel)
        {
            return;
        }
        let Some(panel) = self.panel_rect else {
            self.invalidate();
            return;
        };
        let (width, height) = (self.width, self.height);
        let left = panel.left;
        let top = panel.top;
        let key = [
            width.to_bits(),
            height.to_bits(),
            left.to_bits(),
            top.to_bits(),
            (panel.right - panel.left).to_bits(),
            (panel.bottom - panel.top).to_bits(),
        ];
        if self
            .render_state
            .panel_view
            .as_ref()
            .is_some_and(|p| p.key == key)
        {
            return;
        }
        let rectangles: Vec<Rect> = [
            Rect {
                left: 24.0,
                top: 64.0,
                right: width - 24.0,
                bottom: top - 16.0,
            },
            Rect {
                left: 24.0,
                top: 64.0,
                right: left - 16.0,
                bottom: height - 24.0,
            },
        ]
        .into_iter()
        .filter(|r| r.right - r.left > 32.0 && r.bottom - r.top > 32.0)
        .collect();
        if rectangles.is_empty() {
            return;
        }
        self.restore_panel_view();
        let active = self.active_geometry();
        let mut paths: Vec<(Vec<Point3>, bool)> = self
            .model
            .polygons
            .iter()
            .map(|p| (p.points.clone(), true))
            .collect();
        paths.push((active, false));
        let mut points: Vec<Point3> = Vec::new();
        for (path, closed) in &paths {
            if !path.is_empty() {
                points.extend(path.iter().copied());
                points.extend(self.dimension_bounds(path, *closed));
            }
        }
        let size = self.appearance.num("dimensionTextHeight");
        let text_gap = self.appearance.num("textGap");
        for record in self.current_dimensions() {
            let metrics = render::text_metrics(
                &format_building_length(self.measurement_length(&record)),
                &self.appearance,
            );
            let middle = midpoint(&record.line.line_start, &record.line.line_end);
            for direction in [-1.0, 1.0] {
                for x in [-metrics.width * size / 2.0, metrics.width * size / 2.0] {
                    for y in [text_gap, text_gap + (metrics.top - metrics.bottom) * size] {
                        points.push(render::label_point(&middle, record.axis(), x, y, direction));
                    }
                }
            }
        }
        if points.is_empty() {
            return;
        }
        let saved = self.camera;
        let mut center = [0.0; 3];
        for (axis, value) in center.iter_mut().enumerate() {
            let min = points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
            let max = points.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max);
            *value = (min + max) / 2.0;
        }
        let basis = camera_basis(self.camera.azimuth, self.camera.elevation);
        let slope_x = view_slope() * width / height;
        let mut best: Option<(f64, Point3)> = None;
        for rect in &rectangles {
            let cx = ((rect.left + rect.right) / width - 1.0) * slope_x;
            let cy = (1.0 - (rect.top + rect.bottom) / height) * view_slope();
            let sx = (rect.right - rect.left) / width * slope_x;
            let sy = (rect.bottom - rect.top) / height * view_slope();
            let mut distance = saved.distance;
            for p in &points {
                let q = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
                let depth = dot(&q, &basis.backward);
                distance = distance
                    .max(depth + 0.1)
                    .max(depth + (dot(&q, &basis.right) + cx * depth).abs() / sx)
                    .max(depth + (dot(&q, &basis.up) + cy * depth).abs() / sy);
            }
            distance *= 1.02;
            let target = [
                center[0] - cx * distance * basis.right[0] - cy * distance * basis.up[0],
                center[1] - cx * distance * basis.right[1] - cy * distance * basis.up[1],
                center[2] - cx * distance * basis.right[2] - cy * distance * basis.up[2],
            ];
            if best.as_ref().is_none_or(|(d, _)| distance < *d) {
                best = Some((distance, target));
            }
        }
        let Some((distance, target)) = best else {
            return;
        };
        self.render_state.panel_view = Some(PanelView { camera: saved, key });
        self.camera.target = target;
        self.camera.distance = distance;
    }
}
