# Legacy Python toolkit

This directory contains the pre-Rust toolkit that was used to bring up the
touchpad ring and to probe the WMI/HID channels.

It is **unsupported** and kept for reference only. Use the Rust binaries
(`predator-sense`, `predator-sense-gui`) for everything.

| file | purpose |
|---|---|
| `python/acer_core.py` | original protocol layer (WMI mailbox + HID helpers) |
| `python/acer_gaming.py` | original CLI/probe tool |
| `python/acer_gui.py` | original PyQt5 panel (requires PyQt5) |
| `tools/acer-ring` | old standalone ring helper |

The Rust implementation supersedes all of these. In particular, the old
`acer-ring` helper and the old PyQt5 GUI do not have the safety policy,
snapshots or recovery paths of the Rust tool.

License: GPL-2.0-or-later, same as the rest of the repository.
