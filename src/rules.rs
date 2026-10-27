//! Правила сортування: чи підходить файл і в яку папку його класти.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Local};

use crate::config::{Portal, Rule, Unmatched};

const MB: f64 = 1024.0 * 1024.0;

/// `"pdf, .DOCX; *.xls"` -> `["pdf", "docx", "xls"]`.
pub fn normalize_extensions(text: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for part in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let ext = part.trim().trim_start_matches('*').trim_start_matches('.').to_lowercase();
        if !ext.is_empty() && !result.contains(&ext) {
            result.push(ext);
        }
    }
    result
}

pub fn extension(path: &Path) -> String {
    path.extension().map(|ext| ext.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Маска з `*` (будь-що) і `?` (один символ), без урахування регістру.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                backtrack = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match backtrack {
                Some((star, matched)) => {
                    p = star + 1;
                    t = matched + 1;
                    backtrack = Some((star, matched + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

pub fn rule_matches(rule: &Rule, path: &Path) -> bool {
    if !rule.enabled {
        return false;
    }
    let is_dir = path.is_dir();
    if !rule.extensions.is_empty() && (is_dir || !rule.extensions.contains(&extension(path))) {
        return false;
    }
    if !rule.pattern.is_empty() && !wildcard_match(&rule.pattern, &file_name(path)) {
        return false;
    }
    if rule.min_mb > 0.0 || rule.max_mb > 0.0 {
        if is_dir {
            return false;
        }
        let size = fs::metadata(path).map_or(0, |meta| meta.len()) as f64;
        if rule.min_mb > 0.0 && size < rule.min_mb * MB {
            return false;
        }
        if rule.max_mb > 0.0 && size > rule.max_mb * MB {
            return false;
        }
    }
    true
}

/// Підставляє `{ext} {EXT} {name} {year} {month} {day}` (дата зміни файлу).
/// Результат - безпечний відносний шлях: без `..` і заборонених у Windows символів.
pub fn render_subfolder(template: &str, path: &Path) -> PathBuf {
    let modified: DateTime<Local> =
        fs::metadata(path).and_then(|meta| meta.modified()).map_or_else(|_| Local::now(), DateTime::from);
    let ext = match extension(path) {
        ext if ext.is_empty() => "no_ext".to_owned(),
        ext => ext,
    };
    let value = |token: &str| -> Option<String> {
        Some(match token {
            "ext" => ext.clone(),
            "EXT" => ext.to_uppercase(),
            "name" => path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            "year" => format!("{:04}", modified.year()),
            "month" => format!("{:02}", modified.month()),
            "day" => format!("{:02}", modified.day()),
            _ => return None,
        })
    };

    let mut text = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        text.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}').and_then(|end| value(&after[..end]).map(|v| (end, v))) {
            Some((end, replacement)) => {
                text.push_str(&replacement);
                rest = &after[end + 1..];
            }
            None => {
                text.push('{');
                rest = after;
            }
        }
    }
    text.push_str(rest);

    let mut result = PathBuf::new();
    for part in text.split(['/', '\\']) {
        let clean: String =
            part.chars().map(|c| if "<>:\"|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
        let clean = clean.trim().trim_end_matches('.');
        if !clean.is_empty() {
            result.push(clean);
        }
    }
    result
}

pub fn matching_rule<'a>(portal: &'a Portal, path: &Path) -> Option<&'a Rule> {
    portal.rules.iter().find(|rule| rule_matches(rule, path))
}

/// Папка призначення або `None`, якщо портал файл не приймає.
/// Перше правило, що підійшло, перемагає.
pub fn resolve_destination(portal: &Portal, path: &Path) -> Option<PathBuf> {
    if portal.folder.is_empty() {
        return None;
    }
    let folder = PathBuf::from(&portal.folder);
    match matching_rule(portal, path) {
        Some(rule) => {
            let sub = render_subfolder(&rule.subfolder, path);
            // join("") додав би зайвий слеш у кінці
            Some(if sub.as_os_str().is_empty() { folder } else { folder.join(sub) })
        }
        None if portal.unmatched == Unmatched::Skip && portal.rules.iter().any(|r| r.enabled) => None,
        None => Some(folder),
    }
}

/// Короткий підпис для вікна порталу: `pdf docx xls +12`.
pub fn summary(portal: &Portal, limit: usize) -> String {
    let mut exts: Vec<&str> = Vec::new();
    for rule in portal.rules.iter().filter(|rule| rule.enabled) {
        for ext in &rule.extensions {
            if !exts.contains(&ext.as_str()) {
                exts.push(ext);
            }
        }
    }
    match exts.len() {
        0 => "усі файли".to_owned(),
        n if n > limit => format!("{} +{}", exts[..limit].join(" "), n - limit),
        _ => exts.join(" "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{self, PRESETS};
    use crate::test_util::TempDir;

    #[test]
    fn normalizes_extensions() {
        assert_eq!(normalize_extensions("pdf, .DOCX; *.xls  pdf"), ["pdf", "docx", "xls"]);
        assert!(normalize_extensions("").is_empty());
    }

    #[test]
    fn wildcards() {
        assert!(wildcard_match("*invoice*", "Invoice_March.PDF"));
        assert!(wildcard_match("IMG_????.jpg", "img_0042.JPG"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("a*b*c", "aXXbYYc"));
        assert!(!wildcard_match("report*", "Invoice.pdf"));
        assert!(!wildcard_match("a*b", "aXXc"));
    }

    #[test]
    fn extension_pattern_and_enabled() {
        let tmp = TempDir::new();
        let path = tmp.file("Invoice_March.PDF", b"x");
        let ext = |e: &str| Rule { extensions: vec![e.into()], ..Rule::default() };
        assert!(rule_matches(&ext("pdf"), &path));
        assert!(!rule_matches(&ext("mp4"), &path));
        assert!(rule_matches(&Rule { pattern: "*invoice*".into(), ..Rule::default() }, &path));
        assert!(!rule_matches(&Rule { pattern: "report*".into(), ..Rule::default() }, &path));
        assert!(!rule_matches(&Rule { enabled: false, ..ext("pdf") }, &path));
    }

    #[test]
    fn size_limits() {
        let tmp = TempDir::new();
        let path = tmp.file("big.bin", &vec![0u8; 2 * 1024 * 1024]);
        assert!(rule_matches(&Rule { min_mb: 1.0, ..Rule::default() }, &path));
        assert!(!rule_matches(&Rule { min_mb: 3.0, ..Rule::default() }, &path));
        assert!(!rule_matches(&Rule { max_mb: 1.0, ..Rule::default() }, &path));
    }

    #[test]
    fn folders_skip_extension_rules() {
        let tmp = TempDir::new();
        let folder = tmp.dir("pack");
        assert!(!rule_matches(&Rule { extensions: vec!["zip".into()], ..Rule::default() }, &folder));
        assert!(rule_matches(&Rule { pattern: "pa*".into(), ..Rule::default() }, &folder));
    }

    #[test]
    fn subfolder_template_is_safe() {
        let tmp = TempDir::new();
        let path = tmp.file("clip.mov", b"x");
        assert_eq!(render_subfolder("Video/{EXT}", &path), Path::new("Video").join("MOV"));
        assert_eq!(render_subfolder("../../{ext}", &path), Path::new("mov"));
        assert_eq!(render_subfolder("a<b>:c", &path), Path::new("a_b__c"));
        assert_eq!(render_subfolder("{unknown}", &path), Path::new("{unknown}"));
        assert_eq!(render_subfolder("", &path), Path::new(""));
        let dated = render_subfolder("{year}-{month}", &path).to_string_lossy().into_owned();
        assert!(dated.len() == 7 && dated.as_bytes()[4] == b'-', "{dated}");
    }

    #[test]
    fn preset_portal_rejects_other_types() {
        let tmp = TempDir::new();
        let dst = tmp.path().join("Video");
        let video = presets::find("Відео").unwrap();
        let portal = presets::preset_portal(video, dst.to_str().unwrap());
        assert_eq!(resolve_destination(&portal, &tmp.file("a.mp4", b"x")), Some(dst));
        assert_eq!(resolve_destination(&portal, &tmp.file("a.pdf", b"x")), None);
    }

    #[test]
    fn sorter_uses_subfolders() {
        let tmp = TempDir::new();
        let dst = tmp.path().join("Sorted");
        let portal = presets::sorter_portal(dst.to_str().unwrap());
        assert_eq!(portal.rules.len(), PRESETS.len());
        let dest = |name: &str| resolve_destination(&portal, &tmp.file(name, b"x")).unwrap();
        assert_eq!(dest("a.pdf"), dst.join("Документи"));
        assert_eq!(dest("a.wav"), dst.join("Аудіо"));
        assert_eq!(dest("a.xyz"), dst);
    }

    #[test]
    fn portal_without_folder_or_rules() {
        let tmp = TempDir::new();
        assert_eq!(resolve_destination(&Portal::default(), &tmp.file("a.txt", b"x")), None);
        let folder = tmp.path().join("dst");
        let portal = Portal {
            folder: folder.to_string_lossy().into_owned(),
            unmatched: Unmatched::Skip,
            ..Portal::default()
        };
        assert_eq!(resolve_destination(&portal, &tmp.file("b.txt", b"x")), Some(folder));
    }

    #[test]
    fn summaries() {
        assert_eq!(summary(&Portal::default(), 4), "усі файли");
        let rule =
            Rule { extensions: ["a", "b", "c", "d", "e"].map(String::from).to_vec(), ..Rule::default() };
        let portal = Portal { rules: vec![rule], ..Portal::default() };
        assert_eq!(summary(&portal, 4), "a b c d +1");
    }
}
