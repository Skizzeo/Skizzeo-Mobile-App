//! Fehlerarten des Kerns. Die Oberfläche übersetzt sie in Meldungen.

/// Grund, aus dem der Kern eine Änderung ablehnt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernError {
    /// Länge ist keine positive endliche Zahl.
    NonPositiveLength,
    /// Länge liegt unter dem kleinsten Bauraster-Schritt (5 mm).
    BelowBuildingStep,
    /// Ein Anschlusssegment würde umgekehrt oder aufgehoben.
    ReversesSegment,
    /// Das Maß ist von der gewählten Kante nicht betroffen.
    NotAffectedByEdge,
    /// Eine Maßkette würde ein Anschlusssegment umkehren oder aufheben.
    ChainReversesSegment,
    /// Die Maßkette lässt sich in dieser Zugrichtung nicht übernehmen.
    ChainDirection,
    /// Ein neues Segment würde eine gesetzte Strecke zurückfahren.
    RetracesPath,
}
