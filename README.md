# Skizzeo Mobile App

Rust-Nachbau des Prototyps „3D-Kern · Viewport · V039“.

## Aufbau

Die Pakete hängen nur in einer Richtung voneinander ab:
`skizzeo-kern` ← `skizzeo-darstellung` ← `skizzeo-editor` ← `skizzeo-app`.

- `crates/skizzeo-kern` – Geometriekern: Gebäudemodell (Grundrisse), Geometrie, Maßketten,
  Bauraster und Regeln. Keine Abhängigkeiten, kein Bildschirm, keine Texte; Fehler sind
  Fehlerarten (`KernError`).
- `crates/skizzeo-darstellung` – Kamera und Projektion, Aussehen der Maßlinien, Zahlen- und
  Textformat, Darstellungseinstellungen und Übersetzung eines Frames in neutrale
  Dreieckslisten. Keine Grafikbibliothek.
- `crates/skizzeo-editor` – Bedienlogik: Zeichenmodus, Zeiger und Gesten, Maßeditor,
  Einpassung. Ändert das Modell nur über den Kern, kennt keine Grafikbibliothek.
- `crates/skizzeo-app` – Plattformschicht mit egui/eframe: Fenster bzw. Browser, Eingaben,
  Bedienelemente und Zeichnen der Dreieckslisten.
- `web/` – HTML-Hülle, App-Symbol und Manifest für den Startbildschirm des iPhones.

`crates/skizzeo-app/tests/golden/scenario.txt` protokolliert einen vollständigen
Bedienablauf samt Prüfsumme der gezeichneten Dreiecke. Der Test
`bedienablauf_bleibt_gleich` schlägt an, sobald sich Verhalten oder Bild ändern; bei
gewollten Änderungen schreibt `SKIZZEO_GOLDEN=update cargo test -p skizzeo-app` es neu.

Jeder Push baut die Web-Version (WebAssembly) und veröffentlicht sie über GitHub Pages
(`.github/workflows/pages.yml`). Geöffnete Seiten laden eine neue Version selbst nach.

Lokal: `cargo run -p skizzeo-app` (Desktop-Fenster), `cargo test --workspace` (alle Tests) oder
`./scripts/build-web.sh` (nach `dist/`).

Lokale Entwicklung mit Sofort-Vorschau: `Skizzeo-Dev-Start.cmd` doppelklicken oder
`python scripts/dev.py` starten und http://localhost:8080 öffnen. Jede gespeicherte Änderung
wird in wenigen Sekunden neu gebaut, die offene Seite lädt sich selbst nach.
Einmalig nötig: `rustup target add wasm32-unknown-unknown` und
`cargo install wasm-bindgen-cli --version 0.2.129 --locked`.
