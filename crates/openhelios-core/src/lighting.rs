use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::hid::HidKeyboard;
use crate::keymap::{self, SLOT_COUNT, TOUCHPAD_TAG};
use crate::protocol::Rgb;

pub type KeyColors = [Rgb; SLOT_COUNT];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LightingMode {
    #[default]
    Static,
    Effect,
    Off,
}

#[derive(Clone, Debug)]
pub struct LightingState {
    pub mode: LightingMode,
    /// 1..=100 in PredatorSense; 0 turns the backlight off.
    pub brightness: u8,
    pub color: Rgb,
    pub effect: u8,
    pub speed: u8,
    pub direction: u8,
    /// Per-slot colours. Slot 126 is the touchpad entry.
    pub keys: KeyColors,
}

impl Default for LightingState {
    fn default() -> Self {
        Self {
            mode: LightingMode::Static,
            brightness: 50,
            color: Rgb::DEFAULT_CYAN,
            effect: 2,
            speed: 5,
            direction: 1,
            keys: [Rgb::DEFAULT_CYAN; SLOT_COUNT],
        }
    }
}

impl LightingState {
    pub fn fill(&mut self, color: Rgb) {
        self.keys = [color; SLOT_COUNT];
        self.color = color;
    }

    pub fn set_keys(&mut self, tags: &[u8], color: Rgb) {
        for tag in tags {
            if (*tag as usize) < SLOT_COUNT {
                self.keys[*tag as usize] = color;
            }
        }
    }

    pub fn touchpad(&self) -> Rgb {
        self.keys[TOUCHPAD_TAG as usize]
    }

    pub fn apply(&self, hid: &HidKeyboard) -> Result<()> {
        match self.mode {
            LightingMode::Static => hid.static_colors(&self.keys, self.brightness),
            LightingMode::Effect => hid.effect(
                self.effect,
                self.speed,
                self.brightness,
                self.direction,
                self.color,
            ),
            LightingMode::Off => hid.off(),
        }
    }

    /// Build the state from a profile's sparse key overrides.
    pub fn from_parts(
        mode: LightingMode,
        brightness: u8,
        color: Rgb,
        effect: u8,
        speed: u8,
        direction: u8,
        overrides: &std::collections::BTreeMap<u8, Rgb>,
    ) -> Self {
        let mut keys = [color; SLOT_COUNT];
        for (tag, c) in overrides {
            if (*tag as usize) < SLOT_COUNT {
                keys[*tag as usize] = *c;
            }
        }
        Self {
            mode,
            brightness,
            color,
            effect,
            speed,
            direction,
            keys,
        }
    }

    /// Sparse map of colours that differ from the base colour.
    pub fn overrides(&self) -> std::collections::BTreeMap<u8, Rgb> {
        let mut map = std::collections::BTreeMap::new();
        for (i, c) in self.keys.iter().enumerate() {
            if *c != self.color {
                map.insert(i as u8, *c);
            }
        }
        map
    }

    /// Convenience for the GUI: colour of a key by hardware tag.
    pub fn color_of(&self, tag: u8) -> Option<Rgb> {
        if (tag as usize) < SLOT_COUNT {
            Some(self.keys[tag as usize])
        } else {
            None
        }
    }

    pub fn tagged_keys(&self) -> impl Iterator<Item = (&'static keymap::KeyDef, Rgb)> + use<'_> {
        keymap::keys()
            .iter()
            .map(|k| (k, self.keys[k.tag as usize]))
    }
}
