# Skizzeo Mobile App

Rust-Nachbau des Prototyps „3D-Kern · Viewport · V039“.

- `crates/skizzeo-kern` – Rechenkern (Geometrie, Maßketten, Kamera, Formatierung), mit Tests.
- `crates/skizzeo-app` – Anwendung mit egui/eframe: 3D-Darstellung, Bedienung, Einstellungen.
- `web/` – HTML-Hülle, App-Symbol und Manifest für den Startbildschirm des iPhones.

Jeder Push baut die Web-Version (WebAssembly) und veröffentlicht sie über GitHub Pages
(`.github/workflows/pages.yml`). Geöffnete Seiten laden eine neue Version selbst nach.

Lokal: `cargo run -p skizzeo-app` (Desktop-Fenster) oder `./scripts/build-web.sh` (nach `dist/`).
