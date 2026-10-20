//! Gebäudemodell: bestätigte Grundrisse und der offene Zug in Eingabe.

use crate::geometry::Geometry;
use crate::Point3;

/// Voreingestellte Länge der ersten Geraden in Metern (Vorschau und Zahlenwert).
pub const START_SEGMENT: f64 = 10.0;

/// Bestätigtes geschlossenes Polygon mit stabiler Identität.
#[derive(Clone, Debug)]
pub struct Polygon {
    pub id: u64,
    pub points: Vec<Point3>,
}

/// Verweis auf einen Pfad (Ersatz für die Array-Identitäten des Prototyps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKey {
    Polygon(u64),
    /// Offener Zug mit Generationsnummer (neue Zuweisung = neue Identität).
    Active(u64),
    /// Eigenständiger Vorschaupfad außerhalb des Modells.
    Temp,
}

/// Ergebnis des Schließens: neues Polygon und der offene Zug davor.
#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    pub polygon: u64,
    pub path: Vec<Point3>,
}

#[derive(Debug, Default)]
pub struct Model {
    pub polygons: Vec<Polygon>,
    pub active: Vec<Point3>,
    pub active_gen: u64,
    pub next_id: u64,
}

impl Model {
    pub fn active_key(&self) -> PathKey {
        PathKey::Active(self.active_gen)
    }

    /// `model.activePath = ...` erzeugt eine neue Identität.
    pub fn replace_active(&mut self, points: Vec<Point3>) {
        self.active = points;
        self.active_gen += 1;
    }

    /// Neuer Zug: nur der Startpunkt, die erste Gerade gibt der Nutzer selbst ein.
    pub fn start_path(&mut self) {
        self.replace_active(vec![[0.0, 0.0, 0.0]]);
    }

    pub fn clear_active(&mut self) {
        self.replace_active(Vec::new());
    }

    pub fn push_point(&mut self, point: Point3) {
        self.active.push(point);
    }

    /// Letzten Punkt des Zuges zurücknehmen; der Startpunkt bleibt.
    pub fn pop_point(&mut self) -> bool {
        if self.active.len() > 1 {
            self.active.pop();
            true
        } else {
            false
        }
    }

    pub fn path(&self, key: PathKey) -> Option<&Vec<Point3>> {
        match key {
            PathKey::Polygon(id) => self.polygons.iter().find(|p| p.id == id).map(|p| &p.points),
            PathKey::Active(generation) if generation == self.active_gen => Some(&self.active),
            _ => None,
        }
    }

    pub fn path_mut(&mut self, key: PathKey) -> Option<&mut Vec<Point3>> {
        match key {
            PathKey::Polygon(id) => self
                .polygons
                .iter_mut()
                .find(|p| p.id == id)
                .map(|p| &mut p.points),
            PathKey::Active(generation) if generation == self.active_gen => Some(&mut self.active),
            _ => None,
        }
    }

    /// Pfad ersetzen; `false`, wenn der Verweis nicht mehr gilt.
    pub fn set_path(&mut self, key: PathKey, points: Vec<Point3>) -> bool {
        match self.path_mut(key) {
            Some(path) => {
                *path = points;
                true
            }
            None => false,
        }
    }

    pub fn has_polygon(&self, id: u64) -> bool {
        self.polygons.iter().any(|p| p.id == id)
    }

    pub fn push_polygon(&mut self, points: Vec<Point3>) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.polygons.push(Polygon { id, points });
        id
    }

    pub fn remove_polygon(&mut self, id: u64) {
        self.polygons.retain(|p| p.id != id);
    }

    /// Kann der offene Zug zu einem gültigen Polygon geschlossen werden?
    pub fn can_close(&self, geometry: &Geometry) -> bool {
        geometry.close_path(&self.active).is_some()
    }

    /// Offenen Zug als Polygon übernehmen. Der Zug selbst bleibt unverändert.
    pub fn close_active(&mut self, geometry: &Geometry) -> Option<Closure> {
        let closed = geometry.close_path(&self.active)?;
        let mut path = self.active.clone();
        if path.len() > 1 && geometry.same_point(&path[path.len() - 1], &path[0]) {
            path.pop();
        }
        let polygon = self.push_polygon(closed);
        Some(Closure { polygon, path })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SETTINGS;

    #[test]
    fn start_close_and_reopen() {
        let geometry = Geometry::new(&SETTINGS);
        let mut model = Model::default();
        model.start_path();
        assert_eq!(model.active, vec![[0.0; 3]]);
        model.push_point([START_SEGMENT, 0.0, 0.0]);
        assert!(!model.can_close(&geometry));
        model.push_point([START_SEGMENT, -4.0, 0.0]);
        let closure = model.close_active(&geometry).expect("schließbar");
        assert_eq!(model.polygons.len(), 1);
        assert_eq!(model.polygons[0].points.len(), 4);
        assert_eq!(closure.path.len(), 3);
        model.remove_polygon(closure.polygon);
        assert!(!model.has_polygon(closure.polygon));
    }

    #[test]
    fn identity_of_active_path() {
        let mut model = Model::default();
        model.start_path();
        let key = model.active_key();
        assert!(model.path(key).is_some());
        model.clear_active();
        assert!(model.path(key).is_none());
        assert!(!model.set_path(key, Vec::new()));
    }

    #[test]
    fn pop_keeps_start_point() {
        let mut model = Model::default();
        model.start_path();
        model.push_point([START_SEGMENT, 0.0, 0.0]);
        assert!(model.pop_point());
        assert!(!model.pop_point());
        assert_eq!(model.active.len(), 1);
    }
}
