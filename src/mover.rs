//! Переміщення / копіювання файлів у портал і скасування останньої дії.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::{Conflict, Mode, Portal};
use crate::rules::resolve_destination;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Moved,
    Copied,
    Skipped,
    Error,
}

#[derive(Clone, Debug)]
pub struct TransferResult {
    pub source: PathBuf,
    pub target: Option<PathBuf>,
    pub action: Action,
    pub reason: String,
}

impl TransferResult {
    fn new(source: &Path, action: Action, reason: impl Into<String>) -> Self {
        Self { source: source.to_path_buf(), target: None, action, reason: reason.into() }
    }

    fn done(source: &Path, target: PathBuf, mode: Mode) -> Self {
        let action = if mode == Mode::Copy { Action::Copied } else { Action::Moved };
        Self { source: source.to_path_buf(), target: Some(target), action, reason: String::new() }
    }

    pub fn ok(&self) -> bool {
        matches!(self.action, Action::Moved | Action::Copied)
    }

    pub fn file_name(&self) -> String {
        self.source.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }
}

/// Шлях для порівняння: абсолютний, а на Windows ще й без урахування регістру.
fn comparable(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = absolute.to_string_lossy().trim_end_matches(['/', '\\']).to_owned();
    if cfg!(windows) { text.to_lowercase() } else { text }
}

fn is_inside(child: &Path, parent: &Path) -> bool {
    let (child, parent) = (comparable(child), comparable(parent));
    child == parent || child.strip_prefix(&parent).is_some_and(|rest| rest.starts_with(['/', '\\']))
}

/// `file.txt` -> `file (2).txt` -> `file (3).txt` ...
pub fn unique_path(path: &Path) -> PathBuf {
    if fs::symlink_metadata(path).is_err() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or(Path::new(""));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let (stem, ext) = match (path.is_dir(), name.rfind('.')) {
        (false, Some(dot)) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (&name[..], ""),
    };
    (2..)
        .map(|index| parent.join(format!("{stem} ({index}){ext}")))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .expect("нескінченний ряд імен")
}

fn copy_item(src: &Path, dst: &Path) -> io::Result<()> {
    if !src.is_dir() {
        return fs::copy(src, dst).map(|_| ());
    }
    fs::create_dir(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        copy_item(&entry.path(), &dst.join(entry.file_name()))?;
    }
    Ok(())
}

fn remove_item(path: &Path) -> io::Result<()> {
    if path.is_dir() && !path.is_symlink() { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}

fn crosses_devices(error: &io::Error) -> bool {
    // EXDEV (18) на Unix, ERROR_NOT_SAME_DEVICE (17) на Windows
    let code = if cfg!(windows) { 17 } else { 18 };
    error.kind() == io::ErrorKind::CrossesDevices || error.raw_os_error() == Some(code)
}

/// Перейменування; між дисками - копія з видаленням оригіналу.
fn move_item(src: &Path, dst: &Path) -> io::Result<()> {
    match fs::rename(src, dst) {
        Err(error) if crosses_devices(&error) => {
            if let Err(error) = copy_item(src, dst) {
                let _ = remove_item(dst); // недокопійоване прибираємо, оригінал цілий
                return Err(error);
            }
            remove_item(src)
        }
        other => other,
    }
}

fn put(src: &Path, dst: &Path, mode: Mode) -> io::Result<()> {
    match mode {
        Mode::Copy => copy_item(src, dst),
        Mode::Move => move_item(src, dst),
    }
}

pub fn transfer(source: &Path, dest_dir: &Path, mode: Mode, conflict: Conflict) -> TransferResult {
    let src = std::path::absolute(source).unwrap_or_else(|_| source.to_path_buf());
    if fs::symlink_metadata(&src).is_err() {
        return TransferResult::new(&src, Action::Skipped, "файл не знайдено");
    }
    let Some(name) = src.file_name() else {
        return TransferResult::new(&src, Action::Error, "не можна перенести корінь диска");
    };
    let target = dest_dir.join(name);
    if comparable(&src) == comparable(&target) {
        return TransferResult::new(&src, Action::Skipped, "вже в цій папці");
    }
    if src.is_dir() && is_inside(dest_dir, &src) {
        return TransferResult::new(&src, Action::Error, "папку не можна покласти саму в себе");
    }
    place(&src, dest_dir, target, mode, conflict)
        .unwrap_or_else(|error| TransferResult::new(&src, Action::Error, error.to_string()))
}

fn place(
    src: &Path,
    dest_dir: &Path,
    mut target: PathBuf,
    mode: Mode,
    conflict: Conflict,
) -> io::Result<TransferResult> {
    fs::create_dir_all(dest_dir)?;
    if fs::symlink_metadata(&target).is_ok() {
        match conflict {
            Conflict::Skip => return Ok(TransferResult::new(src, Action::Skipped, "такий файл уже є")),
            Conflict::Replace if target.is_file() && src.is_file() => {
                // спершу кладемо поруч під тимчасовим ім'ям, потім підміняємо:
                // якщо щось піде не так, старий файл не пропаде
                let tmp = dest_dir.join(format!(".dropportals-{}.tmp", crate::config::new_id()));
                put(src, &tmp, mode)?;
                fs::rename(&tmp, &target)?;
                let mut result = TransferResult::done(src, target, mode);
                result.reason = "замінено".into();
                return Ok(result);
            }
            _ => target = unique_path(&target),
        }
    }
    put(src, &target, mode)?;
    Ok(TransferResult::done(src, target, mode))
}

pub fn transfer_many(
    portal: &Portal,
    paths: &[PathBuf],
    mode: Mode,
    mut progress: impl FnMut(usize, usize),
) -> Vec<TransferResult> {
    let mut results = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        results.push(match resolve_destination(portal, path) {
            Some(dest) => transfer(path, &dest, mode, portal.conflict),
            None => TransferResult::new(path, Action::Skipped, "не підходить під правила порталу"),
        });
        progress(index + 1, paths.len());
    }
    results
}

/// Повертає переміщене на місце і прибирає копії. Повертає список помилок.
pub fn undo(results: &[TransferResult]) -> Vec<String> {
    let mut errors = Vec::new();
    for item in results.iter().rev().filter(|item| item.ok()) {
        let Some(target) = &item.target else { continue };
        let outcome = match item.action {
            Action::Moved if fs::symlink_metadata(&item.source).is_ok() => {
                errors.push(format!("{}: на старому місці вже є файл", item.file_name()));
                continue;
            }
            Action::Moved => item
                .source
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|_| move_item(target, &item.source)),
            _ => remove_item(target),
        };
        if let Err(error) = outcome {
            errors.push(format!("{}: {error}", item.file_name()));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;
    use crate::test_util::TempDir;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn move_and_undo() {
        let tmp = TempDir::new();
        let path = tmp.file("Downloads/doc.pdf", b"hello");
        let dst = tmp.path().join("Target");
        let result = transfer(&path, &dst, Mode::Move, Conflict::Rename);
        assert_eq!(result.action, Action::Moved);
        assert!(!path.exists());
        assert_eq!(fs::read(result.target.as_ref().unwrap()).unwrap(), b"hello");
        assert!(undo(std::slice::from_ref(&result)).is_empty());
        assert!(path.exists());
        assert!(!result.target.unwrap().exists());
    }

    #[test]
    fn copy_and_undo() {
        let tmp = TempDir::new();
        let path = tmp.file("Downloads/doc.pdf", b"x");
        let result = transfer(&path, &tmp.path().join("Target"), Mode::Copy, Conflict::Rename);
        assert_eq!(result.action, Action::Copied);
        assert!(path.exists());
        undo(std::slice::from_ref(&result));
        assert!(path.exists());
        assert!(!result.target.unwrap().exists());
    }

    #[test]
    fn conflict_rename() {
        let tmp = TempDir::new();
        let dst = tmp.dir("Target");
        tmp.file("Target/a.txt", b"old");
        let result = transfer(&tmp.file("Downloads/a.txt", b"new"), &dst, Mode::Move, Conflict::Rename);
        assert_eq!(result.target.unwrap().file_name().unwrap(), "a (2).txt");
        assert_eq!(fs::read(dst.join("a.txt")).unwrap(), b"old");
        assert_eq!(unique_path(&dst.join("a.txt")).file_name().unwrap(), "a (3).txt");
        assert_eq!(unique_path(&dst.join(".hidden")).file_name().unwrap(), ".hidden");
    }

    #[test]
    fn conflict_skip_and_replace() {
        let tmp = TempDir::new();
        let dst = tmp.dir("Target");
        tmp.file("Target/a.txt", b"old");
        let skipped = transfer(&tmp.file("Downloads/a.txt", b"new"), &dst, Mode::Move, Conflict::Skip);
        assert_eq!(skipped.action, Action::Skipped);
        let replaced = transfer(&tmp.file("Downloads/a.txt", b"new"), &dst, Mode::Move, Conflict::Replace);
        assert_eq!(replaced.action, Action::Moved);
        assert_eq!(fs::read(dst.join("a.txt")).unwrap(), b"new");
        assert_eq!(names(&dst), ["a.txt"]);
    }

    #[test]
    fn folder_move_and_self_protection() {
        let tmp = TempDir::new();
        let folder = tmp.dir("Downloads/pack");
        tmp.file("Downloads/pack/inner/f.txt", b"1");
        let bad = transfer(&folder, &folder.join("inner"), Mode::Move, Conflict::Rename);
        assert_eq!(bad.action, Action::Error);
        let good = transfer(&folder, &tmp.path().join("Target"), Mode::Move, Conflict::Rename);
        assert_eq!(good.action, Action::Moved);
        assert!(tmp.path().join("Target/pack/inner/f.txt").exists());
    }

    #[test]
    fn folder_copy() {
        let tmp = TempDir::new();
        let folder = tmp.dir("Downloads/pack");
        tmp.file("Downloads/pack/inner/f.txt", b"1");
        let result = transfer(&folder, &tmp.path().join("Target"), Mode::Copy, Conflict::Rename);
        assert_eq!(result.action, Action::Copied);
        assert!(tmp.path().join("Target/pack/inner/f.txt").exists());
        assert!(folder.join("inner/f.txt").exists());
    }

    #[test]
    fn same_folder_and_missing() {
        let tmp = TempDir::new();
        let path = tmp.file("Downloads/a.txt", b"x");
        let downloads = tmp.path().join("Downloads");
        assert_eq!(transfer(&path, &downloads, Mode::Move, Conflict::Rename).action, Action::Skipped);
        let missing = transfer(&downloads.join("nope.txt"), tmp.path(), Mode::Move, Conflict::Rename);
        assert_eq!(missing.action, Action::Skipped);
    }

    #[test]
    fn many_with_rules_progress_and_undo() {
        let tmp = TempDir::new();
        let dst = tmp.path().join("Target");
        let portal = presets::sorter_portal(dst.to_str().unwrap());
        let paths = ["a.pdf", "b.mp4", "c.unknown"].map(|n| tmp.file(&format!("Downloads/{n}"), b"x"));
        let mut progress = Vec::new();
        let results = transfer_many(&portal, &paths, Mode::Move, |done, total| progress.push((done, total)));
        assert!(results.iter().all(TransferResult::ok));
        assert!(dst.join("Документи/a.pdf").exists());
        assert!(dst.join("Відео/b.mp4").exists());
        assert!(dst.join("c.unknown").exists());
        assert_eq!(progress.last(), Some(&(3, 3)));
        assert!(undo(&results).is_empty());
        assert_eq!(names(&tmp.path().join("Downloads")), ["a.pdf", "b.mp4", "c.unknown"]);
    }

    #[test]
    fn many_skips_rejected() {
        let tmp = TempDir::new();
        let portal =
            presets::preset_portal(presets::find("Відео").unwrap(), tmp.path().join("V").to_str().unwrap());
        let path = tmp.file("Downloads/a.pdf", b"x");
        let results = transfer_many(&portal, std::slice::from_ref(&path), Mode::Move, |_, _| {});
        assert_eq!(results[0].action, Action::Skipped);
        assert!(path.exists());
    }

    #[test]
    fn undo_does_not_overwrite() {
        let tmp = TempDir::new();
        let path = tmp.file("Downloads/a.txt", b"first");
        let result = transfer(&path, &tmp.path().join("Target"), Mode::Move, Conflict::Rename);
        tmp.file("Downloads/a.txt", b"second");
        assert_eq!(undo(std::slice::from_ref(&result)).len(), 1);
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert!(result.target.unwrap().exists());
    }
}
