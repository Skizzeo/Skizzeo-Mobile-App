//! Plattformanbindung: Zeigerereignisse, Speicher, Download, Medienabfragen, Safe Area.
//!
//! Im Browser werden die Pointer-Events des Prototyps direkt abgegriffen (inklusive
//! Pointer-Capture). Nativ (Entwicklung am PC) entstehen sie aus den egui-Ereignissen.

use skizzeo_editor::state::PointerType;

#[derive(Clone, Copy, Debug)]
pub struct RawPointer {
    pub id: i64,
    pub kind: PointerType,
    pub x: f64,
    pub y: f64,
    pub button: i16,
    pub buttons: u16,
}

#[derive(Clone, Copy, Debug)]
pub enum RawInput {
    Down(RawPointer),
    Move(RawPointer),
    Up(RawPointer),
    /// pointercancel / lostpointercapture
    Cancel(RawPointer),
    /// Zeiger hat die Zeichenfläche verlassen.
    Leave,
    Wheel { x: f64, y: f64, delta_y: f64, mode: u32 },
    /// Fenster verliert den Fokus.
    Blur,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[cfg(target_arch = "wasm32")]
pub use web::Platform;

#[cfg(not(target_arch = "wasm32"))]
pub use native::Platform;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::rc::Rc;

    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use super::{Insets, RawInput, RawPointer};
    use skizzeo_editor::state::PointerType;

    pub struct Platform {
        queue: Rc<RefCell<Vec<RawInput>>>,
        window: web_sys::Window,
        canvas: web_sys::HtmlCanvasElement,
        _listeners: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    }

    fn pointer_kind(event: &web_sys::PointerEvent) -> PointerType {
        match event.pointer_type().as_str() {
            "touch" => PointerType::Touch,
            "pen" => PointerType::Pen,
            _ => PointerType::Mouse,
        }
    }

    impl Platform {
        pub fn new(ctx: &egui::Context, canvas: web_sys::HtmlCanvasElement) -> Self {
            let window = web_sys::window().expect("Fenster");
            let queue: Rc<RefCell<Vec<RawInput>>> = Rc::default();
            let mut listeners = Vec::new();
            let options = web_sys::AddEventListenerOptions::new();
            options.set_capture(true);
            options.set_passive(false);

            let raw = {
                let canvas = canvas.clone();
                move |event: &web_sys::PointerEvent| {
                    let rect = canvas.get_bounding_client_rect();
                    RawPointer {
                        id: i64::from(event.pointer_id()),
                        kind: pointer_kind(event),
                        x: f64::from(event.client_x()) - rect.left(),
                        y: f64::from(event.client_y()) - rect.top(),
                        button: event.button(),
                        buttons: event.buttons(),
                    }
                }
            };
            let mut listen = |target: &web_sys::EventTarget,
                              name: &str,
                              handler: Box<dyn FnMut(web_sys::Event)>| {
                let closure = Closure::wrap(handler);
                target
                    .add_event_listener_with_callback_and_add_event_listener_options(
                        name,
                        closure.as_ref().unchecked_ref(),
                        &options,
                    )
                    .expect("Ereignis");
                listeners.push(closure);
            };
            let canvas_target: web_sys::EventTarget = canvas.clone().into();
            let window_target: web_sys::EventTarget = window.clone().into();
            for name in [
                "pointerdown",
                "pointermove",
                "pointerup",
                "pointercancel",
                "lostpointercapture",
            ] {
                let queue = queue.clone();
                let ctx = ctx.clone();
                let raw = raw.clone();
                let canvas = canvas.clone();
                listen(
                    &canvas_target,
                    name,
                    Box::new(move |event: web_sys::Event| {
                        let Some(event) = event.dyn_ref::<web_sys::PointerEvent>() else {
                            return;
                        };
                        let pointer = raw(event);
                        let input = match event.type_().as_str() {
                            "pointerdown" => {
                                let _ = canvas.set_pointer_capture(event.pointer_id());
                                RawInput::Down(pointer)
                            }
                            "pointermove" => RawInput::Move(pointer),
                            "pointerup" => RawInput::Up(pointer),
                            _ => RawInput::Cancel(pointer),
                        };
                        queue.borrow_mut().push(input);
                        ctx.request_repaint();
                    }),
                );
            }
            {
                let queue = queue.clone();
                let ctx = ctx.clone();
                listen(
                    &canvas_target,
                    "pointerleave",
                    Box::new(move |_| {
                        queue.borrow_mut().push(RawInput::Leave);
                        ctx.request_repaint();
                    }),
                );
            }
            {
                let queue = queue.clone();
                let ctx = ctx.clone();
                let canvas = canvas.clone();
                listen(
                    &canvas_target,
                    "wheel",
                    Box::new(move |event: web_sys::Event| {
                        let Some(event) = event.dyn_ref::<web_sys::WheelEvent>() else {
                            return;
                        };
                        let rect = canvas.get_bounding_client_rect();
                        queue.borrow_mut().push(RawInput::Wheel {
                            x: f64::from(event.client_x()) - rect.left(),
                            y: f64::from(event.client_y()) - rect.top(),
                            delta_y: event.delta_y(),
                            mode: event.delta_mode(),
                        });
                        ctx.request_repaint();
                    }),
                );
            }
            listen(
                &canvas_target,
                "contextmenu",
                Box::new(|event: web_sys::Event| event.prevent_default()),
            );
            for name in ["auxclick", "mousedown"] {
                listen(
                    &canvas_target,
                    name,
                    Box::new(|event: web_sys::Event| {
                        if event
                            .dyn_ref::<web_sys::MouseEvent>()
                            .is_some_and(|e| e.button() == 1)
                        {
                            event.prevent_default();
                        }
                    }),
                );
            }
            {
                let queue = queue.clone();
                let ctx = ctx.clone();
                listen(
                    &window_target,
                    "blur",
                    Box::new(move |_| {
                        queue.borrow_mut().push(RawInput::Blur);
                        ctx.request_repaint();
                    }),
                );
            }
            Self {
                queue,
                window,
                canvas,
                _listeners: listeners,
            }
        }

        pub fn drain(&mut self, _ctx: &egui::Context) -> Vec<RawInput> {
            std::mem::take(&mut *self.queue.borrow_mut())
        }

        pub fn now(&self) -> f64 {
            self.window.performance().map_or(0.0, |p| p.now())
        }

        fn matches(&self, query: &str) -> bool {
            self.window
                .match_media(query)
                .ok()
                .flatten()
                .is_some_and(|m| m.matches())
        }

        pub fn hover_capable(&self) -> bool {
            self.matches("(hover: hover)")
        }

        pub fn reduced_motion(&self) -> bool {
            self.matches("(prefers-reduced-motion: reduce)")
        }

        pub fn coarse_touch_device(&self) -> bool {
            self.window.navigator().max_touch_points() > 0 && self.matches("(pointer: coarse)")
        }

        pub fn safe_area(&self) -> Insets {
            let Some(document) = self.window.document() else {
                return Insets::default();
            };
            let Some(probe) = document.get_element_by_id("safe-area-probe") else {
                return Insets::default();
            };
            let Ok(Some(style)) = self.window.get_computed_style(&probe) else {
                return Insets::default();
            };
            let read = |name: &str| {
                style
                    .get_property_value(name)
                    .ok()
                    .and_then(|v| v.trim_end_matches("px").trim().parse::<f64>().ok())
                    .unwrap_or(0.0)
            };
            Insets {
                top: read("padding-top"),
                right: read("padding-right"),
                bottom: read("padding-bottom"),
                left: read("padding-left"),
            }
        }

        fn storage(&self) -> Option<web_sys::Storage> {
            self.window.local_storage().ok().flatten()
        }

        pub fn storage_get(&self, key: &str) -> Option<String> {
            self.storage()?.get_item(key).ok().flatten()
        }

        pub fn storage_set(&self, key: &str, value: &str) -> bool {
            self.storage()
                .is_some_and(|s| s.set_item(key, value).is_ok())
        }

        pub fn download(&self, name: &str, text: &str) -> bool {
            let run = || -> Result<(), JsValue> {
                let document = self.window.document().ok_or(JsValue::NULL)?;
                let parts = js_sys::Array::of1(&JsValue::from_str(text));
                let options = web_sys::BlobPropertyBag::new();
                options.set_type("application/json;charset=utf-8");
                let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)?;
                let url = web_sys::Url::create_object_url_with_blob(&blob)?;
                let link: web_sys::HtmlAnchorElement =
                    document.create_element("a")?.dyn_into()?;
                link.set_href(&url);
                link.set_download(name);
                document.body().ok_or(JsValue::NULL)?.append_child(&link)?;
                link.click();
                link.remove();
                let window = self.window.clone();
                let revoke = Closure::once_into_js(move || {
                    let _ = web_sys::Url::revoke_object_url(&url);
                    drop(window);
                });
                self.window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        revoke.unchecked_ref(),
                        0,
                    )?;
                Ok(())
            };
            run().is_ok()
        }

        pub fn hide_startup_message(&self) {
            if let Some(element) = self
                .window
                .document()
                .and_then(|d| d.get_element_by_id("error"))
            {
                let _ = element.set_attribute("hidden", "");
            }
        }

        pub fn focus_canvas(&self) {
            let _ = self.canvas.focus();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::HashMap;
    use std::time::Instant;

    use super::{Insets, RawInput, RawPointer};
    use skizzeo_editor::state::PointerType;

    pub struct Platform {
        started: Instant,
        storage: HashMap<String, String>,
        buttons: u16,
        last: Option<egui::Pos2>,
        touches: HashMap<u64, i64>,
    }

    impl Platform {
        pub fn new() -> Self {
            Self {
                started: Instant::now(),
                storage: HashMap::new(),
                buttons: 0,
                last: None,
                touches: HashMap::new(),
            }
        }

        /// Ersatzadapter: egui-Ereignisse in Pointer-Events übersetzen.
        pub fn drain(&mut self, ctx: &egui::Context) -> Vec<RawInput> {
            let events = ctx.input(|i| i.raw.events.clone());
            let mut out = Vec::new();
            let mouse = |x: f32, y: f32, button: i16, buttons: u16| RawPointer {
                id: 1,
                kind: PointerType::Mouse,
                x: f64::from(x),
                y: f64::from(y),
                button,
                buttons,
            };
            for event in events {
                match event {
                    egui::Event::PointerMoved(pos) => {
                        self.last = Some(pos);
                        out.push(RawInput::Move(mouse(pos.x, pos.y, -1, self.buttons)));
                    }
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        ..
                    } => {
                        let (index, bit) = match button {
                            egui::PointerButton::Primary => (0, 1),
                            egui::PointerButton::Secondary => (2, 2),
                            egui::PointerButton::Middle => (1, 4),
                            _ => continue,
                        };
                        if pressed {
                            self.buttons |= bit;
                            out.push(RawInput::Down(mouse(pos.x, pos.y, index, self.buttons)));
                        } else {
                            self.buttons &= !bit;
                            out.push(RawInput::Up(mouse(pos.x, pos.y, index, self.buttons)));
                        }
                    }
                    egui::Event::PointerGone => {
                        self.last = None;
                        out.push(RawInput::Leave);
                    }
                    egui::Event::MouseWheel { unit, delta, .. } => {
                        let pos = self.last.unwrap_or_default();
                        let (delta_y, mode) = match unit {
                            egui::MouseWheelUnit::Point => (-f64::from(delta.y), 0),
                            egui::MouseWheelUnit::Line => (-f64::from(delta.y), 1),
                            egui::MouseWheelUnit::Page => (-f64::from(delta.y), 2),
                        };
                        out.push(RawInput::Wheel {
                            x: f64::from(pos.x),
                            y: f64::from(pos.y),
                            delta_y,
                            mode,
                        });
                    }
                    egui::Event::Touch { id, phase, pos, .. } => {
                        let next = self.touches.len() as i64 + 2;
                        let pointer_id = *self.touches.entry(id.0).or_insert(next);
                        let pointer = RawPointer {
                            id: pointer_id,
                            kind: PointerType::Touch,
                            x: f64::from(pos.x),
                            y: f64::from(pos.y),
                            button: 0,
                            buttons: 1,
                        };
                        match phase {
                            egui::TouchPhase::Start => out.push(RawInput::Down(pointer)),
                            egui::TouchPhase::Move => out.push(RawInput::Move(pointer)),
                            egui::TouchPhase::End => {
                                self.touches.remove(&id.0);
                                out.push(RawInput::Up(RawPointer { buttons: 0, ..pointer }));
                            }
                            egui::TouchPhase::Cancel => {
                                self.touches.remove(&id.0);
                                out.push(RawInput::Cancel(pointer));
                            }
                        }
                    }
                    egui::Event::WindowFocused(false) => out.push(RawInput::Blur),
                    _ => {}
                }
            }
            out
        }

        pub fn now(&self) -> f64 {
            self.started.elapsed().as_secs_f64() * 1000.0
        }

        pub fn hover_capable(&self) -> bool {
            true
        }

        pub fn reduced_motion(&self) -> bool {
            false
        }

        pub fn coarse_touch_device(&self) -> bool {
            false
        }

        pub fn safe_area(&self) -> Insets {
            Insets::default()
        }

        pub fn storage_get(&self, key: &str) -> Option<String> {
            self.storage.get(key).cloned()
        }

        pub fn storage_set(&mut self, key: &str, value: &str) -> bool {
            self.storage.insert(key.to_owned(), value.to_owned());
            true
        }

        pub fn download(&self, name: &str, text: &str) -> bool {
            std::fs::write(name, text).is_ok()
        }

        pub fn hide_startup_message(&self) {}

        pub fn focus_canvas(&self) {}
    }
}
