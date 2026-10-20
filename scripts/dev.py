#!/usr/bin/env python3
"""Lokale Entwicklung: baut die Web-Version bei jeder Änderung neu und lädt
den offenen Browser selbst nach.

Start:  python scripts/dev.py      (danach http://localhost:8080 öffnen)

Überwacht crates/, web/ und Cargo.toml. Nach einer Änderung wird mit dem
schnellen Profil `web-dev` gebaut, nach dist-dev/ geschrieben und die Seite
lädt sich innerhalb einer Sekunde neu. Schlägt der Bau fehl, bleibt die alte
Version offen und der Fehler steht im Fenster dieses Skripts.
Nur Standardbibliothek, keine zusätzlichen Pakete nötig.
"""

import http.server
import os
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "dist-dev"
WATCH = [ROOT / "crates", ROOT / "web", ROOT / "Cargo.toml"]
PORT = int(os.environ.get("PORT", "8080"))
TARGET = "wasm32-unknown-unknown"
PROFILE = "web-dev"

# Schnelle Nachladeprüfung nur für die lokale Entwicklung.
RELOAD_SNIPPET = """
    <script>
      (() => {
        let known = null;
        setInterval(async () => {
          try {
            const r = await fetch('version.txt?t=' + Date.now(), { cache: 'no-store' });
            const v = (await r.text()).trim();
            if (known === null) known = v;
            else if (v !== known) location.reload();
          } catch {}
        }, 700);
      })();
    </script>
  </body>"""


def snapshot():
    stamps = {}
    for base in WATCH:
        paths = [base] if base.is_file() else base.rglob("*")
        for p in paths:
            if p.is_file():
                try:
                    stamps[p] = p.stat().st_mtime_ns
                except OSError:
                    pass
    return stamps


def build():
    start = time.time()
    print(f"\n[{time.strftime('%H:%M:%S')}] Baue ...", flush=True)
    cargo = subprocess.run(
        ["cargo", "build", "-p", "skizzeo-app", "--profile", PROFILE, "--target", TARGET],
        cwd=ROOT,
    )
    if cargo.returncode != 0:
        print("Bau fehlgeschlagen, alte Version bleibt offen.", flush=True)
        return False
    staging = ROOT / "dist-dev.tmp"
    shutil.rmtree(staging, ignore_errors=True)
    staging.mkdir()
    wasm = ROOT / "target" / TARGET / PROFILE / "skizzeo-app.wasm"
    bindgen = subprocess.run(
        ["wasm-bindgen", "--target", "web", "--no-typescript", "--out-dir", str(staging), str(wasm)],
        cwd=ROOT,
    )
    if bindgen.returncode != 0:
        print("wasm-bindgen fehlgeschlagen.", flush=True)
        return False
    for f in (ROOT / "web").iterdir():
        if f.name != "index.html" and f.is_file():
            shutil.copy2(f, staging / f.name)
    version = f"dev-{int(time.time() * 1000)}"
    html = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
    html = html.replace("__BUILD__", version).replace("</body>", RELOAD_SNIPPET, 1)
    (staging / "index.html").write_text(html, encoding="utf-8")
    # version.txt zuletzt schreiben, damit der Browser erst nachlädt, wenn alles liegt.
    OUT.mkdir(exist_ok=True)
    for f in staging.iterdir():
        os.replace(f, OUT / f.name)
    staging.rmdir()
    (OUT / "version.txt").write_text(version, encoding="utf-8")
    print(f"Fertig in {time.time() - start:.1f} s. Browser lädt neu.", flush=True)
    return True


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(OUT), **kwargs)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


Handler.extensions_map[".wasm"] = "application/wasm"
Handler.extensions_map[".webmanifest"] = "application/manifest+json"


def main():
    build()
    server = http.server.ThreadingHTTPServer(("0.0.0.0", PORT), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    print(f"Skizzeo läuft auf http://localhost:{PORT}  (Beenden: Strg+C)", flush=True)
    last = snapshot()
    try:
        while True:
            time.sleep(0.4)
            now = snapshot()
            if now != last:
                time.sleep(0.3)  # Speichern mehrerer Dateien abwarten
                last = snapshot()
                build()
    except KeyboardInterrupt:
        server.shutdown()


if __name__ == "__main__":
    sys.exit(main())
