//! Turning panel text into Linux key codes.
//!
//! The helper types key combinations (`ctrl+shift+t`), snippets (plain text) and the
//! media/desktop keys Konnect-style actions need. Everything is ordinary Linux `KEY_*`
//! codes emitted through uinput, so apps treat the trackball keys like any keyboard.

use evdev::KeyCode;

/// Short names the panel accepts, exactly the way people write them.
/// Pairs a written word with its Linux key code.
pub const WORDS: &[(&str, u16)] = &[
    // Modifiers.
    ("ctrl", 0x1d),          // KEY_LEFTCTRL
    ("control", 0x1d),
    ("shift", 0x2a),         // KEY_LEFTSHIFT
    ("alt", 0x38),           // KEY_LEFTALT
    ("super", 0x7d),         // KEY_LEFTMETA
    ("meta", 0x7d),
    ("win", 0x7d),
    ("cmd", 0x7d),
    // Common editing keys.
    ("return", 0x1c),
    ("enter", 0x1c),
    ("esc", 0x01),
    ("escape", 0x01),
    ("tab", 0x0f),
    ("space", 0x39),
    ("backspace", 0x0e),
    ("delete", 0x6f),
    ("del", 0x6f),
    ("insert", 0x70),
    ("home", 0x66),
    ("end", 0x6b),
    ("pageup", 0x68),
    ("pagedown", 0x6d),
    ("pgup", 0x68),
    ("pgdn", 0x6d),
    ("up", 0x67),
    ("down", 0x6c),
    ("left", 0x69),
    ("right", 0x6a),
    // Punctuation words (US layout).
    ("minus", 0x0c),
    ("equal", 0x0d),
    ("comma", 0x33),
    ("dot", 0x34),
    ("period", 0x34),
    ("slash", 0x35),
    ("semicolon", 0x27),
    ("quote", 0x28),
    ("apostrophe", 0x28),
    ("bracketleft", 0x1a),
    ("bracketright", 0x1b),
    ("backslash", 0x2b),
    ("grave", 0x29),
    ("backtick", 0x29),
    // Media keys.
    ("volumeup", 0x73),
    ("volumedown", 0x72),
    ("volumemute", 0x71),
    ("mute", 0x71),
    ("micmute", 0xf8),
    ("playpause", 0xa4),
    ("play", 0xa4),
    ("nexttrack", 0xa3),
    ("next", 0xa3),
    ("prevtrack", 0xa2),
    ("previous", 0xa2),
    ("screenlock", 0x98),
    ("lock", 0x98),
    ("sleep", 0x8e),
    ("calculator", 0x8c),
    ("calc", 0x8c),
    ("search", 0xd9),
    ("find", 0x88),
    ("menu", 0x8b),
    ("brightnessup", 0xe1),
    ("brightnessdown", 0xe0),
];

/// A written key word (or single letter/digit/f-key) to its Linux code.
pub fn key_code(word: &str) -> Option<u16> {
    let name = word.trim().to_ascii_lowercase();
    if name.len() == 1 {
        if let Some((code, _)) = char_code(name.chars().next().unwrap_or_default()) {
            return Some(code);
        }
    }
    // Function keys: f1..f19.
    if let Some(n) = name.strip_prefix('f') {
        if let Ok(n) = n.parse::<u16>() {
            if (1..=19).contains(&n) {
                return Some(0x3a + n - 1); // KEY_F1..KEY_F19
            }
        }
    }
    WORDS.iter().find(|(known, _)| *known == name).map(|(_, code)| *code)
}

/// A quick label for a key code, for the panel to show.
pub fn key_name(code: u16) -> String {
    for (word, known) in WORDS {
        if *known == code {
            return (*word).to_string();
        }
    }
    let debug = format!("{:?}", KeyCode::new(code));
    if debug.contains("Unspecified") || debug.contains('(') {
        format!("key 0x{code:x}")
    } else {
        debug
    }
}

/// Linux key codes for lowercase letters, in typing order.
/// (They follow the old keyboard scan order, not the alphabet.)
const LETTERS: [(char, u16); 26] = [
    ('a', 0x1e),
    ('b', 0x30),
    ('c', 0x2e),
    ('d', 0x20),
    ('e', 0x12),
    ('f', 0x21),
    ('g', 0x22),
    ('h', 0x23),
    ('i', 0x17),
    ('j', 0x24),
    ('k', 0x25),
    ('l', 0x26),
    ('m', 0x32),
    ('n', 0x31),
    ('o', 0x18),
    ('p', 0x19),
    ('q', 0x10),
    ('r', 0x13),
    ('s', 0x1f),
    ('t', 0x14),
    ('u', 0x16),
    ('v', 0x2f),
    ('w', 0x11),
    ('x', 0x2d),
    ('y', 0x15),
    ('z', 0x2c),
];

/// A character to (key code, needs the shift key), for snippets. US layout, ASCII only.
pub fn char_code(c: char) -> Option<(u16, bool)> {
    match c {
        ' ' => Some((0x39, false)),
        '\n' | '\r' => Some((0x1c, false)),
        '\t' => Some((0x0f, false)),
        'a'..='z' | '0'..='9' | '-' | '=' | '[' | ']' | '\\' | ';' | '\'' | '`' | ',' | '.' | '/' => {
            if let Some((_, code)) = LETTERS.iter().find(|(letter, _)| *letter == c) {
                return Some((*code, false));
            }
            match c {
                '1' => Some((0x02, false)),
                '2' => Some((0x03, false)),
                '3' => Some((0x04, false)),
                '4' => Some((0x05, false)),
                '5' => Some((0x06, false)),
                '6' => Some((0x07, false)),
                '7' => Some((0x08, false)),
                '8' => Some((0x09, false)),
                '9' => Some((0x0a, false)),
                '0' => Some((0x0b, false)),
                '-' => Some((0x0c, false)),
                '=' => Some((0x0d, false)),
                '[' => Some((0x1a, false)),
                ']' => Some((0x1b, false)),
                '\\' => Some((0x2b, false)),
                ';' => Some((0x27, false)),
                '\'' => Some((0x28, false)),
                '`' => Some((0x29, false)),
                ',' => Some((0x33, false)),
                '.' => Some((0x34, false)),
                '/' => Some((0x35, false)),
                _ => None,
            }
        }
        'A'..='Z' => {
            let lower = c.to_ascii_lowercase();
            LETTERS.iter().find(|(letter, _)| *letter == lower).map(|(_, code)| (*code, true))
        }
        '!' => Some((0x02, true)),
        '@' => Some((0x03, true)),
        '#' => Some((0x04, true)),
        '$' => Some((0x05, true)),
        '%' => Some((0x06, true)),
        '^' => Some((0x07, true)),
        '&' => Some((0x08, true)),
        '*' => Some((0x09, true)),
        '(' => Some((0x0a, true)),
        ')' => Some((0x0b, true)),
        '_' => Some((0x0c, true)),
        '+' => Some((0x0d, true)),
        '{' => Some((0x1a, true)),
        '}' => Some((0x1b, true)),
        '|' => Some((0x2b, true)),
        ':' => Some((0x27, true)),
        '"' => Some((0x28, true)),
        '~' => Some((0x29, true)),
        '<' => Some((0x33, true)),
        '>' => Some((0x34, true)),
        '?' => Some((0x35, true)),
        _ => None,
    }
}

/// A written combination like `ctrl+shift+t` in press order: modifiers first.
pub fn parse_combo(text: &str) -> Vec<u16> {
    let mut out = Vec::new();
    let mut main: Option<u16> = None;
    for word in text.split('+').map(str::trim).filter(|w| !w.is_empty()) {
        let low = word.to_ascii_lowercase();
        let is_modifier = matches!(low.as_str(), "ctrl" | "control" | "shift" | "alt" | "super" | "meta" | "win" | "cmd");
        if let Some(code) = key_code(&low) {
            if is_modifier {
                out.push(code);
            } else {
                main = Some(code);
            }
        }
    }
    if let Some(main) = main {
        out.push(main);
    }
    out
}

/// Well-known single codes for media and desktop actions.
pub mod media {
    pub const VOLUME_UP: u16 = 0x73;
    pub const VOLUME_DOWN: u16 = 0x72;
    pub const VOLUME_MUTE: u16 = 0x71;
    pub const PLAY_PAUSE: u16 = 0xa4;
    pub const NEXT_TRACK: u16 = 0xa3;
    pub const PREV_TRACK: u16 = 0xa2;
    pub const SCREENLOCK: u16 = 0x98;
    pub const SLEEP: u16 = 0x8e;
    pub const CALCULATOR: u16 = 0x8c;
}
