//! Deutsche Meldungen und Bezeichnungen für die Fehlerarten und Maße des Kerns.

use skizzeo_kern::measurements::{ChainKind, ChainRecord, Side};
use skizzeo_kern::KernError;

pub const MSG_NUMBER: &str = "Bitte eine Zahl eingeben, zum Beispiel 2,50.";
pub const MSG_INACTIVE_SEGMENT: &str = "Der Zeichenabschnitt ist nicht mehr aktiv.";

pub fn kern_error(error: KernError) -> &'static str {
    match error {
        KernError::NonPositiveLength => "Bitte eine positive Länge eingeben.",
        KernError::BelowBuildingStep => "Bitte mindestens 0,005 m eingeben (5-mm-Schritte).",
        KernError::ReversesSegment => "Diese Länge würde ein Anschlusssegment umkehren oder aufheben.",
        KernError::NotAffectedByEdge => "Dieses Maß ist von der gewählten Kante nicht betroffen.",
        KernError::ChainReversesSegment => "Dieses Maß würde ein Anschlusssegment umkehren oder aufheben.",
        KernError::ChainDirection => "Das Maß kann in dieser Zugrichtung nicht übernommen werden.",
        KernError::RetracesPath => "Dieses Segment würde eine bereits gesetzte Strecke zurückfahren.",
    }
}

pub fn side_name(side: Side) -> &'static str {
    match side {
        Side::South => "Süd",
        Side::East => "Ost",
        Side::North => "Nord",
        Side::West => "West",
    }
}

pub fn chain_kind_name(kind: ChainKind) -> &'static str {
    match kind {
        ChainKind::Offset => "Versatzmaß",
        ChainKind::Total => "Gesamtmaß",
    }
}

/// Bezeichnung einer Maßkette, z. B. `Süd · Gesamtmaß`.
pub fn chain_name(record: &ChainRecord) -> String {
    format!("{} · {}", side_name(record.side), chain_kind_name(record.kind))
}
