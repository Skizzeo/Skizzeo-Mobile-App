//! 04-geometry: reiner Polygonkern (createGeometryKernel).

use std::collections::BTreeSet;

use crate::{js_sign, KernError, Point3, Settings};

/// Unveränderliche Grenzwerte des Kerns.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub geometry_epsilon: f64,
    pub minimum_length: f64,
}

/// Ausgewählte Kante mit signierten Änderungsraten der betroffenen Segmentmaße.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeSelection {
    pub index: usize,
    pub axis: usize,
    /// Segmentindex -> Rate, in Einfügereihenfolge (aufsteigender Index).
    pub affected: Vec<(usize, f64)>,
    pub original: Vec<Point3>,
}

impl EdgeSelection {
    pub fn affects(&self, index: usize) -> bool {
        self.affected.iter().any(|(i, _)| *i == index)
    }

    pub fn rate(&self, index: usize) -> Option<f64> {
        self.affected.iter().find(|(i, _)| *i == index).map(|(_, r)| *r)
    }
}

/// Ergebnis von `orthogonalDraft`.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub axis: usize,
    pub point: Option<Point3>,
    pub blocked: bool,
}

impl Geometry {
    pub fn new(config: &Settings) -> Self {
        Self {
            geometry_epsilon: config.geometry_epsilon,
            minimum_length: config.minimum_length,
        }
    }

    pub fn same_point(&self, a: &Point3, b: &Point3) -> bool {
        (a[0] - b[0]).hypot(a[1] - b[1]) < self.geometry_epsilon
    }

    pub fn segment_axis(&self, path: &[Point3], index: usize) -> usize {
        if (path[index][0] - path[(index + 1) % path.len()][0]).abs() > self.geometry_epsilon {
            0
        } else {
            1
        }
    }

    pub fn segment_middle(&self, path: &[Point3], index: usize) -> Point3 {
        let a = path[index];
        let b = path[(index + 1) % path.len()];
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
    }

    pub fn overlaps_path(&self, path: &[Point3], end: &Point3) -> bool {
        let eps = self.geometry_epsilon;
        let Some(start) = path.last() else {
            return false;
        };
        let axis = if (end[0] - start[0]).abs() > eps { 0 } else { 1 };
        let fixed = 1 - axis;
        for i in 1..path.len() {
            let a = path[i - 1];
            let b = path[i];
            if (a[fixed] - start[fixed]).abs() > eps || (b[fixed] - start[fixed]).abs() > eps {
                continue;
            }
            let overlap = start[axis].max(end[axis]).min(a[axis].max(b[axis]))
                - start[axis].min(end[axis]).max(a[axis].min(b[axis]));
            if overlap > eps {
                return true;
            }
        }
        false
    }

    pub fn close_path(&self, path: &[Point3]) -> Option<Vec<Point3>> {
        let eps = self.geometry_epsilon;
        if path.len() < 3 {
            return None;
        }
        let mut result = path.to_vec();
        let first = result[0];
        let last = result[result.len() - 1];
        let previous = result[result.len() - 2];
        if (last[0] - first[0]).abs() > eps && (last[1] - first[1]).abs() > eps {
            result.push(if (last[0] - previous[0]).abs() > eps {
                [last[0], first[1], 0.0]
            } else {
                [first[0], last[1], 0.0]
            });
        }
        if self.same_point(&result[result.len() - 1], &first) {
            result.pop();
        }
        for i in 1..=result.len() {
            if self.overlaps_path(&result[..i], &result[i % result.len()]) {
                return None;
            }
        }
        let mut area = 0.0;
        for i in 0..result.len() {
            let a = result[i];
            let b = result[(i + 1) % result.len()];
            area += a[0] * b[1] - b[0] * a[1];
        }
        if area.abs() > eps {
            Some(result)
        } else {
            None
        }
    }

    pub fn moving_vertices(
        &self,
        path: &[Point3],
        initial: &[usize],
        axis: usize,
        closed: bool,
    ) -> BTreeSet<usize> {
        let count = path.len();
        let mut moved: BTreeSet<usize> = initial.iter().copied().collect();
        let limit = if closed { count } else { count.saturating_sub(1) };
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..limit {
                let j = (i + 1) % count;
                if (path[i][axis] - path[j][axis]).abs() < self.geometry_epsilon
                    && (moved.contains(&i) || moved.contains(&j))
                {
                    if !moved.contains(&i) || !moved.contains(&j) {
                        changed = true;
                    }
                    moved.insert(i);
                    moved.insert(j);
                }
            }
        }
        moved
    }

    pub fn translated_vertices(
        &self,
        path: &[Point3],
        initial: &[usize],
        axis: usize,
        requested_delta: f64,
        closed: bool,
    ) -> Vec<Point3> {
        let count = path.len();
        let moved = self.moving_vertices(path, initial, axis, closed);
        if !closed && moved.contains(&0) {
            return path.to_vec();
        }
        let mut lower = f64::NEG_INFINITY;
        let mut upper = f64::INFINITY;
        let limit = if closed { count } else { count.saturating_sub(1) };
        for i in 0..limit {
            let j = (i + 1) % count;
            let coefficient =
                f64::from(u8::from(moved.contains(&j))) - f64::from(u8::from(moved.contains(&i)));
            if coefficient == 0.0 {
                continue;
            }
            let length = path[j][axis] - path[i][axis];
            if length.abs() < self.geometry_epsilon {
                continue;
            }
            let rate = js_sign(length) * coefficient;
            let limit = (1e-6_f64.max(length.abs() * 0.0001) - length.abs()) / rate;
            if rate > 0.0 {
                lower = lower.max(limit);
            } else {
                upper = upper.min(limit);
            }
        }
        let delta = lower.max(upper.min(requested_delta));
        path.iter()
            .enumerate()
            .map(|(i, p)| {
                let mut q = *p;
                if moved.contains(&i) {
                    q[axis] += delta;
                }
                q
            })
            .collect()
    }

    pub fn shifted_segment(
        &self,
        path: &[Point3],
        index: usize,
        requested_delta: f64,
        closed: bool,
    ) -> Vec<Point3> {
        let next = (index + 1) % path.len();
        let axis = 1 - self.segment_axis(path, index);
        self.translated_vertices(path, &[index, next], axis, requested_delta, closed)
    }

    pub fn segment_length(&self, path: &[Point3], index: usize) -> f64 {
        let a = path[index];
        let b = path[(index + 1) % path.len()];
        (b[0] - a[0]).hypot(b[1] - a[1])
    }

    pub fn select_edge(&self, path: &[Point3], index: usize) -> EdgeSelection {
        let next = (index + 1) % path.len();
        let axis = 1 - self.segment_axis(path, index);
        let moved = self.moving_vertices(path, &[index, next], axis, true);
        let mut affected = Vec::new();
        for (i, p) in path.iter().enumerate() {
            let j = (i + 1) % path.len();
            let rate = (f64::from(u8::from(moved.contains(&j)))
                - f64::from(u8::from(moved.contains(&i))))
                * js_sign(path[j][axis] - p[axis]);
            if rate != 0.0 && !rate.is_nan() {
                affected.push((i, rate));
            }
        }
        EdgeSelection {
            index,
            axis,
            affected,
            original: path.to_vec(),
        }
    }

    pub fn positive_length(&self, length: f64) -> Result<f64, KernError> {
        if !length.is_finite() || length < self.minimum_length {
            return Err(KernError::NonPositiveLength);
        }
        Ok(length)
    }

    pub fn matches_length(&self, path: &[Point3], index: usize, length: f64) -> bool {
        (self.segment_length(path, index) - length).abs() <= length.max(1.0) * 1e-9
    }

    pub fn checked_length(
        &self,
        path: Vec<Point3>,
        index: usize,
        length: f64,
    ) -> Result<Vec<Point3>, KernError> {
        if !self.matches_length(&path, index, length) {
            return Err(KernError::ReversesSegment);
        }
        Ok(path)
    }

    /// `resizeFromEdge`: `path` ist der aktuelle Pfad der Auswahl.
    pub fn resize_from_edge(
        &self,
        path: &[Point3],
        selection: &EdgeSelection,
        index: usize,
        length: f64,
    ) -> Result<Vec<Point3>, KernError> {
        self.positive_length(length)?;
        let rate = selection
            .rate(index)
            .ok_or(KernError::NotAffectedByEdge)?;
        let delta = (length - self.segment_length(path, index)) / rate;
        let updated = self.shifted_segment(path, selection.index, delta, true);
        self.checked_length(updated, index, length)
    }

    pub fn resized_segment(
        &self,
        path: &[Point3],
        index: usize,
        length: f64,
        closed: bool,
    ) -> Result<Vec<Point3>, KernError> {
        self.positive_length(length)?;
        let next = (index + 1) % path.len();
        let a = path[index];
        let b = path[next];
        let axis = self.segment_axis(path, index);
        let sign = js_sign(b[axis] - a[axis]);
        let delta = sign * (length - self.segment_length(path, index));
        let mut updated = self.translated_vertices(path, &[next], axis, delta, closed);
        if !self.matches_length(&updated, index, length) {
            updated = self.translated_vertices(path, &[index], axis, -delta, closed);
        }
        self.checked_length(updated, index, length)
    }

    pub fn next_draft(&self, path: &[Point3]) -> Option<Point3> {
        let last = *path.last()?;
        if path.len() == 1 {
            return Some([last[0] + 1.0, last[1], 0.0]);
        }
        let previous = path[path.len() - 2];
        // Eingabe im Uhrzeigersinn: Vorschlag biegt nach rechts ab.
        Some([
            last[0] + js_sign(last[1] - previous[1]),
            last[1] - js_sign(last[0] - previous[0]),
            0.0,
        ])
    }

    pub fn orthogonal_draft(
        &self,
        path: &[Point3],
        candidate: &Point3,
        previous_axis: Option<usize>,
        hysteresis: f64,
    ) -> Draft {
        let last = path[path.len() - 1];
        let delta = [candidate[0] - last[0], candidate[1] - last[1]];
        let mut axis = if delta[0].abs() >= delta[1].abs() { 0 } else { 1 };
        if let Some(previous) = previous_axis {
            if delta[1 - previous].abs() <= delta[previous].abs() * hysteresis {
                axis = previous;
            }
        }
        let mut point = last;
        point[axis] = candidate[axis];
        let blocked = self.overlaps_path(path, &point);
        Draft {
            axis,
            point: if blocked { None } else { Some(point) },
            blocked,
        }
    }
}

/// Umlaufsinn eines geschlossenen Pfades in der Draufsicht (Fläche ≤ 0 = Uhrzeigersinn).
pub fn is_clockwise(path: &[Point3]) -> bool {
    let area: f64 = (0..path.len())
        .map(|i| {
            let a = path[i];
            let b = path[(i + 1) % path.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    area <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SETTINGS;

    fn geo() -> Geometry {
        Geometry::new(&SETTINGS)
    }

    fn rect(w: f64, h: f64) -> Vec<Point3> {
        vec![[0.0, 0.0, 0.0], [w, 0.0, 0.0], [w, h, 0.0], [0.0, h, 0.0]]
    }

    #[test]
    fn close_path_adds_corner() {
        let g = geo();
        let path = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 1.0, 0.0]];
        let closed = g.close_path(&path).unwrap();
        assert_eq!(closed, rect(2.0, 1.0));
    }

    #[test]
    fn close_path_drops_duplicate_start() {
        let g = geo();
        let mut path = rect(2.0, 1.0);
        path.push([0.0, 0.0, 0.0]);
        assert_eq!(g.close_path(&path).unwrap(), rect(2.0, 1.0));
    }

    #[test]
    fn close_path_rejects_short_and_flat() {
        let g = geo();
        assert!(g.close_path(&[[0.0; 3], [1.0, 0.0, 0.0]]).is_none());
        let back = vec![[0.0; 3], [2.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert!(g.close_path(&back).is_none());
    }

    #[test]
    fn overlap_only_for_positive_overlap() {
        let g = geo();
        let path = vec![[0.0; 3], [2.0, 0.0, 0.0]];
        assert!(g.overlaps_path(&path, &[1.0, 0.0, 0.0]));
        assert!(!g.overlaps_path(&path, &[3.0, 0.0, 0.0]));
        assert!(!g.overlaps_path(&path, &[2.0, 1.0, 0.0]));
    }

    #[test]
    fn shifted_segment_moves_edge() {
        let g = geo();
        let r = rect(4.0, 3.0);
        let moved = g.shifted_segment(&r, 1, 1.0, true);
        assert_eq!(moved, rect(5.0, 3.0));
        let limited = g.shifted_segment(&r, 1, -10.0, true);
        assert!(limited[1][0] > 0.0 && limited[1][0] < 0.001);
        assert_eq!(r, rect(4.0, 3.0));
    }

    #[test]
    fn resized_segment_moves_end_side() {
        let g = geo();
        let r = rect(4.0, 3.0);
        let out = g.resized_segment(&r, 0, 5.0, true).unwrap();
        assert_eq!(out, rect(5.0, 3.0));
        let out = g.resized_segment(&r, 2, 6.0, true).unwrap();
        assert!((g.segment_length(&out, 2) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn open_path_keeps_origin() {
        let g = geo();
        let path = vec![[0.0; 3], [2.0, 0.0, 0.0]];
        let out = g.resized_segment(&path, 0, 3.0, false).unwrap();
        assert_eq!(out, vec![[0.0; 3], [3.0, 0.0, 0.0]]);
        let back = vec![[0.0; 3], [-2.0, 0.0, 0.0]];
        let out = g.resized_segment(&back, 0, 3.0, false).unwrap();
        assert_eq!(out, vec![[0.0; 3], [-3.0, 0.0, 0.0]]);
    }

    #[test]
    fn select_edge_rates() {
        let g = geo();
        let r = rect(4.0, 3.0);
        let s = g.select_edge(&r, 1);
        assert_eq!(s.axis, 0);
        assert_eq!(s.affected, vec![(0, 1.0), (2, 1.0)]);
        let out = g.resize_from_edge(&r, &s, 0, 6.0).unwrap();
        assert_eq!(out, rect(6.0, 3.0));
        assert_eq!(g.resize_from_edge(&r, &s, 1, 6.0), Err(KernError::NotAffectedByEdge));
    }

    #[test]
    fn clockwise() {
        let path = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, -4.0, 0.0], [0.0, -4.0, 0.0]];
        assert!(is_clockwise(&path));
        let reversed: Vec<Point3> = path.iter().rev().copied().collect();
        assert!(!is_clockwise(&reversed));
    }

    #[test]
    fn drafts() {
        let g = geo();
        assert_eq!(g.next_draft(&[[0.0; 3]]), Some([1.0, 0.0, 0.0]));
        assert_eq!(
            g.next_draft(&[[0.0; 3], [2.0, 0.0, 0.0]]),
            Some([2.0, -1.0, 0.0])
        );
        let d = g.orthogonal_draft(&[[0.0; 3]], &[3.0, 1.0, 0.0], None, 1.4);
        assert_eq!(d.point, Some([3.0, 0.0, 0.0]));
        let d = g.orthogonal_draft(&[[0.0; 3]], &[1.0, 1.3, 0.0], Some(0), 1.4);
        assert_eq!(d.axis, 0);
        let path = vec![[0.0; 3], [2.0, 0.0, 0.0]];
        let d = g.orthogonal_draft(&path, &[1.0, 0.1, 0.0], None, 1.4);
        assert!(d.blocked && d.point.is_none());
    }
}
