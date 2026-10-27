//! Готові набори розширень для швидкого створення порталів.

use crate::config::{Portal, Rule, Unmatched};

pub struct Preset {
    pub name: &'static str,
    pub color: &'static str,
    pub extensions: &'static [&'static str],
}

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Документи",
        color: "#4F7CFF",
        extensions: &[
            "pdf", "doc", "docx", "rtf", "txt", "md", "odt", "xls", "xlsx", "csv", "ods", "ppt", "pptx",
            "odp", "key", "pages",
        ],
    },
    Preset {
        name: "Відео",
        color: "#E5484D",
        extensions: &["mp4", "mov", "avi", "mkv", "webm", "m4v", "wmv", "mxf", "mpg", "mpeg"],
    },
    Preset {
        name: "Зображення",
        color: "#30A46C",
        extensions: &[
            "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "heic", "svg", "exr", "tga", "dpx",
            "hdr",
        ],
    },
    Preset {
        name: "Аудіо",
        color: "#8E4EC6",
        extensions: &["mp3", "wav", "aif", "aiff", "flac", "ogg", "m4a", "aac", "wma"],
    },
    Preset {
        name: "Дизайн",
        color: "#F76B15",
        extensions: &["psd", "psb", "ai", "eps", "indd", "fig", "sketch", "xd", "afdesign", "afphoto"],
    },
    Preset {
        name: "Моушн і 3D",
        color: "#0090FF",
        extensions: &[
            "aep", "aepx", "prproj", "blend", "c4d", "fbx", "obj", "glb", "gltf", "abc", "usd", "usdz",
            "riv", "rev", "lottie",
        ],
    },
    Preset { name: "Шрифти", color: "#6E56CF", extensions: &["ttf", "otf", "woff", "woff2"] },
    Preset { name: "Архіви", color: "#978365", extensions: &["zip", "rar", "7z", "tar", "gz"] },
    Preset {
        name: "Програми", color: "#687076", extensions: &["exe", "msi", "dmg", "pkg", "zxp", "appx"]
    },
];

pub const SORTER_NAME: &str = "Сортувальник";

pub fn find(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.name == name)
}

pub fn rule(preset: &Preset, subfolder: &str) -> Rule {
    Rule {
        extensions: preset.extensions.iter().map(|ext| (*ext).to_owned()).collect(),
        subfolder: subfolder.to_owned(),
        ..Rule::default()
    }
}

/// Портал для одного типу: приймає лише свої файли.
pub fn preset_portal(preset: &Preset, folder: &str) -> Portal {
    Portal {
        name: preset.name.to_owned(),
        folder: folder.to_owned(),
        color: preset.color.to_owned(),
        rules: vec![rule(preset, "")],
        unmatched: Unmatched::Skip,
        ..Portal::default()
    }
}

/// Як DSort: усе в одну папку, розкладаючи по підпапках за типом.
pub fn sorter_portal(folder: &str) -> Portal {
    Portal {
        name: SORTER_NAME.to_owned(),
        folder: folder.to_owned(),
        color: "#12A594".to_owned(),
        rules: PRESETS.iter().map(|preset| rule(preset, preset.name)).collect(),
        unmatched: Unmatched::Root,
        width: 200.0,
        ..Portal::default()
    }
}
