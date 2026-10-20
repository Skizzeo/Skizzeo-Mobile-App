//! Bauzahlenformat und Metereingabe (V030).

use skizzeo_kern::js_round;
use skizzeo_kern::units::building_millimeters;

/// Metertext für das Eingabefeld, z. B. `2,01⁵`.
pub fn format_length_input(length: f64) -> String {
    let mm = building_millimeters(length);
    format!(
        "{},{:02}{}",
        mm.div_euclid(1000),
        mm.rem_euclid(1000) / 10,
        if mm % 10 != 0 { "⁵" } else { "" }
    )
}

/// Anzeige an der Maßlinie: unter 1 m Zentimeter, sonst Meter.
pub fn format_building_length(length: f64) -> String {
    let mm = building_millimeters(length);
    if mm == 0 {
        return "<0,5".to_string();
    }
    let fraction = if mm % 10 != 0 { "⁵" } else { "" };
    if mm < 1000 {
        format!("{}{}", mm.div_euclid(10), fraction)
    } else {
        format!(
            "{},{:02}{}",
            mm.div_euclid(1000),
            mm.rem_euclid(1000) / 10,
            fraction
        )
    }
}

/// `Intl.NumberFormat('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 3 })`.
pub fn format_de_meters(value: f64) -> String {
    let negative = value < 0.0;
    let thousandths = js_round(value.abs() * 1000.0) as i64;
    let integer = thousandths / 1000;
    let mut fraction = format!("{:03}", thousandths % 1000);
    if fraction.ends_with('0') {
        fraction.pop();
    }
    let digits = integer.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    format!("{}{grouped},{fraction}", if negative { "-" } else { "" })
}

/// Zugängliche Meterangabe einer Länge (gerundet auf 5 mm).
pub fn spoken_length(length: f64) -> String {
    let mm = building_millimeters(length);
    if mm == 0 {
        "unter 0,005".to_string()
    } else {
        format_de_meters(mm as f64 / 1000.0)
    }
}

/// Zahl aus dem Längenfeld lesen (`2,50`, `+3.`, `,5`, `2,01⁵`); `None` bei ungültiger
/// Schreibweise. Ob die Länge zulässig ist, entscheidet der Kern.
pub fn parse_length_input(text: &str) -> Option<f64> {
    let text = js_trim(text);
    let superscript;
    let text = if is_superscript_input(text) {
        superscript = text.replace('⁵', "5");
        superscript.as_str()
    } else {
        text
    };
    let cleaned = js_trim(text);
    if !is_length_syntax(cleaned) {
        return None;
    }
    let normalized = cleaned.replacen(',', ".", 1);
    let normalized = normalized.strip_prefix('+').unwrap_or(&normalized);
    let normalized = if normalized.ends_with('.') {
        format!("{normalized}0")
    } else if normalized.starts_with('.') {
        format!("0{normalized}")
    } else {
        normalized.to_string()
    };
    Some(normalized.parse::<f64>().unwrap_or(f64::NAN))
}

/// `String.prototype.trim` (Leerraum und Zeilenumbrüche).
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// `/^\+?(?:\d+(?:[.,]\d*)?|[.,]\d+)$/`
fn is_length_syntax(text: &str) -> bool {
    let body = text.strip_prefix('+').unwrap_or(text);
    let bytes = body.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let is_sep = |b: u8| b == b'.' || b == b',';
    if is_sep(bytes[0]) {
        return bytes.len() > 1 && bytes[1..].iter().all(u8::is_ascii_digit);
    }
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return false;
    }
    let rest = &bytes[digits..];
    if rest.is_empty() {
        return true;
    }
    is_sep(rest[0]) && rest[1..].iter().all(u8::is_ascii_digit)
}

/// `/^\+?\d+[.,]\d{2}⁵$/`
fn is_superscript_input(text: &str) -> bool {
    let Some(body) = text.strip_suffix('⁵') else {
        return false;
    };
    let body = body.strip_prefix('+').unwrap_or(body);
    let bytes = body.as_bytes();
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || bytes.len() != digits + 3 {
        return false;
    }
    (bytes[digits] == b'.' || bytes[digits] == b',')
        && bytes[digits + 1].is_ascii_digit()
        && bytes[digits + 2].is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_length_rules() {
        assert_eq!(parse_length_input("2,50"), Some(2.5));
        assert_eq!(parse_length_input(" +3. "), Some(3.0));
        assert_eq!(parse_length_input(",5"), Some(0.5));
        assert_eq!(parse_length_input("2,01⁵"), Some(2.015));
        assert_eq!(parse_length_input("0"), Some(0.0));
        assert_eq!(parse_length_input("1e3"), None);
        assert_eq!(parse_length_input("-1"), None);
        assert_eq!(parse_length_input("1.000,5"), None);
        assert_eq!(parse_length_input("2,0⁵"), None);
        assert_eq!(parse_length_input(""), None);
    }

    #[test]
    fn display() {
        assert_eq!(format_length_input(2.015), "2,01⁵");
        assert_eq!(format_length_input(0.5), "0,50");
        assert_eq!(format_building_length(0.995), "99⁵");
        assert_eq!(format_building_length(0.9999), "1,00");
        assert_eq!(format_building_length(12.3), "12,30");
        assert_eq!(format_building_length(0.001), "<0,5");
        assert_eq!(format_de_meters(2.015), "2,015");
        assert_eq!(format_de_meters(2.0), "2,00");
        assert_eq!(format_de_meters(1234.5), "1.234,50");
    }
}
