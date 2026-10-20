//! Darstellungsadapter der Maßlinien (createDimensionGeometry).

use skizzeo_kern::measurements::{ChainKind, ChainRecord};
use skizzeo_kern::{js_sign, Point3};

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

/// Maßlinie eines Einzelsegments, immer auf der Außenseite.
///
/// `clockwise`: Der Pfad läuft im Uhrzeigersinn (Draufsicht), außen liegt dann links
/// der Laufrichtung, sonst rechts.
pub fn segment_dimension(
    path: &[Point3],
    index: usize,
    axis: usize,
    style: &DimensionStyle,
    clockwise: bool,
) -> DimensionLine {
    let origin_start = path[index];
    let origin_end = path[(index + 1) % path.len()];
    let fixed = 1 - axis;
    let direction = js_sign(origin_end[axis] - origin_start[axis]);
    let left = if axis == 0 { direction } else { -direction };
    let mut sign = if clockwise { left } else { -left };
    if sign == 0.0 {
        sign = -1.0;
    }
    let mut line_start = origin_start;
    let mut line_end = origin_end;
    line_start[fixed] += sign * style.dimension_offset;
    line_end[fixed] += sign * style.dimension_offset;
    DimensionLine {
        axis,
        sign,
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

#[cfg(test)]
mod tests {
    use super::*;
    use skizzeo_kern::geometry::is_clockwise;

    fn style() -> DimensionStyle {
        DimensionStyle {
            dimension_offset: 1.5,
            dimension_spacing: 1.0,
            extension_gap: 0.2,
            extension_overhang: 0.2,
            dimension_overhang: 0.2,
            symbol: DimensionSymbol::Slash,
            tick_length: 0.4,
        }
    }

    #[test]
    fn segment_dimensions_lie_outside() {
        // Rechteck im Uhrzeigersinn: oben nach rechts, rechts nach unten, ...
        let path = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, -4.0, 0.0], [0.0, -4.0, 0.0]];
        assert!(is_clockwise(&path));
        let top = segment_dimension(&path, 0, 0, &style(), true);
        assert_eq!(top.line_start[1], 1.5);
        let right = segment_dimension(&path, 1, 1, &style(), true);
        assert_eq!(right.line_start[0], 11.5);
        let bottom = segment_dimension(&path, 2, 0, &style(), true);
        assert_eq!(bottom.line_start[1], -5.5);
        let left = segment_dimension(&path, 3, 1, &style(), true);
        assert_eq!(left.line_start[0], -1.5);
        let reversed: Vec<Point3> = path.iter().rev().copied().collect();
        assert!(!is_clockwise(&reversed));
        let top = segment_dimension(&reversed, 2, 0, &style(), false);
        assert_eq!(top.line_start[1], 1.5);
    }
}
