//! Finding the trackball on this computer and learning what the kernel says it has:
//! which buttons, which axes, whether it is ready to be read.

use evdev::{Device, KeyCode, RelativeAxisCode};
use std::path::Path;

/// An input device the kernel knows, for example `event13` named "Kensington Expert Mouse".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDevice {
    /// The kernel event node, like `event13`.
    pub sys_name: String,
    /// A human name, like "Kensington Expert Mouse" (may be empty for some devices).
    pub name: String,
    /// USB vendor id, like `047d` for Kensington.
    pub vendor: String,
    /// USB product id, like `1001` for an Expert Mouse.
    pub product: String,
    /// The device file, like `/dev/input/event13`.
    pub dev_node: String,
}

impl EventDevice {
    /// Kensington's USB vendor id is `047d`; some models also say Kensington in their name.
    pub fn is_kensington(&self) -> bool {
        self.vendor.eq_ignore_ascii_case("047d")
            || self.name.to_ascii_lowercase().contains("kensington")
            || self.name.to_ascii_lowercase().contains("expert")
    }

    /// A line the panel can show: name, id and the event node.
    pub fn line(&self) -> String {
        if self.name.is_empty() {
            format!("{} (usb {}:{})", self.sys_name, self.vendor, self.product)
        } else {
            format!("{} — {} (usb {}:{})", self.sys_name, self.name, self.vendor, self.product)
        }
    }
}

/// One button the kernel says the trackball has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevButton {
    /// The Linux event code, like 272 for BTN_LEFT.
    pub code: u16,
    /// The kernel name, like `BTN_LEFT`.
    pub name: &'static str,
}

/// The standard mouse-relevant buttons, in kernel order.
pub const KNOWN_BUTTONS: [(u16, &str, KeyCode); 8] = [
    (0x110, "BTN_LEFT", KeyCode::BTN_LEFT),
    (0x111, "BTN_RIGHT", KeyCode::BTN_RIGHT),
    (0x112, "BTN_MIDDLE", KeyCode::BTN_MIDDLE),
    (0x113, "BTN_SIDE", KeyCode::BTN_SIDE),
    (0x114, "BTN_EXTRA", KeyCode::BTN_EXTRA),
    (0x115, "BTN_FORWARD", KeyCode::BTN_FORWARD),
    (0x116, "BTN_BACK", KeyCode::BTN_BACK),
    (0x117, "BTN_TASK", KeyCode::BTN_TASK),
];

/// What a device can do, as far as the panel cares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceCaps {
    /// Buttons the kernel reports, in code order.
    pub buttons: Vec<DevButton>,
    /// Whether the device moves the pointer (has X and Y axes).
    pub has_pointer: bool,
    /// Whether there is a wheel (the scroll ring reports like one).
    pub has_wheel: bool,
    /// Whether the wheel also reports high-resolution rows (smooth scrolling).
    pub has_wheel_hi_res: bool,
    /// Whether there is a sideways wheel.
    pub has_hwheel: bool,
    /// True when the device file could be opened and asked (otherwise this is empty).
    pub readable: bool,
    /// The device's own name as the kernel sees it, when readable.
    pub kernel_name: String,
}

impl DeviceCaps {
    /// A short line for the panel: "4 buttons, pointer, scroll wheel".
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.readable {
            parts.push(format!("{} button(s)", self.buttons.len()));
            if self.has_pointer {
                parts.push("pointer".to_string());
            }
            if self.has_wheel {
                parts.push(if self.has_wheel_hi_res { "scroll wheel (smooth)".to_string() } else { "scroll wheel".to_string() });
            }
            if self.has_hwheel {
                parts.push("sideways wheel".to_string());
            }
            parts.join(", ")
        } else {
            "the panel cannot read this device yet (see Setup)".to_string()
        }
    }
}

/// Ask the kernel what a device can do. Empty result when the file can't be read.
pub fn caps(dev_node: &str) -> DeviceCaps {
    let mut out = DeviceCaps::default();
    let device = match Device::open(Path::new(dev_node)) {
        Ok(device) => device,
        Err(_) => return out,
    };
    out.readable = true;
    out.kernel_name = device.name().unwrap_or("").to_string();
    if let Some(keys) = device.supported_keys() {
        for (code, name, variant) in KNOWN_BUTTONS {
            if keys.contains(variant) {
                out.buttons.push(DevButton { code, name });
            }
        }
    }
    if let Some(rel) = device.supported_relative_axes() {
        out.has_pointer = rel.contains(RelativeAxisCode::REL_X) && rel.contains(RelativeAxisCode::REL_Y);
        out.has_wheel = rel.contains(RelativeAxisCode::REL_WHEEL);
        out.has_wheel_hi_res = rel.contains(RelativeAxisCode::REL_WHEEL_HI_RES);
        out.has_hwheel = rel.contains(RelativeAxisCode::REL_HWHEEL);
    }
    out
}

/// Read one small file and trim it.
fn read_trimmed(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

/// Every input device the kernel currently lists.
pub fn scan() -> Vec<EventDevice> {
    let class = Path::new("/sys/class/input");
    let mut entries = match std::fs::read_dir(class) {
        Ok(entries) => entries.flatten().collect::<Vec<_>>(),
        Err(_) => return Vec::new(),
    };
    entries.sort_by_key(|entry| entry.file_name());
    let mut devices = Vec::new();
    for entry in entries {
        let sys_name = entry.file_name().to_string_lossy().into_owned();
        if !sys_name.starts_with("event") {
            continue;
        }
        let device_dir = entry.path().join("device");
        let name = read_trimmed(&device_dir.join("name"));
        let vendor = read_trimmed(&device_dir.join("id").join("vendor"));
        let product = read_trimmed(&device_dir.join("id").join("product"));
        devices.push(EventDevice {
            sys_name: sys_name.clone(),
            name,
            vendor,
            product,
            dev_node: format!("/dev/input/{sys_name}"),
        });
    }
    devices
}

/// Whether an event node moves the pointer, judged from the sysfs capability bits
/// (REL_X and REL_Y both set). This works without opening the device, so the panel can
/// pick the right node even before permission is in place.
pub fn sysfs_pointer(sys_name: &str) -> bool {
    let rel = read_trimmed(&Path::new("/sys/class/input").join(sys_name).join("device").join("capabilities").join("rel"));
    let hex = rel.trim_start_matches("0x").trim();
    match u128::from_str_radix(hex, 16) {
        Ok(bits) => bits & 3 == 3,
        Err(_) => false,
    }
}

/// The one Kensington that actually moves the pointer, if any — the node worth grabbing.
/// Wireless models also list extra helper nodes (media keys and the like); those are
/// skipped by their sysfs capabilities, so this works even before the permission is set.
pub fn kensington(devices: &[EventDevice]) -> Option<&EventDevice> {
    let mut best: Option<(&EventDevice, u8)> = None;
    for device in devices {
        if !device.is_kensington() {
            continue;
        }
        let mut score = 0u8;
        if sysfs_pointer(&device.sys_name) {
            score += 2;
        }
        if !device.name.to_ascii_lowercase().contains("consumer") {
            score += 1;
        }
        match best {
            Some((_, best_score)) if best_score >= score => {}
            _ => best = Some((device, score)),
        }
    }
    best.map(|(device, _)| device)
}

/// Whether the panel may read the device today (what the Setup tab is about otherwise).
pub fn openable(dev_node: &str) -> bool {
    std::fs::OpenOptions::new().read(true).open(dev_node).is_ok()
}
