//! Darstellung von Skizzeo: Kamera und Projektion, Aussehen der Maßlinien,
//! Zahlen- und Textformat, Darstellungseinstellungen sowie die Übersetzung eines
//! Frames in Zeichenaufrufe.
//!
//! Die Darstellung liest den Geometriekern, verändert ihn aber nie. Sie ist frei von
//! einer bestimmten Grafikbibliothek: [`render::render`] liefert neutrale Dreieckslisten.

pub mod camera;
pub mod dimension_geometry;
pub mod format;
pub mod glyphs;
pub mod render;
pub mod style;
pub mod texte;
