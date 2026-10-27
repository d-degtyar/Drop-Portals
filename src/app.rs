//! Програма: зберігає портали, показує їхні вікна, виконує команди з порталів і трею.
//!
//! Головне вікно eframe приховане - на екрані лише портали (окремі вікна ОС)
//! і, за потреби, панель налаштувань.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use egui::{Pos2, Rect, Ui, Vec2, ViewportBuilder, ViewportCommand, ViewportId};

use crate::config::{self, AppConfig, Mode, Portal};
use crate::mover::{self, Action, TransferResult};
use crate::platform::{self, autostart};
use crate::portal_view::{self, PortalState, Progress};
use crate::presets;
use crate::settings_view::{self, SettingsState};
use crate::tray::{Tray, TrayCommand};

/// Що просять зробити вікна порталів і панель налаштувань.
#[derive(Debug)]
pub enum Command {
    Drop {
        id: String,
        paths: Vec<PathBuf>,
        mode: Mode,
    },
    OpenFolder(String),
    ChooseFolder(String),
    OpenSettings(String),
    CloseSettings,
    Undo(String),
    Duplicate(String),
    Hide(String),
    Delete(String),
    /// Змінились налаштування - треба зберегти.
    Changed,
}

/// Події з інших потоків: трей і діалог вибору папки.
enum Event {
    Tray(TrayCommand),
    FolderPicked { target: FolderTarget, folder: PathBuf },
}

enum FolderTarget {
    /// Папка для наявного порталу; після вибору можна одразу перенести кинуті файли.
    Portal { id: String, then_drop: Option<(Vec<PathBuf>, Mode)> },
    /// Новий портал, який створиться після вибору папки.
    New(Box<Portal>),
}

struct Job {
    portal_id: String,
    progress: Arc<Progress>,
    handle: JoinHandle<Vec<TransferResult>>,
}

const SAVE_DELAY: f64 = 0.5;

pub struct PortalsApp {
    config: AppConfig,
    config_path: PathBuf,
    states: HashMap<String, PortalState>,
    settings: Option<SettingsState>,
    jobs: Vec<Job>,
    tray: Option<Tray>,
    autostart_on: bool,
    events: Sender<Event>,
    inbox: Receiver<Event>,
    ctx: egui::Context,
    save_at: Option<f64>,
    frames: u32,
}

impl PortalsApp {
    pub fn new(ctx: &egui::Context, config_path: PathBuf) -> Self {
        let (events, inbox) = channel();
        let mut config = config::load(&config_path);
        if config.portals.is_empty() {
            config.portals.push(Portal::default());
        }

        let (tray_tx, tray_rx) = channel::<TrayCommand>();
        let tray = Tray::new(ctx, tray_tx);
        // команди трею перекладаємо в загальну чергу подій
        let forward = events.clone();
        std::thread::spawn(move || {
            while let Ok(command) = tray_rx.recv() {
                if forward.send(Event::Tray(command)).is_err() {
                    break;
                }
            }
        });

        Self {
            config,
            config_path,
            states: HashMap::new(),
            settings: None,
            jobs: Vec::new(),
            tray,
            autostart_on: autostart::is_enabled(),
            events,
            inbox,
            ctx: ctx.clone(),
            save_at: None,
            frames: 0,
        }
    }

    fn portal_index(&self, id: &str) -> Option<usize> {
        self.config.portals.iter().position(|portal| portal.id == id)
    }

    fn schedule_save(&mut self) {
        let now = self.ctx.input(|i| i.time);
        self.save_at = Some(now + SAVE_DELAY);
        self.ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAVE_DELAY));
    }

    fn save_now(&mut self) {
        self.save_at = None;
        if let Err(error) = config::save(&self.config, &self.config_path) {
            eprintln!("Не вдалося зберегти налаштування: {error}");
        }
    }

    /// Головне вікно приховане, тож щоб показати портал, просимо egui
    /// один раз відмалювати інтерфейс навіть так.
    fn wake_ui(&self) {
        self.ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::RequestPaintWhileHidden);
        self.ctx.request_repaint();
    }

    fn points_per_pixel(&self) -> f32 {
        1.0 / self.ctx.input(|i| i.viewport().native_pixels_per_point).unwrap_or(1.0)
    }

    /// Новий портал з'являється під курсором (або каскадом, якщо курсор невідомий).
    fn place_new(&self, portal: &mut Portal) {
        let scale = self.points_per_pixel();
        match platform::cursor_position() {
            Some((x, y)) => {
                portal.x = x * scale - portal.width / 2.0;
                portal.y = y * scale - portal.height / 2.0;
            }
            None => {
                let shift = 30.0 * (self.config.portals.len() % 10) as f32;
                portal.x = 120.0 + shift;
                portal.y = 120.0 + shift;
            }
        }
    }

    /// Якщо монітор від'єднали, портал повертається на видимий екран.
    fn fit_on_screen(&mut self) {
        let Some((x, y, w, h)) = platform::virtual_screen() else { return };
        let scale = self.points_per_pixel();
        let screen = Rect::from_min_size(Pos2::new(x, y) * scale, Vec2::new(w, h) * scale);
        let mut moved = false;
        for portal in &mut self.config.portals {
            let rect =
                Rect::from_min_size(Pos2::new(portal.x, portal.y), Vec2::new(portal.width, portal.height));
            if !screen.intersects(rect.shrink(20.0)) {
                portal.x = screen.left() + 60.0;
                portal.y = screen.top() + 60.0;
                moved = true;
            }
        }
        if moved {
            self.schedule_save();
        }
    }

    fn ask_folder(&self, title: String, start: String, target: FolderTarget) {
        let events = self.events.clone();
        let ctx = self.ctx.clone();
        // діалог - у своєму потоці, щоб портали не завмирали
        std::thread::spawn(move || {
            if let Some(folder) = platform::pick_folder(&title, &start) {
                let _ = events.send(Event::FolderPicked { target, folder });
                ctx.request_repaint();
            }
        });
    }

    fn add_portal(&mut self, mut portal: Portal) {
        self.place_new(&mut portal);
        self.config.portals.push(portal);
        self.schedule_save();
        self.wake_ui();
    }

    fn new_portal(&mut self, portal: Portal) {
        if cfg!(windows) && portal.folder.is_empty() {
            let title = format!("Папка для порталу «{}»", portal.name);
            self.ask_folder(title, String::new(), FolderTarget::New(Box::new(portal)));
        } else {
            self.add_portal(portal);
        }
    }

    fn set_visible(&mut self, id: &str, visible: bool) {
        if let Some(index) = self.portal_index(id) {
            self.config.portals[index].visible = visible;
            if !visible {
                // наступного разу вікно створиться там, де портал був
                self.states.entry(id.to_owned()).or_default().spawn = None;
            }
            self.schedule_save();
            self.wake_ui();
        }
    }

    // ---------- команди ----------

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Tray(command) => self.handle_tray(command),
            Event::FolderPicked { target: FolderTarget::New(mut portal), folder } => {
                set_folder(&mut portal, &folder);
                self.add_portal(*portal);
            }
            Event::FolderPicked { target: FolderTarget::Portal { id, then_drop }, folder } => {
                let Some(index) = self.portal_index(&id) else { return };
                set_folder(&mut self.config.portals[index], &folder);
                self.schedule_save();
                if let Some((paths, mode)) = then_drop {
                    self.start_transfer(&id, paths, mode);
                }
            }
        }
    }

    fn handle_tray(&mut self, command: TrayCommand) {
        match command {
            TrayCommand::NewEmpty => self.new_portal(Portal::default()),
            TrayCommand::NewSorter => self.new_portal(presets::sorter_portal("")),
            TrayCommand::NewPreset(name) => {
                if let Some(preset) = presets::find(&name) {
                    self.new_portal(presets::preset_portal(preset, ""));
                }
            }
            TrayCommand::Toggle(id) => {
                let visible = self.portal_index(&id).is_some_and(|i| self.config.portals[i].visible);
                self.set_visible(&id, !visible);
            }
            TrayCommand::ShowAll => self.set_all_visible(true),
            TrayCommand::HideAll => self.set_all_visible(false),
            TrayCommand::Autostart => {
                if autostart::set_enabled(!self.autostart_on).is_ok() {
                    self.autostart_on = autostart::is_enabled();
                }
            }
            TrayCommand::ConfigFolder => {
                if let Some(dir) = self.config_path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                    platform::open_folder(dir);
                }
            }
            TrayCommand::Quit => {
                self.save_now();
                self.tray = None; // прибирає іконку з трею
                std::process::exit(0);
            }
        }
    }

    fn set_all_visible(&mut self, visible: bool) {
        let ids: Vec<String> = self.config.portals.iter().map(|p| p.id.clone()).collect();
        for id in ids {
            self.set_visible(&id, visible);
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Changed => self.schedule_save(),
            Command::Drop { id, paths, mode } => self.start_transfer(&id, paths, mode),
            Command::OpenFolder(id) => {
                if let Some(index) = self.portal_index(&id) {
                    platform::open_folder(Path::new(&self.config.portals[index].folder));
                }
            }
            Command::ChooseFolder(id) => self.choose_folder(id, None),
            Command::OpenSettings(id) => self.open_settings(&id),
            Command::CloseSettings => self.settings = None,
            Command::Undo(id) => self.undo(&id),
            Command::Duplicate(id) => {
                if let Some(index) = self.portal_index(&id) {
                    let source = &self.config.portals[index];
                    let copy = Portal {
                        id: config::new_id(),
                        name: format!("{} (копія)", source.name),
                        visible: true,
                        x: source.x + 30.0,
                        y: source.y + 30.0,
                        ..source.clone()
                    };
                    self.config.portals.push(copy);
                    self.schedule_save();
                }
            }
            Command::Hide(id) => {
                self.set_visible(&id, false);
                self.close_settings_of(&id);
            }
            Command::Delete(id) => {
                self.config.portals.retain(|portal| portal.id != id);
                self.states.remove(&id);
                self.close_settings_of(&id);
                self.schedule_save();
            }
        }
    }

    fn close_settings_of(&mut self, id: &str) {
        if self.settings.as_ref().is_some_and(|s| s.portal_id == id) {
            self.settings = None;
        }
    }

    fn choose_folder(&mut self, id: String, then_drop: Option<(Vec<PathBuf>, Mode)>) {
        let Some(index) = self.portal_index(&id) else { return };
        let portal = &self.config.portals[index];
        let title = format!("Папка для порталу «{}»", portal.name);
        let start = portal.folder.clone();
        self.ask_folder(title, start, FolderTarget::Portal { id, then_drop });
    }

    fn open_settings(&mut self, id: &str) {
        let Some(index) = self.portal_index(id) else { return };
        let portal = &self.config.portals[index];
        // праворуч від порталу
        let position = Pos2::new(portal.x + portal.width + 12.0, portal.y);
        self.settings = Some(SettingsState::new(portal, position));
    }

    fn start_transfer(&mut self, id: &str, paths: Vec<PathBuf>, mode: Mode) {
        let Some(index) = self.portal_index(id) else { return };
        let portal = self.config.portals[index].clone();
        if portal.folder.is_empty() {
            self.choose_folder(id.to_owned(), Some((paths, mode)));
            return;
        }
        let progress = Arc::new(Progress { done: AtomicUsize::new(0), total: paths.len() });
        let ctx = self.ctx.clone();
        let worker_progress = progress.clone();
        let handle = std::thread::spawn(move || {
            mover::transfer_many(&portal, &paths, mode, |done, _| {
                worker_progress.done.store(done, Ordering::Relaxed);
                ctx.request_repaint();
            })
        });
        self.states.entry(id.to_owned()).or_default().progress = Some(progress.clone());
        self.jobs.push(Job { portal_id: id.to_owned(), progress, handle });
    }

    fn poll_jobs(&mut self) {
        let (finished, running): (Vec<Job>, Vec<Job>) =
            std::mem::take(&mut self.jobs).into_iter().partition(|job| job.handle.is_finished());
        self.jobs = running;
        let now = self.ctx.input(|i| i.time);
        for job in finished {
            let results = job.handle.join().unwrap_or_default();
            let state = self.states.entry(job.portal_id.clone()).or_default();
            if state.progress.as_ref().is_some_and(|p| Arc::ptr_eq(p, &job.progress)) {
                state.progress = None;
            }
            let (text, ok) = describe(&results);
            state.flash = Some(portal_view::Flash { text, ok, until: now + 4.0 });
            state.report = results
                .iter()
                .filter(|r| matches!(r.action, Action::Skipped | Action::Error))
                .map(|r| format!("{}: {}", r.file_name(), r.reason))
                .collect();
            if results.iter().any(TransferResult::ok) {
                state.last_results = results;
            }
            self.ctx.request_repaint();
        }
    }

    fn undo(&mut self, id: &str) {
        let state = self.states.entry(id.to_owned()).or_default();
        let errors = mover::undo(&std::mem::take(&mut state.last_results));
        let now = self.ctx.input(|i| i.time);
        let (text, ok) = if errors.is_empty() {
            ("↩ повернуто".to_owned(), true)
        } else {
            (format!("не вдалося повернути: {}", errors.len()), false)
        };
        state.report = errors;
        state.flash = Some(portal_view::Flash { text, ok, until: now + 4.0 });
    }

    // ---------- вікна ----------

    fn show_portals(&mut self, ui: &Ui, commands: &mut Vec<Command>) {
        for portal in self.config.portals.iter_mut().filter(|p| p.visible) {
            let state = self.states.entry(portal.id.clone()).or_default();
            let spawn = *state.spawn.get_or_insert_with(|| {
                Rect::from_min_size(Pos2::new(portal.x, portal.y), Vec2::new(portal.width, portal.height))
            });
            let builder = ViewportBuilder::default()
                .with_title(format!("{} - {}", portal.name, crate::APP_NAME))
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(true)
                .with_drag_and_drop(true)
                .with_min_inner_size(portal_view::MIN_SIZE)
                .with_position(spawn.min)
                .with_inner_size(spawn.size());
            ui.ctx().show_viewport_immediate(
                ViewportId::from_hash_of(("portal", &portal.id)),
                builder,
                |ui, _| {
                    portal_view::show(ui, portal, state, commands);
                },
            );
        }
    }

    fn show_settings(&mut self, ui: &Ui, commands: &mut Vec<Command>) {
        let Some(settings) = &mut self.settings else { return };
        let Some(portal) = self.config.portals.iter_mut().find(|p| p.id == settings.portal_id) else {
            self.settings = None;
            return;
        };
        let state = self.states.entry(portal.id.clone()).or_default();
        let builder = ViewportBuilder::default()
            .with_title(format!("{} - налаштування", portal.name))
            .with_always_on_top()
            .with_position(settings.position)
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([520.0, 360.0]);
        ui.ctx().show_viewport_immediate(ViewportId::from_hash_of("settings"), builder, |ui, _| {
            if ui.input(|i| i.viewport().close_requested()) {
                commands.push(Command::CloseSettings);
            }
            settings_view::show(ui, portal, settings, state, commands);
        });
    }
}

fn set_folder(portal: &mut Portal, folder: &Path) {
    portal.folder = folder.to_string_lossy().into_owned();
    if portal.name == Portal::default().name
        && let Some(name) = folder.file_name()
    {
        portal.name = name.to_string_lossy().into_owned();
    }
}

/// Підсумок кидання для порталу: текст і чи все добре.
fn describe(results: &[TransferResult]) -> (String, bool) {
    let count = |action| results.iter().filter(|r| r.action == action).count();
    let (moved, copied, skipped, errors) =
        (count(Action::Moved), count(Action::Copied), count(Action::Skipped), count(Action::Error));
    if errors > 0 {
        return (format!("помилок: {errors}"), false);
    }
    if moved + copied == 0 {
        return (format!("пропущено {skipped}"), false);
    }
    let verb = if copied > 0 && moved == 0 { "скопійовано" } else { "переміщено" };
    let mut text = format!("✓ {verb} {}", moved + copied);
    if skipped > 0 {
        text.push_str(&format!(" · пропущено {skipped}"));
    }
    (text, true)
}

impl eframe::App for PortalsApp {
    /// Працює завжди, навіть коли жодного вікна не видно.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.inbox.try_recv() {
            self.handle_event(event);
        }
        self.poll_jobs();
        if let Some(tray) = &mut self.tray {
            tray.update(&self.config.portals, self.autostart_on);
        }
        if self.save_at.is_some_and(|at| ctx.input(|i| i.time) >= at) {
            self.save_now();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // eframe показує головне вікно після першого кадру - ховаємо його знову
        if self.frames < 3 {
            if self.frames == 0 {
                self.fit_on_screen();
            }
            ui.send_viewport_cmd(ViewportCommand::Visible(false));
            self.frames += 1;
        }
        let mut commands = Vec::new();
        self.show_portals(ui, &mut commands);
        self.show_settings(ui, &mut commands);
        for command in commands {
            self.handle_command(command);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4] // прозоре тло: видно лише картку порталу
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mover::TransferResult;

    fn result(action: Action) -> TransferResult {
        TransferResult { source: "a.txt".into(), target: None, action, reason: String::new() }
    }

    #[test]
    fn describes_results() {
        assert_eq!(
            describe(&[result(Action::Moved), result(Action::Moved)]),
            ("✓ переміщено 2".into(), true)
        );
        assert_eq!(
            describe(&[result(Action::Copied), result(Action::Skipped)]).0,
            "✓ скопійовано 1 · пропущено 1"
        );
        assert_eq!(describe(&[result(Action::Skipped)]), ("пропущено 1".into(), false));
        assert_eq!(describe(&[result(Action::Moved), result(Action::Error)]), ("помилок: 1".into(), false));
    }

    #[test]
    fn folder_names_new_portal() {
        let mut portal = Portal::default();
        set_folder(&mut portal, Path::new("/x/Clients"));
        assert_eq!(portal.name, "Clients");
        let mut named = presets::sorter_portal("");
        set_folder(&mut named, Path::new("/x/Clients"));
        assert_eq!(named.name, presets::SORTER_NAME);
    }
}
