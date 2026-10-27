//! Тимчасові папки для тестів.

use std::fs;
use std::path::{Path, PathBuf};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("dropportals-test-{}", crate::config::new_id()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Створює файл (і проміжні папки) з вмістом.
    pub fn file(&self, relative: &str, data: &[u8]) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, data).unwrap();
        path
    }

    pub fn dir(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
