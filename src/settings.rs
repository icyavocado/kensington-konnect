//! The settings the panel edits: what each button of the trackball does (keyed by the
//! button code the kernel reports), how the pointer and the scroll ring behave, and where
//! it all is saved — a plain, readable JSON file in the person's config folder.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Bumped whenever the meaning of this file changes; the panel loads nothing older.
pub const SETTINGS_VERSION: u32 = 2;

/// What one of the trackball's buttons does, drawn from the actions Konnect-style
/// control panels offer: clicks, scrolling, keystrokes, snippets, macros, media
/// and desktop actions — and doing nothing.
///
/// Saved as readable JSON, for example `{"kind": "key", "value": "ctrl+shift+t"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ButtonAction {
    /// The normal primary click.
    LeftClick,
    /// The middle button (paste, open in a new tab…).
    MiddleClick,
    /// The secondary click.
    RightClick,
    /// Two quick primary clicks.
    DoubleClick,
    /// Click once to hold the button down; the next click lets go (for dragging).
    ClickLock,
    /// Go back (browsers and most apps).
    Back,
    /// Go forward.
    Forward,
    /// Scroll up, and keep scrolling while the button is held.
    ScrollUp,
    /// Scroll down, and keep scrolling while the button is held.
    ScrollDown,
    /// Scroll left (some apps: sideways page move).
    ScrollLeft,
    /// Scroll right.
    ScrollRight,
    /// Send a keystroke, written like `ctrl+shift+t`.
    Key(String),
    /// Type a short piece of text, written here once and inserted anywhere.
    Snippet(String),
    /// A simple macro: steps separated by `|`, each a key like `ctrl+z`,
    /// a pause like `200ms`, or typed text after `type:`.
    Macro(String),
    /// Louder.
    VolumeUp,
    /// Quieter.
    VolumeDown,
    /// Mute the sound.
    VolumeMute,
    /// Play or pause.
    PlayPause,
    /// The next track.
    NextTrack,
    /// The previous track.
    PrevTrack,
    /// Close the focused window (via the desktop, Hyprland first).
    CloseWindow,
    /// Toggle full screen on the focused window.
    FullscreenToggle,
    /// Move to the next window.
    SwitchWindow,
    /// Lock the screen.
    LockScreen,
    /// Put the computer to sleep.
    Sleep,
    /// Open the calculator program.
    Calculator,
    /// Let the button do what the trackball firmware already does.
    Nothing,
}

impl Default for ButtonAction {
    fn default() -> Self {
        Self::LeftClick
    }
}

impl ButtonAction {
    /// The choices the panel offers, grouped the way the Buttons tab shows them.
    pub const ALL: [Self; 24] = [
        // Clicks.
        Self::LeftClick,
        Self::MiddleClick,
        Self::RightClick,
        Self::DoubleClick,
        Self::ClickLock,
        Self::Back,
        Self::Forward,
        // Scrolling.
        Self::ScrollUp,
        Self::ScrollDown,
        Self::ScrollLeft,
        Self::ScrollRight,
        // Keys and text.
        Self::Key(String::new()),
        Self::Snippet(String::new()),
        Self::Macro(String::new()),
        // Media.
        Self::VolumeUp,
        Self::VolumeDown,
        Self::VolumeMute,
        Self::PlayPause,
        Self::NextTrack,
        Self::PrevTrack,
        // Desktop.
        Self::CloseWindow,
        Self::FullscreenToggle,
        Self::SwitchWindow,
        Self::Nothing,
    ];

    /// The name shown for this choice.
    pub fn label(&self) -> String {
        match self {
            Self::LeftClick => "Left click".into(),
            Self::MiddleClick => "Middle click".into(),
            Self::RightClick => "Right click".into(),
            Self::DoubleClick => "Double click".into(),
            Self::ClickLock => "Click lock (hold to drag)".into(),
            Self::Back => "Back".into(),
            Self::Forward => "Forward".into(),
            Self::ScrollUp => "Scroll up".into(),
            Self::ScrollDown => "Scroll down".into(),
            Self::ScrollLeft => "Scroll left".into(),
            Self::ScrollRight => "Scroll right".into(),
            Self::Key(key) => format!("Keystroke {key}"),
            Self::Snippet(text) => format!("Snippet: {}", short_text(text, 24)),
            Self::Macro(steps) => format!("Macro: {}", short_text(steps, 24)),
            Self::VolumeUp => "Volume up".into(),
            Self::VolumeDown => "Volume down".into(),
            Self::VolumeMute => "Volume mute".into(),
            Self::PlayPause => "Play / pause".into(),
            Self::NextTrack => "Next track".into(),
            Self::PrevTrack => "Previous track".into(),
            Self::CloseWindow => "Close window".into(),
            Self::FullscreenToggle => "Toggle full screen".into(),
            Self::SwitchWindow => "Switch window".into(),
            Self::LockScreen => "Lock screen".into(),
            Self::Sleep => "Sleep".into(),
            Self::Calculator => "Calculator".into(),
            Self::Nothing => "Do nothing (block it)".into(),
        }
    }

    /// Whether this action carries a piece of text the person edits in the panel.
    pub fn wants_text(&self) -> Option<&'static str> {
        match self {
            Self::Key(_) => Some("e.g. ctrl+shift+t"),
            Self::Snippet(_) => Some("the text to type, e.g. Best regards"),
            Self::Macro(_) => Some("steps like ctrl+z | 200ms | type:done"),
            _ => None,
        }
    }

    /// The text the person typed for this action, if it carries one.
    pub fn text(&self) -> &str {
        match self {
            Self::Key(t) | Self::Snippet(t) | Self::Macro(t) => t,
            _ => "",
        }
    }

    /// Replace the text of this action (same kind, new text).
    pub fn with_text(&self, text: String) -> Self {
        match self {
            Self::Key(_) => Self::Key(text),
            Self::Snippet(_) => Self::Snippet(text),
            Self::Macro(_) => Self::Macro(text),
            other => other.clone(),
        }
    }
}

fn short_text(text: &str, limit: usize) -> String {
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        out.push('…');
    }
    out
}

/// One button's remap: the action and nothing else.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ButtonEntry {
    /// What the button does once the helper applies it.
    pub action: ButtonAction,
}

/// How the pointer moves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pointer {
    /// With a trackball a flat profile (no acceleration) feels predictable.
    pub flat_profile: bool,
    /// Pointer speed, from -1 (slowest) to 1 (fastest).
    pub accel_speed: f32,
}

impl Default for Pointer {
    fn default() -> Self {
        Self { flat_profile: true, accel_speed: 0.3 }
    }
}

/// How the scroll ring behaves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scrolling {
    /// Scroll the other way round ("natural scrolling").
    pub invert: bool,
    /// How fast the ring scrolls, 1.0 is normal.
    pub speed: f32,
    /// Turn scrolling sideways into vertical scrolling (some models report a
    /// side roller). When on, sideways wheel turns count as vertical.
    pub merge_horizontal: bool,
}

impl Default for Scrolling {
    fn default() -> Self {
        Self { invert: false, speed: 1.0, merge_horizontal: false }
    }
}

/// Everything the panel edits, saved between restarts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The layout version; an older file is not read.
    pub version: u32,
    /// What each button does, keyed by the button code the kernel reports (272 = BTN_LEFT…).
    pub buttons: BTreeMap<u16, ButtonEntry>,
    /// Pointer feel.
    pub pointer: Pointer,
    /// Scroll ring feel.
    pub scrolling: Scrolling,
}

impl Default for Settings {
    fn default() -> Self {
        // The way the Expert trackball behaves out of the box: clicks on the front
        // four codes, back/forward on the side codes, everything else untouched.
        let mut buttons = BTreeMap::new();
        for (code, action) in [
            (0x110u16, ButtonAction::LeftClick),   // BTN_LEFT
            (0x111, ButtonAction::RightClick),     // BTN_RIGHT
            (0x112, ButtonAction::MiddleClick),    // BTN_MIDDLE
            (0x113, ButtonAction::Back),           // BTN_SIDE
            (0x114, ButtonAction::Forward),        // BTN_EXTRA
            (0x116, ButtonAction::Back),           // BTN_BACK (historical models)
            (0x115, ButtonAction::Forward),        // BTN_FORWARD (historical models)
        ] {
            buttons.insert(code, ButtonEntry { action });
        }
        Self { version: SETTINGS_VERSION, buttons, pointer: Pointer::default(), scrolling: Scrolling::default() }
    }
}

/// Where the settings live, from the `$HOME` of the person using the panel.
fn settings_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return std::path::PathBuf::from("kensington-trackball-settings.json");
    }
    std::path::PathBuf::from(home)
        .join(".config")
        .join("kensington-trackball")
        .join("settings.json")
}

impl Settings {
    /// Read the saved settings; a missing file is the defaults, a broken one is reported.
    pub fn load() -> (Self, Option<String>) {
        let path = settings_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => return (Self::default(), None),
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(settings) if settings.version == SETTINGS_VERSION => (settings, None),
            Ok(settings) => (
                Self::default(),
                Some(format!(
                    "The saved settings were written by another version (layout {}, this one wants {}). \
                     They were kept in the file; the panel shows its own defaults until you save again.",
                    settings.version, SETTINGS_VERSION
                )),
            ),
            Err(e) => (
                Self::default(),
                Some(format!("The saved settings could not be read ({e}), using the defaults until you save again.")),
            ),
        }
    }

    /// Save the settings so they survive a reboot.
    pub fn save(&self) -> Result<std::path::PathBuf, String> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("Could not write the settings: {e}"))?;
        std::fs::write(&path, json + "\n").map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        Ok(path)
    }

    /// Where the settings live, shown in the panel.
    pub fn path_display() -> String {
        settings_path().display().to_string()
    }

    /// The path, for the helper and the backup buttons.
    pub fn path() -> std::path::PathBuf {
        settings_path()
    }

    /// What one button (by code) does; buttons without a row are left alone.
    pub fn action_for(&self, code: u16) -> Option<&ButtonAction> {
        self.buttons.get(&code).map(|entry| &entry.action)
    }
}
