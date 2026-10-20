//! Maßdatensätze und ihre Maßlinien (Stiladapter zur Darstellung).

use skizzeo_darstellung::dimension_geometry::{
    self as dimgeo, DimensionLine, DimensionStroke, DimensionStyle, DimensionSymbol,
};
use skizzeo_darstellung::texte;
use skizzeo_kern::geometry::is_clockwise;
use skizzeo_kern::model::PathKey;
use skizzeo_kern::Point3;

use super::*;

impl Viewport {
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
}

pub(super) fn midpoint(a: &Point3, b: &Point3) -> Point3 {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
}
