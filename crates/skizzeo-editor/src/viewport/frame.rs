//! 07-frame: Darstellungseinstellungen, Frame-Vorbereitung und Bedienelemente.

use skizzeo_darstellung::format::{
    format_building_length, spoken_length,
};
use skizzeo_darstellung::render::{
    self, DrawList, FrameDimension, FrameSelected, RenderFrame, RenderOutput,
};
use skizzeo_darstellung::style::Appearance;
use skizzeo_kern::measurements::ChainKind;
use skizzeo_kern::model::PathKey;
use skizzeo_kern::{Point3, SETTINGS};

use super::*;

impl Viewport {
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

    pub(super) fn measurement_affected(&self, record: &Record) -> bool {
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

    pub(super) fn prepare_render_frame(&self) -> (RenderFrame, Vec<Record>) {
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

    pub(super) fn selected_segment(&self) -> Option<FrameSelected> {
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
}
