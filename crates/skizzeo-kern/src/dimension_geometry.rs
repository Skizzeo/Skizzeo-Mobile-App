//! Darstellungsadapter der Maßlinien (createDimensionGeometry).

use crate::measurements::{ChainKind, ChainRecord};
use crate::{js_sign, Point3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionSymbol {
    Slash,
    Circle,
}

/// Explizit übergebene Stilwerte der Maßgeometrie.
#[derive(Clone, Copy, Debug)]
pub struct DimensionStyle {
    pub dimension_offset: f64,
    pub dimension_spacing: f64,
    pub extension_gap: f64,
    pub extension_overhang: f64,
    pub dimension_overhang: f64,
    pub symbol: DimensionSymbol,
    pub tick_length: f64,
}

/// Lage einer Maßlinie mit ihren Bezugspunkten.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DimensionLine {
    pub axis: usize,
    pub sign: f64,
    pub line_start: Point3,
    pub line_end: Point3,
    pub origin_start: Point3,
    pub origin_end: Point3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DimensionStroke {
    pub a: Point3,
    pub b: Point3,
    pub tick: bool,
}

pub fn place_measurement(path: &[Point3], record: &ChainRecord, style: &DimensionStyle) -> DimensionLine {
    let offset = style.dimension_offset
        + if record.kind == ChainKind::Total && record.has_offsets {
            style.dimension_spacing
        } else {
            0.0
        };
    let axis = record.axis;
    let mut line_start = [0.0; 3];
    let mut line_end = [0.0; 3];
    line_start[axis] = path[record.from][axis];
    line_end[axis] = path[record.to][axis];
    let fixed_value = record.extreme + record.sign * offset;
    line_start[1 - axis] = fixed_value;
    line_end[1 - axis] = fixed_value;
    DimensionLine {
        axis,
        sign: record.sign,
        line_start,
        line_end,
        origin_start: record.origin_start,
        origin_end: record.origin_end,
    }
}

pub fn segment_dimension(path: &[Point3], index: usize, axis: usize, style: &DimensionStyle) -> DimensionLine {
    let origin_start = path[index];
    let origin_end = path[(index + 1) % path.len()];
    let fixed = 1 - axis;
    let mut line_start = origin_start;
    let mut line_end = origin_end;
    line_start[fixed] -= style.dimension_offset;
    line_end[fixed] -= style.dimension_offset;
    DimensionLine {
        axis,
        sign: -1.0,
        line_start,
        line_end,
        origin_start,
        origin_end,
    }
}

pub fn dimension_strokes(line: &DimensionLine, style: &DimensionStyle) -> Vec<DimensionStroke> {
    let DimensionLine {
        axis,
        sign,
        line_start: a,
        line_end: b,
        origin_start,
        origin_end,
    } = *line;
    let fixed = 1 - axis;
    let mut strokes = Vec::new();
    let extend = |point: Point3, amount: f64| {
        let mut p = point;
        p[fixed] += sign * amount;
        p
    };
    let mut start = a;
    let mut end = b;
    let direction = js_sign(b[axis] - a[axis]);
    start[axis] -= direction * style.dimension_overhang;
    end[axis] += direction * style.dimension_overhang;
    strokes.push(DimensionStroke {
        a: start,
        b: end,
        tick: false,
    });
    for (origin, end) in [(origin_start, a), (origin_end, b)] {
        strokes.push(DimensionStroke {
            a: extend(origin, style.extension_gap),
            b: extend(end, style.extension_overhang),
            tick: false,
        });
        match style.symbol {
            DimensionSymbol::Circle => {
                let radius = style.tick_length / 2.0;
                for i in 0..32 {
                    let mut p = end;
                    let mut q = end;
                    let t0 = f64::from(i) * std::f64::consts::PI / 16.0;
                    let t1 = f64::from(i + 1) * std::f64::consts::PI / 16.0;
                    p[axis] += radius * t0.cos();
                    p[fixed] += radius * t0.sin();
                    q[axis] += radius * t1.cos();
                    q[fixed] += radius * t1.sin();
                    strokes.push(DimensionStroke { a: p, b: q, tick: true });
                }
            }
            DimensionSymbol::Slash => {
                let mut p = end;
                let mut q = end;
                let half = style.tick_length / 8f64.sqrt();
                p[axis] -= half;
                p[fixed] -= if axis == 0 { half } else { -half };
                q[axis] += half;
                q[fixed] += if axis == 0 { half } else { -half };
                strokes.push(DimensionStroke { a: p, b: q, tick: true });
            }
        }
    }
    strokes
}
