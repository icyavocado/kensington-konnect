# Kensington Expert Trackball — control panel

A small desktop app for Linux that puts a Kensington Expert Trackball under your control:
what its buttons do, how the pointer feels, and how the scroll ring behaves — the same core
job the Kensington Konnect software does on Windows, rebuilt for Linux.

- Built on **Rust + eframe (egui)**, native Wayland first (a Hyprland desktop),
  X11 as fallback.
- Your settings live in `~/.config/kensington-trackball/settings.json` — plain JSON you can
  read and back up. The panel saves the moment something changes.

## The tabs

- **Buttons** — every button the kernel reports (listed with its code, so nothing is
  guessed). Each can be: as the trackball sends it, a click (left/middle/right, double,
  click-lock for dragging), back/forward, a scroll direction (repeats while held), a
  keystroke like `ctrl+shift+t`, a snippet of text, a simple macro (`ctrl+z | 200ms |
  type:done`), media keys, or desktop/power actions. Windows-only extras (per-app profiles,
  cloud backup, firmware updates) are left out honestly — see the Setup tab's note.
- **Pointer** — pointer speed, flat-vs-adaptive acceleration. Written to Hyprland as a
  per-device `device {}` block with the one `source` line to load it.
- **Scrolling** — ring speed (×0.25–×3), invert direction, and folding a sideways wheel
  into the ring. Applied **live** by the helper.
- **Device** — what the kernel sees: name, USB id, the buttons and axes it reports, and
  whether the panel may read it.
- **Setup** — the one-time permission (a single udev rule via `pkexec`, with a direct
  grant beside the usual seat tag, because the seat tag alone can miss devices that sit
  behind a USB hub), run-helper-at-login (an autostart entry), and settings backups saved
  next to the settings file. The Setup tab also shows a three-step connection check
  (dongle on USB → button device exists → your user may use it) so a "doesn't work"
  always comes with the reason and the exact next click.

## The helper

The helper is this program itself, run with `--helper`. It takes exclusive hold of the
trackball, re-sends its movements untouched, and makes the buttons do what the Buttons tab
says — through a virtual device the desktop treats as normal input. It watches the settings
file and applies changes within a second. The panel starts and stops it.

## Build and install

```sh
git clone https://github.com/icyavocado/kensington-konnect
cd kensington-konnect
cargo build --release
./target/release/kensington-trackball
```

Working straight from a source folder: `cargo run` from this folder does the same
(Rust itself comes from https://rustup.rs if it's missing).

Other modes:

- `--helper` — the daemon described above (the panel uses this for you).
- `--screenshot <file.png>` — draw about 120 frames, save the picture, exit. What the
  automated build checks use.
- `--version`.

## Requirements

- Linux with a Wayland session. The window also opens under X11; the Pointer tab is
  written for Hyprland, the desktop this was built on.
- A kernel with `uinput` support — the helper publishes its virtual device through it.
- Permission to read the trackball's device node: the Setup tab installs one udev rule
  for this (a single password prompt, done once).

## License

MIT — see [LICENSE](LICENSE). This is not official Kensington software; it simply
operates the trackball you already own.
