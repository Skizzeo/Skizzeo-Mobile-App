//! 07-measurements: Längeneditor und Übernahme von Maßen über den Kern.

use skizzeo_darstellung::format::{
    format_length_input, parse_length_input,
};
use skizzeo_darstellung::texte;
use skizzeo_kern::units::building_length;
use skizzeo_kern::{KernError, Point3};

use super::*;

impl Viewport {
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

    pub(super) fn clear_length_error(&mut self) {
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

    pub(super) fn compute_length_commit(&self, edit: &DimensionEdit) -> Result<Vec<Point3>, LengthError> {
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
}

/// Grund, aus dem eine Längeneingabe nicht übernommen wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LengthError {
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
    pub(super) fn message(self) -> &'static str {
        match self {
            LengthError::Number => texte::MSG_NUMBER,
            LengthError::Inactive => texte::MSG_INACTIVE_SEGMENT,
            LengthError::Kern(error) => texte::kern_error(error),
        }
    }
}
