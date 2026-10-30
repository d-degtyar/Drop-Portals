# Drop Portals

Floating folder portals for Windows. You scroll through a folder full of mixed files:
see a document, drop it on the "Documents" portal and it is there. See a video, drop it on "Video".

[Українською](README.uk.md)

- As many portals as you like, each in its own always-on-top window.
- A portal can be simple ("everything goes here") or sort files with rules, like DSort:
  by extension, name mask or size, into subfolders.
- Large files move in the background with progress on the portal.
- The last drop can be undone.
- A single ~5 MB `DropPortals.exe`, no Python or .NET needed. Starts instantly.
- English and Ukrainian interface.

Written in Rust with [egui](https://github.com/emilk/egui).

## Download

Get `DropPortals.exe` from the [latest release](https://github.com/d-degtyar/Drop-Portals/releases/latest)
and run it. No installation needed.

The file is not code-signed yet, so Windows SmartScreen may warn on the first start:
click **More info → Run anyway**.

## How to use

| Action | How |
|---|---|
| New portal | Right-click the tray icon → "New portal" → empty, Sorter or a preset (Documents, Video, Images, Audio, Design, Motion & 3D, Fonts, Archives, Apps) |
| Move files | Drag files or folders onto a portal |
| Copy instead of move | Hold **Ctrl** while dropping (or change the action in the portal settings) |
| Open the folder | Double-click the portal |
| Move / resize | Drag the portal / its bottom-right corner |
| Settings, undo, duplicate, delete | Right-click the portal |
| Show all portals | Click the tray icon |
| Start with Windows, language | Tray menu |

If a portal does not take a file (say, a PDF on "Video"), it turns grey with a red outline
while you drag, and the file stays where it was.

## Rules

A rule is a set of conditions plus a subfolder. The first rule that matches decides where a file goes.
An empty condition means "anything".

| Field | Example |
|---|---|
| Extensions | `pdf docx xlsx` |
| Name mask | `*invoice*`, `IMG_????` |
| Min / Max, MB | `100`: only files larger than 100 MB |
| Subfolder | `Video`, `{EXT}`, `{year}-{month}`, `Footage\{ext}` |

Subfolder templates can use `{ext}`, `{EXT}`, `{name}`, `{year}`, `{month}` and `{day}`
(the file's modification date).

"Files without a rule" decides what happens to a file that matches no rule:
put it straight into the portal folder, or do not accept it.

"If the file exists": add a number (`file (2).mp4`), skip, or replace.

Settings live in `%APPDATA%\DropPortals\config.json`. The tray menu opens that folder.

## Build from source

On Windows, install [Rust](https://rustup.rs) and run:

```bat
cargo build --release
```

The result is `target\release\DropPortals.exe`.

Cross-compiling from Linux:

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

Every push is built and tested on Windows by GitHub Actions; the exe is attached to each run
under *Artifacts*. Pushing a `v*` tag publishes a release with notes from [CHANGELOG.md](CHANGELOG.md).

## Code layout

| File | What it does |
|---|---|
| `src/main.rs` | startup: single instance, hidden main window |
| `src/app.rs` | app state: portals, commands, background transfers, saving |
| `src/portal_view.rs` | the portal window: drawing, file drops, moving and resizing |
| `src/settings_view.rs` | the portal settings panel |
| `src/tray.rs` | tray icon and menu |
| `src/rules.rs` | rules: does a file match, and where does it go |
| `src/mover.rs` | move, copy, undo |
| `src/config.rs` | settings in JSON |
| `src/presets.rs` | ready-made extension sets |
| `src/i18n.rs` | interface languages |
| `src/platform.rs` | Windows bits: Explorer, folder dialog, Ctrl key, autostart, keeping portals on top |
| `src/fonts.rs`, `src/icon.rs` | fonts and the icon (drawn in code and embedded into the exe by `build.rs`) |

UI strings are written as pairs right where they are used: `tr("Відкрити папку", "Open folder")`.

The same checks as in CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Limitations

- Attachments dragged straight from Outlook or a browser are not files on disk,
  so a portal cannot take them. Save them to a folder first.
- Only the last drop on each portal can be undone, and only while the app is running.

## License

[MIT](LICENSE)
