//! Панель порталу (ПКМ по порталу): швидкі дії, папка, колір, режим і правила.
//! Зміни застосовуються одразу, без кнопки «Зберегти».

use egui::color_picker::{Alpha, color_edit_button_srgba};
use egui::{ComboBox, Grid, Pos2, Rect, RichText, ScrollArea, TextEdit, Ui, Vec2};

use crate::app::Command;
use crate::config::{Conflict, Mode, Portal, Rule, Unmatched};
use crate::i18n::tr;
use crate::portal_view::{PortalState, parse_color};
use crate::presets::{self, PRESETS};
use crate::rules::normalize_extensions;

fn help() -> &'static str {
    tr(
        "Перше правило, що підійшло, вирішує, куди піде файл. Порожня клітинка = без умови.\n\
         Підпапка: порожньо - прямо в папку порталу. Шаблон: {ext}, {EXT}, {name}, {year}, {month}, {day}, \
         напр. «Video\\{year}-{month}».",
        "The first matching rule decides where a file goes. An empty cell means no condition.\n\
         Subfolder: empty - straight into the portal folder. Template: {ext}, {EXT}, {name}, {year}, \
         {month}, {day}, e.g. \"Video\\{year}-{month}\".",
    )
}

/// Рядок таблиці правил у вигляді тексту, як його набирає користувач.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleRow {
    pub enabled: bool,
    pub extensions: String,
    pub pattern: String,
    pub min_mb: String,
    pub max_mb: String,
    pub subfolder: String,
}

fn number_text(value: f64) -> String {
    if value > 0.0 { format!("{value}") } else { String::new() }
}

fn parse_number(text: &str) -> f64 {
    text.trim().replace(',', ".").parse::<f64>().map_or(0.0, |v| v.max(0.0))
}

impl RuleRow {
    pub fn from_rule(rule: &Rule) -> Self {
        Self {
            enabled: rule.enabled,
            extensions: rule.extensions.join(" "),
            pattern: rule.pattern.clone(),
            min_mb: number_text(rule.min_mb),
            max_mb: number_text(rule.max_mb),
            subfolder: rule.subfolder.clone(),
        }
    }

    /// `None` для порожнього рядка - такий не потрапляє в налаштування.
    pub fn to_rule(&self) -> Option<Rule> {
        let rule = Rule {
            enabled: self.enabled,
            extensions: normalize_extensions(&self.extensions),
            pattern: self.pattern.trim().to_owned(),
            min_mb: parse_number(&self.min_mb),
            max_mb: parse_number(&self.max_mb),
            subfolder: self.subfolder.trim().to_owned(),
        };
        let empty = rule.extensions.is_empty()
            && rule.pattern.is_empty()
            && rule.min_mb == 0.0
            && rule.max_mb == 0.0
            && rule.subfolder.is_empty();
        (!empty).then_some(rule)
    }
}

/// Бажаний розмір панелі; на маленькому екрані вона зменшується до робочої області.
pub const SIZE: Vec2 = Vec2::new(760.0, 560.0);
const GAP: f32 = 12.0;

pub struct SettingsState {
    pub portal_id: String,
    pub rows: Vec<RuleRow>,
    pub confirm_delete: bool,
    /// Де і якого розміру відкрилось вікно - далі його рухає лише користувач.
    pub rect: Rect,
}

impl SettingsState {
    pub fn new(portal: &Portal, rect: Rect) -> Self {
        Self {
            portal_id: portal.id.clone(),
            rows: portal.rules.iter().map(RuleRow::from_rule).collect(),
            confirm_delete: false,
            rect,
        }
    }
}

/// Панель поруч із порталом: праворуч, якщо влазить, інакше ліворуч,
/// і завжди в межах робочої області монітора.
pub fn place_beside(portal: Rect, area: Rect) -> Rect {
    let size = SIZE.min(area.size());
    let right = portal.right() + GAP;
    let left = portal.left() - GAP - size.x;
    let x = if right + size.x <= area.right() {
        right
    } else if left >= area.left() {
        left
    } else {
        area.right() - size.x
    };
    let y = portal.top().clamp(area.top(), area.bottom() - size.y);
    Rect::from_min_size(Pos2::new(x.clamp(area.left(), area.right() - size.x), y), size)
}

pub fn show(
    ui: &mut Ui,
    portal: &mut Portal,
    settings: &mut SettingsState,
    state: &PortalState,
    out: &mut Vec<Command>,
) {
    let before = portal.clone();
    egui::CentralPanel::default().show(ui, |ui| {
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            actions(ui, portal, settings, state, out);
            ui.separator();
            form(ui, portal, out);
            ui.add_space(8.0);
            rules_table(ui, settings);
            ui.add_space(6.0);
            ui.label(RichText::new(help()).weak().small());
        });
    });
    portal.rules = settings.rows.iter().filter_map(RuleRow::to_rule).collect();
    if *portal != before {
        out.push(Command::Changed);
    }
}

fn actions(
    ui: &mut Ui,
    portal: &Portal,
    settings: &mut SettingsState,
    state: &PortalState,
    out: &mut Vec<Command>,
) {
    let id = || portal.id.clone();
    if settings.confirm_delete {
        // окремий рядок замість кнопок, щоб питання і відповіді стояли поруч
        ui.horizontal(|ui| {
            let question = format!(
                "{} {}? {}",
                tr("Видалити портал", "Delete portal"),
                crate::i18n::quoted(&portal.name),
                tr("Файли в папці залишаться.", "Files in the folder stay."),
            );
            ui.label(RichText::new(question).color(ui.visuals().warn_fg_color));
            if ui.button(tr("Так, видалити", "Yes, delete")).clicked() {
                out.push(Command::Delete(id()));
            }
            if ui.button(tr("Скасувати", "Cancel")).clicked() {
                settings.confirm_delete = false;
            }
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            let has_folder = !portal.folder.is_empty();
            if ui.add_enabled(has_folder, egui::Button::new(tr("Відкрити папку", "Open folder"))).clicked()
            {
                out.push(Command::OpenFolder(id()));
            }
            let can_undo = !state.last_results.is_empty() && state.progress.is_none();
            if ui.add_enabled(can_undo, egui::Button::new(tr("↩ Скасувати останнє", "↩ Undo last"))).clicked()
            {
                out.push(Command::Undo(id()));
            }
            if ui.button(tr("Дублювати", "Duplicate")).clicked() {
                out.push(Command::Duplicate(id()));
            }
            if ui.button(tr("Сховати", "Hide")).clicked() {
                out.push(Command::Hide(id()));
            }
            if ui.button(tr("Видалити…", "Delete…")).clicked() {
                settings.confirm_delete = true;
            }
        });
    }
    if !state.report.is_empty() {
        ui.collapsing(
            format!("{}: {}", tr("Останнє кидання, проблем", "Last drop, problems"), state.report.len()),
            |ui| {
                for line in &state.report {
                    ui.label(RichText::new(line).small());
                }
            },
        );
    }
}

fn form(ui: &mut Ui, portal: &mut Portal, out: &mut Vec<Command>) {
    Grid::new("form").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(tr("Назва", "Name"));
        ui.add(TextEdit::singleline(&mut portal.name).desired_width(320.0));
        ui.end_row();

        ui.label(tr("Папка", "Folder"));
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut portal.folder)
                    .desired_width(320.0)
                    .hint_text("D:\\Projects\\Video"),
            );
            if cfg!(windows) && ui.button(tr("Огляд…", "Browse…")).clicked() {
                out.push(Command::ChooseFolder(portal.id.clone()));
            }
        });
        ui.end_row();

        ui.label(tr("Колір", "Color"));
        let mut color = parse_color(&portal.color);
        if color_edit_button_srgba(ui, &mut color, Alpha::Opaque).changed() {
            portal.color = color.to_hex()[..7].to_owned();
        }
        ui.end_row();

        ui.label(tr("Дія", "Action"));
        ui.horizontal(|ui| {
            ui.radio_value(&mut portal.mode, Mode::Move, tr("Переміщувати", "Move"));
            ui.radio_value(&mut portal.mode, Mode::Copy, tr("Копіювати", "Copy"));
            ui.label(
                RichText::new(tr(
                    "(Ctrl під час кидання - навпаки)",
                    "(hold Ctrl while dropping for the opposite)",
                ))
                .weak(),
            );
        });
        ui.end_row();

        ui.label(tr("Якщо файл уже є", "If the file exists"));
        let conflict_label = |c: Conflict| match c {
            Conflict::Rename => tr("Додати номер: файл (2).ext", "Add a number: file (2).ext"),
            Conflict::Skip => tr("Пропустити", "Skip"),
            Conflict::Replace => tr("Замінити", "Replace"),
        };
        ComboBox::from_id_salt("conflict")
            .width(260.0)
            .selected_text(conflict_label(portal.conflict))
            .show_ui(ui, |ui| {
                for value in [Conflict::Rename, Conflict::Skip, Conflict::Replace] {
                    ui.selectable_value(&mut portal.conflict, value, conflict_label(value));
                }
            });
        ui.end_row();

        ui.label(tr("Файли без правила", "Files without a rule"));
        let unmatched_label = |u: Unmatched| match u {
            Unmatched::Root => tr("Класти прямо в папку порталу", "Put straight into the portal folder"),
            Unmatched::Skip => tr("Не приймати", "Do not accept"),
        };
        ComboBox::from_id_salt("unmatched")
            .width(260.0)
            .selected_text(unmatched_label(portal.unmatched))
            .show_ui(ui, |ui| {
                for value in [Unmatched::Root, Unmatched::Skip] {
                    ui.selectable_value(&mut portal.unmatched, value, unmatched_label(value));
                }
            });
        ui.end_row();

        ui.label(tr("Непрозорість", "Opacity"));
        ui.add(
            egui::Slider::new(&mut portal.opacity, 0.3..=1.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        );
        ui.end_row();
    });
}

fn rules_table(ui: &mut Ui, settings: &mut SettingsState) {
    ui.label(RichText::new(tr("Правила сортування", "Sorting rules")).strong());
    let rows = &mut settings.rows;
    let mut action: Option<(usize, i32)> = None; // (рядок, -1 вгору / 1 вниз / 0 видалити)
    Grid::new("rules").striped(true).num_columns(7).spacing([6.0, 4.0]).show(ui, |ui| {
        let titles = [
            "",
            tr("Розширення", "Extensions"),
            tr("Маска імені", "Name mask"),
            tr("Мін, МБ", "Min, MB"),
            tr("Макс, МБ", "Max, MB"),
            tr("Підпапка", "Subfolder"),
            "",
        ];
        for title in titles {
            ui.label(RichText::new(title).weak());
        }
        ui.end_row();
        let count = rows.len();
        for (index, row) in rows.iter_mut().enumerate() {
            ui.checkbox(&mut row.enabled, "").on_hover_text(tr("Увімкнено", "Enabled"));
            ui.add_sized([170.0, 20.0], TextEdit::singleline(&mut row.extensions).hint_text("pdf docx"));
            ui.add_sized([90.0, 20.0], TextEdit::singleline(&mut row.pattern).hint_text("*invoice*"));
            ui.add_sized([50.0, 20.0], TextEdit::singleline(&mut row.min_mb));
            ui.add_sized([50.0, 20.0], TextEdit::singleline(&mut row.max_mb));
            ui.add_sized([130.0, 20.0], TextEdit::singleline(&mut row.subfolder).hint_text("Video"));
            ui.horizontal(|ui| {
                if ui.add_enabled(index > 0, egui::Button::new("↑").small()).clicked() {
                    action = Some((index, -1));
                }
                if ui.add_enabled(index + 1 < count, egui::Button::new("↓").small()).clicked() {
                    action = Some((index, 1));
                }
                if ui.small_button("✕").on_hover_text(tr("Видалити правило", "Delete rule")).clicked()
                {
                    action = Some((index, 0));
                }
            });
            ui.end_row();
        }
    });
    match action {
        Some((index, 0)) => {
            rows.remove(index);
        }
        Some((index, step)) => rows.swap(index, (index as i32 + step) as usize),
        None => {}
    }

    ui.horizontal(|ui| {
        if ui.button(tr("+ Правило", "+ Rule")).clicked() {
            rows.push(RuleRow { enabled: true, ..RuleRow::default() });
        }
        ui.menu_button(tr("+ З набору", "+ From preset"), |ui| {
            for preset in PRESETS {
                if ui.button(preset.name()).clicked() {
                    rows.push(RuleRow::from_rule(&presets::rule(preset, "")));
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    struct Screen {
        portal: Portal,
        settings: SettingsState,
        state: PortalState,
        out: Vec<Command>,
    }

    fn harness(portal: Portal) -> Harness<'static, Screen> {
        let settings = SettingsState::new(&portal, Rect::from_min_size(Pos2::ZERO, SIZE));
        let screen = Screen { portal, settings, state: PortalState::default(), out: Vec::new() };
        Harness::builder()
            .with_size([760.0, 560.0])
            .with_context_setup(crate::fonts::setup)
            // підписи полів стоять у сітці поруч, а не прив'язані до полів
            .with_accessibility_check(false)
            .build_ui_state(
                |ui, s: &mut Screen| show(ui, &mut s.portal, &mut s.settings, &s.state, &mut s.out),
                screen,
            )
    }

    #[test]
    fn buttons_send_commands() {
        let mut harness = harness(presets::sorter_portal("D:/Sorted"));
        harness.run();
        assert!(harness.state().out.is_empty(), "без змін нічого не зберігаємо");

        harness.get_by_label("Дублювати").click();
        harness.run();
        assert!(harness.state().out.iter().any(|c| matches!(c, Command::Duplicate(_))));

        harness.get_by_label("Видалити…").click();
        harness.run();
        harness.get_by_label("Так, видалити").click();
        harness.run();
        assert!(harness.state().out.iter().any(|c| matches!(c, Command::Delete(_))));
    }

    #[test]
    fn empty_rule_row_is_not_saved() {
        let mut harness = harness(Portal { folder: "D:/x".into(), ..Portal::default() });
        harness.run();
        harness.get_by_label("+ Правило").click();
        harness.run();
        assert_eq!(harness.state().settings.rows.len(), 1);
        assert!(harness.state().portal.rules.is_empty());

        harness.state_mut().settings.rows[0].extensions = "PDF, .docx".into();
        harness.run();
        assert_eq!(harness.state().portal.rules[0].extensions, ["pdf", "docx"]);
        assert!(harness.state().out.iter().any(|c| matches!(c, Command::Changed)));
    }

    #[test]
    fn panel_stays_on_screen() {
        let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1040.0));
        let portal = |x: f32, y: f32| Rect::from_min_size(Pos2::new(x, y), Vec2::new(180.0, 110.0));

        // місця праворуч досить
        let panel = place_beside(portal(100.0, 100.0), area);
        assert_eq!(panel.min, Pos2::new(292.0, 100.0));

        // портал біля правого краю - панель ліворуч від нього
        let panel = place_beside(portal(1700.0, 100.0), area);
        assert_eq!(panel.right(), 1700.0 - GAP);

        // портал унизу - панель піднімається, щоб не вилізти за екран
        let panel = place_beside(portal(100.0, 900.0), area);
        assert_eq!(panel.bottom(), area.bottom());

        // ні праворуч, ні ліворуч не влазить - притискаємо до краю
        let narrow = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 1040.0));
        let panel = place_beside(portal(400.0, 100.0), narrow);
        assert!(narrow.contains_rect(panel), "{panel:?}");

        // маленький екран - панель зменшується
        let small = Rect::from_min_size(Pos2::new(-1366.0, 0.0), Vec2::new(1366.0, 500.0));
        let panel = place_beside(portal(-200.0, 50.0), small);
        assert!(small.contains_rect(panel), "{panel:?}");
        assert_eq!(panel.height(), 500.0);
    }

    #[test]
    fn rule_rows_roundtrip() {
        let rule = Rule {
            extensions: vec!["pdf".into(), "docx".into()],
            pattern: "*a*".into(),
            min_mb: 1.5,
            max_mb: 0.0,
            subfolder: "Docs".into(),
            enabled: false,
        };
        let row = RuleRow::from_rule(&rule);
        assert_eq!(row.min_mb, "1.5");
        assert_eq!(row.max_mb, "");
        assert_eq!(row.to_rule(), Some(rule));
    }

    #[test]
    fn typed_values_are_normalized() {
        let row = RuleRow {
            enabled: true,
            extensions: ".PDF, docx".into(),
            min_mb: "1,5".into(),
            max_mb: "abc".into(),
            ..RuleRow::default()
        };
        let rule = row.to_rule().unwrap();
        assert_eq!(rule.extensions, ["pdf", "docx"]);
        assert_eq!(rule.min_mb, 1.5);
        assert_eq!(rule.max_mb, 0.0);
        assert_eq!(RuleRow { enabled: true, ..RuleRow::default() }.to_rule(), None);
    }
}
