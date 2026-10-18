# OpenHelios parity inventory

Target: Acer Predator PH717-71 / Levante_CFS, BIOS V1.05.

The Windows product is a UWP WPF front end (`PredatorSense.exe`) plus the
`PredatorSense Service` (`PSAdminAgent.exe` / `PSAgent.exe`). The service owns
the EC/WMI mailbox, the vendor HID channel, Intel XTU and NVAPI. This project
replaces both layers with:

```
openhelios-gui / openhelios
        │
        ├── openhelios_core::hid   → /dev/hidraw*  (0d62:9a1b, 0xFF01)
        └── openhelios_core::wmi   → /dev/acer_gaming_wmi → acer_gaming_probe.ko → WMBH
```

## Hardware feature matrix

| Windows feature | Windows path | Linux status | Notes |
|---|---|---|---|
| Per-key static RGB | HID `0x12`, 512-byte colour buffer | **working** | Verified red → cyan on the PH717-71 |
| Keyboard effects | HID `0x08 0x02`, `0x14`, `0x08 0x00` | **working** | Static, Breathing, Neon, Wave, Ripple, Rainbow, Snake, Zoom, Spot, Star, plus raw IDs 17–22 |
| Keyboard brightness | HID effect `(51,5,1)` | **working** | Getter 0x88 returns the current brightness in byte 4 |
| MagForce actuation | HID `0x32` / getter `0xB2` | **working** | Verified 46 → 45 → 46 |
| MagForce switch/linear/joystick | HID `0x31` / getter `0xB1` | **working** | `magforce setting` |
| MagForce analog curve | HID `0x35` / getter `0xB5` | **working** | `magforce analog` |
| MagForce replace mode / 30 s | HID `0x30` / getter `0xB0` | **working** | `magforce replace` |
| MagForce game keys (W/A/S/D) | HID `0x34`, 25-byte map | **working** | `magforce gamekeys`; the agent maps four slots to usages 0x1a/0x04/0x16/0x07 |
| Touchpad ring | WMI 10 group 0x80 | **working** | On/off verified; this firmware ignores hue |
| Heat-pipe light | WMI 10 group 0x40 | **working** | On/off |
| Fan LEDs / logo | WMI 10 groups 0x20/0x01/0x100, WMI 12 | **implemented, inert on this unit** | Accepted by firmware; no visible change on PH717-71 |
| Keyboard effect struct (WMI) | WMI 6/20 | **implemented, inert on this unit** | The keyboard is on HID, not WMI |
| Fan automatic | WMI 14 (`0x10001` etc.) | **working** | CPU/GPU groups; verified readback |
| Fan manual duty | WMI 14 + WMI 16 | **working** | CPU/GPU; payloads from PredatorSense |
| Fan RPM | WMI health index 2/6 | **working** | `EC CPU=…`, `EC GPU=…` in `monitor` |
| CPU/system/GPU temperatures | WMI health index 1/3/10 | **working** | Same indices PredatorSense uses |
| GPU clock/temp/power/load | NVAPI (Windows) | **working** | `nvidia-smi` |
| CPU clock/load/package power | Performance counters / XTU | **working** | cpufreq, `/proc/stat`, RAPL |
| RAM / battery / AC | WMI / Windows APIs | **working** | `/proc/meminfo`, power_supply |
| Firmware CPU/GPU OC levels | `SetGamingMiscSetting` | **working** | Levels Normal/Fast/Turbo (0/1/2) |
| Max-Q mode/level | WMI 22 payloads | **implemented** | 4101/4357/4613, 8197/8453/8709 |
| GPU mode / POST sound | WMI 22 | **implemented** | Guarded by `--allow-oc` |
| CPU power limits (XTU PL1/PL2) | Intel XTU SDK | **working equivalent** | powercap RAPL; not the encrypted `.xtu` files |
| GPU power limit / locked clocks | NVAPI | **working equivalent** | `nvidia-smi -pl`, `-lgc`, `-rgc` |
| CPU voltage offsets / core ratios | Intel XTU / MSR OC mailbox | **not implemented** | Requires MSR/OC-mailbox writes; deliberately out of the default safe path |
| GPU voltage offsets | NVAPI | **unavailable** | Not exposed by NVML or the Linux driver |
| Hotkeys / Fn remapping | Acer service + Windows hotkey XML | **not implemented** | The kernel already exposes Fn events as input; a userspace mapper is separate |
| Macros | PredatorSense userspace | **not implemented** | Input-level feature, not hardware |
| GameSync | PredatorSense service | **not implemented** | Could be a userspace process watcher + profile switcher |
| App Center | Windows app launcher | **not applicable** | Windows-only |
| Sound modes (DTS/MaxxAudio) | Windows audio stack | **not applicable** | No Linux equivalent |
| BIOS logo animation | Acer firmware/EC | **not implemented** | Firmware-specific and not reversible enough for this tool |
| Mobile link / QR pairing | Acer cloud service | **not applicable** | Windows/phone service |
| Profile manager | Windows XML profiles | **implemented differently** | TOML profiles + JSON snapshots |
| Monitoring page | Service named pipe | **implemented** | Dashboard, sparkline, EC + sysfs + NVML |

## What "1:1" means here

The **hardware control** surface is the project's target, not a certified 1:1
replacement. Every light, fan group, MagForce command and sensor index that
PredatorSense uses on the PH717 has a direct Linux code path, and the wire
bytes were identified from functional interface evidence. Some firmware modes
are accepted but inert or model-specific; see the status column and the
Verified-versus-implemented note in the README. The UI is a functional
equivalent, not a pixel copy. See [`PROVENANCE.md`](PROVENANCE.md).

The **Windows-only application modules** (App Center, GameSync, sound profiles,
BIOS logo, mobile pairing) do not have a direct hardware counterpart. They are
listed explicitly above rather than silently omitted.

The **XTU/NVAPI layer** is the main OS boundary. Acer's `.xtu` files are
encrypted by Intel's SDK, so the exact profiles cannot be replayed. The Linux
tool sets the same class of limits through the kernel's powercap interface
(PL1/PL2) and the NVIDIA driver (power limit, locked clocks), plus the EC's
own Normal/Fast/Turbo levels. Voltage offsets are not exposed by the Linux
NVIDIA driver and are not written by this tool.

## Safety and reversibility

- Every write is either a WMI mailbox call or a HID feature/output report.
- `--dry-run` prints the intended writes.
- `--read-only` disables writes.
- `--allow-oc` is required for EC OC, RAPL, and NVIDIA writes.
- Every profile apply snapshots the known state first.
- `panic` writes a safe state and is allowed even when normal writes are off.
- `docs/RECOVERY.md` covers EC reset, BIOS defaults, module removal, and OC
  recovery.
