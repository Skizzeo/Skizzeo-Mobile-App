//! Bauraster: Längen werden in ganzen 5-mm-Schritten geführt.

use crate::{js_round, KernError};

/// Kleinste zulässige Baulänge in Metern.
pub const MINIMUM_BUILDING_LENGTH: f64 = 0.005;

/// Rundet positive Meterlängen auf ganze 5-mm-Schritte (Ergebnis in Millimetern).
pub fn building_millimeters(length: f64) -> i64 {
    js_round(length * 200.0 + 1e-8) as i64 * 5
}

/// Eingegebene Länge auf das Bauraster bringen und prüfen.
pub fn building_length(length: f64) -> Result<f64, KernError> {
    let rounded = building_millimeters(length) as f64 / 1000.0;
    if rounded < MINIMUM_BUILDING_LENGTH {
        return Err(KernError::BelowBuildingStep);
    }
    Ok(rounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding() {
        assert_eq!(building_millimeters(2.0149), 2015);
        assert_eq!(building_millimeters(2.0124), 2010);
        assert_eq!(building_millimeters(0.0024), 0);
        assert_eq!(building_millimeters(0.0025), 5);
    }

    #[test]
    fn building_length_rules() {
        assert_eq!(building_length(2.0149), Ok(2.015));
        assert_eq!(building_length(0.0025), Ok(0.005));
        assert_eq!(building_length(0.001), Err(KernError::BelowBuildingStep));
    }
}
