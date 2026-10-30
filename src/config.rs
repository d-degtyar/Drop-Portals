//! Налаштування порталів: збереження та завантаження з JSON.

use std::path::{Path, PathBuf};
use std::{env, fs, io};

use serde::{Deserialize, Serialize};

use crate::i18n::{Lang, tr};

/// Що робити з файлом: перемістити чи скопіювати.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Move,
    Copy,
}

impl Mode {
    pub fn toggled(self) -> Self {
        match self {
            Mode::Move => Mode::Copy,
            Mode::Copy => Mode::Move,
        }
    }
}

/// Що робити, якщо в папці вже є файл з таким ім'ям.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Conflict {
    /// Додати номер: `файл (2).mp4`.
    #[default]
    Rename,
    Skip,
    Replace,
}

/// Що робити з файлом, який не підійшов під жодне правило.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unmatched {
    /// Класти прямо в папку порталу.
    #[default]
    Root,
    /// Не приймати.
    Skip,
}

/// Умова сортування: якщо файл підходить, він іде в `subfolder`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    /// Без крапки, у нижньому регістрі.
    pub extensions: Vec<String>,
    /// Маска імені, напр. `*invoice*`; порожньо - будь-яке ім'я.
    pub pattern: String,
    /// 0 - без обмеження.
    pub min_mb: f64,
    /// 0 - без обмеження.
    pub max_mb: f64,
    /// Шаблон підпапки: `Video`, `{ext}`, `{year}-{month}`.
    pub subfolder: String,
    pub enabled: bool,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            extensions: Vec::new(),
            pattern: String::new(),
            min_mb: 0.0,
            max_mb: 0.0,
            subfolder: String::new(),
            enabled: true,
        }
    }
}

/// Один портал = одне плаваюче вікно.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Portal {
    pub id: String,
    pub name: String,
    pub folder: String,
    pub color: String,
    #[serde(deserialize_with = "or_default")]
    pub mode: Mode,
    #[serde(deserialize_with = "or_default")]
    pub conflict: Conflict,
    #[serde(deserialize_with = "or_default")]
    pub unmatched: Unmatched,
    pub rules: Vec<Rule>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub visible: bool,
}

impl Default for Portal {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: tr("Новий портал", "New portal").to_owned(),
            folder: String::new(),
            color: "#4F7CFF".to_owned(),
            mode: Mode::Move,
            conflict: Conflict::Rename,
            unmatched: Unmatched::Root,
            rules: Vec::new(),
            x: 120.0,
            y: 120.0,
            width: 180.0,
            height: 110.0,
            opacity: 0.92,
            visible: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub portals: Vec<Portal>,
    /// Мова інтерфейсу; `None` - як у Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Lang>,
}

/// Назва, яку портал отримує до вибору папки (будь-якою мовою).
pub fn is_placeholder_name(name: &str) -> bool {
    matches!(name, "Новий портал" | "New portal" | "Портал")
}

/// Невідоме значення (напр. `"mode": "teleport"`) стає значенням за замовчуванням.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(T::deserialize(value).unwrap_or_default())
}

/// Короткий випадковий ідентифікатор порталу.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let mixed = nanos ^ COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{:08x}", mixed as u32)
}

/// `%APPDATA%\DropPortals` на Windows, `~/.dropportals` деінде.
/// `DROP_PORTALS_HOME` перекриває обидва (для тестів).
pub fn config_dir() -> PathBuf {
    if let Some(dir) = env::var_os("DROP_PORTALS_HOME") {
        return PathBuf::from(dir);
    }
    if cfg!(windows)
        && let Some(appdata) = env::var_os("APPDATA")
    {
        return PathBuf::from(appdata).join("DropPortals");
    }
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")).unwrap_or_default();
    PathBuf::from(home).join(".dropportals")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// Пошкоджений або старий конфіг не ламає програму: зламані портали просто пропускаються.
pub fn load(path: &Path) -> AppConfig {
    let Ok(text) = fs::read_to_string(path) else {
        return AppConfig::default();
    };
    let Ok(serde_json::Value::Object(root)) = serde_json::from_str(&text) else {
        return AppConfig::default();
    };
    let portals = match root.get("portals") {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter(|item| item.is_object())
            .filter_map(|item| serde_json::from_value(item.clone()).ok())
            .collect(),
        _ => Vec::new(),
    };
    let language = root.get("language").and_then(|value| serde_json::from_value(value.clone()).ok());
    AppConfig { portals, language }
}

/// Пише у тимчасовий файл і підміняє, щоб збій не залишив пів-конфігу.
pub fn save(config: &AppConfig, path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("dropportals-config-{name}-{}", new_id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip() {
        let path = temp_dir("roundtrip").join("cfg").join("config.json");
        let config = AppConfig {
            portals: vec![
                presets::sorter_portal("D:/x"),
                Portal { name: "Відео".into(), mode: Mode::Copy, ..Portal::default() },
            ],
            language: Some(Lang::En),
        };
        save(&config, &path).unwrap();
        assert_eq!(load(&path), config);
    }

    #[test]
    fn broken_or_old_config() {
        let dir = temp_dir("broken");
        let path = dir.join("config.json");
        assert!(load(&path).portals.is_empty());

        fs::write(&path, "{not json").unwrap();
        assert!(load(&path).portals.is_empty());

        fs::write(
            &path,
            r#"{"portals": [
                {"name": "A", "mode": "teleport", "unknown_field": 1, "x": 10,
                 "rules": [{"extensions": ["a"], "zzz": 2}]},
                "garbage",
                {"name": "B", "x": "not a number"}
            ]}"#,
        )
        .unwrap();
        let portals = load(&path).portals;
        assert_eq!(portals.len(), 1);
        assert_eq!(portals[0].name, "A");
        assert_eq!(portals[0].mode, Mode::Move);
        assert_eq!(portals[0].x, 10.0);
        assert_eq!(portals[0].rules[0].extensions, vec!["a"]);
        assert!(portals[0].rules[0].enabled);
    }

    #[test]
    fn reads_python_version_config() {
        // формат першої версії (Python) читається без змін
        let json = r##"{"portals": [{"id": "ab12cd34", "name": "Відео", "folder": "D:\\Video",
            "color": "#E5484D", "mode": "copy", "conflict": "skip", "unmatched": "skip",
            "rules": [{"extensions": ["mp4"], "pattern": "", "min_mb": 0.0, "max_mb": 0.0,
                       "subfolder": "", "enabled": true}],
            "x": 120, "y": 140, "width": 180, "height": 110, "opacity": 0.92, "visible": true}]}"##;
        let path = temp_dir("python").join("config.json");
        fs::write(&path, json).unwrap();
        let portal = &load(&path).portals[0];
        assert_eq!(portal.id, "ab12cd34");
        assert_eq!(portal.mode, Mode::Copy);
        assert_eq!(portal.conflict, Conflict::Skip);
        assert_eq!(portal.unmatched, Unmatched::Skip);
        assert_eq!(portal.y, 140.0);
    }
}
