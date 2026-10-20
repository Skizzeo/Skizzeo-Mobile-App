//! 10-input: Zeiger, Gesten, Kantenbearbeitung und Bedienknöpfe.

use skizzeo_kern::model::PathKey;
use skizzeo_kern::Point3;

use crate::INPUT;

use super::*;

impl Viewport {
    pub(super) fn hit_segment(&self, x: f64, y: f64, radius: f64) -> Option<Edge> {
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

    pub(super) fn restore_edit(&mut self) {
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

    pub(super) fn start_gesture(&mut self) {
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

    pub(super) fn move_gesture(&mut self) {
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

    pub(super) fn after_edge_drag(&mut self, edge: Edge, axis: usize, original: &[Point3], position: Position) {
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
}
