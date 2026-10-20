//! 05- bis 11-Kapitel: Kamera-Einpassung, Zeichnen, Maßeingabe, Rahmenvorbereitung,
//! Zeiger- und Gestensteuerung sowie Größenwechsel.

use skizzeo_darstellung::camera::{
    camera_basis, dot, ground_point, project, view_projection, view_slope, Camera, Matrix,
    ViewSnapshot,
};
use skizzeo_darstellung::dimension_geometry::{
    self as dimgeo, DimensionLine, DimensionStroke, DimensionStyle, DimensionSymbol,
};
use skizzeo_darstellung::format::{
    format_building_length, format_length_input, parse_length_input, spoken_length,
};
use skizzeo_darstellung::render::{
    self, DrawList, FrameDimension, FrameSelected, Rect, RenderFrame, RenderOutput,
};
use skizzeo_darstellung::style::Appearance;
use skizzeo_darstellung::texte;
use skizzeo_kern::geometry::{is_clockwise, Geometry};
use skizzeo_kern::measurements::{ChainKind, Measurements};
use skizzeo_kern::model::{Model, PathKey};
use skizzeo_kern::units::building_length;
use skizzeo_kern::{KernError, Point3, SETTINGS};

use crate::state::*;
use crate::workflow::{transition, WorkflowEvent, WorkflowState};
use crate::INPUT;

/// Normalisierte Zeigerereignisse der Zeichenfläche.
#[derive(Clone, Copy, Debug)]
pub struct PointerEvent {
    pub id: i64,
    pub kind: PointerType,
    pub x: f64,
    pub y: f64,
    pub button: i16,
    pub buttons: u16,
}

pub struct Viewport {
    pub geometry: Geometry,
    pub measurements: Measurements,
    pub camera: Camera,
    pub model: Model,
    pub interaction: Interaction,
    pub fit: FitState,
    pub render_state: RenderState,
    pub workflow: WorkflowState,
    pub appearance: Appearance,
    /// Breite/Höhe der Zeichenfläche in CSS-Pixeln.
    pub width: f64,
    pub height: f64,
    /// Monotone Zeit in Millisekunden.
    pub now: f64,
    pub reduced_motion: bool,
    /// Gerät mit Touch und grobem primären Zeiger (für unbekannte Zeigerart).
    pub coarse_touch_device: bool,
    /// Aktueller Text des Längenfeldes (lengthInput.value).
    pub length_input: String,
    pub length_error: Option<String>,
    /// Sichtbarer Auswahl-Dialog bei überlagerten Maßen.
    pub dimension_choice: Option<DimensionChoice>,
    /// Maßdatensätze des letzten Frames nach Slot.
    pub frame_records: Vec<Record>,
    /// Hervorgehobene Maß-Trefferfläche (Hover/Fokus).
    pub highlighted_slot: Option<usize>,
    /// Bildschirmrechteck des Touch-Zahlenpanels im letzten Frame.
    pub panel_rect: Option<Rect>,
    /// Wiederholtes Zeichnen erforderlich (Animation).
    pub needs_repaint: bool,
    /// Fokuswunsch an die Oberfläche.
    pub focus_request: Option<Focus>,
    /// Achsbeschriftungen des letzten Frames.
    pub axis_labels: Vec<Option<(f64, f64)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Canvas,
    LengthInput,
    LengthDisplay,
    ChoiceFirst,
}

#[derive(Clone, Debug)]
pub struct DimensionChoice {
    pub options: Vec<(String, usize)>,
    pub left: f64,
    pub top: f64,
}

impl Viewport {
    pub fn new(appearance: Appearance) -> Self {
        let geometry = Geometry::new(&SETTINGS);
        let mut viewport = Self {
            geometry,
            measurements: Measurements::new(geometry),
            camera: Camera::default(),
            model: Model::default(),
            interaction: Interaction::default(),
            fit: FitState::default(),
            render_state: RenderState {
                first_frame: true,
                ..RenderState::default()
            },
            workflow: WorkflowState::View,
            appearance,
            width: 0.0,
            height: 0.0,
            now: 0.0,
            reduced_motion: false,
            coarse_touch_device: false,
            length_input: String::new(),
            length_error: None,
            dimension_choice: None,
            frame_records: Vec::new(),
            highlighted_slot: None,
            panel_rect: None,
            needs_repaint: true,
            focus_request: None,
            axis_labels: Vec::new(),
        };
        viewport.sync_default_axes();
        viewport
    }

    // ---------------------------------------------------------------- workflow

    fn transition_workflow(&mut self, event: WorkflowEvent) {
        match transition(self.workflow, event) {
            Ok(next) => self.workflow = next,
            Err(message) => log::error!("{message}"),
        }
    }

    pub fn is_drawing(&self) -> bool {
        self.workflow != WorkflowState::View
    }

    pub fn is_closing(&self) -> bool {
        self.workflow == WorkflowState::Close
    }

    pub fn interaction_phase(&self) -> &'static str {
        if self.is_closing() {
            return "CONFIRM_CLOSE";
        }
        if self.interaction.dimension_edit.is_some() {
            return "LENGTH";
        }
        if self.interaction.navigation {
            return "GESTURE";
        }
        match &self.interaction.drag {
            Some(Drag::Orbit { .. }) => "ORBIT",
            Some(Drag::Draw { .. }) => "DRAW",
            Some(Drag::Edit { .. }) => "EDIT",
            None if self.is_drawing() => "DRAW_READY",
            None => "VIEW_READY",
        }
    }

    pub fn invalidate(&mut self) {
        self.needs_repaint = true;
    }

    // ---------------------------------------------------------------- style adapters

    pub fn dimension_style(&self) -> DimensionStyle {
        let a = &self.appearance;
        DimensionStyle {
            dimension_offset: a.num("dimensionOffset"),
            dimension_spacing: a.num("dimensionSpacing"),
            extension_gap: a.num("extensionGap"),
            extension_overhang: a.num("extensionOverhang"),
            dimension_overhang: a.num("dimensionOverhang"),
            symbol: if a.text("symbol") == "circle" {
                DimensionSymbol::Circle
            } else {
                DimensionSymbol::Slash
            },
            tick_length: a.num("tickLength"),
        }
    }

    pub fn polygon_dimensions(&self, key: PathKey, path: &[Point3]) -> Vec<Record> {
        let style = self.dimension_style();
        self.measurements
            .polygon_measurements(path)
            .into_iter()
            .map(|chain| {
                let line = dimgeo::place_measurement(path, &chain, &style);
                Record {
                    key,
                    temp: None,
                    name: texte::chain_name(&chain),
                    kind: MeasureKind::Chain(chain),
                    line,
                    source: None,
                    follow_measure: false,
                }
            })
            .collect()
    }

    /// Maßlinie eines Segments außen; offene Züge werden im Uhrzeigersinn eingegeben.
    pub fn segment_dimension_line(&self, path: &[Point3], index: usize, closed: bool) -> DimensionLine {
        dimgeo::segment_dimension(
            path,
            index,
            self.geometry.segment_axis(path, index),
            &self.dimension_style(),
            !closed || is_clockwise(path),
        )
    }

    pub fn dimension_strokes(&self, line: &DimensionLine) -> Vec<DimensionStroke> {
        dimgeo::dimension_strokes(line, &self.dimension_style())
    }

    pub fn dimension_bounds(&self, path: &[Point3], closed: bool) -> Vec<Point3> {
        let lines: Vec<DimensionLine> = if closed {
            self.polygon_dimensions(PathKey::Temp, path)
                .into_iter()
                .map(|r| r.line)
                .collect()
        } else {
            (0..path.len().saturating_sub(1))
                .map(|index| self.segment_dimension_line(path, index, false))
                .collect()
        };
        lines
            .iter()
            .flat_map(|line| self.dimension_strokes(line))
            .flat_map(|stroke| [stroke.a, stroke.b])
            .collect()
    }

    pub fn chain_anchor(&self, record: &Record) -> Point3 {
        let fresh = record.points(&self.model).and_then(|path| {
            let key = &record.chain()?.key;
            self.polygon_dimensions(record.key, path)
                .into_iter()
                .find(|item| item.chain().is_some_and(|c| &c.key == key))
        });
        let line = fresh.map_or(record.line, |item| item.line);
        midpoint(&line.line_start, &line.line_end)
    }

    pub fn measurement_length(&self, record: &Record) -> f64 {
        let Some(path) = record.points(&self.model) else {
            return f64::NAN;
        };
        match &record.kind {
            MeasureKind::Chain(chain) => self.measurements.chain_length(path, chain),
            MeasureKind::Segment { index, .. } => self.geometry.segment_length(path, *index),
        }
    }

    // ---------------------------------------------------------------- camera

    pub fn matrix(&self) -> Matrix {
        view_projection(&self.camera, self.width, self.height)
    }

    pub fn project(&self, p: &Point3, matrix: &Matrix) -> Option<[f64; 2]> {
        project(p, matrix, self.width, self.height)
    }

    fn snapshot(&self) -> ViewSnapshot {
        ViewSnapshot {
            camera: self.camera,
            width: self.width,
            height: self.height,
        }
    }

    fn freeze_input_view(&mut self) {
        self.fit.frozen_input = Some(self.snapshot());
    }

    fn ground_point(&self, x: f64, y: f64) -> Option<Point3> {
        ground_point(x, y, &self.snapshot())
    }

    fn drawing_point(&mut self, x: f64, y: f64) -> Option<Point3> {
        if self.fit.animation.is_some() && self.fit.frozen_input.is_none() {
            self.freeze_input_view();
        }
        let view = self.fit.frozen_input.unwrap_or_else(|| self.snapshot());
        ground_point(x, y, &view)
    }

    fn advance_fit(&mut self) {
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
    fn active_geometry(&self) -> Vec<Point3> {
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

    fn fit_drawing(&mut self) {
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

    fn fit_panel_view(&mut self) {
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

    // ---------------------------------------------------------------- 06-drawing

    fn snap_draft(&mut self, candidate: Point3, position: Position) -> Option<Point3> {
        let last = *self.model.active.last()?;
        let deadband = if position.kind == PointerType::Touch {
            INPUT.touch_deadband
        } else {
            INPUT.mouse_deadband
        };
        if let Some(snap) = self.interaction.draft_snap {
            if (position.x - snap.x).hypot(position.y - snap.y) < deadband {
                return snap.point;
            }
        }
        let delta = [candidate[0] - last[0], candidate[1] - last[1]];
        let view = self.fit.frozen_input.unwrap_or_else(|| self.snapshot());
        let view_height = if view.height != 0.0 { view.height } else { self.height };
        let world_deadzone = view.camera.distance * 2.0 * view_slope() / view_height * deadband;
        if delta[0].hypot(delta[1]) < world_deadzone {
            self.interaction.draft_blocked = false;
            return None;
        }
        let draft = self.geometry.orthogonal_draft(
            &self.model.active,
            &candidate,
            self.interaction.draft_snap.map(|s| s.axis),
            SETTINGS.direction_hysteresis,
        );
        self.interaction.draft_blocked = draft.blocked;
        self.interaction.draft_snap = Some(DraftSnap {
            axis: draft.axis,
            x: position.x,
            y: position.y,
            point: draft.point,
        });
        draft.point
    }

    pub fn set_drawing(&mut self, enabled: bool) {
        self.transition_workflow(if enabled {
            WorkflowEvent::Start
        } else {
            WorkflowEvent::Stop
        });
        self.fit.path = None;
        self.fit.animation = None;
        self.interaction.start_hovered = false;
        self.fit.tight = false;
        self.interaction.draft_snap = None;
        self.interaction.draft_blocked = false;
        self.interaction.start_near = false;
        self.interaction.edge_selection = None;
        self.fit.frozen_input = None;
        self.fit.requested = enabled;
        if enabled {
            self.camera.elevation = self.camera.elevation.max(std::f64::consts::PI / 5.0);
        }
        self.interaction.hover_edge = None;
        // Erstes Segment im Uhrzeigersinn vorgegeben (per Maß änderbar).
        if enabled {
            self.model.start_path();
        } else {
            self.model.clear_active();
        }
        self.interaction.pointer = None;
        self.interaction.start_armed = false;
        self.interaction.preview_bounds = None;
        self.interaction.live_point = if enabled { self.default_draft() } else { None };
        self.invalidate();
    }

    fn default_draft(&self) -> Option<Point3> {
        self.geometry.next_draft(&self.model.active)
    }

    fn finish_path(&mut self, x: f64, y: f64, radius: f64) -> bool {
        if self.model.active.len() < 3 {
            return false;
        }
        let matrix = self.matrix();
        let Some(start) = self.project(&self.model.active[0], &matrix) else {
            return false;
        };
        if (x - start[0]).hypot(y - start[1]) > radius {
            return false;
        }
        if !self.model.can_close(&self.geometry) {
            return false;
        }
        self.request_close();
        true
    }

    pub fn request_close(&mut self) {
        if !self.is_drawing() || !self.model.can_close(&self.geometry) {
            return;
        }
        self.close_length_editor(false);
        self.cancel_interactions(false);
        self.transition_workflow(WorkflowEvent::RequestClose);
        self.interaction.start_near = true;
        self.interaction.live_point = None;
        self.fit.requested = true;
        self.invalidate();
    }

    pub fn dismiss_close(&mut self) {
        self.transition_workflow(WorkflowEvent::Resume);
        self.interaction.start_near = false;
        self.interaction.start_armed = false;
        self.interaction.pointer = None;
        self.interaction.draft_snap = None;
        self.interaction.live_point = self.default_draft();
        self.invalidate();
    }

    pub fn complete_polygon(&mut self) {
        if !self.is_closing() {
            return;
        }
        let Some(closure) = self.model.close_active(&self.geometry) else {
            self.dismiss_close();
            return;
        };
        let id = closure.polygon;
        self.interaction.last_closure = Some(LastClosure {
            path: closure.path,
            polygon: id,
        });
        self.set_drawing(false);
        self.request_fit(Some(PathKey::Polygon(id)), true);
    }

    pub fn undo_point(&mut self) {
        let pending_draft = self
            .interaction
            .dimension_edit
            .as_ref()
            .is_some_and(|e| e.record.is_preview());
        self.close_length_editor(false);
        self.cancel_interactions(false);
        self.transition_workflow(WorkflowEvent::Resume);
        self.interaction.start_near = false;
        let reopen = !self.is_drawing()
            && self
                .interaction
                .last_closure
                .as_ref()
                .is_some_and(|c| self.model.has_polygon(c.polygon));
        if reopen {
            let saved = self.interaction.last_closure.take().expect("letzte Schließung");
            self.model.remove_polygon(saved.polygon);
            self.set_drawing(true);
            self.model.replace_active(saved.path);
            self.interaction.last_closure = None;
        } else if self.is_drawing() && !pending_draft {
            self.model.pop_point();
        }
        self.interaction.draft_snap = None;
        self.interaction.draft_blocked = false;
        self.interaction.start_armed = false;
        self.interaction.live_point = if self.is_drawing() {
            self.default_draft()
        } else {
            None
        };
        if self.is_drawing() {
            self.request_fit(Some(self.model.active_key()), true);
        }
        self.invalidate();
    }

    fn add_point(&mut self, position: Position) {
        if !self.is_drawing() {
            return;
        }
        let radius = if position.kind == PointerType::Touch {
            INPUT.touch_close_radius
        } else {
            INPUT.mouse_close_radius
        };
        if self.finish_path(position.x, position.y, radius) {
            return;
        }
        let Some(candidate) = self.drawing_point(position.x, position.y) else {
            return;
        };
        let Some(&last) = self.model.active.last() else {
            return;
        };
        let Some(next) = self.snap_draft(candidate, position) else {
            self.invalidate();
            return;
        };
        let matrix = self.matrix();
        let a = self.project(&last, &matrix);
        let b = self.project(&next, &matrix);
        let (Some(a), Some(b)) = (a, b) else { return };
        if self.geometry.same_point(&last, &next) || (a[0] - b[0]).hypot(a[1] - b[1]) < 2.0 {
            return;
        }
        let path = vec![last, next];
        let line = self.segment_dimension_line(&path, 0, false);
        let record = Record {
            key: PathKey::Temp,
            temp: Some(path),
            kind: MeasureKind::Segment {
                index: 0,
                closed: false,
                preview: true,
            },
            line,
            name: String::new(),
            source: Some(self.model.active_key()),
            follow_measure: false,
        };
        self.open_length_editor(record, Some((b[0], b[1])));
        self.invalidate();
    }

    pub fn update_live_point(&mut self, allow_close: bool, auto_fit: bool) {
        if auto_fit {
            self.fit.requested = true;
            if self.is_drawing() {
                self.fit.path = Some(self.model.active_key());
            }
        }
        if !self.is_drawing() || self.model.active.is_empty() {
            self.interaction.live_point = None;
            return;
        }
        if self.is_closing() {
            self.interaction.live_point = None;
            return;
        }
        if let Some(edit) = &self.interaction.dimension_edit {
            if edit.record.is_preview() {
                self.interaction.live_point = edit.record.temp.as_ref().map(|p| p[1]);
            }
            return;
        }
        if self.interaction.navigation
            || self.interaction.drag.as_ref().is_some_and(|d| !d.is_draw())
        {
            self.interaction.live_point = None;
            return;
        }
        let Some(pointer) = self.interaction.pointer else {
            if self.interaction.live_point.is_none() {
                self.interaction.live_point = self.default_draft();
            }
            return;
        };
        let matrix = self.matrix();
        let distance = self
            .project(&self.model.active[0], &matrix)
            .map_or(f64::INFINITY, |s| (pointer.x - s[0]).hypot(pointer.y - s[1]));
        let radius = if pointer.kind == PointerType::Touch {
            INPUT.touch_close_radius
        } else {
            INPUT.mouse_close_radius
        };
        self.interaction.start_near = (self.interaction.start_hovered || distance <= radius)
            && self.model.can_close(&self.geometry);
        if distance > radius + 12.0 {
            self.interaction.start_armed = true;
        }
        if allow_close
            && self.interaction.start_armed
            && self.interaction.start_near
            && self.finish_path(pointer.x, pointer.y, radius)
        {
            return;
        }
        if self.interaction.start_near {
            self.interaction.live_point = None;
            return;
        }
        if self.interaction.drag.is_none() && pointer.kind != PointerType::Touch {
            if let (Some(bounds), Some(_)) =
                (self.interaction.preview_bounds, self.interaction.live_point)
            {
                if (pointer.x - bounds.x).abs() < bounds.half_width + 14.0
                    && (pointer.y - bounds.y).abs() < 36.0
                {
                    return;
                }
            }
        }
        if let Some(candidate) = self.drawing_point(pointer.x, pointer.y) {
            match self.snap_draft(candidate, pointer) {
                None => {
                    self.interaction.live_point = self.model.active.last().copied();
                    return;
                }
                Some(next) => {
                    let last = self.model.active[self.model.active.len() - 1];
                    if !self.geometry.same_point(&next, &last) {
                        self.interaction.live_point = Some(next);
                    }
                }
            }
        }
        if self.interaction.live_point.is_none() {
            self.interaction.live_point = self.default_draft();
        }
    }

    // ---------------------------------------------------------------- 07-measurements

    /// Linke obere Ecke des nativen Längeneditors (`positionLengthEditor`).
    pub fn length_editor_position(&self) -> Option<(f64, f64)> {
        let edit = self.interaction.dimension_edit.as_ref()?;
        if edit.touch_panel {
            return None;
        }
        let left = 8f64.max((self.width - 192.0).min(edit.x - 92.0));
        let top = 8f64.max((self.height - 120.0).min(edit.y - 24.0));
        Some((left, top))
    }

    fn clear_length_error(&mut self) {
        self.length_error = None;
    }

    pub fn enter_length_key(&mut self, key: &str) {
        let Some(edit) = self.interaction.dimension_edit.as_mut() else {
            return;
        };
        if !edit.touch_panel {
            return;
        }
        let mut value = if edit.replace_input {
            String::new()
        } else {
            self.length_input.replace('⁵', "5")
        };
        if key == "delete" {
            value.pop();
        } else if key == "," {
            if !value.contains(['.', ',']) {
                if value.is_empty() {
                    value.push('0');
                }
                value.push(',');
            }
        } else if key.len() == 1 && key.as_bytes()[0].is_ascii_digit() {
            value.push_str(key);
        } else {
            return;
        }
        edit.replace_input = false;
        self.length_input = value;
        self.clear_length_error();
    }

    /// Anzeige der Touch-Wertausgabe.
    pub fn length_display(&self) -> String {
        if self.length_input.is_empty() {
            "0".into()
        } else {
            self.length_input.clone()
        }
    }

    pub fn submit_length(&mut self) {
        if self.commit_length() {
            self.focus_request = Some(Focus::Canvas);
        }
    }

    pub fn close_length_editor(&mut self, restore_draft: bool) {
        self.restore_panel_view();
        if self
            .interaction
            .dimension_edit
            .as_ref()
            .is_some_and(|e| e.touch_panel)
        {
            self.fit.requested = false;
            self.fit.animation = None;
        }
        if restore_draft {
            if let Some(edit) = &self.interaction.dimension_edit {
                if edit.record.is_preview() {
                    self.interaction.live_point = edit.record.temp.as_ref().map(|p| p[1]);
                    self.interaction.pointer = None;
                }
            }
        }
        self.interaction.dimension_edit = None;
        self.panel_rect = None;
        self.clear_length_error();
        self.invalidate();
    }

    pub fn commit_length(&mut self) -> bool {
        let Some(edit) = self.interaction.dimension_edit.clone() else {
            return true;
        };
        let preview = edit.record.is_preview();
        let preserve_view = !preview && edit.touch_panel;
        if !preview && self.length_input == edit.initial_input {
            self.close_length_editor(false);
            return true;
        }
        match self.compute_length_commit(&edit) {
            Ok(next) => {
                if preview {
                    self.model.push_point(next[1]);
                    self.interaction.draft_snap = None;
                    self.interaction.draft_blocked = false;
                    self.interaction.start_near = false;
                } else {
                    self.model.set_path(edit.record.key, next.clone());
                }
                if self.is_drawing() {
                    self.interaction.pointer = None;
                    self.interaction.live_point = self.default_draft();
                    self.interaction.preview_bounds = None;
                }
                self.close_length_editor(false);
                if preview
                    && self.model.active.len() >= 4
                    && self.geometry.same_point(&next[1], &self.model.active[0])
                {
                    self.request_close();
                }
                self.update_live_point(false, false);
                if !preserve_view {
                    let key = if preview {
                        self.model.active_key()
                    } else {
                        edit.record.key
                    };
                    self.request_fit(Some(key), true);
                }
                true
            }
            Err(error) => {
                self.length_error = Some(error.message().to_string());
                self.invalidate();
                false
            }
        }
    }

    fn compute_length_commit(&self, edit: &DimensionEdit) -> Result<Vec<Point3>, LengthError> {
        let raw = parse_length_input(&self.length_input).ok_or(LengthError::Number)?;
        let length = building_length(self.geometry.positive_length(raw)?)?;
        let path = edit
            .record
            .points(&self.model)
            .ok_or(LengthError::Inactive)?
            .to_vec();
        let edge_control = edit
            .edge_control
            .as_ref()
            .filter(|c| edit.record.same_path(c.key))
            .map(|c| &c.sel);
        let next = match &edit.record.kind {
            MeasureKind::Chain(chain) => {
                self.measurements
                    .resize_chain(&path, chain, length, edge_control)?
            }
            MeasureKind::Segment { index, closed, .. } => match &edit.edge_control {
                Some(control) => {
                    let current = self
                        .model
                        .path(control.key)
                        .cloned()
                        .unwrap_or_else(|| path.clone());
                    self.geometry
                        .resize_from_edge(&current, &control.sel, *index, length)?
                }
                None => self.geometry.resized_segment(&path, *index, length, *closed)?,
            },
        };
        if edit.record.is_preview() {
            if !self.is_drawing() || edit.record.source != Some(self.model.active_key()) {
                return Err(LengthError::Inactive);
            }
            if self.geometry.overlaps_path(&self.model.active, &next[1]) {
                return Err(KernError::RetracesPath.into());
            }
        }
        Ok(next)
    }

    pub fn open_length_editor(&mut self, record: Record, anchor: Option<(f64, f64)>) {
        if !record.is_preview() && record.is_chain() {
            self.interaction.last_closure = None;
        }
        if let MeasureKind::Segment {
            closed: true,
            preview: false,
            ..
        } = record.kind
        {
            self.interaction.last_closure = None;
        }
        if self.interaction.dimension_edit.is_some() && !self.commit_length() {
            return;
        }
        self.cancel_interactions(true);
        let edge_control = self.interaction.edge_selection.clone().filter(|selection| {
            record.same_path(selection.key)
                && match &record.kind {
                    MeasureKind::Chain(chain) => {
                        record.points(&self.model).is_some_and(|path| {
                            self.measurements.chain_rate(path, chain, Some(&selection.sel)) != 0.0
                        })
                    }
                    MeasureKind::Segment { index, .. } => selection.sel.affects(*index),
                }
        });
        if edge_control.is_none() {
            self.interaction.edge_selection = None;
        }
        let (x, y) = anchor.unwrap_or((0.0, 0.0));
        let touch_panel = self.interaction.input_type == Some(PointerType::Touch)
            || (self.interaction.input_type.is_none() && self.coarse_touch_device);
        let preview = record.is_preview();
        let mut edit = DimensionEdit {
            record,
            touch_panel,
            replace_input: true,
            edge_control,
            x,
            y,
            initial_input: String::new(),
        };
        self.length_input = format_length_input(self.measurement_length(&edit.record));
        edit.initial_input = self.length_input.clone();
        let record_key = edit.record.key;
        self.interaction.dimension_edit = Some(edit);
        self.clear_length_error();
        if touch_panel {
            self.fit.requested = false;
            self.focus_request = Some(Focus::LengthDisplay);
        } else {
            self.focus_request = Some(Focus::LengthInput);
            self.request_fit(
                Some(if preview {
                    self.model.active_key()
                } else {
                    record_key
                }),
                false,
            );
        }
        self.invalidate();
    }

    // ---------------------------------------------------------------- 07-settings (Kamera-Teil)

    pub fn apply_style(&mut self, next: Appearance) {
        self.restore_panel_view();
        let axes_changed = next.text("axesDefault") != self.appearance.text("axesDefault");
        self.appearance = next;
        if axes_changed {
            self.sync_default_axes();
        }
        if !self
            .interaction
            .dimension_edit
            .as_ref()
            .is_some_and(|e| e.touch_panel)
        {
            self.request_fit(None, true);
        }
        self.invalidate();
    }

    pub fn sync_default_axes(&mut self) {
        self.render_state.axes_visible = self.appearance.text("axesDefault") == "on";
    }

    // ---------------------------------------------------------------- 07-frame

    pub fn current_dimensions(&self) -> Vec<Record> {
        let mut records = Vec::new();
        for (p, polygon) in self.model.polygons.iter().enumerate() {
            for mut record in self.polygon_dimensions(PathKey::Polygon(polygon.id), &polygon.points)
            {
                record.name = format!("Polygon {}, {}", p + 1, record.name);
                records.push(record);
            }
        }
        let active = &self.model.active;
        for index in 0..active.len().saturating_sub(1) {
            records.push(Record {
                key: self.model.active_key(),
                temp: None,
                kind: MeasureKind::Segment {
                    index,
                    closed: false,
                    preview: false,
                },
                line: self.segment_dimension_line(active, index, false),
                name: format!("Aktueller Zug, Segment {}", index + 1),
                source: None,
                follow_measure: false,
            });
        }
        if let (Some(&last), Some(live)) = (active.last(), self.interaction.live_point) {
            if !self.geometry.same_point(&last, &live) {
                let path = vec![last, live];
                records.push(Record {
                    key: PathKey::Temp,
                    line: self.segment_dimension_line(&path, 0, false),
                    temp: Some(path),
                    kind: MeasureKind::Segment {
                        index: 0,
                        closed: false,
                        preview: true,
                    },
                    name: format!("Laufendes Segment {}", active.len()),
                    source: Some(self.model.active_key()),
                    follow_measure: false,
                });
            }
        }
        records
    }

    fn measurement_affected(&self, record: &Record) -> bool {
        let Some(selected) = &self.interaction.edge_selection else {
            return false;
        };
        if !record.same_path(selected.key) {
            return false;
        }
        let moving = match &self.interaction.drag {
            Some(Drag::Edit { edge, original, .. }) => self.model.path(edge.key).is_some_and(|path| {
                path.iter()
                    .zip(original)
                    .any(|(p, o)| !self.geometry.same_point(p, o))
            }),
            _ => false,
        };
        if !moving {
            return false;
        }
        let Some(chain) = record.chain() else {
            return record
                .segment_index()
                .is_some_and(|index| selected.sel.affects(index));
        };
        let Some(path) = record.points(&self.model) else {
            return false;
        };
        if self.measurements.chain_rate(path, chain, Some(&selected.sel)) != 0.0 {
            return true;
        }
        let original = match &self.interaction.drag {
            Some(Drag::Edit { original, .. }) => original,
            _ => &selected.sel.original,
        };
        let before = if chain.kind == ChainKind::Total {
            let values = original.iter().map(|p| p[chain.axis]);
            values.clone().fold(f64::NEG_INFINITY, f64::max) - values.fold(f64::INFINITY, f64::min)
        } else {
            (original[chain.to][chain.axis] - original[chain.from][chain.axis]).abs()
        };
        (before - self.measurement_length(record)).abs() > SETTINGS.geometry_epsilon
    }

    fn prepare_render_frame(&self) -> (RenderFrame, Vec<Record>) {
        let mut records = self.current_dimensions();
        let lengths: Vec<f64> = records.iter().map(|r| self.measurement_length(r)).collect();
        let mut order: Vec<usize> = (0..records.len()).collect();
        order.sort_by(|a, b| {
            (lengths[*a] - lengths[*b])
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut sorted = Vec::with_capacity(records.len());
        let mut taken: Vec<Option<Record>> = records.drain(..).map(Some).collect();
        for index in order {
            sorted.push(taken[index].take().expect("Datensatz"));
        }
        let records = sorted;
        let edit = self.interaction.dimension_edit.as_ref();
        let dimensions = records
            .iter()
            .enumerate()
            .map(|(slot, record)| {
                let editing = edit.is_some_and(|edit| {
                    record.same_path(edit.record.key)
                        && edit.record.temp.is_none()
                        && match (&record.kind, &edit.record.kind) {
                            (MeasureKind::Chain(a), MeasureKind::Chain(b)) => a.key == b.key,
                            (MeasureKind::Chain(_), _) => false,
                            (MeasureKind::Segment { index, .. }, kind) => match kind {
                                MeasureKind::Segment { index: other, .. } => index == other,
                                MeasureKind::Chain(_) => false,
                            },
                        }
                });
                let length = self.measurement_length(record);
                FrameDimension {
                    slot,
                    axis: record.axis(),
                    line_start: record.line.line_start,
                    line_end: record.line.line_end,
                    length,
                    text: format_building_length(length),
                    affected: self.measurement_affected(record),
                    preview: record.is_preview(),
                    hidden: editing
                        || (record.is_preview() && edit.is_some_and(|e| e.record.is_preview())),
                    highlighted: self.highlighted_slot == Some(slot),
                    strokes: self.dimension_strokes(&record.line),
                }
            })
            .collect();
        let selected = self.selected_segment();
        let close_tail = if self.interaction.start_near || self.is_closing() {
            self.geometry
                .close_path(&self.model.active)
                .map(|closed| {
                    let mut tail = vec![*self.model.active.last().expect("Zug")];
                    tail.extend_from_slice(&closed[self.model.active.len().min(closed.len())..]);
                    tail.push(closed[0]);
                    tail
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let frame = RenderFrame {
            width: self.width,
            height: self.height,
            matrix: self.matrix(),
            style: self.appearance.clone(),
            elevation: self.camera.elevation,
            axes_visible: self.render_state.axes_visible,
            polygons: self.model.polygons.iter().map(|p| p.points.clone()).collect(),
            active: self.model.active.clone(),
            live_point: self.interaction.live_point,
            close_tail,
            selected,
            dimensions,
        };
        (frame, records)
    }

    fn selected_segment(&self) -> Option<FrameSelected> {
        let segment = |key: PathKey, temp: Option<&Vec<Point3>>, index: usize, hover: bool| {
            let path = match temp {
                Some(points) => points,
                None => self.model.path(key)?,
            };
            if path.is_empty() {
                return None;
            }
            Some(FrameSelected {
                a: path[index % path.len()],
                b: path[(index + 1) % path.len()],
                hover,
            })
        };
        if let Some(selection) = &self.interaction.edge_selection {
            return segment(selection.key, None, selection.sel.index, false);
        }
        if let Some(edit) = &self.interaction.dimension_edit {
            if let Some(index) = edit.record.segment_index() {
                return segment(edit.record.key, edit.record.temp.as_ref(), index, false);
            }
        }
        if let Some(Drag::Edit { edge, .. }) = &self.interaction.drag {
            return segment(edge.key, None, edge.index, Some(*edge) == self.interaction.hover_edge);
        }
        let edge = self.interaction.hover_edge?;
        segment(edge.key, None, edge.index, true)
    }

    pub fn dismiss_dimension_choice(&mut self) {
        self.dimension_choice = None;
    }

    /// Auswahl beim Antippen einer Maßzahl (`selectDimensionAt`).
    pub fn select_dimension_at(&mut self, slot: usize, x: f64, y: f64) {
        let candidates: Vec<usize> = self
            .render_state
            .dimension_layouts
            .iter()
            .filter(|layout| layout.hit_box.contains(x, y))
            .map(|layout| layout.slot)
            .filter(|slot| *slot < self.frame_records.len())
            .collect();
        if candidates.len() <= 1 {
            self.open_dimension_slot(slot);
            return;
        }
        let options = candidates
            .into_iter()
            .map(|slot| (self.dimension_title(slot), slot))
            .collect();
        self.dimension_choice = Some(DimensionChoice {
            options,
            left: 8f64.max((self.width - 328.0).min(x - 160.0)),
            top: 8.0,
        });
        self.focus_request = Some(Focus::ChoiceFirst);
    }

    pub fn dimension_title(&self, slot: usize) -> String {
        let record = &self.frame_records[slot];
        format!(
            "{}: {} m – Länge in Metern ändern",
            record.name,
            spoken_length(self.measurement_length(record))
        )
    }

    pub fn open_dimension_slot(&mut self, slot: usize) {
        let Some(record) = self.frame_records.get(slot).cloned() else {
            return;
        };
        let anchor = self
            .render_state
            .dimension_layouts
            .iter()
            .find(|l| l.slot == slot)
            .map(|l| ((l.box_.left + l.box_.right) / 2.0, (l.box_.top + l.box_.bottom) / 2.0));
        self.dimension_choice = None;
        self.open_length_editor(record, anchor);
    }

    /// Ein Frame: Kameraanimation, Entwurf, Einpassung, Darstellung.
    ///
    /// Liefert die Zeichenaufrufe; `pixels_per_point` ist die Pixeldichte der Ausgabe.
    pub fn render_frame(&mut self, pixels_per_point: f64) -> DrawList {
        if self.width == 0.0 || self.height == 0.0 {
            return DrawList::default();
        }
        self.needs_repaint = false;
        let editing_viewport = self
            .interaction
            .dimension_edit
            .as_ref()
            .is_some_and(|e| e.touch_panel);
        if !editing_viewport {
            self.advance_fit();
        }
        self.update_live_point(false, false);
        self.fit_panel_view();
        if self.fit.requested && !editing_viewport {
            self.fit.requested = false;
            self.fit_drawing();
        }
        let (frame, records) = self.prepare_render_frame();
        if let Some(edit) = &self.interaction.dimension_edit {
            if edit.record.is_preview() || edit.record.follow_measure {
                let anchor = if edit.record.is_preview() {
                    edit.record.temp.as_ref().map(|p| p[1])
                } else if edit.record.is_chain() {
                    Some(self.chain_anchor(&edit.record))
                } else {
                    edit.record.points(&self.model).map(|path| {
                        self.geometry
                            .segment_middle(path, edit.record.segment_index().unwrap_or(0))
                    })
                };
                if let Some(end) = anchor.and_then(|a| self.project(&a, &frame.matrix)) {
                    if let Some(edit) = self.interaction.dimension_edit.as_mut() {
                        edit.x = end[0];
                        edit.y = end[1];
                    }
                }
            }
        }
        let (list, output) = render::render(&frame, pixels_per_point);
        let RenderOutput {
            dimension_layouts,
            preview_bounds,
            axis_labels,
        } = output;
        self.axis_labels = axis_labels;
        self.frame_records = records;
        self.interaction.preview_bounds = preview_bounds;
        self.render_state.dimension_layouts = dimension_layouts;
        if self
            .highlighted_slot
            .is_some_and(|slot| slot >= self.frame_records.len())
        {
            self.highlighted_slot = None;
        }
        self.render_state.first_frame = false;
        if !editing_viewport && (self.fit.animation.is_some() || self.fit.requested) {
            self.invalidate();
        }
        list
    }

    /// Sichtbarkeit und Lage der Eingabe-Bedienelemente (`drawInputControls`).
    pub fn start_target(&self) -> Option<[f64; 2]> {
        if !self.is_drawing() || !self.model.can_close(&self.geometry) {
            return None;
        }
        if self.interaction.dimension_edit.is_some() || self.interaction.navigation {
            return None;
        }
        self.project(&self.model.active[0], &self.matrix())
    }

    pub fn undo_visible(&self) -> bool {
        self.is_drawing() || self.interaction.last_closure.is_some()
    }

    pub fn undo_disabled(&self) -> bool {
        self.interaction.last_closure.is_none()
            && self.model.active.len() <= 1
            && !self
                .interaction
                .dimension_edit
                .as_ref()
                .is_some_and(|e| e.record.is_preview())
    }

    /// Hinweis zum Uhrzeigersinn zu Beginn der Polygoneingabe.
    pub fn direction_hint_visible(&self) -> bool {
        self.is_drawing()
            && !self.is_closing()
            && !self.interaction.draft_blocked
            && self.model.active.len() <= 2
    }

    pub fn hint_visible(&self) -> bool {
        self.is_drawing() && self.interaction.draft_blocked && !self.is_closing()
    }

    // ---------------------------------------------------------------- 10-input

    fn hit_segment(&self, x: f64, y: f64, radius: f64) -> Option<Edge> {
        let matrix = self.matrix();
        let mut best = None;
        let mut distance = radius;
        for polygon in &self.model.polygons {
            let path = &polygon.points;
            for i in 0..path.len() {
                let a = self.project(&path[i], &matrix);
                let b = self.project(&path[(i + 1) % path.len()], &matrix);
                let (Some(a), Some(b)) = (a, b) else { continue };
                let dx = b[0] - a[0];
                let dy = b[1] - a[1];
                let length2 = dx * dx + dy * dy;
                if length2 < 1e-6 {
                    continue;
                }
                let t = (((x - a[0]) * dx + (y - a[1]) * dy) / length2).clamp(0.0, 1.0);
                let d = (x - a[0] - t * dx).hypot(y - a[1] - t * dy);
                if d < distance {
                    distance = d;
                    best = Some(Edge {
                        key: PathKey::Polygon(polygon.id),
                        index: i,
                    });
                }
            }
        }
        best
    }

    fn restore_edit(&mut self) {
        if let Some(Drag::Edit { edge, original, .. }) = &self.interaction.drag {
            let (key, original) = (edge.key, original.clone());
            self.model.set_path(key, original);
        }
    }

    pub fn cancel_interactions(&mut self, preserve_selection: bool) {
        self.dismiss_dimension_choice();
        self.fit.animation = None;
        self.interaction.start_hovered = false;
        self.restore_edit();
        if !preserve_selection {
            self.interaction.edge_selection = None;
        }
        self.interaction.drag = None;
        self.interaction.gesture = None;
        self.interaction.navigation = false;
        self.interaction.pointers.clear();
        self.interaction.pointer = None;
        self.interaction.live_point = None;
        self.interaction.hover_edge = None;
        self.fit.frozen_input = None;
        self.interaction.draft_snap = None;
        self.interaction.draft_blocked = false;
        self.interaction.start_near = false;
        self.invalidate();
    }

    fn start_gesture(&mut self) {
        self.fit.animation = None;
        self.fit.requested = false;
        self.interaction.edge_selection = None;
        self.fit.frozen_input = None;
        self.interaction.draft_snap = None;
        self.interaction.draft_blocked = false;
        self.interaction.start_near = false;
        self.restore_edit();
        self.interaction.drag = None;
        self.interaction.live_point = None;
        self.interaction.hover_edge = None;
        self.interaction.navigation = true;
        let contacts: Vec<(i64, Position)> = self
            .interaction
            .pointers
            .iter()
            .filter(|(_, p)| p.kind == PointerType::Touch)
            .copied()
            .collect();
        if contacts.len() < 2 {
            self.interaction.gesture = None;
            return;
        }
        let (id_a, a) = contacts[0];
        let (id_b, b) = contacts[1];
        self.interaction.gesture = Some(Gesture {
            ids: [id_a, id_b],
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
            separation: 10f64.max((a.x - b.x).hypot(a.y - b.y)),
            camera: self.camera,
        });
    }

    fn move_gesture(&mut self) {
        let Some(gesture) = self.interaction.gesture.clone() else {
            return;
        };
        let a = self.interaction.get_pointer(gesture.ids[0]);
        let b = self.interaction.get_pointer(gesture.ids[1]);
        let (Some(a), Some(b)) = (a, b) else {
            self.start_gesture();
            return;
        };
        let limit = std::f64::consts::FRAC_PI_2 - 0.015;
        self.camera.azimuth = gesture.camera.azimuth
            - ((a.x + b.x) / 2.0 - gesture.x) * INPUT.orbit_sensitivity;
        self.camera.elevation = (gesture.camera.elevation
            + ((a.y + b.y) / 2.0 - gesture.y) * INPUT.orbit_sensitivity)
            .min(limit)
            .max(-limit);
        self.camera.distance = (gesture.camera.distance * gesture.separation
            / 10f64.max((a.x - b.x).hypot(a.y - b.y)))
        .min(150f64.max(gesture.camera.distance * 10.0))
        .max(1.0);
    }

    pub fn note_input_type(&mut self, kind: PointerType) {
        self.interaction.input_type = Some(kind);
    }

    pub fn on_pointer_down(&mut self, event: PointerEvent) {
        self.dismiss_dimension_choice();
        if self.is_closing() {
            self.dismiss_close();
            return;
        }
        if self.interaction.dimension_edit.is_some() {
            self.submit_length();
            return;
        }
        if event.kind != PointerType::Touch && event.button != 0 && event.button != 1 {
            return;
        }
        self.fit.animation = None;
        self.focus_request = Some(Focus::Canvas);
        let position = Position {
            x: event.x,
            y: event.y,
            kind: event.kind,
        };
        self.interaction.pointer = Some(position);
        self.interaction.set_pointer(event.id, position);
        let touches = self
            .interaction
            .pointers
            .iter()
            .filter(|(_, p)| p.kind == PointerType::Touch)
            .count();
        if touches >= 2 {
            self.start_gesture();
            self.invalidate();
            return;
        }
        if self.interaction.navigation || self.interaction.drag.is_some() {
            return;
        }
        if event.button == 1 && event.kind != PointerType::Touch {
            self.fit.requested = false;
            self.interaction.edge_selection = None;
            self.fit.frozen_input = None;
            self.interaction.drag = Some(Drag::Orbit {
                id: event.id,
                x: position.x,
                y: position.y,
            });
            self.interaction.live_point = None;
            self.interaction.hover_edge = None;
        } else if self.is_drawing() {
            self.interaction.draft_snap = None;
            self.interaction.draft_blocked = false;
            self.interaction.drag = Some(Drag::Draw { id: event.id });
            self.update_live_point(false, true);
        } else {
            let radius = if position.kind == PointerType::Touch { 26.0 } else { 10.0 };
            let edge = self.hit_segment(position.x, position.y, radius);
            self.interaction.edge_selection = edge.and_then(|edge| {
                self.model.path(edge.key).map(|path| EdgeSel {
                    key: edge.key,
                    sel: self.geometry.select_edge(path, edge.index),
                })
            });
            let hit = edge.and_then(|_| self.ground_point(position.x, position.y));
            if let (Some(edge), Some(hit)) = (edge, hit) {
                self.interaction.last_closure = None;
                let axis = self
                    .interaction
                    .edge_selection
                    .as_ref()
                    .map_or(0, |s| s.sel.axis);
                let original = self.model.path(edge.key).cloned().unwrap_or_default();
                self.interaction.drag = Some(Drag::Edit {
                    id: event.id,
                    edge,
                    hit,
                    axis,
                    original,
                });
                self.interaction.hover_edge = Some(edge);
            }
        }
        self.invalidate();
    }

    pub fn on_pointer_move(&mut self, event: PointerEvent) {
        let phase = self.interaction_phase();
        if phase == "LENGTH" || phase == "CONFIRM_CLOSE" {
            return;
        }
        let position = Position {
            x: event.x,
            y: event.y,
            kind: event.kind,
        };
        self.interaction.pointer = Some(position);
        if self.interaction.has_pointer(event.id) {
            self.interaction.set_pointer(event.id, position);
        }
        if self.interaction.navigation {
            self.move_gesture();
            self.invalidate();
            return;
        }
        let drag_id = self.interaction.drag.as_ref().map(Drag::id);
        if drag_id == Some(event.id) {
            match self.interaction.drag.clone() {
                Some(Drag::Orbit { id, x, y }) => {
                    let limit = std::f64::consts::FRAC_PI_2 - 0.015;
                    self.camera.azimuth -= (position.x - x) * INPUT.orbit_sensitivity;
                    self.camera.elevation = (self.camera.elevation
                        + (position.y - y) * INPUT.orbit_sensitivity)
                        .min(limit)
                        .max(-limit);
                    self.interaction.drag = Some(Drag::Orbit {
                        id,
                        x: position.x,
                        y: position.y,
                    });
                }
                Some(Drag::Edit {
                    edge,
                    hit: start,
                    axis,
                    original,
                    ..
                }) => {
                    if let Some(hit) = self.ground_point(position.x, position.y) {
                        let delta = hit[axis] - start[axis];
                        let updated = self.geometry.shifted_segment(&original, edge.index, delta, true);
                        self.model.set_path(edge.key, updated);
                    }
                }
                _ => self.update_live_point(false, true),
            }
        } else if self.interaction.pointers.is_empty() {
            if self.is_drawing() {
                self.update_live_point(position.kind != PointerType::Touch && event.buttons == 0, true);
            }
            self.interaction.hover_edge = if self.is_drawing() {
                None
            } else {
                self.hit_segment(position.x, position.y, 9.0)
            };
        }
        self.invalidate();
    }

    pub fn release_pointer(&mut self, event: PointerEvent, cancelled: bool) {
        if !self.interaction.has_pointer(event.id) {
            return;
        }
        let position = Position {
            x: event.x,
            y: event.y,
            kind: event.kind,
        };
        self.interaction.remove_pointer(event.id);
        if self.interaction.navigation {
            if self.interaction.pointers.is_empty() {
                self.interaction.navigation = false;
                self.interaction.gesture = None;
            } else {
                self.start_gesture();
            }
        } else if self.interaction.drag.as_ref().map(Drag::id) == Some(event.id) {
            let action = self.interaction.drag.clone().expect("Drag");
            if cancelled {
                self.restore_edit();
                self.interaction.edge_selection = None;
            }
            self.interaction.drag = None;
            if !cancelled && action.is_draw() {
                self.add_point(position);
            }
            if let Drag::Edit {
                edge,
                axis,
                original,
                ..
            } = &action
            {
                let changed = self.model.path(edge.key).is_some_and(|path| {
                    path.iter()
                        .zip(original)
                        .any(|(p, o)| !self.geometry.same_point(p, o))
                });
                if !cancelled && changed {
                    self.after_edge_drag(*edge, *axis, original, position);
                }
            }
        }
        self.interaction.pointer = if position.kind == PointerType::Touch || cancelled {
            None
        } else {
            Some(position)
        };
        if self.interaction.pointer.is_none() {
            if !self.is_drawing() {
                self.interaction.live_point = None;
            }
            self.interaction.hover_edge = None;
        }
        self.update_live_point(false, false);
        self.invalidate();
    }

    fn after_edge_drag(&mut self, edge: Edge, axis: usize, original: &[Point3], position: Position) {
        let Some(path) = self.model.path(edge.key).cloned() else {
            return;
        };
        let selection = match &self.interaction.edge_selection {
            Some(selection) => selection.sel.clone(),
            None => self.geometry.select_edge(&path, edge.index),
        };
        let delta = path[edge.index][axis] - original[edge.index][axis];
        let measure = selection
            .affected
            .iter()
            .find(|(_, rate)| rate * delta > 0.0)
            .or_else(|| selection.affected.first())
            .copied();
        if let Some((index, _)) = measure {
            let dimensions = self.polygon_dimensions(edge.key, &path);
            let rate_of = |record: &Record| {
                record.chain().map_or(0.0, |chain| {
                    self.measurements.chain_rate(&path, chain, Some(&selection))
                })
            };
            let chain_record = dimensions
                .iter()
                .find(|record| rate_of(record) * delta > 0.0)
                .or_else(|| dimensions.iter().find(|record| rate_of(record) != 0.0))
                .cloned();
            let middle = match &chain_record {
                Some(record) => self.chain_anchor(record),
                None => self.geometry.segment_middle(&path, index),
            };
            let screen = self.project(&middle, &self.matrix());
            let mut record = chain_record.unwrap_or_else(|| Record {
                key: edge.key,
                temp: None,
                kind: MeasureKind::Segment {
                    index,
                    closed: true,
                    preview: false,
                },
                line: self.segment_dimension_line(&path, index, true),
                name: String::new(),
                source: None,
                follow_measure: false,
            });
            record.follow_measure = true;
            let anchor = screen.map_or((position.x, position.y), |s| (s[0], s[1]));
            self.open_length_editor(record, Some(anchor));
        }
        if !self
            .interaction
            .dimension_edit
            .as_ref()
            .is_some_and(|e| e.touch_panel)
        {
            self.request_fit(Some(edge.key), true);
        }
    }

    pub fn on_pointer_leave(&mut self) {
        if self.interaction.pointers.is_empty() {
            self.interaction.pointer = None;
            if !self.is_drawing() {
                self.interaction.live_point = None;
            }
            self.interaction.hover_edge = None;
            self.invalidate();
        }
    }

    pub fn on_escape(&mut self) {
        if self.is_closing() {
            self.dismiss_close();
        } else {
            self.cancel_interactions(false);
            self.set_drawing(false);
            self.interaction.last_closure = None;
        }
    }

    pub fn on_wheel(&mut self, delta_y: f64, delta_mode: u32) {
        self.dismiss_dimension_choice();
        if self.interaction.drag.is_some()
            || self.interaction.navigation
            || self.interaction.dimension_edit.is_some()
        {
            return;
        }
        self.fit.animation = None;
        self.fit.requested = false;
        self.fit.frozen_input = None;
        let delta = delta_y
            * match delta_mode {
                1 => 16.0,
                2 => self.height,
                _ => 1.0,
            };
        self.camera.distance = (self.camera.distance * (delta * 0.001).clamp(-1.0, 1.0).exp())
            .min(150f64.max(self.camera.distance * 10.0))
            .max(1.0);
        self.invalidate();
    }

    pub fn on_polygon_toggle(&mut self) {
        if !self.commit_length() {
            return;
        }
        self.interaction.last_closure = None;
        self.cancel_interactions(false);
        let enabled = !self.is_drawing();
        self.set_drawing(enabled);
    }

    pub fn on_start_target_enter(&mut self) {
        if !self.is_drawing()
            || self.interaction.dimension_edit.is_some()
            || self.interaction.navigation
        {
            return;
        }
        self.interaction.start_hovered = true;
        self.interaction.start_near = true;
        self.interaction.live_point = None;
        self.invalidate();
    }

    pub fn on_start_target_leave(&mut self) {
        self.interaction.start_hovered = false;
        if !self.is_closing() {
            self.interaction.start_near = false;
            self.invalidate();
        }
    }

    pub fn on_axes_toggle(&mut self) {
        if !self.commit_length() {
            return;
        }
        self.render_state.axes_visible = !self.render_state.axes_visible;
        self.invalidate();
    }

    // ---------------------------------------------------------------- 11-lifecycle

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

    fn restore_viewport_view(&mut self, width: f64, height: f64, preserve_scale: bool) -> bool {
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

fn midpoint(a: &Point3, b: &Point3) -> Point3 {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
}

/// Grund, aus dem eine Längeneingabe nicht übernommen wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LengthError {
    Number,
    Inactive,
    Kern(KernError),
}

impl From<KernError> for LengthError {
    fn from(error: KernError) -> Self {
        LengthError::Kern(error)
    }
}

impl LengthError {
    fn message(self) -> &'static str {
        match self {
            LengthError::Number => texte::MSG_NUMBER,
            LengthError::Inactive => texte::MSG_INACTIVE_SEGMENT,
            LengthError::Kern(error) => texte::kern_error(error),
        }
    }
}
