//! Вміст вікна-порталу: малювання, перетягування файлів, рух і зміна розміру вікна.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, CornerRadius, CursorIcon, FontFamily, FontId, Id, Pos2, Rect, ResizeDirection, Sense, Stroke,
    StrokeKind, TextFormat, Ui, Vec2, ViewportCommand,
};

use crate::app::Command;
use crate::config::Portal;
use crate::fonts::BOLD;
use crate::i18n::tr;
use crate::mover::TransferResult;
use crate::platform;
use crate::rules::{resolve_destination, summary};

pub const MIN_SIZE: Vec2 = Vec2::new(110.0, 64.0);
const OK: Color32 = Color32::from_rgb(0x3D, 0xD6, 0x8C);
const ERROR: Color32 = Color32::from_rgb(0xFF, 0x63, 0x69);
const GRIP: f32 = 16.0;

/// Короткий підсумок останнього кидання поверх порталу.
pub struct Flash {
    pub text: String,
    pub ok: bool,
    pub until: f64,
}

/// Хід фонового перенесення.
pub struct Progress {
    pub done: AtomicUsize,
    pub total: usize,
}

/// Тимчасовий стан порталу, що не зберігається в конфіг.
#[derive(Default)]
pub struct PortalState {
    pub flash: Option<Flash>,
    pub progress: Option<Arc<Progress>>,
    pub last_results: Vec<TransferResult>,
    /// Що пропущено або не вдалося під час останнього кидання.
    pub report: Vec<String>,
    /// Положення, з яким вікно створили. Поки вікно живе, його не чіпаємо,
    /// інакше egui сперечався б з користувачем, що тягне вікно.
    pub spawn: Option<Rect>,
    /// Пікселів на точку на моніторі, де стоїть портал.
    pub pixels_per_point: Option<f32>,
}

impl PortalState {
    pub fn flash(&mut self, ui: &Ui, text: impl Into<String>, ok: bool) {
        let until = ui.input(|i| i.time) + 4.0;
        self.flash = Some(Flash { text: text.into(), ok, until });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hover {
    None,
    Accept,
    Reject,
}

fn hover_state(portal: &Portal, paths: &[PathBuf]) -> Hover {
    match paths {
        [] => Hover::None,
        // без папки приймаємо все: папку спитаємо після кидання
        _ if portal.folder.is_empty() => Hover::Accept,
        _ if paths.iter().any(|p| resolve_destination(portal, p).is_some()) => Hover::Accept,
        _ => Hover::Reject,
    }
}

pub fn parse_color(hex: &str) -> Color32 {
    Color32::from_hex(hex).unwrap_or(Color32::from_rgb(0x4F, 0x7C, 0xFF))
}

fn faded(color: Color32, opacity: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * opacity) as u8)
}

pub fn show(ui: &mut Ui, portal: &mut Portal, state: &mut PortalState, out: &mut Vec<Command>) {
    let rect = ui.max_rect();
    track_geometry(ui, portal, out);
    state.pixels_per_point = ui.input(|i| i.viewport().native_pixels_per_point);

    let (hovered, dropped) = ui.input(|i| {
        let hovered: Vec<PathBuf> = i.raw.hovered_files.iter().filter_map(|f| f.path.clone()).collect();
        let dropped: Vec<PathBuf> = i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect();
        (hovered, dropped)
    });
    let hover = hover_state(portal, &hovered);
    if !dropped.is_empty() {
        if hover_state(portal, &dropped) == Hover::Accept {
            let ctrl = platform::ctrl_pressed().unwrap_or_else(|| ui.input(|i| i.modifiers.ctrl));
            let mode = if ctrl { portal.mode.toggled() } else { portal.mode };
            out.push(Command::Drop { id: portal.id.clone(), paths: dropped, mode });
        } else {
            state.flash(ui, tr("не приймає ці файли", "does not take these files"), false);
        }
    }

    handle_mouse(ui, rect, portal, out);
    paint(ui, rect, portal, state, hover);
}

/// Запам'ятовує, куди користувач пересунув вікно і якого розміру зробив.
fn track_geometry(ui: &Ui, portal: &mut Portal, out: &mut Vec<Command>) {
    let Some(outer) = ui.input(|i| i.viewport().outer_rect) else { return };
    let inner = ui.input(|i| i.viewport().inner_rect).unwrap_or(outer);
    let current = [outer.min.x, outer.min.y, inner.width(), inner.height()];
    let saved = [portal.x, portal.y, portal.width, portal.height];
    if current.iter().zip(saved).any(|(a, b)| (a - b).abs() > 0.5) {
        [portal.x, portal.y, portal.width, portal.height] = current;
        out.push(Command::Changed);
    }
}

fn handle_mouse(ui: &mut Ui, rect: Rect, portal: &Portal, out: &mut Vec<Command>) {
    let id = Id::unique(("portal", &portal.id));
    let grip = Rect::from_min_size(rect.max - Vec2::splat(GRIP), Vec2::splat(GRIP));
    let body = ui.interact(rect, id, Sense::click_and_drag());
    let corner = ui.interact(grip, id.with("grip"), Sense::drag());

    if corner.hovered() || corner.dragged() {
        ui.set_cursor_icon(CursorIcon::ResizeNwSe);
    }
    if corner.drag_started() {
        ui.send_viewport_cmd(ViewportCommand::BeginResize(ResizeDirection::SouthEast));
    } else if body.drag_started_by(egui::PointerButton::Primary) {
        ui.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if body.double_clicked() {
        out.push(if portal.folder.is_empty() {
            Command::ChooseFolder(portal.id.clone())
        } else {
            Command::OpenFolder(portal.id.clone())
        });
    }
    if body.secondary_clicked() {
        out.push(Command::OpenSettings(portal.id.clone()));
    }
}

fn paint(ui: &Ui, rect: Rect, portal: &Portal, state: &mut PortalState, hover: Hover) {
    let now = ui.input(|i| i.time);
    if let Some(flash) = &state.flash {
        if flash.until <= now {
            state.flash = None;
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(flash.until - now));
        }
    }

    let opacity = portal.opacity.clamp(0.2, 1.0);
    let painter = ui.painter();
    let base = parse_color(&portal.color);
    let fill = match hover {
        Hover::Accept => base.lerp_to_gamma(Color32::WHITE, 0.2),
        Hover::Reject => Color32::from_gray(58),
        Hover::None => base,
    };
    let card = rect.shrink(1.0);
    painter.rect_filled(card, CornerRadius::same(14), faded(fill, 0.96 * opacity));
    if hover != Hover::None {
        let color = if hover == Hover::Accept { Color32::WHITE } else { ERROR };
        painter.rect_stroke(
            card.shrink(4.0),
            CornerRadius::same(11),
            Stroke::new(2.5, color),
            StrokeKind::Inside,
        );
    }

    let inner = card.shrink2(Vec2::new(12.0, 10.0));
    let white = faded(Color32::WHITE, opacity);
    let soft = faded(Color32::from_white_alpha(215), opacity);
    let title = FontId::new(15.0, FontFamily::Name(BOLD.into()));
    let small = FontId::proportional(12.5);

    let mut y = inner.top();
    y += text_line(ui, Pos2::new(inner.left(), y), inner.width(), &portal.name, title, white) + 6.0;

    let lines: Vec<String> = if let Some(flash) = state.flash.as_ref().filter(|_| state.progress.is_none()) {
        vec![flash.text.clone()]
    } else if hover == Hover::Reject {
        vec![tr("не приймає ці файли", "does not take these files").into()]
    } else if portal.folder.is_empty() {
        vec![
            tr("кинь файли або", "drop files or").into(),
            tr("ПКМ → налаштування", "right-click → settings").into(),
        ]
    } else {
        let folder = Path::new(&portal.folder)
            .file_name()
            .map_or(portal.folder.clone(), |n| n.to_string_lossy().into());
        let mut lines = vec![format!("→ {folder}"), summary(portal, 4)];
        if portal.mode == crate::config::Mode::Copy {
            lines.push(tr("копіювання", "copying").into());
        }
        lines
    };
    for line in lines {
        if y + 14.0 > inner.bottom() + 2.0 {
            break;
        }
        y += text_line(ui, Pos2::new(inner.left(), y), inner.width(), &line, small.clone(), soft) + 1.0;
    }

    if let Some(flash) = state.flash.as_ref().filter(|_| state.progress.is_none()) {
        let dot = if flash.ok { OK } else { ERROR };
        painter.circle_filled(Pos2::new(card.right() - 15.0, card.top() + 15.0), 4.5, faded(dot, opacity));
    }

    if let Some(progress) = &state.progress {
        let bar =
            Rect::from_min_size(Pos2::new(inner.left(), card.bottom() - 12.0), Vec2::new(inner.width(), 4.0));
        painter.rect_filled(bar, CornerRadius::same(2), faded(Color32::from_white_alpha(70), opacity));
        let done = progress.done.load(Ordering::Relaxed) as f32 / progress.total.max(1) as f32;
        let filled = Rect::from_min_size(bar.min, Vec2::new(bar.width() * done, bar.height()));
        painter.rect_filled(filled, CornerRadius::same(2), white);
    }

    // три риски в куті - підказка, що тут можна змінити розмір
    let corner = card.max - Vec2::splat(5.0);
    for step in [4.0, 8.0, 12.0] {
        painter.line_segment(
            [corner - Vec2::new(step, 0.0), corner - Vec2::new(0.0, step)],
            Stroke::new(1.2, faded(Color32::from_white_alpha(90), opacity)),
        );
    }
}

/// Один рядок тексту, обрізаний трикрапкою по ширині. Повертає висоту рядка.
fn text_line(ui: &Ui, pos: Pos2, width: f32, text: &str, font: FontId, color: Color32) -> f32 {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap =
        TextWrapping { max_width: width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let galley = ui.painter().layout_job(job);
    let height = galley.size().y;
    ui.painter().galley(pos, galley, color);
    height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Mode;
    use crate::presets;
    use crate::test_util::TempDir;
    use egui_kittest::Harness;

    #[derive(Debug)]
    struct DroppedPath(PathBuf);

    impl egui::DroppedFile for DroppedPath {
        fn path(&self) -> &Path {
            &self.0
        }

        fn bytes(&self) -> Result<Vec<u8>, String> {
            std::fs::read(&self.0).map_err(|e| e.to_string())
        }
    }

    struct Screen {
        portal: Portal,
        state: PortalState,
        out: Vec<Command>,
    }

    fn harness(portal: Portal) -> Harness<'static, Screen> {
        let screen = Screen { portal, state: PortalState::default(), out: Vec::new() };
        Harness::builder()
            .with_size([190.0, 115.0])
            .with_step_dt(1.0 / 60.0) // інакше два кліки не встигнуть стати подвійним
            .with_context_setup(crate::fonts::setup)
            .build_ui_state(|ui, s: &mut Screen| show(ui, &mut s.portal, &mut s.state, &mut s.out), screen)
    }

    fn drop_files(harness: &mut Harness<'static, Screen>, paths: &[PathBuf]) {
        for path in paths {
            harness.input_mut().dropped_files.push(Arc::new(DroppedPath(path.clone())));
        }
        harness.step();
    }

    fn click(harness: &mut Harness<'static, Screen>, button: egui::PointerButton) {
        let pos = Pos2::new(60.0, 60.0);
        for pressed in [true, false] {
            let event = egui::Event::PointerButton { pos, button, pressed, modifiers: Default::default() };
            harness.input_mut().events.push(event);
            harness.step();
        }
    }

    #[test]
    fn dropping_files_asks_for_transfer() {
        let tmp = TempDir::new();
        let video = presets::find("Відео").unwrap();
        let mut harness = harness(presets::preset_portal(video, "D:/Video"));
        harness.input_mut().events.push(egui::Event::PointerMoved(Pos2::new(60.0, 60.0)));
        harness.step();

        let clip = tmp.file("clip.mp4", b"x");
        drop_files(&mut harness, std::slice::from_ref(&clip));
        match &harness.state().out[..] {
            [Command::Drop { id, paths, mode }] => {
                assert_eq!(id, &harness.state().portal.id);
                assert_eq!(paths, &[clip]);
                assert_eq!(*mode, Mode::Move);
            }
            other => panic!("очікували одне кидання, а маємо {other:?}"),
        }

        drop_files(&mut harness, &[tmp.file("doc.pdf", b"x")]);
        assert_eq!(harness.state().out.len(), 1, "PDF на «Відео» не переноситься");
        assert_eq!(harness.state().state.flash.as_ref().unwrap().text, "не приймає ці файли");
    }

    #[test]
    fn mouse_opens_settings_and_folder() {
        let mut harness = harness(presets::sorter_portal("D:/Sorted"));
        harness.input_mut().events.push(egui::Event::PointerMoved(Pos2::new(60.0, 60.0)));
        harness.step();
        click(&mut harness, egui::PointerButton::Secondary);
        assert!(matches!(harness.state().out.last(), Some(Command::OpenSettings(_))));

        click(&mut harness, egui::PointerButton::Primary);
        click(&mut harness, egui::PointerButton::Primary);
        assert!(matches!(harness.state().out.last(), Some(Command::OpenFolder(_))));
    }

    #[test]
    fn empty_portal_asks_for_folder_on_double_click() {
        let mut harness = harness(Portal::default());
        harness.input_mut().events.push(egui::Event::PointerMoved(Pos2::new(60.0, 60.0)));
        harness.step();
        click(&mut harness, egui::PointerButton::Primary);
        click(&mut harness, egui::PointerButton::Primary);
        assert!(matches!(harness.state().out.last(), Some(Command::ChooseFolder(_))));
    }

    #[test]
    fn hover_states() {
        let tmp = TempDir::new();
        let video = presets::preset_portal(presets::find("Відео").unwrap(), "D:/Video");
        assert!(hover_state(&video, &[]) == Hover::None);
        assert!(hover_state(&video, &[tmp.file("a.mp4", b"x")]) == Hover::Accept);
        assert!(hover_state(&video, &[tmp.file("a.pdf", b"x")]) == Hover::Reject);
        // хоч один підходящий файл - портал приймає (решту пропустить)
        assert!(hover_state(&video, &[tmp.file("b.pdf", b"x"), tmp.file("b.mp4", b"x")]) == Hover::Accept);
        assert!(hover_state(&Portal::default(), &[tmp.file("c.pdf", b"x")]) == Hover::Accept);
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#E5484D"), Color32::from_rgb(0xE5, 0x48, 0x4D));
        assert_eq!(parse_color("nonsense"), Color32::from_rgb(0x4F, 0x7C, 0xFF));
        assert_eq!(faded(Color32::WHITE, 0.5).a(), 127);
    }
}
