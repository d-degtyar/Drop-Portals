//! Іконка в треї та її меню. Є лише на Windows; деінде `Tray::new` повертає `None`.

// поза Windows трею немає, тож частина коду там не використовується
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::mpsc::Sender;

use crate::config::Portal;
use crate::i18n::{self, Lang};

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
    /// `None` - як у Windows.
    Language(Option<Lang>),
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
            "lang:system" => Self::Language(None),
            "lang:uk" => Self::Language(Some(Lang::Uk)),
            "lang:en" => Self::Language(Some(Lang::En)),
            "quit" => Self::Quit,
            _ => match id.strip_prefix("new:preset:") {
                Some(name) => Self::NewPreset(name.to_owned()),
                None => Self::Toggle(id.strip_prefix("toggle:")?.to_owned()),
            },
        })
    }
}

/// Від чого залежить меню: якщо це не змінилось, меню не перебудовуємо.
fn signature(portals: &[Portal], autostart: bool, language: Option<Lang>) -> String {
    let mut text = format!("{autostart}:{language:?}:{:?}", i18n::current());
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
    use crate::i18n::tr;
    use crate::presets::{PRESETS, sorter_name};

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
        pub fn update(&mut self, portals: &[Portal], autostart_on: bool, language: Option<Lang>) {
            let signature = signature(portals, autostart_on, language);
            if signature == self.signature {
                return;
            }
            self.signature = signature;
            if let Ok(menu) = build_menu(portals, autostart_on, language) {
                self.icon.set_menu(Some(Box::new(menu)));
            }
        }
    }

    fn build_menu(
        portals: &[Portal],
        autostart_on: bool,
        language: Option<Lang>,
    ) -> tray_icon::menu::Result<Menu> {
        let item = |id: &str, text: &str| MenuItem::with_id(id, text, true, None);
        let separator = PredefinedMenuItem::separator;

        let new_menu = Submenu::new(tr("Новий портал", "New portal"), true);
        new_menu
            .append(&item("new:empty", tr("Порожній (у вибрану папку)…", "Empty (into a chosen folder)…")))?;
        new_menu.append(&item(
            "new:sorter",
            &format!("{} ({})…", sorter_name(), tr("розкладає по підпапках", "sorts into subfolders")),
        ))?;
        new_menu.append(&separator())?;
        for preset in PRESETS {
            new_menu.append(&item(&format!("new:preset:{}", preset.uk), &format!("{}…", preset.name())))?;
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
            menu.append(&item("show_all", tr("Показати всі", "Show all")))?;
            menu.append(&item("hide_all", tr("Сховати всі", "Hide all")))?;
        }
        menu.append(&separator())?;
        let autostart = tr("Запускати з Windows", "Start with Windows");
        menu.append(&CheckMenuItem::with_id("autostart", autostart, true, autostart_on, None))?;
        let language_menu = Submenu::new(tr("Мова", "Language"), true);
        let choices = [
            ("lang:system", tr("Як у Windows", "Same as Windows"), None),
            ("lang:uk", "Українська", Some(Lang::Uk)),
            ("lang:en", "English", Some(Lang::En)),
        ];
        for (id, text, choice) in choices {
            language_menu.append(&CheckMenuItem::with_id(id, text, true, language == choice, None))?;
        }
        menu.append(&language_menu)?;
        menu.append(&item("config_folder", tr("Папка налаштувань", "Settings folder")))?;
        let quit = format!("{} ({} {})", tr("Вихід", "Quit"), crate::APP_NAME, env!("CARGO_PKG_VERSION"));
        menu.append(&item("quit", &quit))?;
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

    pub fn update(&mut self, _portals: &[Portal], _autostart_on: bool, _language: Option<Lang>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_menu_ids() {
        assert_eq!(TrayCommand::parse("quit"), Some(TrayCommand::Quit));
        assert_eq!(TrayCommand::parse("new:preset:Відео"), Some(TrayCommand::NewPreset("Відео".into())));
        assert_eq!(TrayCommand::parse("toggle:ab12"), Some(TrayCommand::Toggle("ab12".into())));
        assert_eq!(TrayCommand::parse("lang:en"), Some(TrayCommand::Language(Some(Lang::En))));
        assert_eq!(TrayCommand::parse("lang:system"), Some(TrayCommand::Language(None)));
        assert_eq!(TrayCommand::parse("whatever"), None);
    }

    #[test]
    fn signature_tracks_visible_portals() {
        let mut portals = vec![Portal::default()];
        let before = signature(&portals, false, None);
        portals[0].visible = false;
        assert_ne!(before, signature(&portals, false, None));
        assert_ne!(signature(&portals, false, None), signature(&portals, true, None));
        assert_ne!(signature(&portals, false, None), signature(&portals, false, Some(Lang::En)));
    }
}
