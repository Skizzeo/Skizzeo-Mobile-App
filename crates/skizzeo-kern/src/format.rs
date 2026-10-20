//! Bauzahlenformat und Metereingabe (V030).

use crate::js_round;

/// Rundet positive Meterlängen auf ganze 5-mm-Schritte (Ergebnis in Millimetern).
pub fn building_millimeters(length: f64) -> i64 {
    js_round(length * 200.0 + 1e-8) as i64 * 5
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_and_display() {
        assert_eq!(building_millimeters(2.0149), 2015);
        assert_eq!(building_millimeters(2.0124), 2010);
        assert_eq!(building_millimeters(0.0024), 0);
        assert_eq!(building_millimeters(0.0025), 5);
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
