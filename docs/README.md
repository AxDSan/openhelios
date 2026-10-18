# Documentation

| file | contents |
|---|---|
| [`PROTOCOL.md`](PROTOCOL.md) | Clean-room description of the WMI and HID protocols, payloads and health indices |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Components, data flow, safety model, config/state, packaging |
| [`PARITY.md`](PARITY.md) | PredatorSense module inventory and Linux status, including Windows-only modules |
| [`RECOVERY.md`](RECOVERY.md) | Snapshots, panic, EC reset, BIOS defaults, module removal, OC recovery |
| [`screenshots/`](screenshots/) | GUI captures used by the README |

## Reading order

1. [`PROTOCOL.md`](PROTOCOL.md) — what the hardware accepts.
2. [`ARCHITECTURE.md`](ARCHITECTURE.md) — how the tool is put together.
3. [`RECOVERY.md`](RECOVERY.md) — before you enable overclocking.
4. [`PARITY.md`](PARITY.md) — what is and is not a 1:1 replacement.

## Evidence

The protocol facts were recovered from this machine's public ACPI tables, the
USB HID descriptors, the observable behaviour of the shipped Windows
application, and static analysis of its service. This repository does not
include Acer firmware, decompiled application source, or other proprietary
material. See the clean-room statement in [`PROTOCOL.md`](PROTOCOL.md) and
[`../CONTRIBUTING.md`](../CONTRIBUTING.md).
