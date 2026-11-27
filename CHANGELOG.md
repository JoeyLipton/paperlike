# Changelog

## Unreleased

- The config file is now `~/.config/paperlike/paperlike.toml`. An existing
  `config.toml` is renamed on first start.
- Edits to the config file apply as soon as it's saved. A file that doesn't
  parse is ignored and the error is printed.
- Monitors that lie inside another one (mirrored outputs, or virtual
  monitors from `xrandr --setmonitor`) no longer get a second overlay,
  which doubled the texture.
- Text in windows behind the control center and settings no longer shows
  faintly through them.
- Memory used while rendering a texture is given back afterwards; Frost
  used to keep about 40 MB. Rendering the screen-sized textures also peaks
  at roughly half the memory it did.

## 0.1.0 (2026-11-26)

First release.

- Ten textures with separate light and dark versions, rendered from SVG-style
  filter primitives and tiled by the X server.
- Light/dark switching by GTK theme, screen content or sunrise/sunset.
- Desk Lamp with six presets.
- Snooze, schedule, battery saver and per-app exceptions.
- Tray icon, control center, settings window, global hotkeys and a CLI.
