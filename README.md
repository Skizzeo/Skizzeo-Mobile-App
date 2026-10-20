# Skizzeo Mobile App

Rust-Nachbau des Prototyps „3D-Kern · Viewport · V039“.

- `crates/skizzeo-kern` – Rechenkern (Geometrie, Maßketten, Kamera, Formatierung), mit Tests.
- `crates/skizzeo-app` – Anwendung mit egui/eframe: 3D-Darstellung, Bedienung, Einstellungen.
- `web/` – HTML-Hülle, App-Symbol und Manifest für den Startbildschirm des iPhones.

Jeder Push baut die Web-Version (WebAssembly) und veröffentlicht sie über GitHub Pages
(`.github/workflows/pages.yml`). Geöffnete Seiten laden eine neue Version selbst nach.

Lokal: `cargo run -p skizzeo-app` (Desktop-Fenster) oder `./scripts/build-web.sh` (nach `dist/`).

Lokale Entwicklung mit Sofort-Vorschau: `Skizzeo-Dev-Start.cmd` doppelklicken oder
`python scripts/dev.py` starten und http://localhost:8080 öffnen. Jede gespeicherte Änderung
wird in wenigen Sekunden neu gebaut, die offene Seite lädt sich selbst nach.
Einmalig nötig: `rustup target add wasm32-unknown-unknown` und
`cargo install wasm-bindgen-cli --version 0.2.129 --locked`.
