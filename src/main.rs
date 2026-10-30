//! Drop Portals - плаваючі вікна-портали в папки.
//! Перетягни файл на портал - і він полетів у потрібну папку.

// без чорного вікна консолі в зібраній версії
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod fonts;
mod i18n;
mod icon;
mod mover;
mod platform;
mod portal_view;
mod presets;
mod rules;
mod settings_view;
#[cfg(test)]
mod test_util;
mod tray;

use std::sync::Arc;

pub const APP_NAME: &str = "Drop Portals";

fn main() -> eframe::Result {
    if !platform::single_instance() {
        return Ok(()); // уже запущено - портали живуть у першій копії
    }

    platform::watch_windows(std::time::Duration::from_millis(1500));

    let size = 64;
    let icon = egui::IconData { rgba: icon::rgba(size), width: size, height: size };
    // головне вікно існує лише заради eframe: крихітне, прозоре, за межами екрана і приховане
    let viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_icon(Arc::new(icon))
        .with_inner_size([1.0, 1.0])
        .with_position([-32000.0, -32000.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_visible(false);
    let options = eframe::NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| {
            fonts::setup(&cc.egui_ctx);
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            Ok(Box::new(app::PortalsApp::new(&cc.egui_ctx, config::config_path())))
        }),
    )
}
