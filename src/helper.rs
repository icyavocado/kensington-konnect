//! The helper: the part that makes the panel's choices real.
//!
//! It opens the trackball, takes exclusive hold of it (`grab`), then forwards every
//! movement untouched and plays each button's action through a virtual device
//! (uinput). From the apps' point of view the trackball still works, but your
//! buttons now do what the panel says. It reads the same settings file the panel
//! writes, picks up changes within a second, and stands down when the panel asks.

use crate::keys::{char_code, key_name, media, parse_combo};
use crate::settings::{ButtonAction, Settings};
use evdev::{
    AttributeSet, Device, InputEvent, KeyCode, RelativeAxisCode,
    uinput::VirtualDevice,
};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// Between two ticks of the helper's own bookkeeping.
const TICK: Duration = Duration::from_millis(20);
/// How often a held scroll button repeats its tick.
const SCROLL_REPEAT: Duration = Duration::from_millis(80);
/// How often the helper re-reads the settings file.
const SETTINGS_RECHECK: Duration = Duration::from_millis(400);
/// Gap between the two clicks of a double click.
const DOUBLE_GAP: Duration = Duration::from_millis(25);
/// Pause between typed characters.
const TYPE_STEP: Duration = Duration::from_millis(6);

/// The virtual device plus what it can do, kept together.
struct Out {
    device: VirtualDevice,
    has_wheel_hi_res: bool,
}

/// The helper's working state for one connected device.
struct Helper {
    settings: Settings,
    settings_mtime: Option<SystemTime>,
    /// What the helper tells the panel about the device it took over.
    status_device: String,
    /// Click-lock: a press waiting for its release, as (physical code, virtual code).
    pending_lock: Option<(u16, u16)>,
    /// Virtual keys held because of a physical button, so they can be let go cleanly.
    held_virtual: HashMap<u16, Vec<u16>>,
    /// Buttons held that repeat a scroll action: physical code -> (0 = ring up/down, 1 = sideways, direction).
    repeat_scroll: HashMap<u16, (u8, bool)>,
    /// Fractions of wheel steps accumulated, so a slow ring can still move things.
    wheel_carry: f32,
    wheel_h_carry: f32,
    wheel_hi_carry: f32,
    wheel_hi_h_carry: f32,
    last_repeat: Instant,
    last_settings_check: Instant,
    last_status: Instant,
}

fn key_event(code: u16, value: i32) -> InputEvent {
    InputEvent::new(1, code, value) // type 1 = EV_KEY
}

fn rel_event(code: u16, value: i32) -> InputEvent {
    InputEvent::new(2, code, value) // type 2 = EV_REL
}

/// Key codes the virtual device can send: every button, every key the panel's words
/// and typing tables reach, plus the media keys.
fn virtual_keys() -> AttributeSet<KeyCode> {
    let mut set = AttributeSet::new();
    for (code, _, _) in crate::device::KNOWN_BUTTONS {
        set.insert(KeyCode::new(code));
    }
    for (_, code) in crate::keys::WORDS {
        set.insert(KeyCode::new(*code));
    }
    // Everything snippets can reach through the char table.
    for c in 'a'..='z' {
        if let Some((code, _)) = char_code(c) {
            set.insert(KeyCode::new(code));
        }
    }
    for c in '0'..='9' {
        if let Some((code, _)) = char_code(c) {
            set.insert(KeyCode::new(code));
        }
    }
    for code in [media::VOLUME_UP, media::VOLUME_DOWN, media::VOLUME_MUTE, media::PLAY_PAUSE, media::NEXT_TRACK, media::PREV_TRACK, media::SCREENLOCK, media::SLEEP, media::CALCULATOR] {
        set.insert(KeyCode::new(code));
    }
    set
}

/// The axes the virtual device re-sends: like the real device, so apps see the same.
/// Returns the axes, whether high-res wheel rows exist, and whether a sideways wheel exists.
fn virtual_axes(real: &Device) -> (AttributeSet<RelativeAxisCode>, bool, bool) {
    let mut set = AttributeSet::new();
    let has = |code: RelativeAxisCode| real.supported_relative_axes().map(|r| r.contains(code)).unwrap_or(false);
    let mut hi_res = false;
    for (code, flag) in [
        (RelativeAxisCode::REL_X, has(RelativeAxisCode::REL_X)),
        (RelativeAxisCode::REL_Y, has(RelativeAxisCode::REL_Y)),
        (RelativeAxisCode::REL_WHEEL, has(RelativeAxisCode::REL_WHEEL)),
        (RelativeAxisCode::REL_HWHEEL, has(RelativeAxisCode::REL_HWHEEL)),
        (RelativeAxisCode::REL_WHEEL_HI_RES, has(RelativeAxisCode::REL_WHEEL_HI_RES)),
        (RelativeAxisCode::REL_HWHEEL_HI_RES, has(RelativeAxisCode::REL_HWHEEL_HI_RES)),
    ] {
        if flag {
            set.insert(code);
            if code == RelativeAxisCode::REL_WHEEL_HI_RES {
                hi_res = true;
            }
        }
    }
    (set, hi_res, has(RelativeAxisCode::REL_HWHEEL))
}

/// Build one virtual device that acts as the trackball and the keyboard in one.
fn build_virtual(real: &Device, name: &str) -> Result<Out, String> {
    let keys = virtual_keys();
    let builder = VirtualDevice::builder().map_err(|e| format!("Could not open uinput: {e} (see Setup)"))?;
    let builder = builder.name(name).with_keys(&keys).map_err(|e| format!("uinput refused the buttons: {e}"))?;
    let (axes, hi_res, _hwheel) = virtual_axes(real);
    let builder = builder.with_relative_axes(&axes).map_err(|e| format!("uinput refused the axes: {e}"))?;
    let device = builder.build().map_err(|e| format!("Could not create the virtual device: {e}"))?;
    Ok(Out { device, has_wheel_hi_res: hi_res })
}

/// Kensington candidates discovered from sysfs alone — this works without any
/// permission, so the helper can report "found but can't open" instead of a bare
/// "waiting". Returns (event node, kernel name, whether it moves the pointer).
fn kensington_nodes() -> Vec<(String, String, bool)> {
    crate::device::scan()
        .into_iter()
        .filter(|device| device.is_kensington())
        .map(|device| (device.dev_node, device.name, crate::device::sysfs_pointer(&device.sys_name)))
        .collect()
}

/// What one search round turned up.
enum Look {
    Opened(Device),
    /// The node is there but won't open — this is exactly what the panel must hear.
    Blocked { node: String, name: String, err: String },
    Nothing,
}

/// Look for the trackball, preferring the node that moves the pointer, and telling the
/// caller exactly what stands in the way when none of them can be opened.
fn look_for_trackball() -> Look {
    let nodes = kensington_nodes();
    let mut ranked: Vec<&(String, String, bool)> = nodes.iter().filter(|(_, _, pointer)| *pointer).collect();
    if ranked.is_empty() {
        ranked = nodes.iter().collect();
    }
    // Nodes that only carry media keys ("Consumer Control") come last.
    ranked.sort_by_key(|(_, name, _)| name.to_ascii_lowercase().contains("consumer"));
    let mut first_blocked: Option<Look> = None;
    for (node, name, _) in ranked {
        match Device::open(std::path::Path::new(node)) {
            Ok(device) => return Look::Opened(device),
            Err(e) => {
                if first_blocked.is_none() {
                    first_blocked = Some(Look::Blocked { node: node.clone(), name: name.clone(), err: e.to_string() });
                }
            }
        }
    }
    first_blocked.unwrap_or(Look::Nothing)
}
/// Scale a wheel flow, returning (whole steps, left-over fraction).
fn scaled_i32(carry: f32, add: f32) -> (i32, f32) {
    let total = carry + add;
    let steps = total.trunc();
    (steps as i32, total - steps)
}

/// Send a batch out; a broken virtual device is reported, not fatal.
fn emit(out: &mut Out, events: &[InputEvent]) {
    if let Err(e) = out.device.emit(events) {
        eprintln!("trackball helper: could not send: {e}");
    }
}

/// Type text through the virtual keyboard, ASCII only; other characters are skipped.
fn type_text(out: &mut Out, text: &str) {
    for c in text.chars() {
        let Some((code, needs_shift)) = char_code(c) else { continue };
        let mut down = Vec::new();
        if needs_shift {
            down.push(key_event(0x2a, 1)); // KEY_LEFTSHIFT
        }
        down.push(key_event(code, 1));
        emit(out, &down);
        std::thread::sleep(TYPE_STEP);
        let mut up = Vec::new();
        up.push(key_event(code, 0));
        if needs_shift {
            up.push(key_event(0x2a, 0));
        }
        emit(out, &up);
        std::thread::sleep(TYPE_STEP);
    }
}

/// One macro: `ctrl+z | 200ms | type:done | Return`.
fn run_macro(out: &mut Out, steps: &str) {
    for step in steps.split('|') {
        let step = step.trim();
        if step.is_empty() {
            continue;
        }
        if let Some(ms_text) = step.strip_suffix("ms") {
            if let Ok(ms) = ms_text.trim().parse::<u64>() {
                std::thread::sleep(Duration::from_millis(ms.min(5000)));
                continue;
            }
        }
        if let Some(text) = step.strip_prefix("type:") {
            type_text(out, text);
            continue;
        }
        tap_combo(out, step);
    }
}

/// Tap a written combination once (press, tiny hold, release).
fn tap_combo(out: &mut Out, text: &str) {
    let codes = parse_combo(text);
    if codes.is_empty() {
        return;
    }
    let mut down = Vec::new();
    for c in &codes {
        down.push(key_event(*c, 1));
    }
    emit(out, &down);
    std::thread::sleep(Duration::from_millis(15));
    let mut up = Vec::new();
    for c in codes.iter().rev() {
        up.push(key_event(*c, 0));
    }
    emit(out, &up);
}

/// Tap one media key quickly when `down`, and ignore the release edge.
fn tap_key(out: &mut Out, down: bool, code: u16) {
    if down {
        emit(out, &[key_event(code, 1)]);
        std::thread::sleep(Duration::from_millis(15));
        emit(out, &[key_event(code, 0)]);
    }
}

/// Run a Hyprland desktop command; false when there is no Hyprland to ask.
fn desktop_dispatch(words: &[&str]) -> bool {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return false;
    }
    let _ = std::process::Command::new("hyprctl").arg("dispatch").args(words).status();
    // Whether Hyprland accepted it or not, it was asked — nothing else to try here.
    true
}

/// Write the status file the panel reads.
fn write_status(state: &str, device: &str, detail: &str, last_button_code: u16) {
    let dir = crate::system::state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let status = crate::system::HelperStatus {
        running: true,
        state: state.to_string(),
        device: device.to_string(),
        detail: detail.to_string(),
        last_button: if last_button_code == 0 { String::new() } else { key_name(last_button_code) },
        last_button_code,
    };
    if let Ok(json) = serde_json::to_string(&status) {
        let _ = std::fs::write(dir.join("status.json"), json);
    }
}

impl Helper {
    fn new() -> Self {
        let (settings, _) = Settings::load();
        let settings_mtime = std::fs::metadata(Settings::path()).and_then(|m| m.modified()).ok();
        Self {
            settings,
            settings_mtime,
            status_device: String::new(),
            pending_lock: None,
            held_virtual: HashMap::new(),
            repeat_scroll: HashMap::new(),
            wheel_carry: 0.0,
            wheel_h_carry: 0.0,
            wheel_hi_carry: 0.0,
            wheel_hi_h_carry: 0.0,
            last_repeat: Instant::now(),
            last_settings_check: Instant::now(),
            last_status: Instant::now(),
        }
    }

    /// Pick up new settings when the file changes; keep the old ones if it broke.
    fn maybe_reload(&mut self) {
        if self.last_settings_check.elapsed() < SETTINGS_RECHECK {
            return;
        }
        self.last_settings_check = Instant::now();
        if let Ok(mtime) = std::fs::metadata(Settings::path()).and_then(|m| m.modified()) {
            if Some(mtime) != self.settings_mtime {
                self.settings_mtime = Some(mtime);
                let (settings, _) = Settings::load();
                self.settings = settings;
            }
        }
    }

    /// One wheel event, scaled and inverted, for both the step flow (low-res) and the
    /// smooth rows (high-res). `vertical` false is the sideways wheel; when the person
    /// folded the sideways wheel into the ring, sideways events count as vertical.
    fn wheel_flow(&mut self, low_res: Option<i32>, high_res: Option<i32>, vertical: bool, out: &mut Out) {
        let scroll = self.settings.scrolling;
        let dir: f32 = if scroll.invert { -1.0 } else { 1.0 };
        let speed = scroll.speed.max(0.05);
        let mut batch: Vec<InputEvent> = Vec::new();
        let (low_code, hi_code) = if vertical { (8u16, 11u16) } else { (6u16, 12u16) };
        if let Some(detents) = low_res {
            let carry = if vertical { self.wheel_carry } else { self.wheel_h_carry };
            let (steps, carry) = scaled_i32(carry, detents as f32 * dir * speed);
            if vertical {
                self.wheel_carry = carry;
            } else {
                self.wheel_h_carry = carry;
            }
            if steps != 0 {
                batch.push(rel_event(low_code, steps));
            }
        }
        if let Some(rows) = high_res {
            let carry = if vertical { self.wheel_hi_carry } else { self.wheel_hi_h_carry };
            let (rows, carry) = scaled_i32(carry, rows as f32 * dir * speed);
            if vertical {
                self.wheel_hi_carry = carry;
            } else {
                self.wheel_hi_h_carry = carry;
            }
            if rows != 0 {
                batch.push(rel_event(hi_code, rows));
            }
        }
        if !batch.is_empty() {
            emit(out, &batch);
        }
    }

    /// One ring detent in the given direction, honoring speed, for scroll buttons.
    fn scroll_tick(&mut self, up: bool, out: &mut Out) {
        let dir: i32 = if up ^ self.settings.scrolling.invert { 1 } else { -1 };
        let speed = self.settings.scrolling.speed.max(0.1);
        let mut batch = vec![rel_event(8, dir)];
        if out.has_wheel_hi_res {
            batch.push(rel_event(11, (120.0 * dir as f32 * speed) as i32));
        }
        emit(out, &batch);
    }

    /// One sideways wheel detent, honoring speed; folded into the ring when asked.
    fn scroll_tick_side(&mut self, right: bool, out: &mut Out) {
        if self.settings.scrolling.merge_horizontal {
            self.scroll_tick(right, out);
            return;
        }
        let speed = self.settings.scrolling.speed.max(0.1);
        let dir: i32 = if right { 1 } else { -1 };
        let mut batch = vec![rel_event(6, dir)];
        if out.has_wheel_hi_res {
            batch.push(rel_event(12, (120.0 * dir as f32 * speed) as i32));
        }
        emit(out, &batch);
    }

    /// What a physical button press/release becomes.
    fn on_button(&mut self, code: u16, down: bool, out: &mut Out) {
        // A click-lock held since earlier releases itself on the next press of any button.
        if down {
            if let Some((_, target)) = self.pending_lock.take() {
                emit(out, &[key_event(target, 0)]);
            }
        }
        let Some(action) = self.settings.action_for(code).cloned() else {
            // Buttons with no row are left exactly as the trackball sends them.
            emit(out, &[key_event(code, if down { 1 } else { 0 })]);
            return;
        };
        match action {
            ButtonAction::LeftClick => self.on_click(out, code, 0x110, down),
            ButtonAction::MiddleClick => self.on_click(out, code, 0x112, down),
            ButtonAction::RightClick => self.on_click(out, code, 0x111, down),
            ButtonAction::Back => self.on_click(out, code, 0x113, down),
            ButtonAction::Forward => self.on_click(out, code, 0x114, down),
            ButtonAction::DoubleClick => {
                if down {
                    emit(out, &[key_event(0x110, 1), key_event(0x110, 0)]);
                    std::thread::sleep(DOUBLE_GAP);
                    emit(out, &[key_event(0x110, 1), key_event(0x110, 0)]);
                }
            }
            ButtonAction::ClickLock => {
                if down {
                    emit(out, &[key_event(0x110, 1)]);
                    self.pending_lock = Some((code, 0x110));
                }
                // The physical release is ignored on purpose: the button stays held
                // until the next press of any button (handled at the top of this fn).
            }
            ButtonAction::ScrollUp | ButtonAction::ScrollDown => {
                let up = matches!(action, ButtonAction::ScrollUp);
                if down {
                    self.scroll_tick(up, out);
                    self.repeat_scroll.insert(code, (0, up));
                } else {
                    self.repeat_scroll.remove(&code);
                }
            }
            ButtonAction::ScrollLeft | ButtonAction::ScrollRight => {
                let right = matches!(action, ButtonAction::ScrollRight);
                if down {
                    self.scroll_tick_side(right, out);
                    self.repeat_scroll.insert(code, (1, right));
                } else {
                    self.repeat_scroll.remove(&code);
                }
            }
            ButtonAction::Key(text) => {
                let codes = parse_combo(&text);
                if down {
                    let down_batch: Vec<InputEvent> = codes.iter().map(|c| key_event(*c, 1)).collect();
                    emit(out, &down_batch);
                    self.held_virtual.insert(code, codes);
                } else if let Some(codes) = self.held_virtual.remove(&code) {
                    let up_batch: Vec<InputEvent> = codes.iter().rev().map(|c| key_event(*c, 0)).collect();
                    emit(out, &up_batch);
                }
            }
            ButtonAction::Snippet(text) => {
                if down {
                    type_text(out, &text);
                }
            }
            ButtonAction::Macro(steps) => {
                if down {
                    run_macro(out, &steps);
                }
            }
            ButtonAction::VolumeUp => tap_key(out, down, media::VOLUME_UP),
            ButtonAction::VolumeDown => tap_key(out, down, media::VOLUME_DOWN),
            ButtonAction::VolumeMute => tap_key(out, down, media::VOLUME_MUTE),
            ButtonAction::PlayPause => tap_key(out, down, media::PLAY_PAUSE),
            ButtonAction::NextTrack => tap_key(out, down, media::NEXT_TRACK),
            ButtonAction::PrevTrack => tap_key(out, down, media::PREV_TRACK),
            ButtonAction::LockScreen => tap_key(out, down, media::SCREENLOCK),
            ButtonAction::Sleep => tap_key(out, down, media::SLEEP),
            ButtonAction::Calculator => tap_key(out, down, media::CALCULATOR),
            ButtonAction::CloseWindow => {
                if down && !desktop_dispatch(&["killactive"]) {
                    tap_combo(out, "alt+f4");
                }
            }
            ButtonAction::FullscreenToggle => {
                if down && !desktop_dispatch(&["fullscreen", "1"]) {
                    tap_combo(out, "f11");
                }
            }
            ButtonAction::SwitchWindow => {
                if down && !desktop_dispatch(&["cyclenext"]) {
                    tap_combo(out, "alt+tab");
                }
            }
            ButtonAction::Nothing => {
                // Blocked on purpose — Konnect-style "No Action".
            }
        }
    }

    /// A plain click mapping: press and release follow the physical button.
    fn on_click(&mut self, out: &mut Out, _physical: u16, target: u16, down: bool) {
        emit(out, &[key_event(target, if down { 1 } else { 0 })]);
    }

    /// Buttons held that repeat scrolling, once per interval.
    fn tick_repeats(&mut self, out: &mut Out) {
        if self.last_repeat.elapsed() < SCROLL_REPEAT {
            return;
        }
        self.last_repeat = Instant::now();
        let entries: Vec<(u8, bool)> = self.repeat_scroll.values().map(|(kind, dir)| (*kind, *dir)).collect();
        for (kind, dir) in entries {
            if kind == 0 {
                self.scroll_tick(dir, out);
            } else {
                self.scroll_tick_side(dir, out);
            }
        }
    }

    /// Note the last button in the status file, throttled so the disk stays calm.
    fn note_button(&mut self, code: u16) {
        if self.last_status.elapsed() < Duration::from_millis(120) {
            return;
        }
        self.last_status = Instant::now();
        write_status("holding", &self.status_device, "", code);
    }
}

/// The pid file, so the panel can find the helper.
fn write_pid() {
    let dir = crate::system::state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("pid"), format!("{}", std::process::id()));
}

fn clean_exit() {
    let dir = crate::system::state_dir();
    let _ = std::fs::remove_file(dir.join("pid"));
    let _ = std::fs::remove_file(dir.join("status.json"));
    let _ = std::fs::remove_file(dir.join("stop"));
}

/// The main loop of the helper process (`--helper`).
pub fn run() -> Result<(), String> {
    if crate::system::helper_running() {
        return Err("A helper is already running for this desktop session.".to_string());
    }
    let _ = std::fs::create_dir_all(crate::system::state_dir());
    let _ = std::fs::remove_file(crate::system::state_dir().join("stop"));
    write_pid();
    write_status("starting", "", "", 0);
    println!("kensington-trackball helper: listening for the trackball (stop it from the panel)");

    let stop_file: PathBuf = crate::system::state_dir().join("stop");

    loop {
        if stop_file.exists() {
            break;
        }
        let mut real = match look_for_trackball() {
            Look::Opened(device) => device,
            Look::Blocked { node, name, err } => {
                write_status(
                    "no-permission",
                    &node,
                    &format!(
                        "Found {name} on {node}, but Linux refused to open it ({err}). \
                         Press “Allow the trackball” once more in the Setup tab — the fix lands \
                         within seconds, nothing needs replugging.",
                        name = name,
                        node = node,
                        err = err
                    ),
                    0,
                );
                std::thread::sleep(Duration::from_millis(1200));
                continue;
            }
            Look::Nothing => {
                write_status(
                    "waiting",
                    "",
                    "No Kensington trackball shows up on USB right now — plug in the trackball's cable or its wireless dongle.",
                    0,
                );
                std::thread::sleep(Duration::from_millis(1500));
                continue;
            }
        };
        let device_name = real.name().unwrap_or("the trackball").to_string();
        if let Err(e) = real.grab() {
            write_status(
                "grab-failed",
                &device_name,
                "The trackball is there, but the helper could not take it over — is something else holding it? Retrying.",
                0,
            );
            eprintln!("trackball helper: grab failed: {e}; retrying in 3s");
            std::thread::sleep(Duration::from_millis(3000));
            continue;
        }
        let _ = real.set_nonblocking(true);
        let mut out = match build_virtual(&real, "Kensington Trackball Panel") {
            Ok(out) => out,
            Err(e) => {
                let _ = real.ungrab();
                write_status("uinput-failed", &device_name, &e, 0);
                eprintln!("trackball helper: {e}; retrying in 3s");
                std::thread::sleep(Duration::from_millis(3000));
                continue;
            }
        };
        let mut helper = Helper::new();
        helper.status_device = device_name.clone();
        write_status("holding", &device_name, "", 0);
        println!("kensington-trackball helper: took over {device_name}");

        // The work loop for one connection.
        loop {
            helper.maybe_reload();
            if stop_file.exists() {
                let _ = real.ungrab();
                drop(out);
                clean_exit();
                println!("kensington-trackball helper: stopped");
                return Ok(());
            }
            let mut device_gone = false;
            match real.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        let kind = ev.event_type().0;
                        let code = ev.code();
                        let value = ev.value();
                        match kind {
                            1 => {
                                // EV_KEY: one of the trackball's buttons (or another key it reports).
                                let known = crate::device::KNOWN_BUTTONS
                                    .iter()
                                    .find(|(bc, _, _)| *bc == code)
                                    .map(|(_, name, _)| *name);
                                if let Some(_name) = known {
                                    if value != 2 {
                                        helper.note_button(code);
                                        helper.on_button(code, value == 1, &mut out);
                                    }
                                } else if value != 2 {
                                    // Media keys and other keys the device reports pass through.
                                    emit(&mut out, &[key_event(code, value)]);
                                }
                            }
                            2 => {
                                // EV_REL: movement passes through; wheels get scaled.
                                let merge = helper.settings.scrolling.merge_horizontal;
                                match code {
                                    0 | 1 => emit(&mut out, &[rel_event(code, value)]),
                                    8 => helper.wheel_flow(Some(value), None, true, &mut out),
                                    6 => {
                                        if merge {
                                            helper.wheel_flow(Some(value), None, true, &mut out);
                                        } else {
                                            helper.wheel_flow(Some(value), None, false, &mut out);
                                        }
                                    }
                                    11 => helper.wheel_flow(None, Some(value), true, &mut out),
                                    12 => {
                                        if merge {
                                            helper.wheel_flow(None, Some(value), true, &mut out);
                                        } else {
                                            helper.wheel_flow(None, Some(value), false, &mut out);
                                        }
                                    }
                                    _ => emit(&mut out, &[rel_event(code, value)]),
                                }
                            }
                            _ => {}
                        }
                    }
                    helper.tick_repeats(&mut out);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.raw_os_error() == Some(11) => {
                    helper.tick_repeats(&mut out);
                    std::thread::sleep(TICK);
                }
                Err(_) => {
                    // The device went away (unplugged): let go and look again outside this match.
                    device_gone = true;
                }
            }
            if device_gone {
                let _ = real.ungrab();
                drop(out);
                write_status("waiting", "", "The trackball went away (unplugged?) — waiting for it again.", 0);
                std::thread::sleep(Duration::from_millis(1500));
                break;
            }
        }
    }
    clean_exit();
    Ok(())
}
