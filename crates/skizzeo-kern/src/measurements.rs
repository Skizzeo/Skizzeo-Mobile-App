//! 04-dimensions: architektonische Maßketten (createMeasurementKernel).

use crate::geometry::{EdgeSelection, Geometry};
use crate::{js_sign, KernError, Point3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainKind {
    Offset,
    Total,
}

/// Seite des Polygons, an der eine Maßkette liegt (Draufsicht, Y nach Norden).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    South,
    East,
    North,
    West,
}

/// Stabile Identität einer Maßkette innerhalb eines Polygons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainKey {
    pub side: Side,
    pub kind: ChainKind,
    pub from: usize,
    pub to: usize,
}

/// Maßkettendatensatz eines geschlossenen Polygons (ohne Pfadreferenz).
#[derive(Clone, Debug, PartialEq)]
pub struct ChainRecord {
    /// Physische Kante der Station oder `None` (-1 im Prototyp).
    pub index: Option<usize>,
    pub axis: usize,
    pub sign: f64,
    pub side: Side,
    pub kind: ChainKind,
    pub from: usize,
    pub to: usize,
    pub extreme: f64,
    pub has_offsets: bool,
    pub origin_start: Point3,
    pub origin_end: Point3,
    pub key: ChainKey,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Station {
    pub value: f64,
    pub index: usize,
}

pub struct Measurements {
    pub core: Geometry,
}

impl Measurements {
    pub fn new(core: Geometry) -> Self {
        Self { core }
    }

    pub fn chain_length(&self, path: &[Point3], record: &ChainRecord) -> f64 {
        (path[record.to][record.axis] - path[record.from][record.axis]).abs()
    }

    pub fn side_stations(&self, path: &[Point3], axis: usize, sign: f64) -> Vec<Station> {
        let eps = self.core.geometry_epsilon;
        let fixed = 1 - axis;
        let mut sorted: Vec<Station> = path
            .iter()
            .enumerate()
            .map(|(index, p)| Station {
                value: p[axis],
                index,
            })
            .collect();
        sorted.sort_by(|a, b| {
            (a.value - b.value)
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut candidates: Vec<Station> = Vec::new();
        for item in sorted {
            match candidates.last_mut() {
                Some(last) if (last.value - item.value).abs() <= eps => {
                    if path[item.index][fixed] * sign > path[last.index][fixed] * sign {
                        last.index = item.index;
                    }
                }
                _ => candidates.push(item),
            }
        }
        let profile: Vec<f64> = candidates
            .iter()
            .skip(1)
            .enumerate()
            .map(|(i, station)| {
                let middle = (candidates[i].value + station.value) / 2.0;
                let mut outside = f64::NEG_INFINITY;
                for (index, point) in path.iter().enumerate() {
                    let next = path[(index + 1) % path.len()];
                    if (point[fixed] - next[fixed]).abs() < eps
                        && middle > point[axis].min(next[axis])
                        && middle < point[axis].max(next[axis])
                    {
                        outside = outside.max(point[fixed] * sign);
                    }
                }
                outside
            })
            .collect();
        let count = candidates.len();
        candidates
            .into_iter()
            .enumerate()
            .filter(|(i, _)| {
                *i == 0 || *i == count - 1 || (profile[*i] - profile[*i - 1]).abs() > eps
            })
            .map(|(_, s)| s)
            .collect()
    }

    pub fn polygon_measurements(&self, path: &[Point3]) -> Vec<ChainRecord> {
        let eps = self.core.geometry_epsilon;
        if path.len() < 3 {
            return Vec::new();
        }
        let mut records = Vec::new();
        let sides = [
            (Side::South, 0, -1.0),
            (Side::East, 1, 1.0),
            (Side::North, 0, 1.0),
            (Side::West, 1, -1.0),
        ];
        for (side, axis, sign) in sides {
            let fixed = 1 - axis;
            let stations = self.side_stations(path, axis, sign);
            let extreme = if sign > 0.0 {
                path.iter().map(|p| p[fixed]).fold(f64::NEG_INFINITY, f64::max)
            } else {
                path.iter().map(|p| p[fixed]).fold(f64::INFINITY, f64::min)
            };
            let has_offsets = stations.len() > 2;
            let mut append = |start: usize, end: usize, kind: ChainKind| {
                let a = stations[start];
                let b = stations[end];
                if b.value - a.value < eps {
                    return;
                }
                let mut index = None;
                let mut nearest = f64::NEG_INFINITY;
                for (i, point) in path.iter().enumerate() {
                    let next = path[(i + 1) % path.len()];
                    if (point[axis].min(next[axis]) - a.value).abs() < eps
                        && (point[axis].max(next[axis]) - b.value).abs() < eps
                        && (point[fixed] - next[fixed]).abs() < eps
                        && point[fixed] * sign > nearest
                    {
                        index = Some(i);
                        nearest = point[fixed] * sign;
                    }
                }
                records.push(ChainRecord {
                    index,
                    axis,
                    sign,
                    side,
                    kind,
                    from: a.index,
                    to: b.index,
                    extreme,
                    has_offsets,
                    origin_start: path[a.index],
                    origin_end: path[b.index],
                    key: ChainKey {
                        side,
                        kind,
                        from: a.index,
                        to: b.index,
                    },
                });
            };
            if has_offsets {
                for i in 1..stations.len() {
                    append(i - 1, i, ChainKind::Offset);
                }
            }
            append(0, stations.len() - 1, ChainKind::Total);
        }
        records
    }

    /// `chainRate`; `selection` nur übergeben, wenn sie zum selben Pfad gehört.
    pub fn chain_rate(
        &self,
        path: &[Point3],
        record: &ChainRecord,
        selection: Option<&EdgeSelection>,
    ) -> f64 {
        let Some(selection) = selection else {
            return 0.0;
        };
        if selection.axis != record.axis {
            return 0.0;
        }
        let moved = self.core.moving_vertices(
            path,
            &[selection.index, (selection.index + 1) % path.len()],
            selection.axis,
            true,
        );
        f64::from(u8::from(moved.contains(&record.to)))
            - f64::from(u8::from(moved.contains(&record.from)))
    }

    pub fn resize_chain(
        &self,
        path: &[Point3],
        record: &ChainRecord,
        length: f64,
        selection: Option<&EdgeSelection>,
    ) -> Result<Vec<Point3>, KernError> {
        let eps = self.core.geometry_epsilon;
        self.core.positive_length(length)?;
        let (axis, from, to) = (record.axis, record.from, record.to);
        let delta = length - self.chain_length(path, record);
        let rate = self.chain_rate(path, record, selection);
        let updated: Vec<Point3> = match selection {
            Some(selection) if rate != 0.0 => {
                self.core.shifted_segment(path, selection.index, delta / rate, true)
            }
            _ => path
                .iter()
                .map(|p| {
                    let mut q = *p;
                    if p[axis] >= path[to][axis] - eps {
                        q[axis] += delta;
                    }
                    q
                })
                .collect(),
        };
        for i in 0..path.len() {
            let j = (i + 1) % path.len();
            for a in 0..2 {
                let old = path[j][a] - path[i][a];
                let next = updated[j][a] - updated[i][a];
                let invalid = if old.abs() < eps {
                    next.abs() > eps
                } else {
                    js_sign(old) != js_sign(next) || next.abs() < self.core.minimum_length
                };
                if invalid {
                    return Err(KernError::ChainReversesSegment);
                }
            }
        }
        if (updated[to][axis] - updated[from][axis] - length).abs() > length.max(1.0) * 1e-9 {
            return Err(KernError::ChainDirection);
        }
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SETTINGS;

    fn kernel() -> Measurements {
        Measurements::new(Geometry::new(&SETTINGS))
    }

    fn l_shape() -> Vec<Point3> {
        vec![
            [0.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
            [6.0, 2.0, 0.0],
            [3.0, 2.0, 0.0],
            [3.0, 5.0, 0.0],
            [0.0, 5.0, 0.0],
        ]
    }

    #[test]
    fn rectangle_has_four_totals() {
        let m = kernel();
        let r = vec![[0.0; 3], [4.0, 0.0, 0.0], [4.0, 3.0, 0.0], [0.0, 3.0, 0.0]];
        let records = m.polygon_measurements(&r);
        assert_eq!(records.len(), 4);
        assert!(records.iter().all(|r| r.kind == ChainKind::Total));
        assert_eq!(m.chain_length(&r, &records[0]), 4.0);
        assert_eq!(m.chain_length(&r, &records[1]), 3.0);
    }

    #[test]
    fn l_shape_offsets_sum_to_total() {
        let m = kernel();
        let path = l_shape();
        let records = m.polygon_measurements(&path);
        assert_eq!(records.len(), 8);
        for side in [Side::South, Side::East, Side::North, Side::West] {
            let offsets: f64 = records
                .iter()
                .filter(|r| r.side == side && r.kind == ChainKind::Offset)
                .map(|r| m.chain_length(&path, r))
                .sum();
            let total = records
                .iter()
                .find(|r| r.side == side && r.kind == ChainKind::Total)
                .map(|r| m.chain_length(&path, r))
                .unwrap();
            if offsets > 0.0 {
                assert!((offsets - total).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn resize_chain_is_atomic() {
        let m = kernel();
        let path = l_shape();
        let records = m.polygon_measurements(&path);
        let total = records
            .iter()
            .find(|r| r.side == Side::South && r.kind == ChainKind::Total)
            .unwrap();
        let out = m.resize_chain(&path, total, 8.0, None).unwrap();
        assert!((m.chain_length(&out, total) - 8.0).abs() < 1e-9);
        let nord_offset = records
            .iter()
            .find(|r| r.side == Side::North && r.kind == ChainKind::Offset)
            .unwrap();
        assert!(m.resize_chain(&path, nord_offset, 0.000_000_1, None).is_err());
    }
}
