# Changelog

## 0.3.0 - 2026-10-30

First public release.

- English interface. The language follows Windows and can be changed in the tray menu
  (Same as Windows / Українська / English).
- Removes the thin system frame that Windows 11 draws around portals (visible on light backgrounds).
- The folder picker no longer flickers with the settings panel, fighting over which one is on top.

Download `DropPortals.exe` below and run it. It is not code-signed yet, so Windows SmartScreen
may warn on the first start: click **More info → Run anyway**.

## 0.2.1 - 2026-10-27

- The settings panel stays on the monitor: it opens to the left of a portal near the right edge
  and shrinks on small screens.
- The delete confirmation fits on one line.
- Portals stay on top even when Windows pushes always-on-top windows down; they do not jump over
  fullscreen video or games.

## 0.2.0 - 2026-10-27

- Rewritten in Rust with egui: a single ~5 MB exe instead of a Python app.
