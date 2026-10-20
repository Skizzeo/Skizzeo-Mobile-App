//! GLYPH_MESH: offline triangulierte OSIFont-Konturen der Maßzahlen.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Glyph {
    pub advance: f64,
    pub triangles: Vec<f64>,
}

pub fn glyph_mesh() -> &'static HashMap<char, Glyph> {
    static MESH: OnceLock<HashMap<char, Glyph>> = OnceLock::new();
    MESH.get_or_init(|| {
        let raw: HashMap<String, Glyph> =
            serde_json::from_str(include_str!("../assets/glyph_mesh.json")).expect("Glyphendaten");
        raw.into_iter()
            .filter_map(|(key, glyph)| key.chars().next().map(|c| (c, glyph)))
            .collect()
    })
}

pub fn glyph(c: char) -> Option<&'static Glyph> {
    glyph_mesh().get(&c)
}
