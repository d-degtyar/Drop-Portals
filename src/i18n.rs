//! Мови інтерфейсу: українська та англійська.
//! Рядки пишуться парою прямо в коді: `tr("Відкрити папку", "Open folder")`.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Uk,
    En,
}

/// Поточна мова для всіх потоків (і для фонових перенесень теж).
static CURRENT: AtomicU8 = AtomicU8::new(Lang::Uk as u8);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::En as u8 { Lang::En } else { Lang::Uk }
}

pub fn pick<T>(lang: Lang, uk: T, en: T) -> T {
    match lang {
        Lang::Uk => uk,
        Lang::En => en,
    }
}

/// Рядок поточною мовою.
pub fn tr(uk: &'static str, en: &'static str) -> &'static str {
    pick(current(), uk, en)
}

/// Назва в лапках, звичних для мови: «назва» або "name".
pub fn quoted(text: &str) -> String {
    pick(current(), format!("«{text}»"), format!("\"{text}\""))
}

/// Мова Windows: українська, якщо система українською, інакше англійська.
#[cfg(windows)]
pub fn system() -> Lang {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    const LANG_UKRAINIAN: u16 = 0x22;
    let primary = unsafe { GetUserDefaultUILanguage() } & 0x3ff;
    if primary == LANG_UKRAINIAN { Lang::Uk } else { Lang::En }
}

#[cfg(not(windows))]
pub fn system() -> Lang {
    let locale = std::env::var("LC_ALL").or_else(|_| std::env::var("LANG")).unwrap_or_default();
    if locale.starts_with("uk") { Lang::Uk } else { Lang::En }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_by_language() {
        assert_eq!(pick(Lang::Uk, "Так", "Yes"), "Так");
        assert_eq!(pick(Lang::En, "Так", "Yes"), "Yes");
        assert_eq!(quoted("Відео"), "«Відео»"); // у тестах мова - українська
    }

    #[test]
    fn language_is_saved_in_lowercase() {
        assert_eq!(serde_json::to_string(&Lang::En).unwrap(), "\"en\"");
        assert_eq!(serde_json::from_str::<Lang>("\"uk\"").unwrap(), Lang::Uk);
    }
}
