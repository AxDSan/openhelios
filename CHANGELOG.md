# Changelog

All notable changes to this project are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/).

## [0.1.1] — 2026-10-18

### Added

- Auto-persistence: GUI changes are written to the active profile within a
  second, and every successful CLI write persists too.
- Boot apply: `predator-sense-apply.service` is installed and enabled by
  default, waits for udev, and applies `default.toml` as root.
- `sudo predator-sense ...` now resolves `SUDO_USER`, writing the invoking
  user's profile and snapshots instead of `/root`.
- Turbo: GUI slide watcher and `predator-sense turbo apply|restore|status|watch`
  (fans 100 %, chassis lights, orange keyboard wave, CPU/GPU performance
  limits, captured-state restore).
- PredatorSense-inspired dark theme: full-label sidebar, cyan accent frame,
  270° gauges, and GPU Normal/Fast/Extreme cards.
- Horizontal and vertical content scrollbars, a 16 px gutter inside the frame,
  and fully wrapped control rows for narrow windows.

### Changed

- GUI layout: Home, Lighting, Overclocking, Fan Control, Power / GPU, MagForce,
  Profiles, Diagnostics, Recovery; the full sidebar is always shown so labels
  stay readable.
- Fan auto clears the EC duty register, preventing a stuck 100 % duty after
  Turbo or a manual run.
- GPU mode cards apply and persist power/clock limits through the privileged
  CLI.
- Version 0.1.1.

### Fixed

- The virtual WMI node now receives a seat/uaccess ACL, so fan, ring and
  keyboard-slide reads work immediately after install without a re-login.
- Slide-triggered Turbo no longer requires the separate "allow OC" switch;
  `auto_turbo` is the opt-in, and AC/battery checks still apply.
- The frame border could cut through headings and controls; the content is now
  inset inside the frame.
- Fan cards clipped "duty %" and stacked Apply unevenly; both cards now fill
  equal columns.
- Gauge arcs were clipped and the needle collided with the readout; gauges are
  centered and the needle has a separating outline.
- Long warnings/status text clipped; the GPU status line is human-readable.
- Test profiles were removed from the default store.

### Verified

- Per-key RGB, ring/heat-pipe, MagForce, fans auto/manual, Turbo apply/restore,
  RAPL PL1/PL2 and the boot service on the PH717-71.

## [0.1.0] — 2026-10-18

Initial public release.

### Added

- Rust workspace: `predator-core`, `predator-sense` CLI, `predator-sense-gui`
  (egui).
- Per-key RGB over the vendor HID channel (`0d62:9a1b`, `0xFF01`): static
  colours, 128-slot colour buffer, click/drag editor, WASD/arrow/F-row/number
  presets.
- Keyboard effects, brightness, and live getter readback.
- MagForce: actuation point, switch/linear/joystick mode, analog curve, replace
  mode, 30-second backlight and game-key mapping.
- WMI chassis lights: touchpad ring and heat-pipe on/off; brightness/colour
  payloads for models that support them.
- Fan control: automatic, quiet/performance presets, manual duty for CPU/GPU
  groups; EC fan RPM readback.
- Sensors: EC CPU/system/GPU temperatures, fan RPM, CPU/GPU clocks, CPU load and
  package power, RAM, battery/AC, NVIDIA telemetry via `nvidia-smi`.
- Overclocking: firmware Normal/Fast/Turbo levels, Max-Q, GPU mode, POST sound;
  RAPL PL1/PL2; NVIDIA power limit and locked clocks.
- Profiles (TOML), snapshots (JSON), export/import, opt-in boot-time apply.
- Safety: `panic`, `restore`, `--dry-run`, `--read-only`, `--allow-oc`,
  pre-write snapshots, AC/battery preflight for EC OC.
- Kernel module `acer_gaming_probe` with DKMS support.
- udev rule using a dedicated `predator-sense` group plus seat `uaccess`.
- Packaging, desktop entry, systemd unit, install/uninstall scripts.
- Documentation: protocol, architecture, parity, recovery, security,
  contribution and support guides.
- CI: formatting, Clippy, tests and release build.

### Verified

- Acer Predator PH717-71 (Helios 700), board `Levante_CFS`, BIOS V1.05,
  Fedora 43, kernel 7.0.12, NVIDIA RTX 2070 Mobile.
- Per-key RGB red → cyan; ring on/off; heat-pipe on/off; fan CPU 60 % → auto;
  EC CPU/GPU fan RPM and temperatures; MagForce actuation 46 → 45 → 46.

### Known limitations

- Acer's encrypted `.xtu` profiles are not replayed; RAPL/NVIDIA limits are the
  Linux equivalent. Voltage offsets are not exposed by the Linux driver.
- WMI methods 2/6/20 and ring colour are accepted but visibly inert on this
  unit; they are implemented for other models.
- Windows-only modules (App Center, GameSync, sound profiles, BIOS logo,
  mobile pairing) are not replicated.
- The pre-Rust Python toolkit is kept under `legacy/` for reference only.
- The boot-time service applies the built-in `default` profile under root; users
  who want a custom boot profile should install a system profile or run the
  service as their own user.
