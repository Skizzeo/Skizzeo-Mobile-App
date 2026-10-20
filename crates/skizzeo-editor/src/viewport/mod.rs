//! Viewport-Steuerung: Zustand, Zeichenmodus und Ereignisse der Zeichenfläche.
//!
//! Die Methoden sind nach den Kapiteln des Prototyps auf Untermodule verteilt
//! (05-camera bis 11-lifecycle).

use skizzeo_darstellung::camera::Camera;
use skizzeo_darstellung::render::Rect;
use skizzeo_darstellung::style::Appearance;
use skizzeo_kern::geometry::Geometry;
use skizzeo_kern::measurements::Measurements;
use skizzeo_kern::model::Model;
use skizzeo_kern::SETTINGS;

use crate::state::*;
use crate::workflow::{transition, WorkflowEvent, WorkflowState};

mod camera;
mod dimensions;
mod drawing;
mod frame;
mod input;
mod length;
mod lifecycle;

use dimensions::midpoint;

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

    pub(super) fn transition_workflow(&mut self, event: WorkflowEvent) {
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
}
