//! Core library for the Acer Predator PH717 Linux control panel.
//!
//! Two independent hardware channels are supported:
//!
//! * the ACPI-WMI "gaming function" mailbox, exposed by `acer_gaming_probe.ko`
//!   as `/dev/acer_gaming_wmi` — ring, heat-pipe, fan LEDs, fans and the
//!   firmware overclock levels;
//! * the vendor HID channel `0d62:9a1b`, interface `0xFF01` — per-key RGB,
//!   effects, brightness and MagForce lighting.
//!
//! Every write goes through [`SafetyPolicy`] and can be snapshotted first.

pub mod config;
pub mod controller;
pub mod error;
pub mod hid;
pub mod keymap;
pub mod lighting;
pub mod monitor;
pub mod oc;
pub mod profile;
pub mod protocol;
pub mod safety;
pub mod wmi;

pub use config::{config_dir, ensure_dirs, profiles_dir, snapshots_dir, state_dir};
pub use controller::{Controller, Status};
pub use error::{Error, Result};
pub use lighting::{KeyColors, LightingMode, LightingState};
pub use monitor::{Monitor, Sample};
pub use oc::OcLevel;
pub use profile::{FanProfile, LightingProfile, OcProfile, Profile, ProfileStore, RingProfile};
pub use protocol::Rgb;
pub use safety::{SafetyPolicy, Snapshot};
