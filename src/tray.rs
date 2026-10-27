//! Іконка в треї та її меню. Є лише на Windows; деінде `Tray::new` повертає `None`.

// поза Windows трею немає, тож частина коду там не використовується
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::mpsc::Sender;

use crate::config::Portal;

/// Що вибрали в меню трею.
#[derive(Clone, Debug, PartialEq)]
pub enum TrayCommand {
    NewEmpty,
    NewSorter,
    NewPreset(String),
    Toggle(String),
    ShowAll,
    HideAll,
    Autostart,
    ConfigFolder,
    Quit,
}

impl TrayCommand {
    /// Ідентифікатор пункту меню -> команда.
    pub fn parse(id: &str) -> Option<Self> {
        Some(match id {
            "new:empty" => Self::NewEmpty,
            "new:sorter" => Self::NewSorter,
            "show_all" => Self::ShowAll,
            "hide_all" => Self::HideAll,
            "autostart" => Self::Autostart,
            "config_folder" => Self::ConfigFolder,
            "quit" => Self::Quit,
            _ => match id.strip_prefix("new:preset:") {
                Some(name) => Self::NewPreset(name.to_owned()),
                None => Self::Toggle(id.strip_prefix("toggle:")?.to_owned()),
            },
        })
    }
}

/// Від чого залежить меню: якщо це не змінилось, меню не перебудовуємо.
fn signature(portals: &[Portal], autostart: bool) -> String {
    let mut text = format!("{autostart}");
    for portal in portals {
        text.push_str(&format!("|{}:{}:{}", portal.id, portal.name, portal.visible));
    }
    text
}

#[cfg(windows)]
pub use windows_tray::Tray;

#[cfg(windows)]
mod windows_tray {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use super::*;
    use crate::presets::{PRESETS, SORTER_NAME};

    pub struct Tray {
        icon: TrayIcon,
        signature: String,
    }

    impl Tray {
        pub fn new(ctx: &egui::Context, events: Sender<TrayCommand>) -> Option<Self> {
            let menu_ctx = ctx.clone();
            let menu_events = events.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if let Some(command) = TrayCommand::parse(event.id.as_ref()) {
                    let _ = menu_events.send(command);
                    menu_ctx.request_repaint();
                }
            }));
            let click_ctx = ctx.clone();
            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let _ = events.send(TrayCommand::ShowAll);
                    click_ctx.request_repaint();
                }
            }));

            let size = 32;
            let icon = Icon::from_rgba(crate::icon::rgba(size), size, size).ok()?;
            let icon = TrayIconBuilder::new()
                .with_tooltip(crate::APP_NAME)
                .with_icon(icon)
                .with_menu_on_left_click(false)
                .build()
                .ok()?;
            Some(Self { icon, signature: String::new() })
        }

        /// Перебудовує меню, якщо змінився список порталів.
        pub fn update(&mut self, portals: &[Portal], autostart_on: bool) {
            let signature = signature(portals, autostart_on);
            if signature == self.signature {
                return;
            }
            self.signature = signature;
            if let Ok(menu) = build_menu(portals, autostart_on) {
                self.icon.set_menu(Some(Box::new(menu)));
            }
        }
    }

    fn build_menu(portals: &[Portal], autostart_on: bool) -> tray_icon::menu::Result<Menu> {
        let item = |id: &str, text: &str| MenuItem::with_id(id, text, true, None);
        let separator = PredefinedMenuItem::separator;

        let new_menu = Submenu::new("Новий портал", true);
        new_menu.append(&item("new:empty", "Порожній (у вибрану папку)…"))?;
        new_menu.append(&item("new:sorter", &format!("{SORTER_NAME} (розкладає по підпапках)…")))?;
        new_menu.append(&separator())?;
        for preset in PRESETS {
            new_menu.append(&item(&format!("new:preset:{}", preset.name), &format!("{}…", preset.name)))?;
        }

        let menu = Menu::new();
        menu.append(&new_menu)?;
        menu.append(&separator())?;
        for portal in portals {
            let id = format!("toggle:{}", portal.id);
            menu.append(&CheckMenuItem::with_id(id, &portal.name, true, portal.visible, None))?;
        }
        if !portals.is_empty() {
            menu.append(&separator())?;
            menu.append(&item("show_all", "Показати всі"))?;
            menu.append(&item("hide_all", "Сховати всі"))?;
        }
        menu.append(&separator())?;
        menu.append(&CheckMenuItem::with_id("autostart", "Запускати з Windows", true, autostart_on, None))?;
        menu.append(&item("config_folder", "Папка налаштувань"))?;
        menu.append(&item("quit", &format!("Вихід ({} {})", crate::APP_NAME, env!("CARGO_PKG_VERSION"))))?;
        Ok(menu)
    }
}

#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn new(_ctx: &egui::Context, _events: Sender<TrayCommand>) -> Option<Self> {
        None
    }

    pub fn update(&mut self, _portals: &[Portal], _autostart_on: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_menu_ids() {
        assert_eq!(TrayCommand::parse("quit"), Some(TrayCommand::Quit));
        assert_eq!(TrayCommand::parse("new:preset:Відео"), Some(TrayCommand::NewPreset("Відео".into())));
        assert_eq!(TrayCommand::parse("toggle:ab12"), Some(TrayCommand::Toggle("ab12".into())));
        assert_eq!(TrayCommand::parse("whatever"), None);
    }

    #[test]
    fn signature_tracks_visible_portals() {
        let mut portals = vec![Portal::default()];
        let before = signature(&portals, false);
        portals[0].visible = false;
        assert_ne!(before, signature(&portals, false));
        assert_ne!(signature(&portals, false), signature(&portals, true));
    }
}
