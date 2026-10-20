//! Skizzeo · 3D-Kern Viewport V039 als Rust-Anwendung.

mod app;
mod controller;
mod fonts;
mod glyphs;
mod model;
mod platform;
mod render;
mod style;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("3D-Kern · Viewport · V039")
            .with_inner_size([390.0, 844.0]),
        multisampling: 4,
        ..Default::default()
    };
    eframe::run_native(
        "3D-Kern · Viewport · V039",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    eframe::WebLogger::init(log::LevelFilter::Warn).ok();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .expect("Dokument");
        let canvas = document
            .get_element_by_id("viewport")
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("Zeichenfläche");
        let options = eframe::WebOptions {
            dithering: false,
            ..Default::default()
        };
        let app_canvas = canvas.clone();
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(move |cc| Ok(Box::new(app::App::new(cc, app_canvas)))),
            )
            .await;
        if let Err(error) = result {
            if let Some(status) = document.get_element_by_id("error") {
                let message = error.as_string().unwrap_or_else(|| format!("{error:?}"));
                status.set_text_content(Some(&format!(
                    "V039 · Start der 3D-Ansicht fehlgeschlagen: {message}"
                )));
                let _ = status.remove_attribute("hidden");
            }
        }
    });
}
