# Documentation

| file | contents |
|---|---|
| [`PROTOCOL.md`](PROTOCOL.md) | Functional-interface description of the WMI and HID protocols, payloads and health indices |
| [`PROVENANCE.md`](PROVENANCE.md) | Research sources, access/copying facts, clean-room accuracy and repository checks |
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

The protocol facts were identified from this machine's public ACPI tables, the
USB HID descriptors, reversible probes, and static inspection of the locally
installed, publicly distributed PredatorSense package for functional interface
details. The repository does not include Acer firmware, decompiled application
source, vendor binaries or vendor assets. It is not a certified clean room; see
[`PROVENANCE.md`](PROVENANCE.md) and [`../CONTRIBUTING.md`](../CONTRIBUTING.md).
