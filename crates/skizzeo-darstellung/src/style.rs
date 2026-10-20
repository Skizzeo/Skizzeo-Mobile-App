//! 07-settings: Schema, Validierung und Standard der Darstellungseinstellungen.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::{Map, Value as Json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Dimensions,
    View,
    Input,
    Program,
}

pub enum Kind {
    Number {
        value: f64,
        min: f64,
        max: f64,
        #[allow(dead_code)] // Schrittweite der Eingabefelder (Schema)
        step: f64,
        factor: f64,
    },
    Choice(&'static str, &'static [(&'static str, &'static str)]),
    Color(&'static str),
}

pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub tab: Tab,
    pub kind: Kind,
}

impl Field {
    pub fn default_value(&self) -> Value {
        match self.kind {
            Kind::Number { value, .. } => Value::Number(value),
            Kind::Choice(value, _) | Kind::Color(value) => Value::Text(value.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
}

pub static FIELDS: &[Field] = &[
    Field { key: "dimensionOffset", label: "Abstand zum Polygon (m)", tab: Tab::Dimensions, kind: Kind::Number { value: 1.5, min: 0.1, max: 100.0, step: 0.1, factor: 1.0 } },
    Field { key: "dimensionSpacing", label: "Abstand der Maßketten (m)", tab: Tab::Dimensions, kind: Kind::Number { value: 1.0, min: 0.1, max: 100.0, step: 0.1, factor: 1.0 } },
    Field { key: "extensionGap", label: "Lücke der Maßhilfslinie (m)", tab: Tab::Dimensions, kind: Kind::Number { value: 0.2, min: 0.0, max: 20.0, step: 0.05, factor: 1.0 } },
    Field { key: "extensionOverhang", label: "Überstand der Maßhilfslinie (m)", tab: Tab::Dimensions, kind: Kind::Number { value: 0.2, min: 0.0, max: 20.0, step: 0.05, factor: 1.0 } },
    Field { key: "dimensionOverhang", label: "Überstand der Maßlinie (m)", tab: Tab::Dimensions, kind: Kind::Number { value: 0.2, min: 0.0, max: 20.0, step: 0.05, factor: 1.0 } },
    Field { key: "symbol", label: "Maßbegrenzungssymbol", tab: Tab::Dimensions, kind: Kind::Choice("slash", &[("slash", "Schrägstrich 45°"), ("circle", "Offener Kreis")]) },
    Field { key: "tickLength", label: "Größe des Begrenzungssymbols (m)", tab: Tab::Dimensions, kind: Kind::Number { value: 0.4, min: 0.05, max: 10.0, step: 0.05, factor: 1.0 } },
    Field { key: "dimensionLineWidth", label: "Linienstärke (mm bei 1:100)", tab: Tab::Dimensions, kind: Kind::Number { value: 0.013, min: 0.005, max: 0.2, step: 0.01, factor: 10.0 } },
    Field { key: "lineMinimumPixels", label: "Mindestlinienstärke am Bildschirm (px)", tab: Tab::Dimensions, kind: Kind::Number { value: 0.8, min: 0.5, max: 5.0, step: 0.1, factor: 1.0 } },
    Field { key: "symbolWeight", label: "Strichstärke Begrenzungssymbol (Faktor)", tab: Tab::Dimensions, kind: Kind::Number { value: 1.8, min: 1.0, max: 4.0, step: 0.1, factor: 1.0 } },
    Field { key: "font", label: "Schriftart / Schnitt", tab: Tab::Dimensions, kind: Kind::Choice("osifont", &[("osifont", "OSIFont"), ("narrow", "OSIFont schmal"), ("mono", "OSIFont mit fester Zeichenbreite")]) },
    Field { key: "dimensionTextHeight", label: "Schrifthöhe auf der Grundebene (m)", tab: Tab::Dimensions, kind: Kind::Number { value: 0.6, min: 0.1, max: 10.0, step: 0.05, factor: 1.0 } },
    Field { key: "textGap", label: "Abstand Schrift zur Maßlinie (m)", tab: Tab::Dimensions, kind: Kind::Number { value: 0.08, min: 0.0, max: 5.0, step: 0.02, factor: 1.0 } },
    Field { key: "hitSize", label: "Gewünschte Touch-Trefferfläche (px)", tab: Tab::Dimensions, kind: Kind::Number { value: 48.0, min: 44.0, max: 72.0, step: 2.0, factor: 1.0 } },
    Field { key: "buttonBackground", label: "Button: Hintergrund", tab: Tab::View, kind: Kind::Color("#20262d") },
    Field { key: "buttonText", label: "Button: Schrift", tab: Tab::View, kind: Kind::Color("#e6e5e0") },
    Field { key: "buttonBorder", label: "Button: Rahmen", tab: Tab::View, kind: Kind::Color("#36424e") },
    Field { key: "buttonActive", label: "Aktiver Button: Hintergrund", tab: Tab::View, kind: Kind::Color("#f5b638") },
    Field { key: "buttonActiveText", label: "Aktiver Button: Schrift", tab: Tab::View, kind: Kind::Color("#20262d") },
    Field { key: "buttonHover", label: "Hover: Hintergrund", tab: Tab::View, kind: Kind::Color("#2a333d") },
    Field { key: "buttonHoverBorder", label: "Hover: Rahmen", tab: Tab::View, kind: Kind::Color("#516171") },
    Field { key: "buttonActiveHover", label: "Aktiver Button: Hover", tab: Tab::View, kind: Kind::Color("#ffc34b") },
    Field { key: "buttonRadius", label: "Button: Eckenradius (px)", tab: Tab::View, kind: Kind::Number { value: 8.0, min: 0.0, max: 24.0, step: 1.0, factor: 1.0 } },
    Field { key: "buttonFontSize", label: "Button: Schriftgröße (px)", tab: Tab::View, kind: Kind::Number { value: 18.0, min: 14.0, max: 24.0, step: 1.0, factor: 1.0 } },
    Field { key: "hoverDuration", label: "Hover: Übergang (ms)", tab: Tab::View, kind: Kind::Number { value: 150.0, min: 0.0, max: 600.0, step: 10.0, factor: 1.0 } },
    Field { key: "hoverLift", label: "Hover: Anheben (px)", tab: Tab::View, kind: Kind::Number { value: 0.0, min: 0.0, max: 6.0, step: 1.0, factor: 1.0 } },
    Field { key: "uiFont", label: "Schrift der Oberfläche", tab: Tab::View, kind: Kind::Choice("system", &[("system", "Systemschrift"), ("arial", "Arial / Sans Serif"), ("serif", "Georgia / Serif"), ("mono", "Monospace"), ("osifont", "OSIFont")]) },
    Field { key: "panelBackground", label: "Panel: Hintergrund", tab: Tab::View, kind: Kind::Color("#20262d") },
    Field { key: "panelText", label: "Panel: Schrift", tab: Tab::View, kind: Kind::Color("#e6e5e0") },
    Field { key: "inputBackground", label: "Eingabefelder: Hintergrund", tab: Tab::View, kind: Kind::Color("#171d23") },
    Field { key: "groundColor", label: "Boden: Farbe", tab: Tab::View, kind: Kind::Color("#393f33") },
    Field { key: "groundMode", label: "Bodendarstellung", tab: Tab::View, kind: Kind::Choice("flat", &[("flat", "Einfarbig"), ("gradient", "Farbverlauf")]) },
    Field { key: "groundNear", label: "Boden: Nahfarbe im Verlauf", tab: Tab::View, kind: Kind::Color("#2b3027") },
    Field { key: "horizonVisible", label: "Horizont und Himmel", tab: Tab::View, kind: Kind::Choice("on", &[("on", "Anzeigen"), ("off", "Ausblenden")]) },
    Field { key: "horizonColor", label: "Horizont: Farbe", tab: Tab::View, kind: Kind::Color("#708397") },
    Field { key: "skyColor", label: "Himmel: obere Farbe", tab: Tab::View, kind: Kind::Color("#506173") },
    Field { key: "horizonSoftness", label: "Horizont: weicher Übergang (px)", tab: Tab::View, kind: Kind::Number { value: 1.0, min: 1.0, max: 100.0, step: 1.0, factor: 1.0 } },
    Field { key: "axesMode", label: "Achsendarstellung", tab: Tab::View, kind: Kind::Choice("both", &[("both", "Positive und negative Achsen"), ("positive", "Nur positive Achsen")]) },
    Field { key: "axisLabels", label: "Achsbeschriftung", tab: Tab::View, kind: Kind::Choice("on", &[("on", "Anzeigen"), ("off", "Ausblenden")]) },
    Field { key: "axisSize", label: "Achslänge je Richtung (m)", tab: Tab::View, kind: Kind::Number { value: 2.5, min: 0.1, max: 100.0, step: 0.1, factor: 1.0 } },
    Field { key: "axisWidth", label: "Achslinienstärke (px)", tab: Tab::View, kind: Kind::Number { value: 1.0, min: 0.5, max: 8.0, step: 0.5, factor: 1.0 } },
    Field { key: "axisX", label: "X-Achse: Farbe", tab: Tab::View, kind: Kind::Color("#ad7373") },
    Field { key: "axisY", label: "Y-Achse: Farbe", tab: Tab::View, kind: Kind::Color("#709182") },
    Field { key: "axisZ", label: "Z-Achse: Farbe", tab: Tab::View, kind: Kind::Color("#6e8cad") },
    Field { key: "polygonColor", label: "Polygon: Linienfarbe", tab: Tab::Input, kind: Kind::Color("#f5b638") },
    Field { key: "polygonWidth", label: "Polygon: Linienstärke (px)", tab: Tab::Input, kind: Kind::Number { value: 2.0, min: 0.5, max: 12.0, step: 0.5, factor: 1.0 } },
    Field { key: "vertexColor", label: "Eckpunkte: Farbe", tab: Tab::Input, kind: Kind::Color("#f5b638") },
    Field { key: "vertexRadius", label: "Eckpunkte: Radius (px)", tab: Tab::Input, kind: Kind::Number { value: 3.0, min: 0.0, max: 12.0, step: 0.5, factor: 1.0 } },
    Field { key: "hoverColor", label: "Segment-Hover: Farbe", tab: Tab::Input, kind: Kind::Color("#c28cff") },
    Field { key: "editColor", label: "Bearbeitung: Highlight-Farbe", tab: Tab::Input, kind: Kind::Color("#c28cff") },
    Field { key: "highlightWidth", label: "Hover / Bearbeitung: Linienstärke (px)", tab: Tab::Input, kind: Kind::Number { value: 4.0, min: 1.0, max: 16.0, step: 0.5, factor: 1.0 } },
    Field { key: "highlightRadius", label: "Hover / Bearbeitung: Punktradius (px)", tab: Tab::Input, kind: Kind::Number { value: 5.0, min: 0.0, max: 16.0, step: 0.5, factor: 1.0 } },
    Field { key: "previewColor", label: "Laufendes Segment: Farbe", tab: Tab::Input, kind: Kind::Color("#c28cff") },
    Field { key: "previewWidth", label: "Laufendes Segment: Linienstärke (px)", tab: Tab::Input, kind: Kind::Number { value: 2.0, min: 0.5, max: 12.0, step: 0.5, factor: 1.0 } },
    Field { key: "dashLength", label: "Vorschau: Strichlänge (px)", tab: Tab::Input, kind: Kind::Number { value: 8.0, min: 1.0, max: 40.0, step: 1.0, factor: 1.0 } },
    Field { key: "dashGap", label: "Vorschau: Strichabstand (px)", tab: Tab::Input, kind: Kind::Number { value: 6.0, min: 0.0, max: 40.0, step: 1.0, factor: 1.0 } },
    Field { key: "startColor", label: "Startpunkt: Farbe", tab: Tab::Input, kind: Kind::Color("#f5b638") },
    Field { key: "startTargetScale", label: "Start-Schaltfläche: Größe (%)", tab: Tab::Input, kind: Kind::Number { value: 0.6, min: 0.3, max: 1.5, step: 5.0, factor: 100.0 } },
    Field { key: "startRadius", label: "Startpunkt: Radius (px)", tab: Tab::Input, kind: Kind::Number { value: 6.0, min: 2.0, max: 20.0, step: 0.5, factor: 1.0 } },
    Field { key: "editFill", label: "Bearbeitung: Feldhintergrund", tab: Tab::Input, kind: Kind::Color("#8052bb") },
    Field { key: "editHover", label: "Bearbeitung: Feld-Hover", tab: Tab::Input, kind: Kind::Color("#9563da") },
    Field { key: "editText", label: "Bearbeitung: Feldschrift / Fokus", tab: Tab::Input, kind: Kind::Color("#d5b5ff") },
    Field { key: "numberPadScale", label: "Nummernblock: Größe (%)", tab: Tab::Program, kind: Kind::Number { value: 0.75, min: 0.5, max: 1.25, step: 5.0, factor: 100.0 } },
    Field { key: "axesDefault", label: "Achsen beim Öffnen", tab: Tab::Program, kind: Kind::Choice("off", &[("off", "Aus"), ("on", "Ein")]) },
    Field { key: "fitMargin", label: "Einpassrand Hochformat (%)", tab: Tab::Program, kind: Kind::Number { value: 0.08, min: 0.0, max: 0.4, step: 1.0, factor: 100.0 } },
    Field { key: "fitMarginLandscape", label: "Einpassrand Querformat (%)", tab: Tab::Program, kind: Kind::Number { value: 0.04, min: 0.0, max: 0.4, step: 1.0, factor: 100.0 } },
];

fn index() -> &'static HashMap<&'static str, usize> {
    static INDEX: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    INDEX.get_or_init(|| FIELDS.iter().enumerate().map(|(i, f)| (f.key, i)).collect())
}

pub fn field_index(key: &str) -> usize {
    *index()
        .get(key)
        .unwrap_or_else(|| panic!("unbekannte Einstellung {key}"))
}

/// Validierte Laufzeitkonfiguration (appearance).
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub values: Vec<Value>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            values: FIELDS.iter().map(Field::default_value).collect(),
        }
    }
}

impl Appearance {
    pub fn num(&self, key: &str) -> f64 {
        match &self.values[field_index(key)] {
            Value::Number(v) => *v,
            Value::Text(_) => f64::NAN,
        }
    }

    pub fn text(&self, key: &str) -> &str {
        match &self.values[field_index(key)] {
            Value::Text(v) => v,
            Value::Number(_) => "",
        }
    }

    pub fn rgb(&self, key: &str) -> [f64; 3] {
        hex_rgb(self.text(key))
    }

    pub fn to_json(&self) -> Json {
        let mut map = Map::new();
        for (field, value) in FIELDS.iter().zip(&self.values) {
            map.insert(
                field.key.to_string(),
                match value {
                    Value::Number(v) => serde_json::Number::from_f64(*v).map_or(Json::Null, Json::Number),
                    Value::Text(t) => Json::String(t.clone()),
                },
            );
        }
        Json::Object(map)
    }
}

/// Teilweise Werte (Schlüssel -> JSON-Wert), wie `{ ...appearance, ...values }`.
pub type Partial = Map<String, Json>;

pub fn hex_rgb(hex: &str) -> [f64; 3] {
    let channel = |offset: usize| {
        hex.get(offset..offset + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .map_or(f64::NAN, |v| f64::from(v) / 255.0)
    };
    [channel(1), channel(3), channel(5)]
}

/// 8-Bit-Farbe (Rot, Grün, Blau) ohne Bindung an eine Grafikbibliothek.
pub type Rgb8 = [u8; 3];

/// Normalisierte Komponenten wie im Shader in 8-Bit-Farbe.
pub fn rgb_color(c: [f64; 3]) -> Rgb8 {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(c[0]), byte(c[1]), byte(c[2])]
}

fn is_hex_color(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// `validatedStyle`: prüft das vollständige Schema.
pub fn validated(values: &Partial) -> Result<Appearance, String> {
    let mut result = Vec::with_capacity(FIELDS.len());
    for field in FIELDS {
        let value = values.get(field.key);
        match &field.kind {
            Kind::Color(_) => match value {
                Some(Json::String(text)) if is_hex_color(text) => result.push(Value::Text(text.clone())),
                _ => return Err(format!("{}: Ungültige Farbe.", field.label)),
            },
            Kind::Choice(_, options) => match value {
                Some(Json::String(text)) if options.iter().any(|(o, _)| o == text) => {
                    result.push(Value::Text(text.clone()));
                }
                _ => return Err(format!("{}: ungültige Auswahl.", field.label)),
            },
            Kind::Number { min, max, .. } => match value.and_then(Json::as_f64) {
                Some(number) if number.is_finite() && number >= *min && number <= *max => {
                    result.push(Value::Number(number));
                }
                _ => {
                    return Err(format!(
                        "{}: Wert außerhalb des zulässigen Bereichs.",
                        field.label
                    ))
                }
            },
        }
    }
    Ok(Appearance { values: result })
}

/// `{ ...base, ...values }` als JSON-Map.
pub fn merged(base: &Appearance, values: &Partial) -> Partial {
    let Json::Object(mut map) = base.to_json() else {
        unreachable!()
    };
    for (key, value) in values {
        map.insert(key.clone(), value.clone());
    }
    map
}

pub fn defaults_json() -> Partial {
    let Json::Object(map) = Appearance::default().to_json() else {
        unreachable!()
    };
    map
}

pub fn migrated(values: &Partial) -> Result<Appearance, String> {
    let mut next = merged(&Appearance::default(), values);
    if matches!(next.get("symbol"), Some(Json::String(s)) if s == "dot" || s == "arrow") {
        next.insert("symbol".into(), Json::String("circle".into()));
    }
    if next.get("dimensionLineWidth").and_then(Json::as_f64) == Some(0.025) {
        next.insert("dimensionLineWidth".into(), serde_json::json!(0.013));
    }
    validated(&next)
}

pub const STORAGE_KEY: &str = "3d-kern.style.v1";
pub const LEGACY_KEYS: [&str; 2] = ["3d-kern.dimension-style.v2", "3d-kern.dimension-style.v1"];

/// `loadStyleStandard` ohne eingebettetes Profil.
pub fn load_standard(get: impl Fn(&str) -> Option<String>) -> Appearance {
    let mut standard = Appearance::default();
    let parse = |text: &str| serde_json::from_str::<Json>(text).ok();
    if let Some(stored) = get(STORAGE_KEY) {
        if let Some(Json::Object(values)) = parse(&stored) {
            if let Ok(next) = validated(&merged(&standard, &values)) {
                standard = next;
            }
        }
    } else if let Some(legacy) = LEGACY_KEYS.iter().find_map(|key| get(key)) {
        if let Some(Json::Object(values)) = parse(&legacy) {
            if let Ok(next) = migrated(&values) {
                standard = next;
            }
        }
    }
    standard
}

/// Anzeigetext eines Zahlenfeldes: `String(Number((v * factor).toFixed(6)))`.
pub fn number_text(value: f64, factor: f64) -> String {
    let rounded: f64 = format!("{:.6}", value * factor).parse().unwrap_or(f64::NAN);
    js_number_string(rounded)
}

pub fn js_number_string(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let text = format!("{value}");
    text
}

/// `Number(text.trim() ? text.replace(',', '.') : NaN)`.
pub fn parse_number_input(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return f64::NAN;
    }
    let normalized = trimmed.replacen(',', ".", 1);
    let lower = normalized.to_ascii_lowercase();
    if lower.contains("inf") || lower.contains("nan") {
        return if normalized == "Infinity" || normalized == "+Infinity" {
            f64::INFINITY
        } else if normalized == "-Infinity" {
            f64::NEG_INFINITY
        } else {
            f64::NAN
        };
    }
    normalized.parse::<f64>().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_66_fields() {
        assert_eq!(FIELDS.len(), 66);
        assert!(validated(&defaults_json()).is_ok());
    }

    #[test]
    fn migration() {
        let mut legacy = Partial::new();
        legacy.insert("symbol".into(), Json::String("arrow".into()));
        legacy.insert("dimensionLineWidth".into(), serde_json::json!(0.025));
        legacy.insert("textPixels".into(), serde_json::json!(12));
        let a = migrated(&legacy).unwrap();
        assert_eq!(a.text("symbol"), "circle");
        assert_eq!(a.num("dimensionLineWidth"), 0.013);
    }

    #[test]
    fn number_text_matches_js() {
        assert_eq!(number_text(0.75, 100.0), "75");
        assert_eq!(number_text(0.013, 10.0), "0.13");
        assert_eq!(number_text(1.5, 1.0), "1.5");
        assert_eq!(number_text(0.08, 100.0), "8");
    }
}
