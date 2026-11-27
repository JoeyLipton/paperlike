# Changelog

## Unreleased

- Releases include ARM64 (aarch64) Linux binaries. They were also added to
  the 0.3.0 release.

## 0.3.0 (2026-11-27)

- The texture hides on monitors showing a fullscreen window, so videos and
  games play without it. Other monitors keep theirs. It can be turned off
  in Settings › General.
- `paperlike hide -- <command>` hides the texture while a command runs,
  for screenshots: `bindsym Print exec paperlike hide -- maim ~/shot.png`.
- The grain is matched across monitors of different pixel densities.
  `grain_scale` now applies to the main monitor, and the others are scaled
  to look the same size. Single-monitor setups are unchanged. It can be
  turned off in Settings › Display.
- Textures render on a background thread. The UI and commands no longer
  freeze while a large texture renders, and the old texture stays up until
  the new one is ready.

## 0.2.0 (2026-11-26)

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
- The tray icon comes back after i3bar rebuilds its bars, which happens
  when monitors are added, removed or rearranged. It used to disappear
  until paperlike was restarted.

## 0.1.0 (2026-11-26)

First release.

- Ten textures with separate light and dark versions, rendered from SVG-style
  filter primitives and tiled by the X server.
- Light/dark switching by GTK theme, screen content or sunrise/sunset.
- Desk Lamp with six presets.
- Snooze, schedule, battery saver and per-app exceptions.
- Tray icon, control center, settings window, global hotkeys and a CLI.
