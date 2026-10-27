//! Шрифти: системні Segoe UI на Windows, вбудовані шрифти egui - запасні.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

use crate::platform;

/// Сімейство жирного шрифту для назв порталів.
pub const BOLD: &str = "bold";

pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut bold = fallback.clone();

    if let Some(regular) = platform::system_font("segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(regular)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
        bold.insert(0, "segoe".into());
    }
    if let Some(semibold) =
        platform::system_font("seguisb.ttf").or_else(|| platform::system_font("segoeuib.ttf"))
    {
        fonts.font_data.insert("segoe-bold".into(), Arc::new(FontData::from_owned(semibold)));
        bold.insert(0, "segoe-bold".into());
    }
    // стрілки, галочки і хрестики, яких немає в основних шрифтах
    if let Some(symbols) = platform::symbol_font() {
        fonts.font_data.insert("symbols".into(), Arc::new(FontData::from_owned(symbols)));
        fonts.families.entry(FontFamily::Proportional).or_default().push("symbols".into());
        bold.push("symbols".into());
    }
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold);
    ctx.set_fonts(fonts);
}
