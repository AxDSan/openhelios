//! Physical `en-us` layout for the PH717. `tag` is the hardware LED index.
//!
//! The data was generated from the keyboard layout shipped with PredatorSense
//! 3.00.3152 for the PH717-72 (same Levante platform and keyboard).

include!("keymap_data.rs");

/// PredatorSense stores the touchpad-ring colour in slot 126.
pub const TOUCHPAD_TAG: u8 = 126;

/// Colour buffer slots, including the touchpad.
pub const SLOT_COUNT: usize = 128;

pub fn keys() -> &'static [KeyDef] {
    KEYS
}

pub fn key_by_tag(tag: u8) -> Option<&'static KeyDef> {
    KEYS.iter().find(|k| k.tag == tag)
}

pub fn key_by_name(name: &str) -> Option<&'static KeyDef> {
    KEYS.iter().find(|k| k.name == name)
}

/// Human-readable keycap label.
pub fn label(key: &KeyDef) -> &'static str {
    match key.name {
        "left_Ctrl" | "right_Ctrl" => "Ctrl",
        "left_Shift" | "right_Shift" => "Shift",
        "Caps_lock" => "Caps",
        "tilde_grave_accent" => "`",
        "broken_bar_backslash" => "\\",
        "Backspace" => "Backspace",
        "left_Enter" | "right_Enter" => "Enter",
        "space" => "Space",
        "Alt" => "Alt",
        "Alt_Gr" => "AltGr",
        "Menu" => "Menu",
        "Win" => "Win",
        "Fn" => "Fn",
        "Esc" => "Esc",
        "Tab" => "Tab",
        "PtrSc" => "PrtSc",
        "Pause" => "Pause",
        "Del" => "Del",
        "Home" => "Home",
        "End" => "End",
        "PgUp" => "PgUp",
        "PgDn" => "PgDn",
        "NumLk" => "Num",
        "predatorsense" => "Predator",
        "arrow_up" => "↑",
        "arrow_down" => "↓",
        "arrow_left" => "←",
        "arrow_right" => "→",
        "asterisk" => "*",
        "right_slash" => "/",
        "right_plus" => "+",
        "right_minus" => "-",
        "left_plus" => "=",
        "left_minus" => "-",
        "left_num0" | "right_num0" => "0",
        "left_num1" | "right_num1" => "1",
        "left_num2" | "right_num2" => "2",
        "left_num3" | "right_num3" => "3",
        "left_num4" | "right_num4" => "4",
        "left_num5" | "right_num5" => "5",
        "left_num6" | "right_num6" => "6",
        "left_num7" | "right_num7" => "7",
        "left_num8" | "right_num8" => "8",
        "left_num9" | "right_num9" => "9",
        "question_slash" => "/",
        "greater_bullet" => ".",
        "less_semicolon" => ",",
        "colon_semicolon" => ";",
        "double_quote_apostrophe" => "'",
        "left_braces_left_brackets" => "[",
        "right_braces_right_brackets" => "]",
        "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12" => {
            key.name
        }
        other => other,
    }
}

pub fn preset(name: &str) -> Vec<u8> {
    let tags = match name.to_ascii_lowercase().as_str() {
        "all" => KEYS.iter().map(|k| k.tag).collect::<Vec<_>>(),
        "wasd" => tags_for(&["W", "A", "S", "D"]),
        "arrows" => tags_for(&["arrow_up", "arrow_down", "arrow_left", "arrow_right"]),
        "fkeys" => tags_for(&[
            "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
        ]),
        "numbers" => tags_for(&[
            "left_num0",
            "left_num1",
            "left_num2",
            "left_num3",
            "left_num4",
            "left_num5",
            "left_num6",
            "left_num7",
            "left_num8",
            "left_num9",
        ]),
        "numpad" => tags_for(&[
            "right_num0",
            "right_num1",
            "right_num2",
            "right_num3",
            "right_num4",
            "right_num5",
            "right_num6",
            "right_num7",
            "right_num8",
            "right_num9",
        ]),
        "modifiers" => tags_for(&[
            "left_Ctrl",
            "right_Ctrl",
            "left_Shift",
            "right_Shift",
            "Alt",
            "Alt_Gr",
            "Win",
            "Fn",
        ]),
        _ => Vec::new(),
    };
    tags
}

pub fn tags_for(names: &[&str]) -> Vec<u8> {
    names
        .iter()
        .filter_map(|n| key_by_name(n).map(|k| k.tag))
        .collect()
}

pub const PRESETS: &[&str] = &[
    "all",
    "wasd",
    "arrows",
    "fkeys",
    "numbers",
    "numpad",
    "modifiers",
];
