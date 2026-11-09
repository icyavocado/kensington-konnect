//! Making the settings take effect on this computer, in honest steps:
//!
//! - **Permission**: without it Linux refuses to read the trackball. A udev rule tags every
//!   Kensington input device with `uaccess` and lets the write side of uinput (the helper's
//!   virtual device) belong to the logged-in session too. Installing it needs one password
//!   prompt (`pkexec`).
//! - **Pointer feel**: the desktop applies per-device pointer settings. Hyprland (this
//!   computer's Wayland compositor) takes them in a `device {}` block inside `input {}`.
//!   The panel writes a small `trackball.conf` and shows the one `source` line needed to
//!   load it. It never edits the person's own Hyprland files.
//! - **The helper**: a small program (this very binary with `--helper`) that takes over the
//!   trackball, re-sends its movements untouched, and makes each button do what the panel
//!   chose. The panel starts and stops it.

use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The person to grant the trackball to, from the session's environment ("" when unknown;
/// the rule then leans on the `input` group alone).
pub fn current_user() -> String {
    std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_default()
}

/// The udev rule that gives this person permission to hear the trackball and to create
/// the helper's virtual device.
///
/// Two grants, on purpose. The usual Linux way (`uaccess`) tags the device for the
/// logged-in session — but on some computers it never reaches devices that are plugged
/// in through a USB hub, and then it silently grants nothing. The second line is a
/// direct grant (mode, group and owner) that applies no matter where the dongle sits.
pub fn udev_rule_text(user: &str) -> String {
    let owner = if user.is_empty() { String::new() } else { format!(", OWNER=\"{user}\"") };
    let lines: Vec<String> = vec![
        "# Kensington trackball — permission for this panel and its helper.".to_string(),
        "# Installed by the Kensington Expert Trackball panel.".to_string(),
        "#".to_string(),
        "# 1) The usual way: tag every Kensington input device for the logged-in session.".to_string(),
        "#    (On some computers this does not reach devices behind a USB hub — hence line 2.)".to_string(),
        "SUBSYSTEM==\"input\", SUBSYSTEMS==\"usb\", ATTRS{idVendor}==\"047d\", TAG+=\"uaccess\"".to_string(),
        "#".to_string(),
        "# 2) A direct grant that does not depend on seats: readable by the named user and".to_string(),
        "#    by the `input` group, wherever the dongle is plugged in.".to_string(),
        format!("SUBSYSTEM==\"input\", SUBSYSTEMS==\"usb\", ATTRS{{idVendor}}==\"047d\", MODE=\"0660\", GROUP=\"input\"{owner}"),
        "#".to_string(),
        "# 3) The helper's virtual device, so button changes can be sent on to the desktop.".to_string(),
        format!("KERNEL==\"uinput\", MODE=\"0660\", GROUP=\"input\"{owner}"),
    ];
    lines.join("\n") + "\n"
}

/// The folder where the panel keeps its small working files (pid, status, stop flag).
pub fn state_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return PathBuf::from("/tmp/kensington-trackball");
    }
    PathBuf::from(home).join(".cache").join("kensington-trackball")
}

/// The Hyprland device settings the panel writes: pointer feel for the trackball.
pub fn hyprland_conf(settings: &Settings, device_name: Option<&str>) -> String {
    let libinput_name = device_name.unwrap_or("Kensington Expert Trackball");
    let hypr_name = hypr_device_name(libinput_name);
    let profile = if settings.pointer.flat_profile { "flat" } else { "adaptive" };
    format!(
        "# The Kensington trackball, as set in its panel.\n\
         # Written by the panel into ~/.config/hypr/trackball.conf.\n\
         # Load it ONCE in your hyprland.conf with:\n\
         #   source = ~/.config/hypr/trackball.conf\n\
         # The scroll ring speed and direction live in the panel's helper, not here.\n\
         input {{\n\
         \x20   device {{\n\
         \x20       # The trackball's name, as Hyprland spells it: \"{libinput_name}\"\n\
         \x20       name = {hypr_name}\n\
         \x20       accel_profile = {profile}\n\
         \x20       accel_speed = {speed:.3}\n\
         \x20   }}\n\
         }}\n",
        libinput_name = libinput_name,
        hypr_name = hypr_name,
        profile = profile,
        speed = settings.pointer.accel_speed,
    )
}

/// Hyprland spells a device name lowercased with dashes instead of spaces.
pub fn hypr_device_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            ' ' => out.push('-'),
            c if c.is_alphanumeric() => out.push(c.to_ascii_lowercase()),
            _ => {}
        }
    }
    if out.is_empty() {
        out = "kensington-expert-trackball".into();
    }
    out
}

/// The Hyprland config file path: `~/.config/hypr/trackball.conf`.
fn trackball_conf_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return Err("No home folder is known, so the Hyprland config folder can't be found.".into());
    }
    Ok(PathBuf::from(home).join(".config").join("hypr").join("trackball.conf"))
}

/// Write the Hyprland config. Only ever creates or replaces its own `trackball.conf`.
pub fn write_trackball_conf(settings: &Settings, device_name: Option<&str>) -> Result<PathBuf, String> {
    let path = trackball_conf_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, hyprland_conf(settings, device_name))
        .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(path)
}

/// Where the Hyprland config file goes, for the panel to show.
pub fn trackball_conf_path_display() -> String {
    trackball_conf_path().map(|p| p.display().to_string()).unwrap_or_else(|_| "~/.config/hypr/trackball.conf".into())
}

/// Whether the computer has `pkexec` (the way the rule is installed with one password prompt).
pub fn has_pkexec() -> bool {
    std::fs::metadata("/usr/bin/pkexec").is_ok()
}

/// The state of the permission job, shared with the worker thread.
#[derive(Debug, Default)]
pub struct AccessJob {
    /// A worker thread is running the installer.
    pub running: bool,
    /// What it did or found, shown in the panel when done.
    pub note: String,
    /// Whether the answer needs attention (it didn't work).
    pub failed: bool,
}

/// Install the rule: save it in the panel's config folder, then copy it to
/// `/etc/udev/rules.d`, make sure the person is in the `input` group, and reload udev
/// under `pkexec` (one password prompt). The `udevadm trigger` step re-applies the rule
/// to devices that are already plugged in — so no replug is needed after it.
/// Run from a worker thread — it may wait for the password prompt.
pub fn install_udev_rule() -> String {
    let user = current_user();
    let rule = udev_rule_text(&user);
    let staging = std::env::var("HOME")
        .ok()
        .filter(|home| !home.is_empty())
        .map(|home| std::path::PathBuf::from(home).join(".config").join("kensington-trackball").join("kensington.rules"))
        .unwrap_or_else(|| PathBuf::from("/tmp/kensington.rules"));
    if let Some(parent) = staging.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&staging, &rule) {
        return format!("Could not prepare the rule file: {e}");
    }

    // The group grant is belt-and-braces beside the owner grant; if `usermod` is missing
    // or refuses, the rest still applies (already-in-group is exit 0 anyway).
    let usermod = if user.is_empty() {
        String::new()
    } else {
        format!(" && (command -v usermod >/dev/null 2>&1 && usermod -aG input {user} || true)")
    };
    let script = format!(
        "cp {src} /etc/udev/rules.d/89-kensington-trackball.rules \
         && udevadm control --reload-rules \
         && udevadm trigger --subsystem-match=input \
         && udevadm trigger --name-match=uinput{usermod}",
        src = staging.display(),
        usermod = usermod,
    );
    let run = std::process::Command::new("pkexec").args(["sh", "-c", &script]).output();
    match run {
        Ok(out) if out.status.success() => {
            "Done: the rule now grants the trackball to you directly — the earlier 'session' \
              permission never reached it where the dongle is plugged in. Nothing needs \
              replugging: this panel and the helper check again by themselves within seconds. \
              If it still says permission is missing, log out and back in once (that fresh \
              login is what the group grant needs)."
                .into()
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            format!("The password prompt was cancelled or the install refused: {stderr}").trim().to_string()
        }
        Err(e) => format!("Could not run pkexec (is polkit installed?): {e}"),
    }
}

/// What the helper tells the panel about itself, written to a small json file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HelperStatus {
    /// Whether the helper is running.
    pub running: bool,
    /// A short word for the state: `starting`, `waiting`, `no-permission`,
    /// `grab-failed`, `uinput-failed` or `holding` (empty for an older helper).
    pub state: String,
    /// The device the helper took over (or the node it found but can't reach).
    pub device: String,
    /// One honest sentence about where the helper stands, ready to show.
    pub detail: String,
    /// The last button the helper heard, e.g. `BTN_SIDE`, for press-to-identify.
    pub last_button: String,
    /// The button code of the last button, as a number.
    pub last_button_code: u16,
}

/// Start the helper: this program, run again with `--helper`, in the background.
/// Returns a short note for the panel.
pub fn start_helper() -> String {
    if helper_running() {
        return "The helper is already running.".into();
    }
    let _ = std::fs::create_dir_all(state_dir());
    let _ = std::fs::remove_file(state_dir().join("stop"));
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => return format!("Could not find the panel's own program: {e}"),
    };
    match std::process::Command::new(exe).arg("--helper").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        Ok(_child) => {
            std::thread::sleep(std::time::Duration::from_millis(600));
            if helper_running() {
                "The helper is running — your buttons and ring settings are live.".into()
            } else {
                "The helper did not come up. Check the Setup tab: it may not have permission (see Access)".into()
            }
        }
        Err(e) => format!("Could not start the helper: {e}"),
    }
}

/// Ask the helper to stop: it notices a stop file, tidies up and exits.
pub fn stop_helper() -> String {
    if !helper_running() {
        return "The helper is not running.".into();
    }
    let stop = state_dir().join("stop");
    if let Err(e) = std::fs::write(&stop, b"stop") {
        return format!("Could not write the stop note: {e}");
    }
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !helper_pid_alive() {
            let _ = std::fs::remove_file(&stop);
            return "The helper stopped; the trackball is back to its built-in behaviour.".into();
        }
    }
    "The helper didn't stop within two seconds — press Start/Stop again or close the panel and reopen it.".into()
}

/// Whether the pid file points at a live helper right now.
pub fn helper_running() -> bool {
    helper_pid_alive()
}

/// The running helper's status, if it wrote one.
pub fn helper_status() -> Option<HelperStatus> {
    let text = std::fs::read_to_string(state_dir().join("status.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The pid file, if the helper is alive.
fn helper_pid_alive() -> bool {
    let pid_file = state_dir().join("pid");
    let text = match std::fs::read_to_string(&pid_file) {
        Ok(text) => text,
        Err(_) => return false,
    };
    let pid: u32 = match text.trim().parse() {
        Ok(pid) => pid,
        Err(_) => return false,
    };
    let cmdline = std::fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    if cmdline.contains("--helper") {
        true
    } else {
        // Stale file from a crashed helper.
        let _ = std::fs::remove_file(&pid_file);
        false
    }
}

/// Write an autostart desktop entry so the helper comes up at every login.
pub fn write_autostart() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return Err("No home folder is known, so the autostart folder can't be found.".into());
    }
    let autostart = PathBuf::from(&home).join(".config").join("autostart");
    std::fs::create_dir_all(&autostart).map_err(|e| format!("Could not create {}: {e}", autostart.display()))?;
    let exe = std::env::current_exe().map_err(|e| format!("Could not find the panel's own program: {e}"))?;
    let path = autostart.join("kensington-trackball-helper.desktop");
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Kensington Trackball helper\n\
         Description=Applies the trackball buttons and scroll ring settings\n\
         Exec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    std::fs::write(&path, entry).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(path)
}

/// Remove the autostart entry again.
pub fn remove_autostart() -> Result<bool, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return Ok(false);
    }
    let path = PathBuf::from(&home).join(".config").join("autostart").join("kensington-trackball-helper.desktop");
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("Could not remove {}: {e}", path.display())),
    }
}

/// Whether the autostart entry exists.
pub fn autostart_installed() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return false;
    }
    std::fs::metadata(PathBuf::from(&home).join(".config").join("autostart").join("kensington-trackball-helper.desktop")).is_ok()
}

/// Save a timestamped copy of the settings under the panel's backups folder.
pub fn save_backup() -> Result<PathBuf, String> {
    let settings_path = Settings::path();
    let text = std::fs::read_to_string(&settings_path).map_err(|e| format!("No settings file to copy yet: {e}"))?;
    let dir = backups_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let path = dir.join(format!("settings-{stamp}.json"));
    std::fs::write(&path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(path)
}

/// Copy the newest backup back over the settings file.
pub fn restore_backup() -> Result<PathBuf, String> {
    let dir = backups_dir();
    let mut best: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if entry.file_name().to_string_lossy().starts_with("settings-") && best.as_ref().map(|(_, t)| modified > *t).unwrap_or(true) {
                best = Some((entry.path(), modified));
            }
        }
    }
    let (path, _) = best.ok_or("There are no backups yet — save one first.")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    std::fs::write(Settings::path(), text).map_err(|e| format!("Could not write {}: {e}", Settings::path().display()))?;
    Ok(path)
}

/// Where the backups live: `~/.config/kensington-trackball/backups`.
fn backups_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return PathBuf::from(".kensington-backups");
    }
    PathBuf::from(&home).join(".config").join("kensington-trackball").join("backups")
}
