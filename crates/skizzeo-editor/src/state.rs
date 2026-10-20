//! 02-state: Interaktion, Maßdatensätze, Einpassung und Darstellungszustand.
//!
//! Das Gebäudemodell selbst liegt im Kern ([`skizzeo_kern::model`]).

use std::collections::HashMap;

use skizzeo_darstellung::camera::{Camera, ViewSnapshot};
use skizzeo_darstellung::dimension_geometry::DimensionLine;
use skizzeo_darstellung::render::{DimensionLayout, PreviewBounds};
use skizzeo_kern::geometry::EdgeSelection;
use skizzeo_kern::measurements::ChainRecord;
use skizzeo_kern::model::{Model, PathKey};
use skizzeo_kern::Point3;

#[derive(Clone, Debug, PartialEq)]
pub enum MeasureKind {
    Segment {
        index: usize,
        closed: bool,
        preview: bool,
    },
    Chain(ChainRecord),
}

/// Maßdatensatz mit Pfadbezug (records im Prototyp).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub key: PathKey,
    /// Eigener Pfad der Vorschau.
    pub temp: Option<Vec<Point3>>,
    pub kind: MeasureKind,
    pub line: DimensionLine,
    pub name: String,
    pub source: Option<PathKey>,
    pub follow_measure: bool,
}

impl Record {
    pub fn is_chain(&self) -> bool {
        matches!(self.kind, MeasureKind::Chain(_))
    }

    pub fn is_preview(&self) -> bool {
        matches!(self.kind, MeasureKind::Segment { preview: true, .. })
    }

    pub fn chain(&self) -> Option<&ChainRecord> {
        match &self.kind {
            MeasureKind::Chain(chain) => Some(chain),
            MeasureKind::Segment { .. } => None,
        }
    }

    pub fn segment_index(&self) -> Option<usize> {
        match self.kind {
            MeasureKind::Segment { index, .. } => Some(index),
            MeasureKind::Chain(_) => None,
        }
    }

    pub fn axis(&self) -> usize {
        self.line.axis
    }

    pub fn points<'a>(&'a self, model: &'a Model) -> Option<&'a [Point3]> {
        match &self.temp {
            Some(points) => Some(points),
            None => model.path(self.key).map(Vec::as_slice),
        }
    }

    /// Gleicher Pfad im Sinne von `a.path === b.path`.
    pub fn same_path(&self, key: PathKey) -> bool {
        self.temp.is_none() && self.key == key
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerType {
    Mouse,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Pen,
    Touch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub kind: PointerType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub key: PathKey,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeSel {
    pub key: PathKey,
    pub sel: EdgeSelection,
}

#[derive(Clone, Debug)]
pub enum Drag {
    Orbit {
        id: i64,
        x: f64,
        y: f64,
    },
    Draw {
        id: i64,
    },
    Edit {
        id: i64,
        edge: Edge,
        hit: Point3,
        axis: usize,
        original: Vec<Point3>,
    },
}

impl Drag {
    pub fn id(&self) -> i64 {
        match self {
            Drag::Orbit { id, .. } | Drag::Draw { id } | Drag::Edit { id, .. } => *id,
        }
    }

    pub fn is_draw(&self) -> bool {
        matches!(self, Drag::Draw { .. })
    }

}

#[derive(Clone, Debug)]
pub struct Gesture {
    pub ids: [i64; 2],
    pub x: f64,
    pub y: f64,
    pub separation: f64,
    pub camera: Camera,
}

#[derive(Clone, Copy, Debug)]
pub struct DraftSnap {
    pub axis: usize,
    pub x: f64,
    pub y: f64,
    pub point: Option<Point3>,
}

#[derive(Clone, Debug)]
pub struct LastClosure {
    pub path: Vec<Point3>,
    pub polygon: u64,
}

#[derive(Clone, Debug)]
pub struct DimensionEdit {
    pub record: Record,
    pub touch_panel: bool,
    pub replace_input: bool,
    pub edge_control: Option<EdgeSel>,
    pub x: f64,
    pub y: f64,
    pub initial_input: String,
}

#[derive(Debug, Default)]
pub struct Interaction {
    pub input_type: Option<PointerType>,
    pub dimension_edit: Option<DimensionEdit>,
    pub edge_selection: Option<EdgeSel>,
    pub drag: Option<Drag>,
    pub live_point: Option<Point3>,
    pub pointer: Option<Position>,
    pub start_armed: bool,
    pub hover_edge: Option<Edge>,
    pub gesture: Option<Gesture>,
    pub navigation: bool,
    pub preview_bounds: Option<PreviewBounds>,
    pub draft_snap: Option<DraftSnap>,
    pub draft_blocked: bool,
    pub start_near: bool,
    pub start_hovered: bool,
    pub last_closure: Option<LastClosure>,
    /// Pointer-Kontakte in Einfügereihenfolge (JavaScript-Map).
    pub pointers: Vec<(i64, Position)>,
}

impl Interaction {
    pub fn set_pointer(&mut self, id: i64, position: Position) {
        if let Some(entry) = self.pointers.iter_mut().find(|(i, _)| *i == id) {
            entry.1 = position;
        } else {
            self.pointers.push((id, position));
        }
    }

    pub fn get_pointer(&self, id: i64) -> Option<Position> {
        self.pointers.iter().find(|(i, _)| *i == id).map(|(_, p)| *p)
    }

    pub fn has_pointer(&self, id: i64) -> bool {
        self.pointers.iter().any(|(i, _)| *i == id)
    }

    pub fn remove_pointer(&mut self, id: i64) {
        self.pointers.retain(|(i, _)| *i != id);
    }
}

#[derive(Clone, Debug)]
pub struct FitAnimation {
    pub from_target: Point3,
    pub from_distance: f64,
    pub target: Point3,
    pub distance: f64,
    pub started: f64,
}

#[derive(Debug, Default)]
pub struct FitState {
    pub requested: bool,
    pub tight: bool,
    pub frozen_input: Option<ViewSnapshot>,
    pub animation: Option<FitAnimation>,
    pub path: Option<PathKey>,
}

#[derive(Clone, Debug)]
pub struct PanelView {
    pub camera: Camera,
    pub key: [u64; 6],
}

#[derive(Clone, Copy, Debug)]
pub struct SavedView {
    pub camera: Camera,
    pub height: f64,
}

#[derive(Debug, Default)]
pub struct RenderState {
    pub first_frame: bool,
    pub axes_visible: bool,
    pub viewport_size: Option<(f64, f64)>,
    pub viewport_views: HashMap<&'static str, SavedView>,
    pub panel_view: Option<PanelView>,
    pub dimension_layouts: Vec<DimensionLayout>,
}
