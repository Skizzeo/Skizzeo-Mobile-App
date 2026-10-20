//! Schriften der Oberfläche: je Schriftwahl (`uiFont`) und CSS-Schnitt eine Familie.
//!
//! Inter ersetzt Segoe UI, Liberation Sans ersetzt Arial, DejaVu Serif ersetzt Georgia.
//! OSIFont ist die Originalschrift aus dem Prototyp.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/inter-regular.otf");
const INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/inter-medium.otf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/inter-semibold.otf");
const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/inter-bold.otf");
const LIBERATION_REGULAR: &[u8] = include_bytes!("../assets/fonts/liberation-sans-regular.ttf");
const LIBERATION_BOLD: &[u8] = include_bytes!("../assets/fonts/liberation-sans-bold.ttf");
const SERIF_REGULAR: &[u8] = include_bytes!("../assets/fonts/dejavu-serif.ttf");
const SERIF_BOLD: &[u8] = include_bytes!("../assets/fonts/dejavu-serif-bold.ttf");
const MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/dejavu-sans-mono.ttf");
const MONO_BOLD: &[u8] = include_bytes!("../assets/fonts/dejavu-sans-mono-bold.ttf");
const OSIFONT: &[u8] = include_bytes!("../assets/fonts/osifont.ttf");

pub const UI_FONTS: [&str; 5] = ["system", "arial", "serif", "mono", "osifont"];
pub const WEIGHTS: [u16; 4] = [400, 500, 600, 700];

fn family_name(ui_font: &str, weight: u16) -> String {
    format!("{ui_font}-{weight}")
}

/// Schrift wie `font: <weight> <size>px var(--ui-font)`.
pub fn font(ui_font: &str, weight: u16, size: f32) -> FontId {
    let ui_font = if UI_FONTS.contains(&ui_font) { ui_font } else { "system" };
    let weight = WEIGHTS
        .iter()
        .copied()
        .min_by_key(|w| w.abs_diff(weight))
        .unwrap_or(400);
    FontId::new(size, FontFamily::Name(family_name(ui_font, weight).into()))
}

pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let data: [(&str, &'static [u8]); 11] = [
        ("inter-400", INTER_REGULAR),
        ("inter-500", INTER_MEDIUM),
        ("inter-600", INTER_SEMIBOLD),
        ("inter-700", INTER_BOLD),
        ("liberation-400", LIBERATION_REGULAR),
        ("liberation-700", LIBERATION_BOLD),
        ("serif-400", SERIF_REGULAR),
        ("serif-700", SERIF_BOLD),
        ("mono-400", MONO_REGULAR),
        ("mono-700", MONO_BOLD),
        ("osifont", OSIFONT),
    ];
    for (name, bytes) in data {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    // Browser wählen ab Schnitt 600 die fette Datei, darunter die normale.
    let face = |ui_font: &str, weight: u16| -> &'static str {
        let bold = weight >= 600;
        match ui_font {
            "system" => match weight {
                400 => "inter-400",
                500 => "inter-500",
                600 => "inter-600",
                _ => "inter-700",
            },
            "arial" => if bold { "liberation-700" } else { "liberation-400" },
            "serif" => if bold { "serif-700" } else { "serif-400" },
            "mono" => if bold { "mono-700" } else { "mono-400" },
            _ => "osifont",
        }
    };
    for ui_font in UI_FONTS {
        for weight in WEIGHTS {
            let primary = face(ui_font, weight);
            let mut chain = vec![primary.to_owned()];
            // Fehlende Zeichen (z. B. ✓, ⌫, ⁵) aus Inter bzw. den egui-Standardschriften.
            for fallback in [face("system", weight), "inter-400"] {
                if !chain.iter().any(|c| c == fallback) {
                    chain.push(fallback.to_owned());
                }
            }
            chain.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
            fonts
                .families
                .insert(FontFamily::Name(family_name(ui_font, weight).into()), chain);
        }
    }
    let mut proportional = vec!["inter-400".to_owned()];
    proportional.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts
}
