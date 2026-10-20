//! 06-drawing: offener Zug, Entwurf, Schließen und Rückgängig.

use skizzeo_darstellung::camera::view_slope;
use skizzeo_kern::model::PathKey;
use skizzeo_kern::{Point3, SETTINGS};

use crate::workflow::WorkflowEvent;
use crate::INPUT;

use super::*;

impl Viewport {
    pub(super) fn snap_draft(&mut self, candidate: Point3, position: Position) -> Option<Point3> {
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
        // Nur der Startpunkt; die erste Gerade gibt der Nutzer selbst vor.
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

    pub(super) fn default_draft(&self) -> Option<Point3> {
        self.geometry.next_draft(&self.model.active)
    }

    pub(super) fn finish_path(&mut self, x: f64, y: f64, radius: f64) -> bool {
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

    pub(super) fn add_point(&mut self, position: Position) {
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
}
